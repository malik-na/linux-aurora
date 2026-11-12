/* SPDX-License-Identifier: GPL-2.0-only */
/*
 * apple-s2r hand-back: what apple-s2r.c and its MMU-off entries (apple-s2r-asm.S) share.
 *
 * With handback set, the finisher writes the m1n1 wake note 'W' at PMU scratch 0x9f04 and
 * the physical address of an entry here, >> 2, little endian, at 0x9f05..0x9f09. On the
 * wake m1n1 jumps there at EL2, MMU and caches off, x0 = 0, with watchdog 1 armed.
 */
#ifndef __SOC_APPLE_S2R_H
#define __SOC_APPLE_S2R_H

#include <linux/const.h>

/* what m1n1's wake stage accepts as a resume entry (the t8103 DRAM it checks for) */
#define S2R_HB_PA_MIN		_AC(0x800000000, UL)
#define S2R_HB_PA_MAX		_AC(0xa00000000, UL)

/* AON watchdog 1, as T8103_AON_WDT and WDT_WD1_* in apple-s2r.c; 24 MHz */
#define S2R_HB_WDT		_AC(0x23d2b0000, UL)
#define S2R_HB_WD1_CNT		0x10
#define S2R_HB_WD1_ALARM	0x14
#define S2R_HB_WD1_CTRL		0x1c
#define S2R_HB_WD1_RESET	4
#define S2R_HB_WDT_HZ		24000000

/*
 * Watchdog 1 times, in seconds from the moment each is armed, so that the time from the
 * commit to the next boot tells the outcome without any console. m1n1's wake stage uses
 * 15, 30, 45, 65, 85 and 100, on 'W' 55 (its per-core setup) and 25 (left armed
 * across the jump, until the kernel re-arms it), 70 in its secondary trap, and 8 after an
 * 'L' note; the driver itself uses 30 (wfi_return_secs) and 170 (deadman_secs).
 *
 * What holds: the times below are 15 s or more from each other and from the stage's 'W'
 * times 25, 55 and 100, with one exception each way: 160 is 10 s from the default
 * dead-man (hand-back runs that can end in 'D' set deadman_secs=178), and 75 is 10 s or
 * less from the stage's 65, 70 and 85. 40 is 15 s from 25 and 35 s from 75, the classes
 * that leave the same PMU bytes if its mark is lost, but only 10 s from 30 (the stage's
 * and wfi_return_secs) and 5 s from the stage's 45, which leave different ones. Seen from
 * the next boot every time is later by the same 10 to 12 s (entry, stage and boot), and
 * the dead-man by less, as it is armed earlier.
 */
#define S2R_HB_STUB_OK_SECS	130	/* handback=1: the stub ran, pattern page intact */
#define S2R_HB_STUB_BAD_SECS	145	/* handback=1: the stub ran, pattern page damaged */
#define S2R_HB_RESUME_SECS	75	/* handback=2: armed before cpu_resume, C never reached */
#define S2R_HB_LOST_SECS	40	/* back in C, but the mode word no longer reads 2 ('M') */
#define S2R_HB_WAKE_SECS	115	/* handback=2: back in C, the checks never finished */
#define S2R_HB_OK_SECS		130	/* handback=2: done, everything intact ('Z') */
#define S2R_HB_DIRTY_SECS	145	/* handback=2: done, only the uncleaned page bad ('Y') */
#define S2R_HB_BAD_SECS		160	/* handback=2: done, cleaned page or text damaged ('D') */

/* the pattern pages: word i = seed ^ (i * step) */
#define S2R_HB_WORDS		512
#define S2R_HB_SEED		_AC(0x4d34636c65616e21, UL)
#define S2R_HB_SEED_DIRTY	_AC(0x4d34646972747921, UL)
#define S2R_HB_STEP		_AC(0x9e3779b97f4a7c15, UL)

#ifndef __ASSEMBLY__
#include <linux/types.h>

/* filled and cleaned to the PoC before the commit; checked with the MMU off by the stub */
extern u64 apple_s2r_hb_pattern[S2R_HB_WORDS];

void apple_s2r_hb_stub(void);
void apple_s2r_hb_resume(void);
#endif

#endif /* __SOC_APPLE_S2R_H */
