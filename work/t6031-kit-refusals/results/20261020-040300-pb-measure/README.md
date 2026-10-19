# PB demand samples, 2026-10-20 04:03-04:10 (kernel 027bd762 stack, boot of 02:34)

`pb:` line of `/sys/kernel/debug/asahi-g15/stats`, MemAvailable (kB), render count and FS busy time sampled
every ~165 ms (`t_ms` since the sample's start). One workload per file; `run.log` has the sequence;
`05-growth-lines.txt` the kernel's growth-request lines of the boot (public-safe: contexts, counters,
positions, block counts). Analysis: `../../research/pb-memory.md` §2. The boot already had 2884 blocks posted
from an earlier full dEQP run; the VK list grew it to 4095 and the GPU fail-stopped (2 refusals).
