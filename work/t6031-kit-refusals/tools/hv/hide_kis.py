# -m script for run_guest.py: runs after the ADT is set up and before it is uploaded, so the
# deletions below reach the guest. The J516C guest brings Apple's "Debug USB" (KIS) gadget up on
# its DFU port (usb-drd0, port 1) the moment its drivers start, which is the port m1n1's proxy
# lives on; the stock hypervisor only strips the port's controller nodes. Try also removing the
# nodes that describe the port as the debug/DFU port.
for name in ("/port-usb-c-1", "/arm-io/cpu-debug-interface", "/arm-io/uart0/debug-console"):
    try:
        del hv.adt[name]
        print(f"hide_kis: removed ADT node {name}")
    except KeyError:
        print(f"hide_kis: no ADT node {name}")
