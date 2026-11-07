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
 *
 * Every address here is t8103 data from those traces; nothing is taken
 * from the device tree, which maps none of these windows.
 */

#define pr_fmt(fmt) "apple-s2r: " fmt

#include <linux/bitfield.h>
#include <linux/bits.h>
#include <linux/delay.h>
#include <linux/init.h>
#include <linux/io.h>
#include <linux/iopoll.h>
#include <linux/kernel.h>
#include <linux/moduleparam.h>
#include <linux/of.h>
#include <linux/smp.h>
#include <linux/suspend.h>

#include <asm/cpu_ops.h>
#include <asm/cpuidle.h>
#include <asm/suspend.h>
#include <asm/sysreg.h>


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
#define PMU_SCR_NOTE			0x9f04	/* 'L' + commit RTC second (not m1n1's 'W') */
#define PMU_SCR_WAKE_MARK		0x9f0b	/* 0x9f0b..0x9f0c: the ESP stage-2 m1n1 wake breadcrumbs */

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
static bool stop_cores = true;
module_param(stop_cores, bool, 0644);
MODULE_PARM_DESC(stop_cores, "power the secondaries down for real (off: they park, for pm_test runs)");

static void __iomem *clkgen, *pmgr, *aon_wake, *aon_wdt, *s2r_timer, *spmi, *spmi1, *aic;
static void __iomem *mcc_glob, *mcc_planes;
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

static bool spmi_macos = true;
module_param(spmi_macos, bool, 0644);
MODULE_PARM_DESC(spmi_macos, "leave the nub-spmi IRQ words as macOS does at the commit (word 6 = 1 on both buses, others untouched); 0 = the old 'alarm only' setup");

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

static int __init apple_s2r_setup(char *arg)
{
	apple_s2r_enabled = !arg || !strcmp(arg, "1") || !strcmp(arg, "on");
	return 0;
}
early_param("apple_s2r", apple_s2r_setup);

/* ---- raw, polled SPMI (IRQs off, no locks, no printing) ---- */

static int s2r_spmi_cmd(u8 opc, u16 extra, const u8 *in, size_t len_in, u8 *out, size_t len_out)
{
	u32 st, reply;
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

	if (readl_poll_timeout_atomic(spmi + SPMI_STATUS, st, !(st & SPMI_STATUS_RX_EMPTY), 10, 10000))
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

/* ---- the entry ---- */

static void s2r_deep_wfi(void)
{
	u64 v;

	v = read_sysreg_s(SYS_APL_ACC_OVRD);
	write_sysreg_s((v & ~ACC_OVRD_S2R_CLR) | ACC_OVRD_S2R_SET, SYS_APL_ACC_OVRD);
	isb();
	v = read_sysreg_s(SYS_APL_CYC_OVRD);
	write_sysreg_s((v & ~CYC_OVRD_MODES) | CYC_OVRD_S2R, SYS_APL_CYC_OVRD);
	isb();
	for (bool rearmed = false;;) {
		dsb(sy);
		isb();
		wfi();
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

	/*
	 * 1. AON AP wake-source masks (1 = masked). macos_wake_mask: exactly the words of
	 * the macOS-under-m1n1 run that woke (try62): only sources 26/27 (AOP outbox / AOP
	 * CPUWakeupAP), 38 (masked by macOS itself) and 81 (SMC outbox) masked, all the rest
	 * open. Otherwise: only the PMU (RTC alarm, source 54) may wake the AP.
	 */
	for (i = 0; i < AON_WAKE_WORDS; i++) {
		static const u32 macos_mask[] = { 0x0c000000, 0x00000040, 0x00020000, 0, 0 };

		if (macos_wake_mask && i < ARRAY_SIZE(macos_mask)) {
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
	s2r_handshake_bits &= ~BIT(4);
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
	s2r_mark('d');

	/*
	 * 5b. SPMI: only the PMU alarm (peripheral IRQ 2) may raise nub-spmi's line, which is
	 * wake source 54; a vector left pending would wake the AP at once. nub-spmi1 fully off.
	 */
	if (spmi_macos) {
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
			writel_relaxed(0, spmi1 + SPMI_IRQ_EN(i));
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

		pmu_write(PMU_SCR_NOTE, note, sizeof(note));
	}

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

	/* 9b. ack whatever our own PMU commands left pending; report both clusters idle */
	if (spmi_macos) {
		writel_relaxed(1, spmi + SPMI_IRQ_ACK(6));
		writel_relaxed(1, spmi1 + SPMI_IRQ_ACK(6));
	} else {
		for (i = 0; i < SPMI_IRQ_WORDS; i++) {
			writel_relaxed(0xffffffff, spmi + SPMI_IRQ_ACK(i));
			writel_relaxed(0xffffffff, spmi1 + SPMI_IRQ_ACK(i));
		}
	}
	writel_relaxed(0, pmgr + PMGR_CLUSTER_STATE(0));
	writel_relaxed(0, pmgr + PMGR_CLUSTER_STATE(1));
	dsb(sy);

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

	s2r_mark('P');	/* devices suspended (normal phase), all CPUs online */
	s2r_write_retention_pattern();
	/* clear the wake-path breadcrumbs the ESP stage-2 m1n1 leaves at 0x9f0b..0x9f0c */
	{
		static const u8 zero[2];

		pmu_write(PMU_SCR_WAKE_MARK, zero, sizeof(zero));
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
	if (stop_cores)
		apple_cpu_hotplug_set_s2r(true);
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
	return 0;
}

static int apple_s2r_begin(suspend_state_t state)
{
	/* the S2R stop's cluster logic assumes every secondary takes part */
	if (!cpumask_equal(cpu_online_mask, cpu_present_mask)) {
		pr_warn("not every CPU is online: refusing \"mem\"\n");
		return -EBUSY;
	}
	return 0;
}

static int apple_s2r_enter(suspend_state_t state)
{
	if (state != PM_SUSPEND_MEM)
		return -EINVAL;
	s2r_mark('E');	/* secondaries offline, IRQs off, syscore suspended */
	/* the wake reboots for now: cpu_suspend() keeps the resume path in place for later */
	return cpu_suspend(0, apple_s2r_finisher);
}

static void apple_s2r_finish(void)
{
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
	apple_cpu_hotplug_set_s2r(false);
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
