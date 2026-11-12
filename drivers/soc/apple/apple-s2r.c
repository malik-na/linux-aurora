// SPDX-License-Identifier: GPL-2.0-only
/*
 * Suspend-to-RAM ("mem", "deep") for the Apple M1 (t8103), first stage.
 *
 * Opt-in only: with apple_s2r=1 on the kernel command line (together with
 * apple_cpu_hotplug=park, which provides CPU offline) this registers
 * platform_suspend_ops for PM_SUSPEND_MEM. Without it nothing changes and
 * the system keeps s2idle.
 *
 * The entry follows macOS 27's own sleep on this SoC, as traced under the
 * m1n1 hypervisor (2026-11-06/07): the PM core suspends the devices and
 * takes the secondaries offline, which the CPU hotplug code turns into the
 * S2R deep stop (stop bit, ACC_OVRD deep sleep, CYC_OVRD WFI mode 3); then,
 * on the boot CPU with interrupts off, this driver
 *
 *   1. masks every AP wake source except the PMU's (source 54), so the RTC
 *      alarm is the only way out (lid, keyboard, USB come later),
 *   2. disables the AON watchdogs (WD0/WD1 control, the 0x23d2b8000 page),
 *   3. writes PMU boot_stage 0x50,
 *   4. parks the five clkgen muxes (P1-P5: set bit 31, clear [27:24], wait
 *      for bit 30),
 *   5. clears the counter blocks (P6/P7), the CPM gating words (P8), the
 *      cluster power states (P9/P10) and the SIO AUTO_ENABLE bits (P11-P13),
 *   6. starts the S2R timer (P15: 0x23d2c4000 = 1),
 *   7. masks every AIC interrupt,
 *   8. programs the PMU RTC alarm (0xd00c echo, 0xd008, 0xd00e = 0,
 *      0xd000 = 0x41) through raw, polled SPMI commands,
 *   9. starts the S2R timer twice more, as macOS does,
 *  10. puts itself into the same deep WFI.
 *
 * Nothing prints after step 6. Each step leaves a letter in PMU scratch
 * 0x9f0a (the byte m1n1's S2R work uses for stage markers), so the next
 * boot can tell how far the entry got.
 *
 * This first stage has no resume: the wake comes back through the locked
 * RVBAR into m1n1, which boots again from scratch. Resume (the m1n1 wake
 * note at 0x9f04 pointing at cpu_resume) is the next stage; the finisher
 * already runs under cpu_suspend() so that only the hand-back is missing.
 * The handback parameter (off by default) builds it up a step at a time.
 *
 * Every address here is t8103 data from those traces; nothing is taken
 * from the device tree, which maps none of these windows.
 */

#define pr_fmt(fmt) "apple-s2r: " fmt

#include <linux/bitfield.h>
#include <linux/bits.h>
#include <linux/cacheflush.h>
#include <linux/delay.h>
#include <linux/gfp.h>
#include <linux/init.h>
#include <linux/io.h>
#include <linux/iopoll.h>
#include <linux/kernel.h>
#include <linux/moduleparam.h>
#include <linux/mm_types.h>
#include <linux/of.h>
#include <linux/percpu.h>
#include <linux/pgtable.h>
#include <linux/scs.h>
#include <linux/sched.h>
#include <linux/sched/task_stack.h>
#include <linux/smp.h>
#include <linux/suspend.h>
#include <linux/unaligned.h>

#include <asm/cpu_ops.h>
#include <asm/cpuidle.h>
#include <asm/sections.h>
#include <asm/smp_plat.h>
#include <asm/suspend.h>
#include <asm/sysreg.h>

#include <clocksource/arm_arch_timer.h>

#include "apple-s2r.h"

/* clkgen (pmgr reg[2]) */
#define T8103_CLKGEN			0x23b040000ULL
static const u32 clkgen_muxes[] = { 0x140, 0x144, 0x148, 0x14c, 0x154 };
#define MUX_ENABLE			BIT(31)
#define MUX_BUSY			BIT(30)
#define MUX_SEL				GENMASK(27, 24)

/* PMGR (pmgr reg[0]) */
#define T8103_PMGR			0x23b700000ULL
#define T8103_PMGR_SIZE			0x80000
#define PMGR_PS_SIO_BUSIF		0x1c0
#define PMGR_PS_SIO			0x1c8
#define PMGR_PS_SIO_CPU			0x1d0
#define PMGR_PS_AUTO_ENABLE		BIT(28)
#define PMGR_PS_GFX			0x3f8
#define PMGR_PS_ATC0_COMMON		0x420
#define PMGR_PS_TARGET			GENMASK(3, 0)
#define PMGR_CLUSTER_PS(cl)		(0x18000 + 4 * (cl))
#define PMGR_CLUSTER_PS_MASK		GENMASK(3, 0)
static const u32 pmgr_cpm_gating[] = { 0x1c080, 0x1c088 };
#define PMGR_CPM_GATING_ON		0x80000000
static const u32 pmgr_counter_blocks[] = {
	0x34000, 0x7c000, 0x78000, 0x48400, 0x48800, 0x48c00, 0x48000,
};

/* AON */
#define T8103_AON_WAKE			0x23d004000ULL
#define AON_WAKE_MASK(i)		(0x08 + 4 * (i))	/* 1 = masked */
#define AON_WAKE_STATUS(i)		(0x1c + 4 * (i))
#define AON_WAKE_WORDS			5
#define WAKE_SRC_PMU			54			/* NUB.SPMI0Sw3IRQ */
#define T8103_AON_WDT			0x23d2b0000ULL
#define WDT_WD0_CTRL			0x0c
#define WDT_WD1_CNT			0x10
#define WDT_WD1_ALARM			0x14
#define WDT_WD1_CTRL			0x1c
#define WDT_CTRL_RESET			BIT(2)
#define WDT_EXTRA_8008			0x8008
#define WDT_EXTRA_800C			0x800c
#define T8103_S2R_TIMER			0x23d2c4000ULL
#define WDT_WD2_CNT			0x20
#define WDT_WD2_ALARM			0x24
#define WDT_WD2_CTRL			0x2c
#define AON_ENGINE_PROGRESS		0x8028	/* second page of the watchdog block */
#define AON_ENGINE_CODE2			0x8040	/* written around the engine's PMU commands */

/* AOP mailbox (ASC v4, as drivers/soc/apple/mailbox.c) and the RTKit power message */
#define T8103_AOP_MBOX			0x24a408000ULL
#define ASC_A2I_CONTROL			0x110
#define ASC_I2A_CONTROL			0x114
#define ASC_CONTROL_FULL		BIT(16)
#define ASC_CONTROL_EMPTY		BIT(17)
#define ASC_A2I_SEND0			0x800
#define ASC_A2I_SEND1			0x808
#define ASC_I2A_RECV0			0x830
#define ASC_I2A_RECV1			0x838
#define RTKIT_MGMT_TYPE			GENMASK_ULL(59, 52)
#define RTKIT_MGMT_SET_AP_PWR_STATE	0xb
#define RTKIT_PWR_STATE_IDLE		0x201

/* nub-spmi (gen1 register layout, as m1n1 src/spmi.c) */
#define T8103_NUB_SPMI			0x23d0d9300ULL
#define T8103_NUB_SPMI1			0x23d0e1300ULL
#define SPMI_IRQ_EN(w)			(0x20 + 4 * (w))	/* 1 = unmasked */
#define SPMI_IRQ_ACK(w)			(0x60 + 4 * (w))	/* write 1 to clear */
#define SPMI_IRQ_WORDS			9			/* 256 peripheral + 32 */
#define SPMI_IRQ_PMU_ALARM		2			/* the RTC alarm, word 0 */
#define SPMI_STATUS			0x00
#define SPMI_CMD			0x04
#define SPMI_REPLY			0x08
#define SPMI_STATUS_RX_EMPTY		BIT(24)
#define SPMI_STATUS_TX_EMPTY		BIT(8)
#define SPMI_CMD_EXTRA			GENMASK(31, 16)
#define SPMI_CMD_ACTIVE			BIT(15)
#define SPMI_CMD_ADDR			GENMASK(14, 8)
#define SPMI_CMD_OPC			GENMASK(7, 0)
#define SPMI_REPLY_ACK			BIT(15)
#define SPMI_REPLY_ADDR			GENMASK(14, 8)
#define SPMI_REPLY_OPC			GENMASK(7, 0)
#define SPMI_OPC_EXT_WRITEL		0x30
#define SPMI_OPC_EXT_READL		0x38
#define PMU_SLAVE			0xf
#define PMU_RTC_CTRL			0xd000
#define PMU_RTC_NOW			0xd002		/* read 6 bytes; seconds in bytes 2..5 */
#define PMU_RTC_ALARM			0xd008
#define PMU_RTC_EVENT			0xd00c
#define PMU_RTC_IRQ_MASK		0xd00e
#define PMU_RTC_CTRL_ALARM_ON		0x41
#define PMU_BOOT_STAGE			0x9f01
#define PMU_BOOT_STAGE_SLEEP		0x50
#define PMU_SCR_STAGE			0x9f0a
#define PMU_SCR_NOTE			0x9f04	/* 'L' + commit RTC s, or 'W' + entry PA >> 2 */
#define PMU_SCR_WAKE_MARK		0x9f0b	/* 0x9f0b..0x9f0c: the ESP stage-2 m1n1 wake breadcrumbs */
/*
 * Diagnostics (apple_s2r.diag, apple_s2r.aop_sleep, apple_s2r.wake_timer_ms): one record of
 * twelve bytes at 0x9f10..0x9f17, 0x9f1a and 0x9f1c..0x9f1e, the bytes of this area shown to
 * keep a value across a boot (0x9f0d, 0x9f0e and 0x9f1b do not; 0x9f0f, 0x9f18, 0x9f19 and
 * 0x9f1f are left alone). Byte 0 says which record it is. The wake stage's entry record uses
 * the same bytes, so a record here survives only a run in which the stage does not run.
 * Nothing is written unless one of the parameters is set.
 */
#define PMU_SCR_PCIE			0x9f1e	/* one byte, see apple_s2r_note_pcie(); shared with diag=4's record, not used together */
#define PMU_SCR_DIAG_A			0x9f10
#define PMU_SCR_DIAG_B			0x9f1a	/* one byte */
#define PMU_SCR_DIAG_C			0x9f1c	/* three bytes */
#define DIAG_REC_STATUS			0x10	/* | number of wake-status bits set, max 15 */
#define DIAG_REC_STATUS_LATE		0x20	/* the same, sampled just before the final WFI */
#define DIAG_REC_ENGINE			0x32
#define DIAG_REC_GATES			0x44
#define DIAG_REC_POKES			0x50
#define DIAG_REC_WFI_RETURN		0x77
/* types keep bit 7 clear: a first byte with bit 7 set is the wake stage's record header */
#define DIAG_REC_CORES			0x36
#define DIAG_REC_SPMI			0x37
#define DIAG_REC_AOP			0x60	/* | result of the AOP message (aop_sleep alone) */

