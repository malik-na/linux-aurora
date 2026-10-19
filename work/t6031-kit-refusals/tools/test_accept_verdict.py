"""Tests for accept_verdict: the J516C acceptance run's parser and verdict (malik-na/linux-aurora#54).

Run: python3 -m unittest test_accept_verdict (from tools/).
"""
import unittest

import accept_verdict as av

DMESG_GOOD = """\
[    3.100000] asahi-g15 6400000000.gpu: G15: probing (T6031 G15C)
[    3.200000] asahi-g15 6400000000.gpu: GPU firmware build: macOS 14.8 stub firmware, J516C
[    3.900000] asahi-g15 6400000000.gpu: G15: firmware at READY, starting the backend
[    4.000000] [drm] Initialized asahi 1.0.0 for 6400000000.gpu on minor 0
[    6.000000] apple-dcp 28c000000.dcp: J516C DCP firmware 14.7.0 handoff admitted
"""


def checks(**over):
    """native-checks' summary lines plus each job log's result line, as the collector prints them."""
    rows = {"compute-check": ("pass", "PASS: 10750 runs in 5.0 s, 0 with wrong results"),
            "vk-render-check": ("pass", "PASS: 8309 runs in 5.0 s, 0 with wrong pixels"),
            "gl-render-check": ("pass", "PASS: 10083 runs in 5.0 s, 0 with wrong pixels"),
            "gl-render-check-zink": ("pass", "PASS: 6418 runs in 5.0 s, 0 with wrong pixels")}
    rows.update(over)
    out = "".join(f"VERDICT {n} exit={0 if c == 'pass' else 1} {c}\n" for n, (c, _) in rows.items() if c)
    out += "".join(f"{n}.log: {l}\n" for n, (_, l) in rows.items() if l)
    return out


def section(name, body):
    return f"== {name}\n{body}"


def good_crtc():
    return good_parts()["crtc"]


def good(**over):
    parts = good_parts()
    parts.update(over)
    return "".join(section(k, v) for k, v in parts.items())


def good_parts():
    parts = {
        "dmesg": DMESG_GOOD,
        "drm": "card0-Unknown disconnected\ncard2-eDP-1 connected 3456x2234 edid=0\n",
        "checks": checks(),
        "kmscube": "Rendered 600 frames in 10.001 sec (59.99 fps)\n",
        "crtc": "crashed=0 presents=2621 brightness_presents=3 blank_presents=1 nits_presents=0 refused=0\n"
        "last_swap_id=9 swap_ns_last=16012345 swap_ns_max=17000000\n"
        "presents_before=2021 window_ns=10000000000\n",
        "engine": "jobs 1000 job_cmds 1200 job_max 4 early 0 recoveries 0 hang_kills 0 clustered 3\n",
        "load": "busy_pct 97.5\nfreq_khz unavailable\n",
    }
    return parts


class Sections(unittest.TestCase):
    def test_split(self):
        s = av.sections(good())
        self.assertIn("dmesg", s)
        self.assertIn("Initialized asahi", s["dmesg"])
        self.assertTrue(s["kmscube"].startswith("Rendered"))


