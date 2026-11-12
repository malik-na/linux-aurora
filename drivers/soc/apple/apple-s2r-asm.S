/* SPDX-License-Identifier: GPL-2.0-only */
/*
 * apple-s2r hand-back entries: where m1n1's wake stage jumps on a suspend-to-RAM wake when
 * the 'W' note names one (see apple-s2r.h). They run at their physical address with the MMU
 * and caches off, at EL2: only PC-relative addressing (adr_l gives the PA), no stack, no
 * memory writes, nothing that needs the kernel's own setup. Plain .text is enough: nothing
 * here is ever run through the idmap, and no alternatives patch it.
 */

#include <linux/linkage.h>
#include <asm/assembler.h>

#include "apple-s2r.h"

/* re-arm watchdog 1 to bite \secs from now; clobbers x10, x11 */
	.macro	s2r_wd1_arm, secs
	mov_q	x10, S2R_HB_WDT
	str	wzr, [x10, #S2R_HB_WD1_CTRL]
	str	wzr, [x10, #S2R_HB_WD1_CNT]
	mov_q	x11, (\secs * S2R_HB_WDT_HZ)
	str	w11, [x10, #S2R_HB_WD1_ALARM]
	mov	w11, #S2R_HB_WD1_RESET
	str	w11, [x10, #S2R_HB_WD1_CTRL]
	dsb	sy
	.endm

	.text
/*
 * handback=1: "the jump lands". Compare the pattern page with what the kernel wrote before
 * the commit (read from DRAM: the MMU is off) and re-arm watchdog 1 with one of two times,
 * S2R_HB_STUB_OK_SECS if every word matches, S2R_HB_STUB_BAD_SECS if any differs. Then wait
 * for it to bite. Never returns.
 */
SYM_CODE_START(apple_s2r_hb_stub)
	adr_l	x1, apple_s2r_hb_pattern
	mov_q	x4, S2R_HB_SEED
	mov_q	x5, S2R_HB_STEP
	mov	x2, xzr				// word index
	mov	x3, xzr				// words that differ
1:	mul	x6, x2, x5
	eor	x6, x6, x4
	ldr	x7, [x1, x2, lsl #3]
	cmp	x6, x7
	cinc	x3, x3, ne
	add	x2, x2, #1
	cmp	x2, #S2R_HB_WORDS
	b.lo	1b

	cbnz	x3, 2f
	s2r_wd1_arm S2R_HB_STUB_OK_SECS
	b	3f
2:	s2r_wd1_arm S2R_HB_STUB_BAD_SECS
3:	b	3b
SYM_CODE_END(apple_s2r_hb_stub)

/*
 * handback=2: "cpu_resume works". Arm watchdog 1 for S2R_HB_RESUME_SECS, so that a hang
 * between here and the C code of the wake has its own reset time, and enter arm64's
 * cpu_resume, whose contract (EL2 or EL1, MMU off, any x0) m1n1's jump already meets.
 */
SYM_CODE_START(apple_s2r_hb_resume)
	s2r_wd1_arm S2R_HB_RESUME_SECS
	b	cpu_resume
SYM_CODE_END(apple_s2r_hb_resume)