/* the AON engine's sleep gates that only read (see s2r_diag_gates()) */
#define T8103_AON_HANDSHAKE		0x23d2c0000ULL
#define AON_HANDSHAKE			0x1c
#define PMGR_CLUSTER_DEEP_IDLE		BIT(6)
#define PMGR_PWRGATE_STATUS(n)		(0x1c010 + 0x18 * (n) + 8)	/* bit 30 */
#define PMGR_COUNTER_ACK(n)		(0x48008 + 0x400 * (n))		/* bit 0 */
#define PMGR_PS_DISP0_CPU0		0x10018
#define PMGR_PS_DISP0_FE		0x360
#define PMGR_PS_DISPEXT_FE		0x368
#define PMGR_PS_DISPEXT_CPU0		0x378
#define PMGR_PS_RMX			0x350
#define PMGR_PS_APCIE			0x348
#define PMGR_PS_PMP			0x3d8
#define PMGR_PS_APCIE_GP		0x3e8
#define PMGR_PS_ANS2			0x3f0
#define PMGR_PS_APCIE_ST		0x418
#define PMGR_PS_ACTUAL			GENMASK(7, 4)
#define PMGR_PS_BUSY			BIT(11)

/* per-cluster CPU state reported to PMP (pmgr + 0x38000): 0 = all idle */
#define PMGR_CLUSTER_STATE(cl)		(0x38004 + 4 * (cl))

/* AIC (AIC1 on t8103) */
#define T8103_AIC			0x23b100000ULL
#define T8103_MCC_GLOBAL		0x200100000ULL	/* /arm-io/mcc reg[0] */
#define T8103_MCC_PLANES		0x200000000ULL	/* /arm-io/mcc reg[1] */
#define MCC_BCAST_A			0x0c
#define MCC_BCAST_B			0x14
#define MCC_PLANE_CACHE_ENABLE		0x1c00
#define MCC_PLANE_CACHE_STATUS(n)	(0x1c04 + (n) * 0x40000)
#define MCC_PLANES			4
#define MCC_STATUS_BUSY			(GENMASK(14, 10) | GENMASK(9, 5))	/* data, tag */
#define AIC_INFO			0x0004
#define AIC_INFO_NR_IRQ			GENMASK(15, 0)
#define AIC_MASK_SET(w)			(0x4100 + 4 * (w))

/* deep WFI (smp_apple_asm.S has the same values for the secondaries) */
#define SYS_APL_CYC_OVRD		sys_reg(3, 5, 15, 5, 0)
#define SYS_APL_ACC_OVRD		sys_reg(3, 5, 15, 6, 0)
#define CYC_OVRD_MODES			GENMASK_ULL(25, 20)
#define CYC_OVRD_S2R			(BIT_ULL(25) | BIT_ULL(24) | BIT_ULL(0))
#define ACC_OVRD_S2R_CLR		((3ULL << 13) | (3ULL << 15) | (3ULL << 17) | (3ULL << 25))
#define ACC_OVRD_S2R_SET		(BIT_ULL(34) | BIT_ULL(32) | (2ULL << 15) | (3ULL << 13) | \
					 (3ULL << 17) | (3ULL << 25))

static bool apple_s2r_enabled __ro_after_init;
static unsigned int alarm_secs = 60;
module_param(alarm_secs, uint, 0644);
MODULE_PARM_DESC(alarm_secs, "PMU RTC alarm, seconds after the entry (0: none, wake by nothing)");
static bool global_sleep = true;
module_param(global_sleep, bool, 0644);
MODULE_PARM_DESC(global_sleep, "secondaries hold in WFE after their stop request until after the commit, then every core enters the deep WFI together, as XNU and the macOS traces do (0: they go down before the finisher)");

static bool s2r_hold_armed;

static bool macos_ps;
module_param(macos_ps, bool, 0644);
MODULE_PARM_DESC(macos_ps, "at the commit, set ps_gfx and ps_atc0_common to target 0 without auto-enable, as the macOS traces have them (recorded as bit 6 of the PMU 0x9f0c byte)");

static bool stop_cores = true;
module_param(stop_cores, bool, 0644);
MODULE_PARM_DESC(stop_cores, "power the secondaries down for real (off: they park, for pm_test runs)");

static void __iomem *clkgen, *pmgr, *aon_wake, *aon_wdt, *s2r_timer, *spmi, *spmi1, *aic;
static void __iomem *mcc_glob, *mcc_planes;
static void __iomem *aon_hs;	/* optional: diagnostics only */
static void __iomem *ecore_impl[3];	/* cpu1..3: optional, diagnostics only */
static void __iomem *aop_mbox;	/* optional: only apple_s2r.aop_sleep uses it */
/*
 * Test bits stored in PMU 0x9f0c at the commit (m1n1 overwrites 0x9f0c only if the wake
 * reaches it): bit 0 MBSE slpw, 1 WKTP=2, 2 MBSE slpa, 3 SMC AP power 0x201 acked (set by
 * macsmc through apple_s2r_note_smc()), 4 every MCC plane flushed, 5 PMP idled, 7 = record valid.
 */
static u8 s2r_handshake_bits;

void apple_s2r_note_smc(u8 bits)
{
	s2r_handshake_bits = (s2r_handshake_bits & ~0x0f) | (bits & 0x0f);
}
EXPORT_SYMBOL_GPL(apple_s2r_note_smc);

/*
 * pcie-apple's noirq suspend reports, when its s2r_suspend switch is on, how many port links
 * it took down and how many it skipped. Saved at PMU 0x9f1e (a byte that survives a reset and
 * that nothing else writes with diag=0) next to the 'N' mark, so a run that ends in a reset
 * still shows whether the ports went down: bit 6 set = pcie-apple reported, bits 5..3 = ports
 * down, bits 2..0 = ports skipped (each saturating at 7). Left untouched (00 after the
 * pre-run clear) when nothing was reported: the switch is off or it is not a t8103 controller.
 */
static u8 s2r_pcie_ports;

void apple_s2r_note_pcie(unsigned int down, unsigned int skipped)
{
	s2r_pcie_ports = BIT(6) | min(down, 7U) << 3 | min(skipped, 7U);
}
EXPORT_SYMBOL_GPL(apple_s2r_note_pcie);

/*
 * DRAM retention probe: with Linux kept below it (mem=), fill the page at retention_pa with
 * a pattern at .prepare; the ESP stage-2 m1n1 counts the intact words on the next cold boot
 * and reports 'R' / 'r' / 'X' at PMU 0x9f0b (see m1n1 s2r_retention_check()).
 */
static unsigned long retention_pa;
module_param(retention_pa, ulong, 0644);
MODULE_PARM_DESC(retention_pa, "physical page outside Linux's RAM to fill with the retention pattern (0 = off)");

static void s2r_write_retention_pattern(void)
{
	void __iomem *p;
	int i;

	if (!retention_pa || pfn_valid(PHYS_PFN(retention_pa)))
		return;
	p = ioremap_np(retention_pa, SZ_4K);
	if (!p)
		return;
	for (i = 0; i < 512; i++)
		writeq_relaxed(0x6d31707772657421ULL ^ ((u64)i * 0x9e3779b97f4a7c15ULL), p + 8 * i);
	dsb(sy);
	iounmap(p);
}

/*
 * The PMP (power-management coprocessor), when the pmp_v1 module runs it: macOS idles it
 * (SET_AP_POWER 0x201 then SET_IOP_POWER 0x201) between MBSE slpw and slpa. .prepare
 * sits exactly there: after every device's .suspend (macsmc sends slpw), before
 * .suspend_late (macsmc sends slpa). pmp_v1 registers the callback once its RTKit is up.
 */
static int (*s2r_pmp_power)(bool idle);
static bool s2r_pmp_idled;

void apple_s2r_set_pmp_power(int (*fn)(bool idle))
{
	WRITE_ONCE(s2r_pmp_power, fn);
	pr_info("PMP power hook %s\n", fn ? "registered" : "removed");
}
EXPORT_SYMBOL_GPL(apple_s2r_set_pmp_power);

/*
 * Self-test of the PMP idle path without suspending: write 1 to
 * /sys/module/apple_s2r/parameters/pmp_idle_test to idle the PMP (AP 0x201, IOP 0x201,
 * each acked) and wake it again; both results go to the kernel log.
 */
static int pmp_idle_test_set(const char *val, const struct kernel_param *kp)
{
	int (*fn)(bool) = READ_ONCE(s2r_pmp_power);
	int ri, rw;

	if (!fn) {
		pr_info("PMP idle test: no PMP hook registered\n");
		return -ENODEV;
	}
	ri = fn(true);
	pr_info("PMP idle test: idle (AP 0x201, IOP 0x201) -> %d\n", ri);
	msleep(200);
	rw = fn(false);
	pr_info("PMP idle test: wake -> %d\n", rw);
	return ri ?: rw;
}

static const struct kernel_param_ops pmp_idle_test_ops = {
	.set = pmp_idle_test_set,
};
module_param_cb(pmp_idle_test, &pmp_idle_test_ops, NULL, 0200);
MODULE_PARM_DESC(pmp_idle_test, "write 1: idle the PMP and wake it again, without suspending");

static bool macos_wake_mask = true;
module_param(macos_wake_mask, bool, 0644);
MODULE_PARM_DESC(macos_wake_mask, "AON wake masks as in the macOS run that woke (only 26/27/38/81 masked); 0 = only the PMU alarm unmasked");

static unsigned int wake_mask[AON_WAKE_WORDS];
static int wake_mask_n;
module_param_array(wake_mask, uint, &wake_mask_n, 0644);
MODULE_PARM_DESC(wake_mask, "five AON wake mask words (1 = masked) written at the commit instead of the built-in ones; used only when exactly five are given");

static unsigned int diag;
module_param(diag, uint, 0644);
MODULE_PARM_DESC(diag, "0 = off; 1 = save which AON wake-status bits are set just before the commit; 2 = save the AON engine's progress word and second code before the commit and about 1 s after it, with watchdog 2's control and count; 3 = as 1 but sampled after the last commit write, just before the final WFI; 4 = save the state of the engine's read-only sleep gates and the PCIe, display and PMP power states just before the commit; 5 = save what the test pokes did; 6 = release the held cores, wait cores_wait_ms, then save both cluster power words and the power status of cpu1..3, before this core's own WFI; 7 = save which nub-spmi IRQ words are enabled and pending after the last acknowledge (PMU scratch, see PMU_SCR_DIAG_*)");

