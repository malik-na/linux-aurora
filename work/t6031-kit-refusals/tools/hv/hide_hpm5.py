# -m script for run_guest.py on the J516C: the ADT carries a second port controller for the DFU
# port, /arm-io/nub-spmi-a0/hpm5 (port-number 1 like hpm0, port-type 17), which the stock
# hypervisor leaves in place when it strips the proxy port's nodes. The guest's writes to the
# port's USB controller never happen (sink_usb0.py run, 2026-10-13 09:29), yet the port flips to
# Apple's "Debug USB" gadget 6 s after entry, so a port-controller path is the suspect. Hide it.
for name in ("/arm-io/nub-spmi-a0/hpm5",):
    try:
        del hv.adt[name]
        print(f"hide_hpm5: removed ADT node {name}")
    except KeyError:
        print(f"hide_hpm5: no ADT node {name}")
