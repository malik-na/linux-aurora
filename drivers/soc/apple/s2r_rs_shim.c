// SPDX-License-Identifier: GPL-2.0-only OR MIT
/*
 * C side of the Rust t8103 suspend-to-RAM driver (s2r_rs.rs).
 *
 * Only kernel interfaces that have no Rust binding yet: the system suspend
 * operations, the arch code that holds and releases the secondary cores, two
 * device lookups, and the two hooks that modules call (the system controller's
 * driver reports its sleep handshake, the AOP driver offers its "AP power"
 * call), and the wake path: the boot loader's resume mailbox and the kernel's
 * cpu_suspend()/cpu_resume(). No decision is taken here; the sequence is in
 * s2r_seq.rs.
 */
#include <linux/cpumask.h>
#include <linux/device.h>
#include <linux/export.h>
#include <linux/init.h>
#include <linux/io.h>
#include <linux/of.h>
#include <linux/of_platform.h>
#include <linux/platform_device.h>
#include <linux/printk.h>
#include <linux/soc/apple/dart.h>
#include <linux/string.h>
#include <linux/suspend.h>

#include <asm/cacheflush.h>
#include <asm/cpu_ops.h>
#include <asm/cputype.h>
#include <asm/memory.h>
#include <asm/smp_plat.h>
#include <asm/suspend.h>

/* s2r_rs.rs */
int s2r_rs_begin(void);
int s2r_rs_prepare(void);
int s2r_rs_enter(void);
void s2r_rs_end(void);
void s2r_rs_note_smc_bits(u8 bits);

/* called from s2r_rs.rs */
int s2r_rs_register_ops(void);
bool s2r_rs_machine_ok(void);
bool s2r_rs_all_online(void);
void s2r_rs_hold_cores(bool on);
void s2r_rs_release_cores(void);
u32 s2r_rs_parked_mask(void);
struct device *s2r_rs_sep_dev(void);
int s2r_rs_sep_dart_quiesce(void);
int s2r_rs_sep_dart_resume(void);
int s2r_rs_aop_power(u32 state);
bool s2r_rs_spmi_unbound(void);
int s2r_rs_mbox_init(void);
u64 s2r_rs_mbox_resumes(void);
int s2r_rs_suspend(int (*finish)(void));

/* called from modules */
void apple_s2r_rs_note_smc(u8 bits);
void apple_s2r_rs_set_aop_power(int (*fn)(u32 state));

static int s2r_rs_ops_valid(suspend_state_t state)
{
	return state == PM_SUSPEND_MEM;
}

static int s2r_rs_ops_begin(suspend_state_t state)
{
	return s2r_rs_begin();
}

static int s2r_rs_ops_enter(suspend_state_t state)
{
	return s2r_rs_enter();
}

static const struct platform_suspend_ops s2r_rs_ops = {
	.valid		= s2r_rs_ops_valid,
	.begin		= s2r_rs_ops_begin,
	.prepare	= s2r_rs_prepare,
	.enter		= s2r_rs_ops_enter,
	.end		= s2r_rs_end,
};

int s2r_rs_register_ops(void)
{
	suspend_set_ops(&s2r_rs_ops);
	return 0;
}

/*
 * Init time only. The older driver (apple-s2r.c) is still built; with its
 * switch on the command line the two would both take "mem", so this one stays out.
 */
bool s2r_rs_machine_ok(void)
{
	if (strstr(boot_command_line, "apple_s2r=1")) {
		pr_warn("apple_s2r_rs: apple_s2r=1 is on the command line: the older driver has \"mem\", this one stays out\n");
		return false;
	}
	return of_machine_is_compatible("apple,t8103") && apple_cpu_hotplug_active();
}

bool s2r_rs_all_online(void)
{
	return cpumask_equal(cpu_online_mask, cpu_present_mask);
}

/*
 * With the hold on, a secondary taken offline for the suspend requests its
 * stop and then waits awake for the boot core's release (smp_apple.c).
 */
void s2r_rs_hold_cores(bool on)
{
	apple_cpu_hotplug_set_s2r(on);
	apple_cpu_hotplug_set_s2r_hold(on);
}

void s2r_rs_release_cores(void)
{
	apple_cpu_s2r_release_all();
}

/* Bit n set: core n is present and has been taken offline. */
u32 s2r_rs_parked_mask(void)
{
	unsigned int cpu;
	u32 mask = 0;

	for_each_present_cpu(cpu)
		if (!cpu_online(cpu) && cpu < 32)
			mask |= BIT(cpu);
	return mask;
}

/* The Secure Enclave's platform device; its reference is kept for good. */
struct device *s2r_rs_sep_dev(void)
{
	static struct device *dev;
	struct platform_device *pdev;
	struct device_node *np;