static unsigned int cores_wait_ms = 100;
module_param(cores_wait_ms, uint, 0644);
MODULE_PARM_DESC(cores_wait_ms, "diag=6: how long after the release the cluster words are read (max 1000)");

static unsigned int wake_timer_ms;
module_param(wake_timer_ms, uint, 0644);
MODULE_PARM_DESC(wake_timer_ms, "test only: arm this core's timer interrupt this long before its final WFI; if the WFI returns (the core was not power-gated) save the engine's progress word, the cluster power words and watchdog 2 (0 = off)");

/*
 * Test pokes: up to eight (physical address, mask, value) triples; each 32-bit register is
 * read, the mask bits replaced by the value, and written back, in order, just before the
 * first commit write. Only PMGR power-state words are accepted (PMGR + 0x100..0xbfc and
 * PMGR + 0x10000..0x100fc, word aligned): not the SEP's word, the cluster power words, the
 * power gates, the clock muxes or anything outside the PMGR. A list with any other address,
 * or whose length is not a multiple of three, is refused as a whole and nothing is written.
 * Checked and mapped at .prepare. Empty by default.
 */
#define S2R_POKES	8
static unsigned long poke[3 * S2R_POKES];
static int poke_n;
module_param_array(poke, ulong, &poke_n, 0644);
MODULE_PARM_DESC(poke, "test only: address,mask,value triples (PMGR power-state words) applied just before the commit");
static void __iomem *poke_map[S2R_POKES];
static bool poke_refused;
static u8 poke_applied, poke_before[S2R_POKES];

static bool s2r_poke_addr_ok(unsigned long a)
{
	if (a & 3)
		return false;
	return (a >= T8103_PMGR + 0x100 && a <= T8103_PMGR + 0xbfc) ||
	       (a >= T8103_PMGR + 0x10000 && a <= T8103_PMGR + 0x100fc);
}

static unsigned int poke_settle_ms;
module_param(poke_settle_ms, uint, 0644);
MODULE_PARM_DESC(poke_settle_ms, "test only: wait this long after the pokes, before the diagnostics sample and the commit");

/*
 * Secondary trap (needs a wake stage that has it): while a suspend is in progress the
 * 8 bytes at the boot CPU's reset vector page + 0x10, padding of the stage's first vector
 * slot, hold "S2RTRAP!". A core other than the boot CPU that comes through the reset vector
 * then stops in the stage under its own WD1 time instead of running the bootloader's
 * secondary path. The page is found from cpu0's locked reset-vector register and is only
 * patched if it looks like the stage (first instruction, a branch, two no-ops at +0x10);
 * if it cannot be armed the suspend is refused. The no-ops are written back on every way
 * out that returns here; a reset reloads the page. Off by default.
 */
static bool sec_trap;
module_param(sec_trap, bool, 0644);
MODULE_PARM_DESC(sec_trap, "test only: arm the wake stage's trap for non-boot cores for the duration of the suspend");

#define T8103_CPU0_IMPL			0x210050000ULL
#define CPU_RVBAR_LOCKED		BIT_ULL(0)
#define CPU_RVBAR_ADDR			GENMASK_ULL(47, 12)
#define S2R_TRAP_OFF			0x10
#define S2R_TRAP_MAGIC			0x2150415254523253ULL	/* "S2RTRAP!" */
#define S2R_TRAP_NOPS			0xd503201fd503201fULL
#define S2R_VEC_INSN0			0xd2800609		/* mov x9, #0x30 */
static void __iomem *s2r_trap_page;

static int s2r_trap_arm(void)
{
	void __iomem *r, *p;
	phys_addr_t pa;
	u64 rv;

	r = ioremap_np(T8103_CPU0_IMPL, 8);
	if (!r)
		return -ENOMEM;
	rv = readq_relaxed(r);
	iounmap(r);
	pa = rv & CPU_RVBAR_ADDR;
	if (!(rv & CPU_RVBAR_LOCKED) || !pa || pfn_valid(PHYS_PFN(pa))) {
		pr_err("sec_trap: cpu0 reset vector %#llx is not a locked page outside Linux's RAM\n", rv);
		return -EINVAL;
	}
	p = ioremap_np(pa, SZ_4K);
	if (!p)
		return -ENOMEM;
	if (readl_relaxed(p) != S2R_VEC_INSN0 || (readl_relaxed(p + 4) >> 26) != 0x05 ||
	    readq_relaxed(p + S2R_TRAP_OFF) != S2R_TRAP_NOPS) {
		pr_err("sec_trap: the page at %pa does not look like the wake stage, not patched\n", &pa);
		iounmap(p);
		return -EINVAL;
	}
	writeq_relaxed(S2R_TRAP_MAGIC, p + S2R_TRAP_OFF);
	dsb(sy);
	s2r_trap_page = p;
	pr_info("sec_trap armed at %pa+%#x\n", &pa, S2R_TRAP_OFF);
	return 0;
}

static void s2r_trap_disarm(void)
{
	if (!s2r_trap_page)
		return;
	writeq_relaxed(S2R_TRAP_NOPS, s2r_trap_page + S2R_TRAP_OFF);
	dsb(sy);
	iounmap(s2r_trap_page);
	s2r_trap_page = NULL;
}

static bool aop_sleep;
module_param(aop_sleep, bool, 0644);
MODULE_PARM_DESC(aop_sleep, "at the entry send the AOP the RTKit 'AP power state 0x201' message and take its reply (with diag=0 the result is saved in PMU 0x9f0e)");

static bool spmi_macos = true;
module_param(spmi_macos, bool, 0644);
MODULE_PARM_DESC(spmi_macos, "leave the nub-spmi IRQ words as macOS does at the commit (word 6 = 1 on both buses, others untouched); 0 = the old 'alarm only' setup");

static unsigned int spmi_final;

static int spmi_final_set(const char *val, const struct kernel_param *kp)
{
	return param_set_uint_minmax(val, kp, 0, 2);
}

static const struct kernel_param_ops spmi_final_ops = {
	.set = spmi_final_set,
	.get = param_get_uint,
};
module_param_cb(spmi_final, &spmi_final_ops, &spmi_final, 0644);
MODULE_PARM_DESC(spmi_final, "test only, overrides spmi_macos: 1 = the nub-spmi IRQ words as the macOS traces have them at the last commit write (nub-spmi: enable word 0 = the alarm bit, enable word 6 = 0; nub-spmi1: enable word 6 = 1; other words untouched; both word-6 bits acked, the alarm bit only before the alarm is armed); 2 = the 'alarm only' setup with nub-spmi1 enable word 6 = 1");

static bool mcc_flush = true;
module_param(mcc_flush, bool, 0644);
MODULE_PARM_DESC(mcc_flush, "turn the system-level cache off and flush it before the commit, as macOS does");

static unsigned int wfi_return_secs = 30;
module_param(wfi_return_secs, uint, 0644);
MODULE_PARM_DESC(wfi_return_secs, "if the final wfi returns (the core was not power-gated), reset via WD1 after this long (0 = off)");

static unsigned int deadman_secs = 170;
module_param(deadman_secs, uint, 0644);
MODULE_PARM_DESC(deadman_secs, "arm watchdog 1 for this long across the sleep (max 178; 0 = off), "
		 "so a wake that hangs in m1n1 still resets the SoC");

/*
 * Hand-back mode: the wake comes back into this kernel instead of rebooting. 0 is the
 * first stage as it was ('L' note, the wake resets in m1n1); nothing below runs. In any
 * other mode the commit also cleans what a return into the kernel reads out of the boot
 * CPU's caches (s2r_handback_clean()). 1: the note is 'W' with apple_s2r_hb_stub, which
 * only reports through WD1 (apple-s2r-asm.S). 2: 'W' with apple_s2r_hb_resume, in front of
 * cpu_resume; when cpu_suspend() returns, s2r_handback_wake() marks, checks memory and
 * resets through WD1, without restoring anything. 3 (carry on into a full resume) is
 * reserved for the next milestone and runs as 2. Latched at .prepare.
 */
static unsigned int handback;

static int handback_set(const char *val, const struct kernel_param *kp)
{
	return param_set_uint_minmax(val, kp, 0, 3);
}

static const struct kernel_param_ops handback_ops = {
	.set = handback_set,
	.get = param_get_uint,
};
module_param_cb(handback, &handback_ops, &handback, 0644);
MODULE_PARM_DESC(handback, "hand the wake back to the kernel: 0 off (the wake reboots); "
		 "1 'W' note to an MMU-off stub that resets through WD1; "
		 "2 'W' note to cpu_resume, then marks, memory check, WD1 reset; 3 reserved (runs as 2)");

static unsigned int s2r_hb_mode;

/*
 * handback=2 state, all kept in RAM across the sleep: the commit second, a checksum of
 * the kernel text taken just before cpu_suspend(), and a page that is filled at the
 * commit but deliberately left out of s2r_handback_clean() (allocated, so it is not in
 * the image's bss): it survives only if the L1 set/way clean and the deep sleep's L2
 * flush between them get dirty lines to DRAM, which is what a full resume will need.
 */
u64 apple_s2r_hb_pattern[S2R_HB_WORDS] __aligned(SZ_4K);
static u32 s2r_hb_commit_rtc;
static u64 s2r_hb_text_sum;
static u64 *s2r_hb_dirty;
static bool s2r_woken;	/* back from cpu_suspend(): no timer use from here on */

static int __init apple_s2r_setup(char *arg)
{
	apple_s2r_enabled = !arg || !strcmp(arg, "1") || !strcmp(arg, "on");
	return 0;
}
early_param("apple_s2r", apple_s2r_setup);

/* ---- raw, polled SPMI (IRQs off, no locks, no printing) ---- */

/*
 * After a hand-back wake the arch timer's event stream is not set up again, so udelay()
 * may never return: poll by count there. Otherwise as it always was.
 */
static int s2r_spmi_wait_reply(void)
{
	u32 st;
	int i;

	if (!s2r_woken)
		return readl_poll_timeout_atomic(spmi + SPMI_STATUS, st,
						 !(st & SPMI_STATUS_RX_EMPTY), 10, 10000);
	for (i = 0; i < 100000; i++)
		if (!(readl_relaxed(spmi + SPMI_STATUS) & SPMI_STATUS_RX_EMPTY))
			return 0;
	return -ETIMEDOUT;
}

