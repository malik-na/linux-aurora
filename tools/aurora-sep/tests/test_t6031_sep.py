"""T6031 (MacBook Pro 16" M3 Max, J516C) SEP and Mesa sensor device tree and profile.

Set DTC to this kernel's built scripts/dtc/dtc for Apple floating-point cells.

Values marked UNVERIFIED in the device tree and profile (DART window size, SPI
mode, sensor part, mailbox address and IRQ order, power GPIO) are pinned here
only so a change to them is deliberate; this test does not confirm them.
"""

from pathlib import Path
import os
import re
import shutil
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[3]
DTS = ROOT / "arch/arm64/boot/dts/apple"
DTC = os.environ.get("DTC", "dtc")
PROFILE = (ROOT / "drivers/soc/apple/profile.rs").read_text()

SEP_DART = "/soc/iommu@388ac0000"
SEP = "/soc/sep@38a400000"
BUS = "/soc/spi@391108000"
SENSOR = BUS + "/fingerprint@0"
PINCTRL_NUB = "/soc/pinctrl@2a01f0000"
MBOX = "/soc/mbox@38a408000"
# AIC_IRQ cells on a multi-die SoC: <type die irq flags>, IRQ_TYPE_LEVEL_HIGH = 4.
LEVEL_HIGH = 4


@unittest.skipUnless(all(shutil.which(tool) for tool in ("cpp", DTC, "fdtget")),
                     "requires cpp, dtc and fdtget")
