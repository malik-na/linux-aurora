# Boots t73 and t74: the SGX setup write (2026-10-14 00:44 and 00:48)

Ticket 43. The armed T6031 start writes the T6030's SGX setup word before the firmware starts: SGX+0xd14000 (Fender
block + 0x14000) = 0x70001, the same write the T6030 and T8122 starts make.

- t73 (package dev-68, `asahi.t6031_sgx_setup=none`, T6030 register values, resume knob): no write. Firmware
  ready, render node up, first job unchanged (ten halts answered by a resume, all words zero).
- t74 (cc9341dd, package dev-72: the register logged before the write): `SGX+0xd14000 reads 0x0 before the setup
  write of 0x70001`. Job unchanged.

So the register starts at 0 on this part, the write sets it, and the firmware and the first job behave the same either
way. The write is not needed for the G15C firmware to start and is not what the dispatch waits on. Decision: keep it
for parity with the two qualified starts; the knob stays for one-boot tests.

With this the frontier of the first-job search is empty on the host side. Two tickets continue it: the kernel-half
address window of the compute graph (one boot), and a direction decision with Naeem (ask the team, trace the kick
differently, or park the GPU for the display and the PR series).