static int s2r_spmi_cmd(u8 opc, u16 extra, const u8 *in, size_t len_in, u8 *out, size_t len_out)
{
	u32 reply;
	int i, j;

	if (!(readl_relaxed(spmi + SPMI_STATUS) & SPMI_STATUS_TX_EMPTY))
		return -EBUSY;
	for (i = 0; i < 16 && !(readl_relaxed(spmi + SPMI_STATUS) & SPMI_STATUS_RX_EMPTY); i++)
		readl_relaxed(spmi + SPMI_REPLY);	/* drop leftovers */

	writel_relaxed(FIELD_PREP(SPMI_CMD_EXTRA, extra) | SPMI_CMD_ACTIVE |
		       FIELD_PREP(SPMI_CMD_ADDR, PMU_SLAVE) | FIELD_PREP(SPMI_CMD_OPC, opc),
		       spmi + SPMI_CMD);
	for (i = 0; i < len_in;) {
		u32 data = 0;

		for (j = 0; j < 4 && i < len_in; j++)
			data |= in[i++] << (8 * j);
		writel_relaxed(data, spmi + SPMI_CMD);
	}

	if (s2r_spmi_wait_reply())
		return -ETIMEDOUT;
	reply = readl_relaxed(spmi + SPMI_REPLY);
	if (FIELD_GET(SPMI_REPLY_OPC, reply) != opc || FIELD_GET(SPMI_REPLY_ADDR, reply) != PMU_SLAVE)
		return -EIO;

	for (i = 0; i < len_out;) {
		u32 data;

		if (readl_relaxed(spmi + SPMI_STATUS) & SPMI_STATUS_RX_EMPTY)
			return -EIO;
		data = readl_relaxed(spmi + SPMI_REPLY);
		for (j = 0; j < 4 && i < len_out; j++)
			out[i++] = data >> (8 * j);
	}
	if (!len_out && !(reply & SPMI_REPLY_ACK))
		return -EIO;
	return 0;
}

static int pmu_write(u16 reg, const u8 *buf, size_t len)
{
	return s2r_spmi_cmd(SPMI_OPC_EXT_WRITEL | (len - 1), reg, buf, len, NULL, 0);
}

static int pmu_read(u16 reg, u8 *buf, size_t len)
{
	return s2r_spmi_cmd(SPMI_OPC_EXT_READL | (len - 1), reg, NULL, 0, buf, len);
}

static void s2r_mark(u8 c)
{
	pmu_write(PMU_SCR_STAGE, &c, 1);
}

/* ---- diagnostics ---- */


/* d: twelve bytes, in write lengths the alarm and the note already use. */
static void s2r_diag_put(const u8 *d)
{
	pmu_write(PMU_SCR_DIAG_A, d, 4);
	pmu_write(PMU_SCR_DIAG_A + 4, d + 4, 4);
	pmu_write(PMU_SCR_DIAG_B, d + 8, 1);
	pmu_write(PMU_SCR_DIAG_C, d + 9, 3);
}

/*
 * diag=1 and 3: which AON AP wake-status bits are set. Byte 0 the record type | count,
 * byte 1 a summary (bit 0..4 = sources 23, 25, 54, 78, 80; bit 5 = any source in 96..127;
 * bit 6 = any other source below 96; bit 7 = any in 128..159), byte 2 = 0x80 | (more than
 * one bit in word 3 ? 0x40 : 0) | the lowest set bit of word 3 (0 if that word is clear),
 * bytes 3..11 the first nine source numbers in ascending order (0xff = no more).
 */
static void s2r_diag_status(u8 rec)
{
	u8 d[12];
	unsigned int n = 0;
	int i, j;

	memset(d, 0xff, sizeof(d));
	d[1] = 0;
	d[2] = 0;
	for (i = 0; i < AON_WAKE_WORDS; i++) {
		u32 v = readl_relaxed(aon_wake + AON_WAKE_STATUS(i));

		for (j = 0; j < 32; j++) {
			if (!(v & BIT(j)))
				continue;
			if (n < 9)
				d[3 + n] = i * 32 + j;
			n++;
			switch (i * 32 + j) {
			case 23: d[1] |= BIT(0); break;
			case 25: d[1] |= BIT(1); break;
			case 54: d[1] |= BIT(2); break;
			case 78: d[1] |= BIT(3); break;
			case 80: d[1] |= BIT(4); break;
			default: d[1] |= i == 3 ? BIT(5) : i == 4 ? BIT(7) : BIT(6); break;
			}
		}
		if (i == 3 && v)
			d[2] = 0x80 | (hweight32(v) > 1 ? 0x40 : 0) | __ffs(v);
	}
	d[0] = rec | min(n, 15U);
	s2r_diag_put(d);
}

/*
 * diag=2: the AON engine's progress word and its second code, read just before the first
 * commit write and again about one second after the last one, with this core still
 * running; and watchdog 2's control and count at that second read. Bytes: type, second
 * code before, second code after, WD2 control, progress before (4), progress after folded
 * to one byte (s2r_prog_code()), WD2 count bits 31..8 (3).
 */
static u32 diag_prog0, diag_code0;

static void s2r_diag_engine_before(void)
{
	diag_prog0 = readl_relaxed(aon_wdt + AON_ENGINE_PROGRESS);
	diag_code0 = readl_relaxed(aon_wdt + AON_ENGINE_CODE2);
}

/* progress word folded to one byte: 0x80 | bit number if exactly one bit is set, 0 if none, 0x7f otherwise */
static u8 s2r_prog_code(u32 p)
{
	if (!p)
		return 0;
	return hweight32(p) == 1 ? 0x80 | __ffs(p) : 0x7f;
}

static void s2r_diag_postcommit(void)
{
	u32 prog, code, ctrl, cnt;
	u8 d[12];

	mdelay(1000);
	prog = readl_relaxed(aon_wdt + AON_ENGINE_PROGRESS);
	code = readl_relaxed(aon_wdt + AON_ENGINE_CODE2);
	ctrl = readl_relaxed(aon_wdt + WDT_WD2_CTRL);
	cnt = readl_relaxed(aon_wdt + WDT_WD2_CNT);
	d[0] = DIAG_REC_ENGINE;
	d[1] = diag_code0;
	d[2] = code;
	d[3] = ctrl;
	put_unaligned_le32(diag_prog0, d + 4);
	d[8] = s2r_prog_code(prog);
	d[9] = cnt >> 8;
	d[10] = cnt >> 16;
	d[11] = cnt >> 24;
	s2r_diag_put(d);
}

static u8 s2r_ps_actual(u32 off)
{
	return FIELD_GET(PMGR_PS_ACTUAL, readl_relaxed(pmgr + off));
}

static u8 s2r_aop_result = 0xff;	/* 0xff: no message sent */

/*
 * diag=4: what the AON engine's sleep entry will find, read just before the first commit
 * write. Reads only. Bytes:
 *  0 type
 *  1 E cluster power word bits 7..0 (bit 6: every core of the cluster in deep idle)
 *  2 P cluster power word bits 7..0
 *  3,4 the handshake word 0x23d2c001c bits 15..0, low byte first (0xffff: not mapped)
 *  5 bit 0..3: counter-block acknowledge bit of blocks 0..3; bit 4..7: status bit 30 of
 *    power gates 35 (never configured by the entry), 59, 33, 5
 *  6 actual power state of apcie_gp << 4 | apcie_st
 *  7 disp0_cpu0 << 4 | disp0_fe
 *  8 dispext_cpu0 << 4 | dispext_fe
 *  9 rmx << 4 | pmp
 * 10 ans2 << 4 | apcie (the parent of apcie_gp and apcie_st)
 * 11 bit 0..4: busy bit of apcie_gp, apcie_st, pmp, ans2, apcie; bit 5: an AOP power message
 *    was sent, bit 6: it was acknowledged, bit 7: it could not be sent
 */
static void s2r_diag_gates(void)
{
	static const u32 busy[] = { PMGR_PS_APCIE_GP, PMGR_PS_APCIE_ST, PMGR_PS_PMP,
				    PMGR_PS_ANS2, PMGR_PS_APCIE };
	static const u8 gates[] = { 35, 59, 33, 5 };
	u32 hs = aon_hs ? readl_relaxed(aon_hs + AON_HANDSHAKE) : 0xffff;
	u8 d[12] = { 0 };
	int i;

	d[0] = (DIAG_REC_GATES & 0xf0) | (poke_refused ? 0xf : poke_applied);
	d[1] = readl_relaxed(pmgr + PMGR_CLUSTER_PS(0));
	d[2] = readl_relaxed(pmgr + PMGR_CLUSTER_PS(1));
	d[3] = hs;
	d[4] = hs >> 8;
	for (i = 0; i < 4; i++) {
		if (readl_relaxed(pmgr + PMGR_COUNTER_ACK(i)) & BIT(0))
			d[5] |= BIT(i);
		if (readl_relaxed(pmgr + PMGR_PWRGATE_STATUS(gates[i])) & BIT(30))
			d[5] |= BIT(4 + i);
	}
	d[6] = s2r_ps_actual(PMGR_PS_APCIE_GP) << 4 | s2r_ps_actual(PMGR_PS_APCIE_ST);
	d[7] = s2r_ps_actual(PMGR_PS_DISP0_CPU0) << 4 | s2r_ps_actual(PMGR_PS_DISP0_FE);
	d[8] = s2r_ps_actual(PMGR_PS_DISPEXT_CPU0) << 4 | s2r_ps_actual(PMGR_PS_DISPEXT_FE);
	d[9] = s2r_ps_actual(PMGR_PS_RMX) << 4 | s2r_ps_actual(PMGR_PS_PMP);
	d[10] = s2r_ps_actual(PMGR_PS_ANS2) << 4 | s2r_ps_actual(PMGR_PS_APCIE);
	for (i = 0; i < ARRAY_SIZE(busy); i++)
		if (readl_relaxed(pmgr + busy[i]) & PMGR_PS_BUSY)
			d[11] |= BIT(i);
	if (s2r_aop_result != 0xff)
		d[11] |= BIT(5) | (s2r_aop_result & 1 ? BIT(6) : 0) | (s2r_aop_result & 2 ? BIT(7) : 0);
	s2r_diag_put(d);
}

/*
 * diag=5: what the test pokes did. Bytes: 0 = 0x50 | pokes applied (0x5f: list refused);
 * 1 = status bit 30 of power gates 35, 59, 33, 5 in bits 0..3, read after the settle time;
 * then for pokes 0..4 the register's low byte before the poke and after the settle time
 * (a power-state word's low byte is actual << 4 | target).
 */
static void s2r_diag_pokes(void)
{
	static const u8 gates[] = { 35, 59, 33, 5 };
	u8 d[12] = { 0 };
	int i;

	d[0] = (DIAG_REC_POKES & 0xf0) | (poke_refused ? 0xf : poke_applied);
	for (i = 0; i < 4; i++)
		if (readl_relaxed(pmgr + PMGR_PWRGATE_STATUS(gates[i])) & BIT(30))
			d[1] |= BIT(i);
	for (i = 0; i < 5 && i < poke_applied; i++) {
		if (!poke_map[i])
			continue;
		d[2 + 2 * i] = poke_before[i];
		d[3 + 2 * i] = readl_relaxed(poke_map[i]);
	}
	s2r_diag_put(d);
}

