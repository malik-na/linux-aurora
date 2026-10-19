"""Parser and verdict of the J516C acceptance run (malik-na/linux-aurora#54).

The M3 side of m3-accept prints one text made of `== NAME` sections:

  dmesg    the G15/DCP lines of this boot
  drm      one line per connector: `NAME STATUS [MODE] [edid=BYTES]`
  checks   native-checks' `VERDICT NAME exit=N CLASS` lines, then `NAME.log: ` + each job
           log's result line (`PASS|FAIL: RUNS runs in S s, WRONG with wrong ...`)
  kmscube  kmscube's own output (`Rendered N frames in S sec (F fps)`)
  crtc     the apple-drm crtc status file after kmscube (presents=, refused=, swap_ns_last=),
           plus `presents_before=N window_ns=N` around the kmscube run
  engine   the asahi-g15 engine statistics file (recoveries, hang_kills)
  load     under the sustained load of the checks: `busy_pct P`; from the asahi-g15/pstate
           file sampled every 100 ms, `freq_source stats|hwdata`, `freq_khz_samples K...`
           (kHz or `unknown` per sample) and `pstate_samples I...`; `freq_khz unavailable`
           without that file

evaluate() judges only what #54 names: no READY, any wrong word or pixel, a failed or missing
check, any GPU fault/recovery line, eDP-1 missing, a refused present. Everything measured
(fps, swap time, throughput, busy, clock) is recorded, never judged.
"""
import re
import statistics
import sys

MEASURED_KEYS = ("edp_mode", "edid_bytes", "kmscube_fps", "presents_per_s", "presents", "swap_ms_last",
                 "swap_ms_max", "gpu_busy_pct", "gpu_freq_khz_max", "gpu_freq_khz_median",
                 "gpu_freq_samples", "gpu_pstate_max", "gpu_pstate_source")

CHECKS = ("compute-check", "vk-render-check", "gl-render-check", "gl-render-check-zink")

RESULT = re.compile(r"^(\S+)\.log: (PASS|FAIL): (\d+) runs in [\d.]+ s, (\d+) with wrong")

MILESTONES = (
    ("probe", re.compile(r"G15: probing")),
    ("firmware build", re.compile(r"GPU firmware build: ")),
    ("READY", re.compile(r"firmware at READY")),
    ("drm initialized", re.compile(r"\[drm\] Initialized asahi")),
)

FAULT = re.compile(
    r"G15: fault bank|fail-stop|GPU firmware crashed|G15 start failed|backend start failed"
    r"|recovery: |recovery \d+:|compute hang|unsupported GPU firmware build|Kernel panic"
)

TIME = re.compile(r"^\[\s*(\d+\.\d+)\]")

PRIVATE = (
    re.compile(r"\b[0-9a-f]{2}(?::[0-9a-f]{2}){5}\b", re.I),
    re.compile(r"serial", re.I),
)


def sections(text):
    out, name = {}, None
    for line in text.splitlines(keepends=True):
        if line.startswith("== "):
            name = line[3:].strip()
            out[name] = ""
        elif name is not None:
            out[name] += line
    return out


def key_values(text):
    return {k: v for k, v in re.findall(r"(\w+)=(\S+)", text)}


def private_lines(text):
    return [l for l in text.splitlines() if any(p.search(l) for p in PRIVATE)]


def numbers(text):
    """The non-negative integers among the words of `text` (others, e.g. `unknown`, skipped)."""
    return [int(w) for w in text.split() if w.isdigit()]


def load_clock(load):
    """The clock measurements of the `load` section's key -> rest-of-line map: the max and the
    median (the lower middle sample, so an observed operating point) of the frequency samples,
    their count, the highest state index and the state's source; all None without a sample."""
    out = dict.fromkeys(("gpu_freq_khz_max", "gpu_freq_khz_median", "gpu_freq_samples",
                         "gpu_pstate_max", "gpu_pstate_source"))
    khz = numbers(load.get("freq_khz_samples", "")) or numbers(load.get("freq_khz", ""))
    if not khz:
        return out
    out["gpu_freq_khz_max"] = max(khz)
    out["gpu_freq_khz_median"] = statistics.median_low(khz)
    out["gpu_freq_samples"] = len(khz)
    ps = numbers(load.get("pstate_samples", ""))
    out["gpu_pstate_max"] = max(ps) if ps else None
    out["gpu_pstate_source"] = load.get("freq_source", "").strip() or None
    return out