	if (dev)
		return dev;
	np = of_find_compatible_node(NULL, NULL, "apple,sep");
	if (!np)
		return NULL;
	pdev = of_find_device_by_node(np);
	of_node_put(np);
	if (pdev)
		dev = &pdev->dev;
	return dev;
}

/*
 * The Secure Enclave's DART sits in its power domain: once that is off, a read
 * of the DART resets the machine. Tell its driver to leave the registers alone
 * from now on (its system-sleep callbacks and interrupt handler then skip them).
 */
static struct platform_device *s2r_rs_sep_dart(void)
{
	struct device *sep = s2r_rs_sep_dev();
	struct platform_device *pdev;
	struct device_node *np;

	if (!sep)
		return NULL;
	np = of_parse_phandle(sep->of_node, "iommus", 0);
	if (!np)
		return NULL;
	pdev = of_find_device_by_node(np);
	of_node_put(np);
	return pdev;
}

int s2r_rs_sep_dart_quiesce(void)
{
	struct platform_device *pdev = s2r_rs_sep_dart();
	int ret;

	if (!pdev)
		return -ENODEV;
	ret = apple_dart_quiesce_commands(&pdev->dev);
	put_device(&pdev->dev);
	return ret;
}

/* Only for an entry that stopped before the Secure Enclave was told to sleep. */
int s2r_rs_sep_dart_resume(void)
{
	struct platform_device *pdev = s2r_rs_sep_dart();
	int ret;

	if (!pdev)
		return -ENODEV;
	ret = apple_dart_resume_commands(&pdev->dev);
	put_device(&pdev->dev);
	return ret;
}

/*
 * True only if the primary power chip's SPMI controller exists as a device and
 * no driver is bound to it. Init time only (the boot-time self-test).
 */
bool s2r_rs_spmi_unbound(void)
{
	struct device *dev;
	bool unbound;

	dev = bus_find_device_by_name(&platform_bus_type, NULL, "23d0d9300.spmi");
	if (!dev)
		return false;
	unbound = !dev->driver;
	put_device(dev);
	return unbound;
}

static int (*s2r_rs_aop_hook)(u32 state);

void apple_s2r_rs_set_aop_power(int (*fn)(u32 state))
{
	WRITE_ONCE(s2r_rs_aop_hook, fn);
}
EXPORT_SYMBOL_GPL(apple_s2r_rs_set_aop_power);

int s2r_rs_aop_power(u32 state)
{
	int (*fn)(u32 state) = READ_ONCE(s2r_rs_aop_hook);

	return fn ? fn(state) : -ENODEV;
}

void apple_s2r_rs_note_smc(u8 bits)
{
	s2r_rs_note_smc_bits(bits);
}
EXPORT_SYMBOL_GPL(apple_s2r_rs_note_smc);

/*
 * The wake path. The second-stage loader (m1n1 with Ante042's S2R mailbox,
 * commit 8b3c17d and after) publishes a mailbox in /chosen/asahi,s2r-mailbox.
 * A core that is reset by the wake jumps to its slot's entry, once, with the
 * MMU off; with no entry the boot core parks there and the others go back to
 * the loader's spin table. Layout as in that loader's src/s2r.h (version 1).
 */
#define S2R_RS_MBOX_MAGIC	0x31584f424d523253ULL	/* "S2RMBOX1" */
#define S2R_RS_MBOX_VERSION	1
#define S2R_RS_MBOX_CPUS	32

struct s2r_rs_mbox_cpu {
	u64 mpidr;
	u64 impl_reg;
	u64 start_reg0;
	u64 start_reg1;
	u32 start_val0;
	u32 start_val1;
	u64 entry;
	u64 resets;
	u64 stage;
};

struct s2r_rs_mbox {
	u64 magic;
	u32 version;
	u32 ncpus;
	u64 rvbar;
	u64 quiet;
	u64 boot_mpidr;
	u64 resumes;
	u64 reserved[2];
	struct s2r_rs_mbox_cpu cpu[S2R_RS_MBOX_CPUS];
};

static struct s2r_rs_mbox *s2r_rs_mbox;
static struct s2r_rs_mbox_cpu *s2r_rs_slot;
static int (*s2r_rs_finish)(void);

static void s2r_rs_mbox_inval(void)
{
	unsigned long va = (unsigned long)s2r_rs_mbox;

	dcache_inval_poc(va, va + sizeof(*s2r_rs_mbox));
}

static void s2r_rs_slot_clean(void)
{
	unsigned long va = (unsigned long)s2r_rs_slot;

	dcache_clean_inval_poc(va, va + sizeof(*s2r_rs_slot));
}