/*
 * The acknowledge of step 9b: whatever our own PMU commands left pending. Also repeated by
 * the records that are written after it. spmi_final=1 acks the two word-6 bits only: the
 * alarm is armed by then (step 8), so an ack of the alarm bit here could only remove a real
 * alarm; that bit is acked before the alarm is armed (5b and step 8) and never after.
 */
static void s2r_spmi_ack(void)
{
	int i;

	if (spmi_final == 1 || (spmi_macos && !spmi_final)) {
		writel_relaxed(1, spmi + SPMI_IRQ_ACK(6));
		writel_relaxed(1, spmi1 + SPMI_IRQ_ACK(6));
	} else {
		for (i = 0; i < SPMI_IRQ_WORDS; i++) {
			writel_relaxed(0xffffffff, spmi + SPMI_IRQ_ACK(i));
			writel_relaxed(0xffffffff, spmi1 + SPMI_IRQ_ACK(i));
		}
	}
}

/*
 * diag=7: what could hold nub-spmi's line (wake source 54) up at the final WFI. Read right
 * after the acknowledge of step 9b; the enable and pending words of both AP ports, never
 * their reply or command words. Bytes: type; nub-spmi words 0..7 with a bit both enabled
 * and pending; bit 0 the same for nub-spmi word 8, bit 4 for nub-spmi1 word 8; nub-spmi
 * enable word 0 and pending word 0 (low bytes); nub-spmi enable word 6 and pending word 6
 * (low bytes); nub-spmi1 words 0..7 both enabled and pending; nub-spmi words 0..7 with any
 * enable bit; nub-spmi1 enable word 6 and pending word 6 (low bytes); nub-spmi words 0..7
 * with any pending bit. The PMU writes come after the acknowledge, so that is repeated.
 */
static void s2r_diag_spmi(void)
{
	u8 d[12] = { DIAG_REC_SPMI };
	u32 en, pe;
	int i;

	for (i = 0; i < SPMI_IRQ_WORDS; i++) {
		en = readl_relaxed(spmi + SPMI_IRQ_EN(i));
		pe = readl_relaxed(spmi + SPMI_IRQ_ACK(i));
		if (i < 8) {
			d[1] |= (en & pe) ? BIT(i) : 0;
			d[8] |= en ? BIT(i) : 0;
			d[11] |= pe ? BIT(i) : 0;
		} else {
			d[2] |= (en & pe) ? BIT(0) : 0;
		}
		if (i == 0) {
			d[3] = en;
			d[4] = pe;
		} else if (i == 6) {
			d[5] = en;
			d[6] = pe;
		}
		en = readl_relaxed(spmi1 + SPMI_IRQ_EN(i));
		pe = readl_relaxed(spmi1 + SPMI_IRQ_ACK(i));
		if (i < 8)
			d[7] |= (en & pe) ? BIT(i) : 0;
		else
			d[2] |= (en & pe) ? BIT(4) : 0;
		if (i == 6) {
			d[9] = en;
			d[10] = pe;
		}
	}
	s2r_diag_put(d);
	s2r_spmi_ack();
	dsb(sy);
}

/*
 * diag=6: have the other cores gone down? Called after the release of the held secondaries
 * and cores_wait_ms later, with this core still running: both cluster power words whole
 * (bit 6: every core of the cluster in deep idle; the E cluster's cannot be set while this
 * core runs) and the power-status byte of cpu1..3, whose registers stay readable while
 * their cluster is up (0xff: not mapped). Bytes: type, E word (4), P word (4), cpu1..3.
 * The PMU writes come after the last acknowledge of step 9b, so that is repeated.
 */
#define T8103_ECORE_IMPL(n)		(0x210050000ULL + (n) * 0x100000ULL)
#define ECORE_IMPL_PWRSTAT		0x100

static void s2r_diag_cores(void)
{
	u8 d[12] = { DIAG_REC_CORES };
	int i;

	mdelay(min(cores_wait_ms, 1000U));
	put_unaligned_le32(readl_relaxed(pmgr + PMGR_CLUSTER_PS(0)), d + 1);
	put_unaligned_le32(readl_relaxed(pmgr + PMGR_CLUSTER_PS(1)), d + 5);
	for (i = 0; i < 3; i++)
		d[9 + i] = ecore_impl[i] ? readq_relaxed(ecore_impl[i] + ECORE_IMPL_PWRSTAT) : 0xff;
	s2r_diag_put(d);
	s2r_spmi_ack();
	dsb(sy);
}

/*
 * wake_timer_ms: this core's own timer is set to fire that long after it enters its final
 * WFI. A core that was really power-gated does not see it; if the WFI returns, the cluster
 * was never taken down, and this record says what the engine was doing then. Bytes: type,
 * E and P cluster power words bits 7..0, second code, WD2 control, progress word (4),
 * WD2 count bits 31..8 (3).
 */
static void s2r_arm_wake_timer(void)
{
	u64 ticks = (u64)wake_timer_ms * arch_timer_get_cntfrq();

	do_div(ticks, 1000);
	write_sysreg(read_sysreg(cntvct_el0) + ticks, cntv_cval_el0);
	write_sysreg(ARCH_TIMER_CTRL_ENABLE, cntv_ctl_el0);
	isb();
}

static void s2r_diag_wfi_return(void)
{
	u32 cnt = readl_relaxed(aon_wdt + WDT_WD2_CNT);
	u8 d[12] = { DIAG_REC_WFI_RETURN };

	write_sysreg(ARCH_TIMER_CTRL_IT_MASK, cntv_ctl_el0);	/* off, and its line down */
	isb();
	d[1] = readl_relaxed(pmgr + PMGR_CLUSTER_PS(0));
	d[2] = readl_relaxed(pmgr + PMGR_CLUSTER_PS(1));
	d[3] = readl_relaxed(aon_wdt + AON_ENGINE_CODE2);
	d[4] = readl_relaxed(aon_wdt + WDT_WD2_CTRL);
	put_unaligned_le32(readl_relaxed(aon_wdt + AON_ENGINE_PROGRESS), d + 5);
	d[9] = cnt >> 8;
	d[10] = cnt >> 16;
	d[11] = cnt >> 24;
	s2r_diag_put(d);
}

/*
 * aop_sleep: the RTKit management message "AP power state = 0x201" to the AOP, raw on its
 * mailbox (interrupts are off and its driver is suspended). Returns bit 0 = acknowledged,
 * bit 1 = the send FIFO was full (nothing sent), bits 2..3 = other messages taken (max 3).
 */
static u8 s2r_aop_sleep(void)
{
	unsigned int other = 0;
	int i;

	if (!aop_mbox)
		return 0x02;
	for (i = 0; i < 64 && !(readl_relaxed(aop_mbox + ASC_I2A_CONTROL) & ASC_CONTROL_EMPTY); i++) {
		readq_relaxed(aop_mbox + ASC_I2A_RECV0);
		readq_relaxed(aop_mbox + ASC_I2A_RECV1);
		other++;
	}
	if (readl_relaxed(aop_mbox + ASC_A2I_CONTROL) & ASC_CONTROL_FULL)
		return 0x02 | (min(other, 3U) << 2);
	writeq_relaxed(FIELD_PREP(RTKIT_MGMT_TYPE, RTKIT_MGMT_SET_AP_PWR_STATE) | RTKIT_PWR_STATE_IDLE,
		       aop_mbox + ASC_A2I_SEND0);
	writeq_relaxed(0, aop_mbox + ASC_A2I_SEND1);	/* endpoint 0: management */
	for (i = 0; i < 20000; i++) {			/* up to about 200 ms */
		u64 m0, m1;

		if (readl_relaxed(aop_mbox + ASC_I2A_CONTROL) & ASC_CONTROL_EMPTY) {
			udelay(10);
			continue;
		}
		m0 = readq_relaxed(aop_mbox + ASC_I2A_RECV0);
		m1 = readq_relaxed(aop_mbox + ASC_I2A_RECV1);
		if (!(m1 & 0xff) && FIELD_GET(RTKIT_MGMT_TYPE, m0) == RTKIT_MGMT_SET_AP_PWR_STATE)
			return 0x01 | (min(other, 3U) << 2);
		other++;
	}
	return min(other, 3U) << 2;
}

/* ---- the entry ---- */

static void s2r_clean(const void *start, size_t len)
{
	dcache_clean_inval_poc((unsigned long)start, (unsigned long)start + len);
}

/*
 * Hand-back: what a return through cpu_resume reads must be in DRAM when the SoC powers
 * off, not only in the boot CPU's L1 or its cluster's L2 (with mcc_flush, step 3b has
 * already turned the system-level cache off, so a clean to the PoC goes to DRAM). By VA to
 * the PoC, which HID4/EHID4 DISABLE_DC_SW_L2_OPS does not affect (smp_apple.c relies on the
 * same for apple_cpu_s2r_release): the kernel image's text (runtime patches), data, bss
 * and ro_after_init (read by __finalise_el2 with the EL2 MMU off), the idmap text, the
 * root tables, this CPU's static per-CPU area, and the task that suspends: its stack (the
 * saved context lives there), task_struct (ptrauth keys), mm and shadow call stack, plus
 * sleep_save_stash. Called once, after the last write any of these get before the WFI.
 */
static void s2r_handback_clean(void)
{
	unsigned long pcpu = (unsigned long)__per_cpu_start +
			     per_cpu_offset(raw_smp_processor_id());

	s2r_clean(_stext, _etext - _stext);
	s2r_clean(_sdata, _edata - _sdata);
	s2r_clean(__bss_start, __bss_stop - __bss_start);
	s2r_clean(__start_ro_after_init, __end_ro_after_init - __start_ro_after_init);
	s2r_clean(__idmap_text_start, __idmap_text_end - __idmap_text_start);
	s2r_clean(idmap_pg_dir, PAGE_SIZE);
	s2r_clean(swapper_pg_dir, PAGE_SIZE);
	s2r_clean((void *)pcpu, __per_cpu_end - __per_cpu_start);
	s2r_clean(task_stack_page(current), THREAD_SIZE);
	s2r_clean(current, sizeof(*current));
	if (current->active_mm)
		s2r_clean(current->active_mm, sizeof(*current->active_mm));
#ifdef CONFIG_SHADOW_CALL_STACK
	if (task_scs(current))
		s2r_clean(task_scs(current), SCS_SIZE);
#endif
	s2r_clean(sleep_save_stash, mpidr_hash_size() * sizeof(*sleep_save_stash));
	dsb(sy);
}

/*
 * The uncleaned page's seed carries the commit second: after the reset that ends a run the
 * same page can be handed out again with the last run's pattern still in DRAM, and a lost
 * cache line would then read as intact.
 */
