# -m script for run_guest.py on the J516C: trace the two paths that can reconfigure a USB-C port
# from the guest without touching the port's own controller -- the SMC mailbox and the SPMI buses
# the CD3217 port controllers hang off -- to see what the guest sends in the second before the
# proxy port turns into Apple's "Debug USB" gadget (2026-10-13 runs: 5-6 s after guest entry,
# with no guest access to usb-drd0/atc-phy0/dart-usb0).
import os, runpy, traceback
HVDIR = "/home/naeem/Projects/m1n1/proxyclient/hv"

try:
    runpy.run_path(os.path.join(HVDIR, "trace_smc.py"), init_globals=globals(), run_name="<hv_script>")
    print("trace_port_actors: SMC tracer on")
except Exception:
    print("trace_port_actors: SMC tracer failed:\n" + traceback.format_exc())

try:
    from m1n1.trace.spmi import SPMITracer, SPMIDevTracer, ACE3SPMIDevTracer
    for dname in ("/arm-io/nub-spmi-a0", "/arm-io/nub-spmi-a1"):
        try:
            t = SPMITracer(hv, dname, verbose=0)
            for dev in t.dev:
                if not hasattr(dev, "reg"):
                    continue
                addr = dev.reg[0]
                cls = ACE3SPMIDevTracer if dev.name.startswith("hpm") else SPMIDevTracer
                t.add_device(addr, cls(addr, dev.name))
            t.start()
            print(f"trace_port_actors: SPMI tracer on {dname}")
        except Exception:
            print(f"trace_port_actors: SPMI tracer on {dname} failed:\n" + traceback.format_exc())
except Exception:
    print("trace_port_actors: SPMI import failed:\n" + traceback.format_exc())
