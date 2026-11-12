/* SPDX-License-Identifier: GPL-2.0-only OR MIT */
/*
 * The boot core's last instructions on the wake path of the Rust t8103
 * suspend-to-RAM driver (s2r_rs.rs, Hw::off_deep).
 *
 * Written by hand, with every register named: an earlier inline-asm version
 * passed its eight scratch registers as inputs, the compiler gave them all
 * the same register, and the set/way loop never ended (runs 8 to ALL).
 */

#include <linux/linkage.h>
#include <asm/assembler.h>
#include <asm/sysreg.h>

#define SYS_APL_CYC_OVRD	sys_reg(3, 5, 15, 5, 0)
#define SYS_APL_ACC_OVRD	sys_reg(3, 5, 15, 6, 0)

/*
 * int apple_s2r_rs_off_deep(u64 acc_ovrd, u64 cyc_ovrd)
 *
 * Interrupts masked, the data cache off, the L1 data cache cleaned and
 * invalidated by set and way (level 1 only: the L2 goes with the cluster),
 * then the two override registers and the wait. No stack and no memory
 * access once the cache is off. A wake through a power cut comes back
 * through the resume entry in the loader's mailbox, not here. If the wait
 * falls through (something stood pending), the data cache is turned on
 * again and -EBUSY returned; the caller records it and waits again.
 */
SYM_FUNC_START(apple_s2r_rs_off_deep)
	msr	daifset, #0xf

	mrs	x3, sctlr_el1
	bic	x3, x3, #SCTLR_ELx_C
	msr	sctlr_el1, x3
	isb

	msr	csselr_el1, xzr			// level 1, data
	isb
	mrs	x3, ccsidr_el1
	and	x4, x3, #7
	add	x4, x4, #4			// set shift: log2 of the line size
	ubfx	x5, x3, #3, #10			// highest way
	ubfx	x6, x3, #13, #15		// highest set
	clz	w7, w5				// way shift
1:	mov	x8, x6				// sets, from the highest down
2:	lsl	x9, x5, x7
	lsl	x10, x8, x4
	orr	x9, x9, x10
	dc	cisw, x9
	subs	x8, x8, #1
	b.ge	2b
	subs	x5, x5, #1			// ways, from the highest down
	b.ge	1b
	dsb	sy
	isb

	msr_s	SYS_APL_ACC_OVRD, x0
	msr_s	SYS_APL_CYC_OVRD, x1
	isb
	dsb	sy
	wfi

	mrs	x3, sctlr_el1
	orr	x3, x3, #SCTLR_ELx_C
	msr	sctlr_el1, x3
	isb
	mov	x0, #-16			// -EBUSY
	ret
SYM_FUNC_END(apple_s2r_rs_off_deep)
