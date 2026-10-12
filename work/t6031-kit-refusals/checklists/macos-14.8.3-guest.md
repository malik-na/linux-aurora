# Checklist: a macOS 14.8.3 volume on the M3 Max for the m1n1 hypervisor (map ticket #22)

Why: Linux on this Mac loads GPU firmware `RTKit-2419.140.12`, image `F9BEA4A3-…`, from the macOS 14.8.3 stub (23J220). A full macOS 14.8.3 install should load the same image; tracing it under the m1n1 hypervisor reads the InitData (version first) that macOS gives the G15C. Clean-room: ADR 0001 allows this, privately. Apple's installer and kernel never leave this Mac or the Air; only observed values go into code, each with its source.

Source for the steps: the Asahi hypervisor guide (`docs/sw/m1n1-hypervisor.md`) and Apple's "Use more than one version of macOS on Mac" (118282). Findings: `research/macos-same-gpu-firmware/findings.md` on the fork.

**While the M3 runs macOS, Linux and the Air's SSH are gone.** Finish any Linux test boot first, and tell me before step 1.

## Part A: at the Mac, in macOS 27 (you)

1. Boot macOS 27: power off, hold the power button for the boot picker, pick Macintosh HD.
2. Check free space in the main container: Apple menu → About This Mac → Storage, or in Terminal `diskutil apfs list` and read the main container's free space. Need about 40 GB.
3. Add a volume to the main container, no system role:
   ```
   diskutil apfs addVolume disk3 APFS macOSTest
   ```
   Replace `disk3` with the main container's disk, the one Macintosh HD is in (`diskutil apfs list`). On this Mac that is **disk4**; disk3 is the 2.5 GB Linux stub container. Done 2026-10-12: `macOSTest` on disk4, no role.
4. Fetch the 14.8.3 installer:
   ```
   softwareupdate --fetch-full-installer --full-installer-version 14.8.3
   ```
   **Blocked 2026-10-12:** Apple no longer offers 14.8.3 (oldest Sonoma listed: 14.8.7, 23J520; build 23J220 is absent from Apple's catalog). Use the archived InstallAssistant.pkg the Asahi hypervisor guide lists for 14.8.3: https://archive.org/details/install-assistant_20250207 (`docs/sw/m1n1-hypervisor.md`, "Using archived InstallAssistant.pkg"). It is a third-party mirror of an Apple-signed package, so before running it: `pkgutil --check-signature InstallAssistant.pkg` must show Apple's signature chain, and after it extracts "Install macOS Sonoma.app" into /Applications, its `Contents/Info.plist` must say 14.8.3 / build 23J220 (`DTPlatformVersion` and the app's version keys). Refuse anything else.

   **Checked 2026-10-12:** Apple's CDN no longer serves the 23J220 package (the direct swcdn URLs listed by Mr. Macintosh answer 403; the 14.8.7 one still answers 200), so the archive is the only source. archive.org was returning 502 "overloaded" on and off; its metadata for the item lists `InstallAssistant.pkg`, 13,658,132,415 bytes, sha1 `ff2e887ecf85489f84b9293ee1038f9c050b904d`. Download with resume and retries, then check the hash before anything else:
   ```
   curl -L -C - --retry 30 --retry-all-errors --retry-delay 30 -o ~/Downloads/InstallAssistant.pkg \
     https://archive.org/download/install-assistant_20250207/InstallAssistant.pkg
   shasum -a 1 ~/Downloads/InstallAssistant.pkg   # expect ff2e887ecf85489f84b9293ee1038f9c050b904d
   pkgutil --check-signature ~/Downloads/InstallAssistant.pkg
   ```
   The sha1 shows the download is the file archive.org holds; Apple's signature on the package shows it is Apple's; the app's Info.plist shows it is 23J220. All three.
5. Run the installer and pick **macOSTest** as the target. Let it finish and set up a user (any name; it's a throwaway install). Skip iCloud and all services.
6. First boot of macOSTest: in System Settings → General → Startup Disk, macOSTest is now the default. Leave it so for the next part.

## Part B: in 1TR, on the new volume (you)

7. Power off, hold power for the boot picker, choose Options (recoveryOS). Pick the macOSTest admin user when asked.
8. In Utilities → Terminal:
   ```
   bputil -nkcas
   csrutil disable
   nvram boot-args=-v
   ```
   `bputil -nkcas` asks for the volume's user and sets *that volume's* policy to Permissive. It must name macOSTest, never Macintosh HD or the Linux stub.
9. Stop here and tell me. I build the m1n1 hypervisor image next (Part C), and you then install it from 1TR:
   ```
   kmutil configure-boot -c m1n1.bin --raw --entry-point 2048 --lowest-virtual-address 0 -v /Volumes/macOSTest
   ```
   (m1n1.bin on a USB stick, or copied to the volume from macOSTest beforehand.)

## Part C: on the Air (me)

10. Build m1n1 from omacom `integration/aurora` with the hypervisor (the fork carries upstream's M3 14.8.3 hypervisor work), hand you `m1n1.bin` for step 9.
11. After step 9: with macOSTest the default, each boot stops in m1n1 and waits for the Air over USB-C. First check: m1n1 prints `FDT: T6031: firmware image gfx=<UUID>`. **It must be `F9BEA4A3-D65B-386D-B884-ECF2DF7B159E`** or the trace would apply to other firmware; then we stop and rethink.
12. Kernel for the guest: copy the kernelcache out of macOSTest's Preboot (the guide's steps, with img4tool), on the Air, kept private.
13. Run the guest with the AGX tracer and read InitData+0. Record the value with its provenance ("observed in an m1n1 hypervisor trace of macOS 14.8.3 (23J220) on J516C") on the ticket; then the layout, as far as the tracer decodes it.

## Part D: back to Linux (you)

14. In macOSTest or macOS 27: System Settings → Startup Disk → the Linux stub (it shows as the Asahi/Omarchy stub). Or hold power at boot and pick it. Linux boots normally; the test-boot harness works again.

## What can go wrong

- The 14.8.3 installer refuses to run under macOS 27: tell me; the fallback is a bootable installer (`createinstallmedia`) or the boot picker.
- 1TR shows no valid users: step 3 gave the volume a system role, or step 5 made no user. Redo from step 3.
- Linux boots don't work after Part B: the default startup disk is still macOSTest. Part D fixes it.