class Verdict(unittest.TestCase):
    def test_good_run_passes(self):
        r = av.evaluate(good())
        self.assertEqual(r["verdict"], "PASS", r["reasons"])
        self.assertEqual(r["reasons"], [])

    def test_no_ready_fails(self):
        dmesg = DMESG_GOOD.replace("firmware at READY", "firmware stalled")
        r = av.evaluate(good(dmesg=dmesg))
        self.assertEqual(r["verdict"], "FAIL")
        self.assertTrue(any("READY" in x for x in r["reasons"]))

    def test_wrong_words_fail(self):
        r = av.evaluate(good(checks=checks(**{"compute-check": (
            "wrong", "FAIL: 10750 runs in 5.0 s, 2 with wrong results")})))
        self.assertEqual(r["verdict"], "FAIL")
        self.assertTrue(any("compute-check" in x for x in r["reasons"]))

    def test_failed_check_fails(self):
        r = av.evaluate(good(checks=checks(**{"compute-check": ("error", "")})))
        self.assertEqual(r["verdict"], "FAIL")

    def test_pass_with_wrong_count_still_fails(self):
        r = av.evaluate(good(checks=checks(**{"vk-render-check": (
            "pass", "PASS: 8309 runs in 5.0 s, 3 with wrong pixels")})))
        self.assertEqual(r["verdict"], "FAIL")

    def test_missing_check_fails(self):
        r = av.evaluate(good(checks=checks(**{"vk-render-check": (None, None)})))
        self.assertEqual(r["verdict"], "FAIL")
        self.assertTrue(any("vk-render-check" in x for x in r["reasons"]))

    def test_fault_line_fails(self):
        dmesg = DMESG_GOOD + "[    9.0] asahi-g15 6400000000.gpu: G15: fault bank 0: status 0x1 va 0x0 (r w level 3 unit 0x2 ctx 1)\n"
        r = av.evaluate(good(dmesg=dmesg))
        self.assertEqual(r["verdict"], "FAIL")
        self.assertEqual(len(r["faults"]), 1)

    def test_recovery_and_crash_lines_fail(self):
        for line in ("G15: fail-stop (ETIMEDOUT); GPU unusable until reboot",
                     "GPU firmware crashed; retaining session until reboot",
                     "recovery: firmware did not halt (100 ms)"):
            r = av.evaluate(good(dmesg=DMESG_GOOD + f"[ 9.0] asahi-g15 x.gpu: {line}\n"))
            self.assertEqual(r["verdict"], "FAIL", line)

    def test_engine_recoveries_fail(self):
        r = av.evaluate(good(engine="jobs 10 job_cmds 10 job_max 1 early 0 recoveries 1 hang_kills 0 clustered 0\n"))
        self.assertEqual(r["verdict"], "FAIL")

    def test_edp_missing_fails(self):
        r = av.evaluate(good(drm="card2-eDP-1 disconnected\n"))
        self.assertEqual(r["verdict"], "FAIL")
        self.assertTrue(any("eDP-1" in x for x in r["reasons"]))

    def test_refused_presents_fail(self):
        r = av.evaluate(good(crtc="presents=10 refused=2\nlast_swap_id=1 swap_ns_last=1 swap_ns_max=1\n"))
        self.assertEqual(r["verdict"], "FAIL")

    def test_kmscube_rendered_nothing_fails(self):
        r = av.evaluate(good(kmscube="failed to create the surface\n"))
        self.assertEqual(r["verdict"], "FAIL")
        self.assertTrue(any("kmscube" in x for x in r["reasons"]))

    def test_presents_not_moving_fails(self):
        for crtc in ("crashed=0 presents=2021 refused=0\npresents_before=2021 window_ns=10000000000\n",
                     "crashed=0 presents=2021 refused=0\n",
                     "crashed=0 refused=0\npresents_before=2021 window_ns=10000000000\n"):
            r = av.evaluate(good(crtc=crtc))
            self.assertEqual(r["verdict"], "FAIL", crtc)
            self.assertTrue(any("presents" in x for x in r["reasons"]), crtc)

    def test_crashed_crtc_fails(self):
        r = av.evaluate(good(crtc=good_crtc().replace("crashed=0", "crashed=1")))
        self.assertEqual(r["verdict"], "FAIL")
        self.assertTrue(any("crashed" in x for x in r["reasons"]))

    def test_accept_prefix_escape_fails(self):
        line = "[ 3.3] asahi-g15 x.gpu: GPU firmware build admitted by g15_accept_text_prefix\n"
        r = av.evaluate(good(dmesg=DMESG_GOOD + line))
        self.assertEqual(r["verdict"], "FAIL")
        self.assertIn("firmware build admitted by the accept-prefix escape", r["reasons"])

    def test_crtc_file_missing_fails(self):
        r = av.evaluate(good(crtc="crtc_file missing\npresents_before=1 window_ns=10000000000\n"))
        self.assertEqual(r["verdict"], "FAIL")
        self.assertTrue(any("crtc_file" in x for x in r["reasons"]))


class Measured(unittest.TestCase):
    def test_numbers_are_recorded_not_judged(self):
        r = av.evaluate(good(kmscube="Rendered 600 frames in 10.0 sec (30.00 fps)\n"))
        self.assertEqual(r["verdict"], "PASS")
        m = r["measured"]
        self.assertAlmostEqual(m["kmscube_fps"], 30.0)
        self.assertAlmostEqual(m["swap_ms_last"], 16.012345)
        self.assertEqual(m["presents"], 2621)
        self.assertEqual(m["throughput"]["compute-check"], 10750)
        self.assertAlmostEqual(m["gpu_busy_pct"], 97.5)
        self.assertIsNone(m["gpu_freq_khz"])
        self.assertEqual(m["edp_mode"], "3456x2234")
        self.assertEqual(m["edid_bytes"], 0)

    def test_kmscube_fps_is_the_final_total(self):
        k = ("Rendered 121 frames in 2.0 sec (60.5 fps)\n"
             "Rendered 1200 frames in 10.0 sec (120.00 fps)\n")
        self.assertAlmostEqual(av.evaluate(good(kmscube=k))["measured"]["kmscube_fps"], 120.0)

    def test_presents_per_second_from_the_counter_delta(self):
        self.assertAlmostEqual(av.evaluate(good())["measured"]["presents_per_s"], 60.0)

    def test_milestones_in_order(self):
        r = av.evaluate(good())
        names = [n for n, _ in r["milestones"]]
        self.assertEqual(names, ["probe", "firmware build", "READY", "drm initialized"])
        self.assertEqual(r["milestones"][2][1], 3.9)


class Scrub(unittest.TestCase):
    def test_mac_address_is_refused(self):
        self.assertTrue(av.private_lines("wlan0: link up 3c:22:fb:01:02:03\n"))

    def test_serial_is_refused(self):
        self.assertTrue(av.private_lines("serial-number: C02XYZ\n"))
        self.assertTrue(av.private_lines("IOPlatformSerialNumber = abc\n"))

    def test_hex_digests_are_fine(self):
        self.assertEqual(av.private_lines(good()), [])


class Render(unittest.TestCase):
    def test_verdict_on_top(self):
        md = av.render(av.evaluate(good()), name="baseline", head={"kernel": "x"})
        first = [l for l in md.splitlines() if l.strip()][1]
        self.assertTrue(first.startswith("**Verdict: PASS**"), first)


if __name__ == "__main__":
    unittest.main()
