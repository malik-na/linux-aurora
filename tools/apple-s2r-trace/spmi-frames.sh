#!/bin/sh
# spmi-frames.sh: shows, off the machine, that the driver's polled power-chip command (spmi_cmd()
# in drivers/soc/apple/s2r_seq.rs) makes the same register accesses, in the same order and with the
# same words, as the older driver's s2r_spmi_cmd() in drivers/soc/apple/apple-s2r.c, which has run
# on the chip. The C function is taken from that file as it stands (nothing retyped), compiled for
# the host over a model of the controller; the Rust function runs over the same model
# (apple-s2r-trace --spmi-frames). Fifteen commands: every one an entry makes, and one with a stale
# word in the reply queue. Exit 0 = the two logs are equal.
# usage: spmi-frames.sh <path to the built apple-s2r-trace> [work directory]
set -eu
here=$(cd "$(dirname "$0")" && pwd)
src="$here/../../drivers/soc/apple/apple-s2r.c"
bin=${1:?path to apple-s2r-trace}
work=${2:-$(mktemp -d)}
{
cat <<'STUB'
#include <stdint.h>
#include <stdio.h>
#include <stddef.h>
#include <stdbool.h>
#include <errno.h>
typedef uint8_t u8; typedef uint16_t u16; typedef uint32_t u32;
#define __iomem
#define BIT(n) (1U << (n))
#define GENMASK(h, l) (((~0U) >> (31 - (h))) & ~((1U << (l)) - 1))
#define FIELD_PREP(mask, val) (((u32)(val) << __builtin_ctz(mask)) & (mask))
#define FIELD_GET(mask, reg) (((reg) & (mask)) >> __builtin_ctz(mask))
static char regs[16]; static void __iomem *spmi = regs; static bool s2r_woken;
/* the model of the controller: the same as FakeSpmi in src/main.rs */
static u32 q[16]; static int qn; static int want; static u32 opc, reg;
static void push(u32 v) { q[qn++] = v; }
static u32 readl_relaxed(void *a) {
	u32 off = (char *)a - regs, v = 0;
	if (off == 0) v = (1U << 8) | (qn ? 0 : 1U << 24);
	else if (off == 8 && qn) { int i; v = q[0]; for (i = 1; i < qn; i++) q[i - 1] = q[i]; qn--; }
	printf("R 0x%02x -> 0x%08x\n", off, v); return v;
}
static void writel_relaxed(u32 v, void *a) {
	u32 off = (char *)a - regs;
	printf("W 0x%02x 0x%08x\n", off, v);
	if (off != 4) return;
	if (!want) { u32 len; opc = v & 0xff; reg = v >> 16; len = (opc & 7) + 1; want = (opc & 0xf8) == 0x30 ? (len + 3) / 4 : 0; }
	else want--;
	if (!want) {
		bool wr = (opc & 0xf8) == 0x30;
		push(opc | 0xf << 8 | (wr ? 1U << 15 : 0));
		if (!wr) { u32 len = (opc & 7) + 1, w, j; for (w = 0; w < (len + 3) / 4; w++) { u32 d = 0; for (j = 0; j < 4; j++) if (4 * w + j < len) d |= ((reg + 4 * w + j) & 0xff) << (8 * j); push(d); } }
	}
}
#define readl_poll_timeout_atomic(addr, val, cond, delay, timeout) \
	({ int __r = -ETIMEDOUT, __i; for (__i = 0; __i < 1000; __i++) { (val) = readl_relaxed(addr); if (cond) { __r = 0; break; } } __r; })
STUB
grep -E '^#define (SPMI_[A-Z0-9_]+|PMU_SLAVE)[[:space:](]' "$src"
sed -n '/^static int s2r_spmi_wait_reply(void)$/,/^static void s2r_mark(u8 c)$/p' "$src" | sed '$d'
cat <<'MAIN'
int main(void) {
	static const struct { u16 reg; int rlen; int wlen; u8 tx[8]; } c[15] = {
		{0x9f01, 1, 0, {0}}, {0x9f01, 0, 1, {0x50}}, {0xd002, 6, 0, {0}}, {0x9f04, 0, 4, {0x11, 0x22, 0x33, 0x44}},
		{0x9f10, 0, 8, {1, 2, 3, 4, 5, 6, 7, 8}}, {0x9f1a, 0, 1, {0xbf}}, {0x9f1c, 0, 3, {0x40, 0x00, 0x80}}, {0x9f0c, 0, 1, {0x8f}},
		{0x9f0a, 0, 1, {0x72}}, {0xd00c, 1, 0, {0}}, {0xd008, 0, 4, {0xc0, 0xd4, 0x01, 0x00}}, {0xd00e, 0, 1, {0x00}},
		{0xd000, 0, 1, {0x41}}, {0xd00c, 0, 1, {0x01}}, {0x9f0a, 1, 0, {0}},
	};
	int n, i;
	for (n = 0; n < 15; n++) {
		u8 rx[8] = {0}; int r;
		if (n == 14) push(0xdeadbeef);
		r = c[n].wlen ? pmu_write(c[n].reg, c[n].tx, c[n].wlen) : pmu_read(c[n].reg, rx, c[n].rlen);
		printf("cmd %d reg 0x%04x -> %d", n, c[n].reg, r ? -1 : 0);
		for (i = 0; i < c[n].rlen; i++) printf(" %02x", rx[i]);
		printf("\n");
	}
	return 0;
}
MAIN
} > "$work/spmi_c.c"
cc -Wall -Wno-unused-function -Wno-unused-variable -o "$work/spmi_c" "$work/spmi_c.c"
"$work/spmi_c" > "$work/frames-c.txt"
"$bin" --spmi-frames > "$work/frames-rust.txt"
if cmp -s "$work/frames-c.txt" "$work/frames-rust.txt"; then
	echo "EQUAL: $(grep -c '^W' "$work/frames-c.txt") controller writes and $(grep -c '^R' "$work/frames-c.txt") reads over $(grep -c '^cmd' "$work/frames-c.txt") commands, the older C function and the Rust one ($work)"
else
	echo "DIFFERENT: see $work/frames-c.txt and $work/frames-rust.txt"; diff "$work/frames-c.txt" "$work/frames-rust.txt" | head -20; exit 1
fi