def evaluate(text):
    s = sections(text)
    reasons, measured = [], dict.fromkeys(MEASURED_KEYS)
    dmesg = s.get("dmesg", "")

    milestones = []
    for name, pat in MILESTONES:
        for line in dmesg.splitlines():
            if pat.search(line):
                t = TIME.match(line)
                milestones.append((name, float(t.group(1)) if t else None))
                break
    if "READY" not in [n for n, _ in milestones]:
        reasons.append("no 'firmware at READY' in the kernel log")

    if "admitted by g15_accept_text_prefix" in dmesg:
        reasons.append("firmware build admitted by the accept-prefix escape")

    faults = [l.strip() for l in dmesg.splitlines() if FAULT.search(l)]
    if faults:
        reasons.append(f"{len(faults)} GPU fault/recovery line(s)")

    throughput, classes, results = {}, {}, {}
    for line in s.get("checks", "").splitlines():
        f = line.split()
        if len(f) == 4 and f[0] == "VERDICT":
            classes[f[1]] = f[3]
        m = RESULT.match(line)
        if m:
            results[m.group(1)] = (m.group(2), int(m.group(3)), int(m.group(4)))
    for c in CHECKS:
        if c not in classes:
            reasons.append(f"{c} did not run")
            continue
        state, runs, wrong = results.get(c, (None, None, None))
        if runs is not None:
            throughput[c] = runs
        if classes[c] != "pass" or state != "PASS" or wrong:
            reasons.append(f"{c}: {classes[c]}, {wrong if wrong is not None else '?'} wrong")
    measured["throughput"] = throughput

    edp = [l.split() for l in s.get("drm", "").splitlines() if "-eDP-1" in l]
    if not edp or edp[0][1] != "connected":
        reasons.append("eDP-1 missing or not connected")
    else:
        f = edp[0]
        measured["edp_mode"] = next((x for x in f[2:] if re.fullmatch(r"\d+x\d+", x)), None)
        e = next((x for x in f[2:] if x.startswith("edid=")), None)
        measured["edid_bytes"] = int(e[5:]) if e else None

    fps = re.findall(r"\(([\d.]+) fps\)", s.get("kmscube", ""))
    measured["kmscube_fps"] = float(fps[-1]) if fps else None
    if not re.search(r"^Rendered ", s.get("kmscube", ""), re.M):
        reasons.append("kmscube rendered nothing")

    crtc = key_values(s.get("crtc", ""))
    measured["presents"] = int(crtc["presents"]) if "presents" in crtc else None
    measured["swap_ms_last"] = int(crtc["swap_ns_last"]) / 1e6 if "swap_ns_last" in crtc else None
    measured["swap_ms_max"] = int(crtc["swap_ns_max"]) / 1e6 if "swap_ns_max" in crtc else None
    if "presents" in crtc and "presents_before" in crtc and int(crtc.get("window_ns", 0)):
        measured["presents_per_s"] = (int(crtc["presents"]) - int(crtc["presents_before"])) / (
            int(crtc["window_ns"]) / 1e9)
    else:
        measured["presents_per_s"] = None
    if "crtc_file missing" in s.get("crtc", ""):
        reasons.append("crtc_file missing: no crtc status file")
    if "presents" not in crtc or "presents_before" not in crtc:
        reasons.append("crtc presents or presents_before missing")
    elif int(crtc["presents"]) <= int(crtc["presents_before"]):
        reasons.append("crtc presents did not move during kmscube")
    if int(crtc.get("crashed", 0)):
        reasons.append("crtc status has crashed=" + crtc["crashed"])
    if int(crtc.get("refused", 0)):
        reasons.append(f"{crtc['refused']} refused present(s)")

    eng = dict(re.findall(r"\b(recoveries|hang_kills) (\d+)", s.get("engine", "")))
    if int(eng.get("recoveries", 0)) or int(eng.get("hang_kills", 0)):
        reasons.append(f"engine: {eng.get('recoveries', 0)} recoveries, {eng.get('hang_kills', 0)} hang kills")

    load = {f[0]: f[1] if len(f) > 1 else "" for f in
            (l.split(None, 1) for l in s.get("load", "").splitlines()) if f}
    measured["gpu_busy_pct"] = float(load["busy_pct"]) if "busy_pct" in load else None
    measured.update(load_clock(load))

    return {
        "verdict": "FAIL" if reasons else "PASS",
        "reasons": reasons,
        "milestones": milestones,
        "faults": faults,
        "measured": measured,
    }


def render(r, name, head):
    m = r["measured"]
    out = [f"# Acceptance run: {name}", "", f"**Verdict: {r['verdict']}**", ""]
    out += [f"- {x}" for x in r["reasons"]] + ([""] if r["reasons"] else [])
    out += ["## Build", ""] + [f"- {k}: {v}" for k, v in head.items()] + [""]
    out += ["## Milestones (s since boot)", ""]
    out += [f"- {n}: {t}" for n, t in r["milestones"]] + [""]
    out += ["## Checks (completed runs in the window)", "", "| check | runs |", "|---|---|"]
    out += [f"| {c} | {n} |" for c, n in m["throughput"].items()] + [""]
    out += ["## Measured (recorded, not judged)", ""]
    for k in MEASURED_KEYS:
        v = m.get(k)
        out.append(f"- {k}: {'unavailable' if v is None else v}")
    out += ["", "## GPU fault/recovery lines", ""]
    out += [f"    {l}" for l in r["faults"]] or ["none"]
    return "\n".join(out) + "\n"


def main(argv):
    """accept_verdict.py COLLECT NAME KEY=VALUE...: print the record; exit 0 on PASS, 1 on FAIL,
    3 if COLLECT holds private lines (nothing printed)."""
    text = open(argv[1]).read()
    if private_lines(text):
        print("refusing: the collected text holds private lines", file=sys.stderr)
        return 3
    r = evaluate(text)
    head = dict(a.split("=", 1) for a in argv[3:])
    sys.stdout.write(render(r, argv[2], head))
    return 0 if r["verdict"] == "PASS" else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