static u64 s2r_hb_dirty_seed(void)
{
	return S2R_HB_SEED_DIRTY ^ ((u64)s2r_hb_commit_rtc << 32 | s2r_hb_commit_rtc);
}

static void s2r_hb_fill(u64 *p, u64 seed)
{
	int i;

	for (i = 0; i < S2R_HB_WORDS; i++)
		p[i] = seed ^ ((u64)i * S2R_HB_STEP);
}

/*
 * Hand-back: the 'W' note in place of 'L', 0x9f05..0x9f09 = the entry's PA >> 2, all six
 * bytes in one SPMI write, so the 'L' note's RTC bytes are not in the PMU on this path
 * (handback=2 keeps the commit second in RAM and puts it in its record). Returns non-zero
 * (the caller then writes 'L' as before) if the entry is outside what m1n1 accepts or the
 * write fails.
 */
static int s2r_handback_publish(const u8 *rtc)
{
	phys_addr_t pa;
	u64 v;
	u8 note[6] = { 'W' };
	int i;

	s2r_hb_commit_rtc = rtc[2] | rtc[3] << 8 | rtc[4] << 16 | (u32)rtc[5] << 24;
	s2r_hb_fill(apple_s2r_hb_pattern, S2R_HB_SEED);
	if (s2r_hb_mode == 1) {
		pa = __pa_symbol(apple_s2r_hb_stub);
	} else {
		s2r_hb_fill(s2r_hb_dirty, s2r_hb_dirty_seed());
		pa = __pa_symbol(apple_s2r_hb_resume);
	}
	if (pa < S2R_HB_PA_MIN || pa >= S2R_HB_PA_MAX || (pa & 3))
		return -ERANGE;
	v = pa >> 2;
	for (i = 0; i < 5; i++)
		note[1 + i] = v >> (8 * i);
	return pmu_write(PMU_SCR_NOTE, note, sizeof(note));
}

static void s2r_deep_wfi(void)
{
	u64 v;

	/*
	 * Hand-back: the boot CPU's L1 by set/way, as the secondaries do before their stop
	 * (smp_apple_asm.S). It reaches L1 only; the cluster L2 is left to the deep sleep's L2
	 * flush (ACC_OVRD [16:15] = 2 below). Every other dirty line depends on that pair.
	 */
	if (s2r_hb_mode)
		apple_dcache_cisw_levels(1);

	v = read_sysreg_s(SYS_APL_ACC_OVRD);
	write_sysreg_s((v & ~ACC_OVRD_S2R_CLR) | ACC_OVRD_S2R_SET, SYS_APL_ACC_OVRD);
	isb();
	v = read_sysreg_s(SYS_APL_CYC_OVRD);
	write_sysreg_s((v & ~CYC_OVRD_MODES) | CYC_OVRD_S2R, SYS_APL_CYC_OVRD);
	isb();
	if (wake_timer_ms)
		s2r_arm_wake_timer();
	for (bool rearmed = false;;) {
		dsb(sy);
		isb();
		wfi();
		if (!rearmed && wake_timer_ms) {
			s2r_diag_wfi_return();
			if (!wfi_return_secs)
				rearmed = true;
		}
		/*
		 * A wfi that returns means this core was not power-gated (the SoC did not
		 * take it down). Mark it once with its own WD1 time, so the reboot time
		 * tells this case apart from every stage of the wake path.
		 */
		if (!rearmed && wfi_return_secs) {
			writel_relaxed(0, aon_wdt + WDT_WD1_CTRL);
			writel_relaxed(0, aon_wdt + WDT_WD1_CNT);
			writel_relaxed(min(wfi_return_secs, 178U) * 24000000U, aon_wdt + WDT_WD1_ALARM);
			writel_relaxed(WDT_CTRL_RESET, aon_wdt + WDT_WD1_CTRL);
			rearmed = true;
		}
	}
}

static void s2r_arm_deadman(void);

static int apple_s2r_finisher(unsigned long arg)
{
	u8 rtc[6], b;
	u32 now, alarm, v, nr_irq;
	int i;

	s2r_mark('a');

	s2r_aop_result = 0xff;
	if (aop_sleep) {
		s2r_aop_result = s2r_aop_sleep();
		if (!diag) {
			u8 d[12] = { DIAG_REC_AOP | s2r_aop_result };

			s2r_diag_put(d);
		}
	}

	/*
	 * 1. AON AP wake-source masks (1 = masked). macos_wake_mask: exactly the words of
	 * the macOS-under-m1n1 run that woke (try62): only sources 26/27 (AOP outbox / AOP
	 * CPUWakeupAP), 38 (masked by macOS itself) and 81 (SMC outbox) masked, all the rest
	 * open. Otherwise: only the PMU (RTC alarm, source 54) may wake the AP.
	 */
	for (i = 0; i < AON_WAKE_WORDS; i++) {
		static const u32 macos_mask[] = { 0x0c000000, 0x00000040, 0x00020000, 0, 0 };

		if (wake_mask_n == AON_WAKE_WORDS) {
			v = wake_mask[i];
		} else if (macos_wake_mask && i < ARRAY_SIZE(macos_mask)) {
			v = macos_mask[i];
		} else {
			v = 0xffffffff;
			if (i == WAKE_SRC_PMU / 32)
				v &= ~BIT(WAKE_SRC_PMU % 32);
		}
		writel_relaxed(v, aon_wake + AON_WAKE_MASK(i));
	}

	/*
	 * 2. AON watchdogs off as macOS does (WD0 CTRL, the 0x8008/0x800c words), except WD1:
	 * the wake has no resume yet and lands in m1n1 without iBoot state, which can hang;
	 * WD1 armed for deadman_secs (longer than the alarm) resets the SoC in that case.
	 */
	writel_relaxed(0, aon_wdt + WDT_EXTRA_8008);
	writel_relaxed(0, aon_wdt + WDT_EXTRA_800C);
	writel_relaxed(0, aon_wdt + WDT_WD0_CTRL);
	writel_relaxed(0, aon_wdt + WDT_WD1_CTRL);
	s2r_arm_deadman();	/* restart the dead-man's count at the commit */

	/* 3. boot_stage: "asleep", so a reset during sleep is not a boot failure */
	b = PMU_BOOT_STAGE_SLEEP;
	pmu_write(PMU_BOOT_STAGE, &b, 1);

	/*
	 * 3b. system-level cache off, as macOS's MCC quiesce does (M1-M4): broadcast on, all
	 * ways off, broadcast off, then wait until every plane's tag and data flush is done.
	 * Dirty SLC lines reach DRAM before the SoC goes down.
	 */
	s2r_handshake_bits &= ~(BIT(4) | BIT(6));
	if (mcc_flush) {
		bool flushed = true;

		writel_relaxed(1, mcc_glob + MCC_BCAST_A);
		writel_relaxed(1, mcc_glob + MCC_BCAST_B);
		readl_relaxed(mcc_glob + MCC_BCAST_B);
		dsb(sy);
		writel_relaxed(0, mcc_planes + MCC_PLANE_CACHE_ENABLE);
		readl_relaxed(mcc_planes + MCC_PLANE_CACHE_ENABLE);
		dsb(sy);
		writel_relaxed(0, mcc_glob + MCC_BCAST_A);
		writel_relaxed(0, mcc_glob + MCC_BCAST_B);
		dsb(sy);
		for (i = 0; i < MCC_PLANES; i++) {
			int t;

			for (t = 0; t < 1000000; t++)
				if (!(readl_relaxed(mcc_planes + MCC_PLANE_CACHE_STATUS(i)) & MCC_STATUS_BUSY))
					break;
			if (t == 1000000)
				flushed = false;
		}
		if (flushed)
			s2r_handshake_bits |= BIT(4);
	}
	s2r_mark('b');

	/* 4. park the clkgen muxes (P1-P5) */
	for (i = 0; i < ARRAY_SIZE(clkgen_muxes); i++) {
		void __iomem *r = clkgen + clkgen_muxes[i];

		v = readl_relaxed(r);
		writel_relaxed((v & ~MUX_SEL) | MUX_ENABLE, r);
		readl_poll_timeout_atomic(r, v, !(v & MUX_BUSY), 1, 8000);
	}
	s2r_mark('c');

	/* 5. counter blocks (P6/P7), CPM gating (P8), cluster PS (P9/P10), SIO (P11-P13) */
	for (i = 0; i < ARRAY_SIZE(pmgr_counter_blocks); i++)
		writel_relaxed(0, pmgr + pmgr_counter_blocks[i]);
	for (i = 0; i < ARRAY_SIZE(pmgr_cpm_gating); i++)
		writel_relaxed(PMGR_CPM_GATING_ON, pmgr + pmgr_cpm_gating[i]);
	for (i = 0; i < 2; i++) {
		v = readl_relaxed(pmgr + PMGR_CLUSTER_PS(i));
		writel_relaxed(v & ~PMGR_CLUSTER_PS_MASK, pmgr + PMGR_CLUSTER_PS(i));
	}
	for (i = 0; i < 3; i++) {
		static const u32 ps[] = { PMGR_PS_SIO, PMGR_PS_SIO_BUSIF, PMGR_PS_SIO_CPU };

		v = readl_relaxed(pmgr + ps[i]);
		writel_relaxed(v & ~PMGR_PS_AUTO_ENABLE, pmgr + ps[i]);
	}
	if (macos_ps) {
		static const u32 ps[] = { PMGR_PS_GFX, PMGR_PS_ATC0_COMMON };

		/* still under mark 'c': a fault here leaves 'c' as the last stage letter */
		for (i = 0; i < ARRAY_SIZE(ps); i++) {
			v = readl_relaxed(pmgr + ps[i]);
			writel_relaxed(v & ~(PMGR_PS_AUTO_ENABLE | PMGR_PS_TARGET), pmgr + ps[i]);
		}
		s2r_handshake_bits |= BIT(6);
	}
	s2r_mark('d');

	/*
	 * 5b. SPMI: only the PMU alarm (peripheral IRQ 2) may raise nub-spmi's line, which is
	 * wake source 54; a vector left pending would wake the AP at once. nub-spmi1 fully off.
	 */
	if (spmi_final == 1) {
		/*
		 * As the macOS traces have it at the last commit write: around its polled
		 * PMU commands macOS takes nub-spmi's enable word 6 to 0 and leaves it there;
		 * enable word 0 is the alarm bit; nub-spmi1's enable word 6 stays 1. Linux has
		 * no handler that acks the alarm vector as it arrives, so that bit is acked here
		 * and once more in step 8, each time before it can stand for the new alarm. Acks
		 * first, enables after: a stale vector must not raise the line even briefly.
		 */
		writel_relaxed(BIT(SPMI_IRQ_PMU_ALARM), spmi + SPMI_IRQ_ACK(0));
		s2r_spmi_ack();
		writel_relaxed(BIT(SPMI_IRQ_PMU_ALARM), spmi + SPMI_IRQ_EN(0));
		writel_relaxed(0, spmi + SPMI_IRQ_EN(6));
		writel_relaxed(1, spmi1 + SPMI_IRQ_EN(6));
	} else if (spmi_macos && !spmi_final) {
		/*
		 * As macOS leaves it at the commit (try55 trace): only IRQ word 6 is written,
		 * on both nub buses, enable = 1 and its bit 0 acked; the other words untouched.
		 */
		writel_relaxed(1, spmi + SPMI_IRQ_EN(6));
		writel_relaxed(1, spmi1 + SPMI_IRQ_EN(6));
		writel_relaxed(1, spmi + SPMI_IRQ_ACK(6));
		writel_relaxed(1, spmi1 + SPMI_IRQ_ACK(6));
	} else {
		for (i = 0; i < SPMI_IRQ_WORDS; i++) {
			writel_relaxed(i == 0 ? BIT(SPMI_IRQ_PMU_ALARM) : 0, spmi + SPMI_IRQ_EN(i));
			/* spmi_final=2: this setup with nub-spmi1's enable word 6 as macOS has it */
			writel_relaxed(spmi_final == 2 && i == 6 ? 1 : 0, spmi1 + SPMI_IRQ_EN(i));
			writel_relaxed(0xffffffff, spmi + SPMI_IRQ_ACK(i));
			writel_relaxed(0xffffffff, spmi1 + SPMI_IRQ_ACK(i));
		}
	}

	/* 5c. the commit's RTC second, so the next boot can tell a real sleep from a WD2 reset */
	/* results byte; bit 6 tells the stage-2 m1n1 it may run its retention probe */
	b = s2r_handshake_bits | BIT(7) | (retention_pa ? BIT(6) : 0);
	pmu_write(PMU_SCR_WAKE_MARK + 1, &b, 1);
	if (!pmu_read(PMU_RTC_NOW, rtc, sizeof(rtc))) {
		u8 note[5] = { 'L', rtc[2], rtc[3], rtc[4], rtc[5] };

		if (!s2r_hb_mode || s2r_handback_publish(rtc))
			pmu_write(PMU_SCR_NOTE, note, sizeof(note));
	}
	/* 5d. hand-back: nothing a resume reads is written after this */
	if (s2r_hb_mode)
		s2r_handback_clean();

	poke_applied = 0;
	for (i = 0; !poke_refused && i + 2 < poke_n; i += 3) {
		void __iomem *r = poke_map[i / 3];

		if (!r)
			continue;
		v = readl_relaxed(r);
		poke_before[i / 3] = v;
		writel_relaxed((v & ~(u32)poke[i + 1]) | (u32)poke[i + 2], r);
		poke_applied++;
	}
	if (poke_applied && poke_settle_ms)
		mdelay(min(poke_settle_ms, 1000U));
	if (diag == 1)
		s2r_diag_status(DIAG_REC_STATUS);
	else if (diag == 2)
		s2r_diag_engine_before();
	else if (diag == 4)
		s2r_diag_gates();
	else if (diag == 5)
		s2r_diag_pokes();

	/* 6. the commit */
	writel_relaxed(1, s2r_timer);
	dsb(sy);
	s2r_mark('e');

	/* 7. mask every AIC interrupt */
	nr_irq = FIELD_GET(AIC_INFO_NR_IRQ, readl_relaxed(aic + AIC_INFO));
	for (i = 0; i < DIV_ROUND_UP(nr_irq, 32); i++)
		writel_relaxed(0xffffffff, aic + AIC_MASK_SET(i));

	/* 8. the PMU RTC alarm */
	if (!pmu_read(PMU_RTC_EVENT, &b, 1) && b)
		pmu_write(PMU_RTC_EVENT, &b, 1);
	/*
	 * spmi_final=1: the alarm bit is acked once more here, before the new alarm is armed,
	 * for a vector that may have latched in the controller since 5b. (The PMU's own event
	 * of an earlier alarm is not expected here: it reads clear before every suspend.)
	 */
	if (spmi_final == 1)
		writel_relaxed(BIT(SPMI_IRQ_PMU_ALARM), spmi + SPMI_IRQ_ACK(0));
	if (alarm_secs && !pmu_read(PMU_RTC_NOW, rtc, sizeof(rtc))) {
		now = rtc[2] | (rtc[3] << 8) | (rtc[4] << 16) | ((u32)rtc[5] << 24);
		alarm = now + alarm_secs;
		pmu_write(PMU_RTC_ALARM, (u8 *)&alarm, 4);
		b = 0;
		pmu_write(PMU_RTC_IRQ_MASK, &b, 1);
		b = PMU_RTC_CTRL_ALARM_ON;
		pmu_write(PMU_RTC_CTRL, &b, 1);
	}
	s2r_mark('f');

	/* 9. the commit, twice more */
	writel_relaxed(1, s2r_timer);
	writel_relaxed(1, s2r_timer);
	dsb(sy);
	s2r_mark('g');
	if (diag == 2)
		s2r_diag_postcommit();
	/*
	 * 'h': global sleep armed, release next. Marked here, before 9b's ack: every SPMI
	 * command leaves a vector that must be acked before the deep WFI.
	 */
	if (s2r_hold_armed)
		s2r_mark('h');
	/* before 9b, whose ack also covers the vectors these PMU writes leave */
	if (diag == 3)
		s2r_diag_status(DIAG_REC_STATUS_LATE);

	/* 9b. ack whatever our own PMU commands left pending; report both clusters idle */
	s2r_spmi_ack();
	writel_relaxed(0, pmgr + PMGR_CLUSTER_STATE(0));
	writel_relaxed(0, pmgr + PMGR_CLUSTER_STATE(1));
	dsb(sy);
	if (diag == 7)
		s2r_diag_spmi();

	/*
	 * 9c. global sleep: release the secondaries held in WFE since their stop request;
	 * each arms deep sleep and enters the deep WFI now, together with this core.
	 */
	if (s2r_hold_armed)
		apple_cpu_s2r_release_all();
	if (diag == 6)
		s2r_diag_cores();

	/* 10. down */
	s2r_deep_wfi();
	return -EIO; /* not reached */
}

