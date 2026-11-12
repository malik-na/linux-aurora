// SPDX-License-Identifier: GPL-2.0-only OR MIT
/*
 * C side of the Rust t8103 suspend-to-RAM driver (s2r_rs.rs).
 *
 * Only kernel interfaces that have no Rust binding yet: the system suspend
 * operations, the arch code that holds and releases the secondary cores, two
 * device lookups, and the two hooks that modules call (the system controller's
 * driver reports its sleep handshake, the AOP driver offers its "AP power"
 * call). No decision is taken here; the sequence is in s2r_seq.rs.
 */
#include <linux/cpumask.h>
#include <linux/device.h>
#include <linux/export.h>
#include <linux/init.h>
#include <linux/of.h>
#include <linux/of_platform.h>
#include <linux/platform_device.h>
#include <linux/printk.h>
#include <linux/soc/apple/dart.h>
#include <linux/string.h>
#include <linux/suspend.h>

#include <asm/cpu_ops.h>

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
