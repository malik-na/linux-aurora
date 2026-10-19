**M3 Max (J516C, 16") update: 19–20 Oct**

The GPU and display port on the G15 stack is in good shape. The tested reference is aurora-silicon/linux#363 (head `027bd762`). Chris is carrying the commits into the 7.2.11 tree (siliconforest-linux#12) on the way to omacom `integration/aurora`.

**Done since the last update**
- **Display:**
  - 120 Hz pacing fixed: kmscube now runs at 120 fps; it was stuck at 60.
  - The panel reports make, model and size (Apple J516, 346x223 mm).
  - Brightness keys and slider work.
  - The display profile now matches the J516C only, after Chris's review. 14" J514C machines need their own profile (#148 has one).
- **GPU:**
  - Firmware 14.7 and 14.8 builds are both recognised. An unknown build now says what to report.
  - Board data is built from the device tree. Only 32 words still come from the private board file: maximum power, relative powers, MTR masks and sensor rows. The sensor rows need an m1n1 export.
  - The clock reaches 1380 MHz (the top state) under load. The idle standby time comes from the device tree.
  - s2idle suspend and resume work.
- **Compute hangs and a driver deadlock, fixed:**
  - About 145 dEQP cases hung the GPU. The cause: compute was seeded with 8 of its 22 memory pools.
  - Two further bugs sat behind it, both fixed: a parameter-buffer livelock and a too-small parameter-buffer heap.
  - The full dEQP subset now runs with nothing excluded. No fail-stop, no recoveries.

**dEQP subset (VK-GL-CTS 1.4.7.0, J516C)**

| Suite | Pass | Fail | Crash |
|---|---|---|---|
| VK | 706 | 2 | 0 |
| GLES3 | 946 | 0 | 0 |
| GLES31 | 796 | 1 | 0 |

GLES3 also has a few flaky depth/stencil cases that pass on retry. The 3 fails are image comparisons (render correctness).

**Testing:** every change is checked with one command (`m3-accept`). It boots the dev kernel, then runs the compute, Vulkan, GL and zink checks, kmscube on the panel, and the GPU clock under load. Each step also gets a full dEQP regression before it goes to the PR.

**Open / next**
- Render register values are still the M3 Pro's (G15S). They're the main suspect for the 3 image-comparison fails.
- The idle confidence and predictive words still use the M3 Pro tuning. Learning the J516C's own values needs a macOS observation run.
- Retiring the private board file needs the m1n1 sensor-row export and a power model.
- External displays: researched; decision is "later". The T6030 code is the template.

**For the team**
- The pool and parameter-buffer fixes touch shared G15 code, so T6030 gets them too. They are untested on an M3 Pro. Chris is checking the T6030 drain path in his carry.
- The #363 description is up to date (current J516C state, dEQP numbers, remaining gaps).