static int apple_s2r_valid(suspend_state_t state)
{
	return state == PM_SUSPEND_MEM;
}

static u64 acc_ovrd_saved;

static void s2r_save_acc(void *unused)
{
	if (smp_processor_id() == 0)
		acc_ovrd_saved = read_sysreg_s(SYS_APL_ACC_OVRD);
}

static int apple_s2r_prepare(void)
{
	u8 rtc[6];
	int k;

	s2r_hb_mode = min(handback, 2U);	/* 3 is reserved: no carrying on yet, run as 2 */
	poke_refused = poke_n % 3 != 0;
	for (k = 0; k < S2R_POKES; k++) {
		if (poke_map[k]) {
			iounmap(poke_map[k]);
			poke_map[k] = NULL;
		}
		if (3 * k + 2 < poke_n && !s2r_poke_addr_ok(poke[3 * k]))
			poke_refused = true;
	}
	for (k = 0; !poke_refused && 3 * k + 2 < poke_n; k++)
		poke_map[k] = ioremap_np(poke[3 * k], 4);
	if (poke_refused)
		pr_warn("poke list refused: an address outside the PMGR power-state words, or a bad length\n");
	if (sec_trap && !s2r_trap_page) {
		int ret = s2r_trap_arm();

		if (ret)
			return ret;
	}
	if (s2r_hb_mode >= 2 && !s2r_hb_dirty) {
		s2r_hb_dirty = (u64 *)get_zeroed_page(GFP_KERNEL);	/* kept for later entries */
		if (!s2r_hb_dirty)
			return -ENOMEM;
	}
	s2r_mark('P');	/* devices suspended (normal phase), all CPUs online */
	s2r_write_retention_pattern();
	/* clear the wake-path breadcrumbs the ESP stage-2 m1n1 leaves at 0x9f0b..0x9f0c */
	{
		static const u8 zero[2];

		pmu_write(PMU_SCR_WAKE_MARK, zero, sizeof(zero));
		/*
		 * Hand-back: also the header of m1n1's entry record (0x9f10, bit 7 = written in
		 * this run), so one left by an earlier run cannot pass for this one's.
		 */
		if (s2r_hb_mode)
			pmu_write(PMU_SCR_DIAG_A, zero, 1);
	}
	on_each_cpu(s2r_save_acc, NULL, 1);
	/* 'P' + the RTC second here; the commit overwrites it with 'L' + its second */
	if (!pmu_read(PMU_RTC_NOW, rtc, sizeof(rtc))) {
		u8 note[5] = { 'P', rtc[2], rtc[3], rtc[4], rtc[5] };

		pmu_write(PMU_SCR_NOTE, note, sizeof(note));
	}
	/*
	 * apple_wdt's suspend has stopped WD1 by now: arm it as the dead-man, so a hang
	 * anywhere from here (late/noirq suspend, CPU offline, the commit) resets the SoC.
	 * .finish runs on every exit after .prepare and gives WD1 back.
	 */
	{
		int (*fn)(bool) = READ_ONCE(s2r_pmp_power);

		s2r_pmp_idled = false;
		s2r_handshake_bits &= ~BIT(5);
		if (fn) {
			int r = fn(true);

			pr_info("PMP idle (AP 0x201, IOP 0x201): %d\n", r);
			if (!r) {
				s2r_pmp_idled = true;
				s2r_handshake_bits |= BIT(5);
			}
		}
	}
	s2r_arm_deadman();
	pr_info("entering: cluster state E %#x P %#x, nub-spmi IRQ enable %#x pending %#x, alarm %u s, dead-man %u s\n",
		readl_relaxed(pmgr + PMGR_CLUSTER_STATE(0)), readl_relaxed(pmgr + PMGR_CLUSTER_STATE(1)),
		readl_relaxed(spmi + SPMI_IRQ_EN(0)), readl_relaxed(spmi + SPMI_IRQ_ACK(0)),
		alarm_secs, deadman_secs);
	if (s2r_hb_mode)
		pr_info("hand-back mode %u%s\n", s2r_hb_mode, handback > 2 ? " (3 runs as 2)" : "");
	/* latched: the finisher releases only what cpu_die actually held */
	s2r_hold_armed = stop_cores && global_sleep;
	if (stop_cores) {
		apple_cpu_hotplug_set_s2r_hold(s2r_hold_armed);
		apple_cpu_hotplug_set_s2r(true);
	}
	return 0;
}

static u32 wd1_bite_saved;
static bool wd1_saved;