class T6031SepTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temp = tempfile.TemporaryDirectory(prefix="aurora-t6031-sep-")
        cls.addClassCleanup(cls.temp.cleanup)
        cls.out = Path(cls.temp.name)

    def compile_board(self, board):
        pp = subprocess.run(["cpp", "-nostdinc", "-undef", "-D__DTS__", "-x",
                             "assembler-with-cpp", "-I", str(ROOT / "include"),
                             "-I", str(DTS), str(DTS / f"t6031-{board}.dts")],
                            check=True, capture_output=True)
        output = self.out / f"{board}.dtb"
        subprocess.run([DTC, "-q", "-I", "dts", "-O", "dtb", "-o", str(output)],
                       input=pp.stdout, check=True, capture_output=True)
        return output

    def prop(self, dtb, node, name, kind="x"):
        return subprocess.run(["fdtget", "-t", kind, str(dtb), node, name],
                              check=True, capture_output=True, text=True).stdout.split()

    def props(self, dtb, node):
        return subprocess.run(["fdtget", "-p", str(dtb), node],
                              check=True, capture_output=True, text=True).stdout.split()

    def cells(self, dtb, node, name):
        return [int(cell, 16) for cell in self.prop(dtb, node, name)]

    def irqs(self, dtb, node):
        cells = self.cells(dtb, node, "interrupts")
        self.assertEqual(len(cells) % 4, 0)
        quads = [cells[i:i + 4] for i in range(0, len(cells), 4)]
        for kind, die, _, flags in quads:
            self.assertEqual((kind, die, flags), (0, 0, LEVEL_HIGH))
        return [irq for _, _, irq, _ in quads]

    def test_j516c_describes_sep_and_sensor(self):
        dtb = self.compile_board("j516c")
        # apple_sep enables the SEP and its DART at module load.
        self.assertEqual(self.prop(dtb, SEP_DART, "status", "s"), ["disabled"])
        self.assertEqual(self.prop(dtb, SEP, "status", "s"), ["disabled"])
        self.assertEqual(self.prop(dtb, MBOX, "status", "s"), ["okay"])
        # J516C ADT vm-base 0x10000004000. UNVERIFIED size 0x3ff00000, not 0xffff0000.
        self.assertEqual(self.cells(dtb, SEP_DART, "apple,dma-range"),
                         [0x100, 0x4000, 0, 0x3ff00000])
        self.assertEqual(self.cells(dtb, SEP_DART, "apple,dma-offset"), [0, 0])
        self.assertEqual(self.prop(dtb, "/aliases", "sep", "s"), [SEP])

        self.assertEqual(self.prop(dtb, BUS, "status", "s"), ["okay"])
        self.assertEqual(self.prop(dtb, SENSOR, "status", "s"), ["okay"])
        self.assertEqual(self.prop(dtb, SENSOR, "firmware-name", "s"),
                         ["apple/mesacal-j516c.bin"])
        nub = int(self.prop(dtb, PINCTRL_NUB, "phandle")[0], 16)
        # UNVERIFIED power GPIO: NUB pin 53, active high.
        self.assertEqual(self.cells(dtb, SENSOR, "enable-gpios"), [nub, 53, 0])
        self.assertEqual(self.prop(dtb, SENSOR, "spi-max-frequency", "u"), ["8000000"])
        self.assertEqual(self.prop(dtb, SENSOR, "spi-cs-setup-delay-ns", "u"), ["20"])
        self.assertEqual(self.prop(dtb, SENSOR, "spi-cs-hold-delay-ns", "u"), ["20"])
        properties = self.props(dtb, SENSOR)
        # UNVERIFIED SPI mode 1.
        self.assertIn("spi-cpha", properties)
        self.assertNotIn("spi-cpol", properties)
        self.assertNotIn("interrupts", properties)
        self.assertNotIn("cs-gpios", self.props(dtb, BUS))

    def test_addresses_and_interrupts_match_the_adt(self):
        dtb = self.compile_board("j516c")
        self.assertEqual(self.cells(dtb, SEP_DART, "reg"), [0x3, 0x88ac0000, 0, 0x4000])
        self.assertEqual(self.irqs(dtb, SEP_DART), [579])
        self.assertEqual(self.cells(dtb, SEP, "reg"), [0x3, 0x8a400000, 0, 0x6c000])
        self.assertEqual(self.cells(dtb, MBOX, "reg"), [0x3, 0x8a408000, 0, 0x4000])
        self.assertEqual(self.prop(dtb, MBOX, "interrupt-names", "s"),
                         ["send-empty", "send-not-empty", "recv-empty", "recv-not-empty"])
        self.assertEqual(self.irqs(dtb, MBOX), [573, 574, 575, 576])
        mbox = int(self.prop(dtb, MBOX, "phandle")[0], 16)
        dart = int(self.prop(dtb, SEP_DART, "phandle")[0], 16)
        self.assertEqual(self.cells(dtb, SEP, "mboxes"), [mbox])
        self.assertEqual(self.cells(dtb, SEP, "iommus"), [dart, 0])
        self.assertEqual(self.cells(dtb, BUS, "reg"), [0x3, 0x91108000, 0, 0x4000])
        self.assertEqual(self.irqs(dtb, BUS), [1313])
        self.assertEqual(self.cells(dtb, SENSOR, "reg"), [0])

    def test_j514c_leaves_sep_and_sensor_off(self):
        dtb = self.compile_board("j514c")
        self.assertNotIn("sep", self.props(dtb, "/aliases"))
        self.assertEqual(self.prop(dtb, MBOX, "status", "s"), ["disabled"])
        self.assertEqual(self.prop(dtb, BUS, "status", "s"), ["disabled"])
        self.assertEqual(self.prop(dtb, SENSOR, "status", "s"), ["disabled"])

    def test_profile_selected_only_on_j516c(self):
        detect = PROFILE.split("pub(crate) fn detect()", 1)[1]
        line = next(line for line in detect.splitlines() if 'b"apple,t6031"' in line)
        self.assertIn('b"apple,j516c"', line)
        profile = re.search(r"const T6031: PlatformProfile = PlatformProfile \{(.*?)\n\};",
                            PROFILE, re.S).group(1)
        self.assertIn("..T8122", profile)
        self.assertIn("dart_window: &DART_T6031", profile)
        # The diagnostic controller base is spi2's, not T8122's.
        self.assertIn("controller_base: 0x3_9110_8000", profile)


if __name__ == "__main__":
    unittest.main()