/* Finds the mailbox and the boot core's slot; 0 if the wake path can be used. */
int s2r_rs_mbox_init(void)
{
	struct device_node *chosen;
	u64 pa, mpidr;
	u32 i;
	int ret;

	chosen = of_find_node_by_path("/chosen");
	if (!chosen)
		return -ENODEV;
	ret = of_property_read_u64(chosen, "asahi,s2r-mailbox", &pa);
	of_node_put(chosen);
	if (ret) {
		pr_err("apple_s2r_rs: wake path: no /chosen/asahi,s2r-mailbox (%d)\n", ret);
		return -ENODEV;
	}
	s2r_rs_mbox = memremap(pa, sizeof(*s2r_rs_mbox), MEMREMAP_WB);
	if (!s2r_rs_mbox)
		return -ENOMEM;
	s2r_rs_mbox_inval();
	if (s2r_rs_mbox->magic != S2R_RS_MBOX_MAGIC ||
	    s2r_rs_mbox->version != S2R_RS_MBOX_VERSION ||
	    !s2r_rs_mbox->ncpus || s2r_rs_mbox->ncpus > S2R_RS_MBOX_CPUS) {
		pr_err("apple_s2r_rs: wake path: mailbox at %#llx not valid (magic %#llx version %u cpus %u)\n",
		       pa, s2r_rs_mbox->magic, s2r_rs_mbox->version, s2r_rs_mbox->ncpus);
		goto out;
	}
	/* CPU 0's hardware id: this init may run on any core. */
	mpidr = cpu_logical_map(0) & 0xffffff;
	if (mpidr != s2r_rs_mbox->boot_mpidr) {
		pr_err("apple_s2r_rs: wake path: CPU 0 is %#llx, the loader booted on %#llx\n",
		       mpidr, s2r_rs_mbox->boot_mpidr);
		goto out;
	}
	for (i = 0; i < s2r_rs_mbox->ncpus; i++)
		if (s2r_rs_mbox->cpu[i].mpidr == mpidr)
			s2r_rs_slot = &s2r_rs_mbox->cpu[i];
	if (!s2r_rs_slot) {
		pr_err("apple_s2r_rs: wake path: no slot for %#llx\n", mpidr);
		goto out;
	}
	if (s2r_rs_slot->entry) {
		pr_err("apple_s2r_rs: wake path: the boot core's slot holds an entry already (%#llx)\n",
		       s2r_rs_slot->entry);
		goto out;
	}
	pr_info("apple_s2r_rs: wake path: mailbox at %#llx, %u cpus, rvbar %#llx, boot core %#llx in slot %ld, resumes %llu, resume entry will be %#lx\n",
		pa, s2r_rs_mbox->ncpus, s2r_rs_mbox->rvbar, mpidr,
		(long)(s2r_rs_slot - s2r_rs_mbox->cpu), s2r_rs_mbox->resumes,
		(unsigned long)__pa_symbol(cpu_resume));
	return 0;
out:
	s2r_rs_slot = NULL;
	memunmap(s2r_rs_mbox);
	s2r_rs_mbox = NULL;
	return -EINVAL;
}

/* The loader's count of resume jumps since it published the mailbox. */
u64 s2r_rs_mbox_resumes(void)
{
	if (!s2r_rs_mbox)
		return 0;
	s2r_rs_mbox_inval();
	return READ_ONCE(s2r_rs_mbox->resumes);
}

/*
 * Called by cpu_suspend() once this core's context is saved: the resume entry
 * goes into the boot core's slot (cleaned to memory), then the whole entry
 * runs. That returns only if it stopped before the cluster words or was dry;
 * then the entry is taken out again, since the loader would use it on any
 * later reset.
 */
static int s2r_rs_finisher(unsigned long arg)
{
	int ret;

	/* The slot is the loader's boot core's: no entry from any other core. */
	if ((read_cpuid_mpidr() & 0xffffff) != s2r_rs_slot->mpidr)
		return -EINVAL;
	WRITE_ONCE(s2r_rs_slot->entry, __pa_symbol(cpu_resume));
	s2r_rs_slot_clean();
	ret = s2r_rs_finish();
	WRITE_ONCE(s2r_rs_slot->entry, 0);
	s2r_rs_slot_clean();
	return ret ? ret : 1;
}

/*
 * The entry through the wake path. 0: this core came back through its resume
 * entry. Otherwise what the entry returned (never 0).
 */
int s2r_rs_suspend(int (*finish)(void))
{
	if (!s2r_rs_slot)
		return -ENODEV;
	s2r_rs_finish = finish;
	return cpu_suspend(0, s2r_rs_finisher);
}