/* WD1 is also apple_wdt's timer: keep its BITE_TIME to give it back in .finish */
static void s2r_arm_deadman(void)
{
	if (!deadman_secs)
		return;
	if (!wd1_saved) {
		wd1_bite_saved = readl_relaxed(aon_wdt + WDT_WD1_ALARM);
		wd1_saved = true;
	}
	writel_relaxed(0, aon_wdt + WDT_WD1_CTRL);
	writel_relaxed(0, aon_wdt + WDT_WD1_CNT);
	writel_relaxed(min(deadman_secs, 178U) * 24000000U, aon_wdt + WDT_WD1_ALARM);
	writel_relaxed(WDT_CTRL_RESET, aon_wdt + WDT_WD1_CTRL);
}

static int apple_s2r_prepare_late(void)
{
	s2r_mark('N');	/* devices suspended (noirq), secondaries still online */
	if (s2r_pcie_ports && !diag)
		pmu_write(PMU_SCR_PCIE, &s2r_pcie_ports, 1);
	return 0;
}

static int apple_s2r_begin(suspend_state_t state)
{
	s2r_pcie_ports = 0;	/* set again by pcie-apple's noirq suspend if its switch is on */
	/* the S2R stop's cluster logic assumes every secondary takes part */
	if (!cpumask_equal(cpu_online_mask, cpu_present_mask)) {
		pr_warn("not every CPU is online: refusing \"mem\"\n");
		return -EBUSY;
	}
	return 0;
}

/* ---- handback=2: back from cpu_resume ---- */

/* a checksum of the kernel text: clean in the caches, so it tests DRAM retention alone */
static u64 s2r_text_sum(void)
{
	const u64 *p = (const u64 *)_stext, *end = (const u64 *)_etext;
	u64 sum = 0;

	for (; p < end; p++)
		sum = rol64(sum, 1) ^ *p;
	return sum;
}

/* bytes of a pattern page that differ from what was written */
static unsigned int s2r_hb_damage(const u64 *p, u64 seed)
{
	unsigned int n = 0;
	int i, j;

	for (i = 0; i < S2R_HB_WORDS; i++) {
		u64 x = READ_ONCE(p[i]) ^ (seed ^ ((u64)i * S2R_HB_STEP));

		for (j = 0; j < 8; j++)
			n += !!(x & (0xffULL << (8 * j)));
	}
	return n;
}

static void s2r_wd1_set(unsigned int secs)
{
	writel_relaxed(0, aon_wdt + WDT_WD1_CTRL);
	writel_relaxed(0, aon_wdt + WDT_WD1_CNT);
	writel_relaxed(secs * S2R_HB_WDT_HZ, aon_wdt + WDT_WD1_ALARM);
	writel_relaxed(WDT_CTRL_RESET, aon_wdt + WDT_WD1_CTRL);
}

/* seconds down: 0..127 as is, then 0x80 | s / 32 up to 0xff (68 min); 0x80 = no RTC */
static u8 s2r_down_code(u32 secs)
{
	if (secs < 0x80)
		return secs;
	return 0x80 | min(secs / 32, 0x7fU);
}

/*
 * cpu_suspend() returned 0: m1n1 took the 'W' note to apple_s2r_hb_resume and cpu_resume
 * brought this CPU back with the saved context. IRQs are off, the secondaries are still
 * down, and the SoC is as the finisher left it (AIC masked, clocks parked, SLC off):
 * nothing is restored. Only the polled SPMI and the watchdog are used, no timer, no
 * printing, nothing that sleeps. WD1 is armed on every path from here and the last one
 * resets the SoC.
 *
 * 0x9f0a: 'Q' on arrival, then 'Z' (all intact), 'Y' (only the uncleaned page damaged)
 * or 'D' (the cleaned page or the text damaged). The note at 0x9f04..0x9f09 becomes
 * 'K' (text checksum matches) or 'k', the commit second's low 16 bits as kept in RAM, the
 * time down (s2r_down_code()), and the differing bytes of the cleaned and the uncleaned
 * page, each saturating at 255. Nothing here depends on how long the SoC was down.
 */
static void __noreturn s2r_handback_wake(void)
{
	unsigned int bad_clean, bad_dirty;
	u8 rec[6], rtc[6], down = 0x80, last;
	bool text_ok;

	s2r_woken = true;
	s2r_wd1_set(S2R_HB_WAKE_SECS);
	s2r_mark('Q');

	text_ok = s2r_text_sum() == s2r_hb_text_sum;
	bad_clean = s2r_hb_damage(apple_s2r_hb_pattern, S2R_HB_SEED);
	bad_dirty = s2r_hb_damage(s2r_hb_dirty, s2r_hb_dirty_seed());
	if (!pmu_read(PMU_RTC_NOW, rtc, sizeof(rtc))) {
		u32 now = rtc[2] | rtc[3] << 8 | rtc[4] << 16 | (u32)rtc[5] << 24;

		down = s2r_down_code(now - s2r_hb_commit_rtc);
	}

	rec[0] = text_ok ? 'K' : 'k';
	rec[1] = s2r_hb_commit_rtc;
	rec[2] = s2r_hb_commit_rtc >> 8;
	rec[3] = down;
	rec[4] = min(bad_clean, 255U);
	rec[5] = min(bad_dirty, 255U);
	pmu_write(PMU_SCR_NOTE, rec, sizeof(rec));

	if (!text_ok || bad_clean) {
		last = 'D';
		s2r_wd1_set(S2R_HB_BAD_SECS);
	} else if (bad_dirty) {
		last = 'Y';
		s2r_wd1_set(S2R_HB_DIRTY_SECS);
	} else {
		last = 'Z';
		s2r_wd1_set(S2R_HB_OK_SECS);
	}
	s2r_mark(last);
	for (;;)
		cpu_relax();
}

/*
 * cpu_suspend() returned 0, which only cpu_resume can do, but the mode word does not read
 * as handback=2: memory did not come back as it was. Going on from here would run the PM
 * core's device resume and .finish, which gives WD1 back to apple_wdt: a half-resumed
 * kernel with no watchdog. Stop instead, with a time and a mark of its own.
 */
static void __noreturn s2r_handback_lost(void)
{
	s2r_woken = true;
	s2r_wd1_set(S2R_HB_LOST_SECS);
	s2r_mark('M');
	for (;;)
		cpu_relax();
}

static int apple_s2r_enter(suspend_state_t state)
{
	int ret;

	if (state != PM_SUSPEND_MEM)
		return -EINVAL;
	s2r_mark('E');	/* secondaries offline, IRQs off, syscore suspended */
	if (s2r_hb_mode >= 2)
		s2r_hb_text_sum = s2r_text_sum();
	/*
	 * Without handback=2 the wake reboots: the finisher never returns, and the 'L' note
	 * makes m1n1 reset. 0 comes back only through cpu_resume (__cpu_suspend_enter()
	 * returns 1 on the way down, and a finisher's 0 becomes -EOPNOTSUPP).
	 */
	ret = cpu_suspend(0, apple_s2r_finisher);
	if (!ret) {
		if (s2r_hb_mode >= 2)
			s2r_handback_wake();
		s2r_handback_lost();
	}
	return ret;
}

static void apple_s2r_finish(void)
{
	s2r_trap_disarm();
	if (s2r_pmp_idled && READ_ONCE(s2r_pmp_power)) {
		s2r_pmp_idled = false;
		pr_info("PMP wake after an aborted entry: %d\n", s2r_pmp_power(false));
	}

	/* only reached when the suspend did not happen: undo the dead-man and the deep-sleep arming */
	if (wd1_saved) {
		writel_relaxed(0, aon_wdt + WDT_WD1_CTRL);
		writel_relaxed(wd1_bite_saved, aon_wdt + WDT_WD1_ALARM);
		wd1_saved = false;
	}
	if (acc_ovrd_saved)
		write_sysreg_s(acc_ovrd_saved, SYS_APL_ACC_OVRD);
	/* a held secondary stays held (marked stuck) after an abort: deep WFI under a running SoC hangs */
	apple_cpu_hotplug_set_s2r(false);
	apple_cpu_hotplug_set_s2r_hold(false);
}

static const struct platform_suspend_ops apple_s2r_ops = {
	.valid		= apple_s2r_valid,
	.begin		= apple_s2r_begin,
	.prepare	= apple_s2r_prepare,
	.prepare_late	= apple_s2r_prepare_late,
	.enter		= apple_s2r_enter,
	.finish		= apple_s2r_finish,
};

static int __init apple_s2r_init(void)
{
	u32 st[AON_WAKE_WORDS];
	int i;

	if (!apple_s2r_enabled)
		return 0;
	if (!of_machine_is_compatible("apple,t8103")) {
		pr_info("not a t8103, staying with s2idle\n");
		return 0;
	}
	if (!apple_cpu_hotplug_active()) {
		pr_warn("needs apple_cpu_hotplug=park (CPU offline); staying with s2idle\n");
		return 0;
	}

	clkgen = ioremap_np(T8103_CLKGEN, SZ_4K);
	pmgr = ioremap_np(T8103_PMGR, T8103_PMGR_SIZE);
	aon_wake = ioremap_np(T8103_AON_WAKE, SZ_4K);
	aon_wdt = ioremap_np(T8103_AON_WDT, SZ_64K);
	s2r_timer = ioremap_np(T8103_S2R_TIMER, SZ_4K);
	spmi = ioremap_np(T8103_NUB_SPMI, SZ_256);
	spmi1 = ioremap_np(T8103_NUB_SPMI1, SZ_256);
	aic = ioremap_np(T8103_AIC, SZ_64K);
	mcc_glob = ioremap_np(T8103_MCC_GLOBAL, SZ_4K);
	mcc_planes = ioremap_np(T8103_MCC_PLANES, MCC_PLANE_CACHE_STATUS(MCC_PLANES - 1) + SZ_4K);
	aop_mbox = ioremap_np(T8103_AOP_MBOX, SZ_4K);
	aon_hs = ioremap_np(T8103_AON_HANDSHAKE, SZ_4K);
	for (i = 0; i < 3; i++)
		ecore_impl[i] = ioremap_np(T8103_ECORE_IMPL(i + 1), SZ_4K);
	if (!clkgen || !pmgr || !aon_wake || !aon_wdt || !s2r_timer || !spmi || !spmi1 || !aic ||
	    !mcc_glob || !mcc_planes) {
		pr_err("ioremap failed\n");
		return -ENOMEM;
	}

	for (i = 0; i < AON_WAKE_WORDS; i++)
		st[i] = readl_relaxed(aon_wake + AON_WAKE_STATUS(i));
	pr_info("enabled: suspend-to-RAM for \"mem\" (the wake reboots); AP wake status at boot %08x %08x %08x %08x %08x\n",
		st[0], st[1], st[2], st[3], st[4]);
	suspend_set_ops(&apple_s2r_ops);
	return 0;
}
late_initcall(apple_s2r_init);
