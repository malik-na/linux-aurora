# M3 Max bring-up

Getting Linux (linux-aurora) running natively on the M3 Max MacBook Pro 16" (J516C, T6031): its GPU (G15C) and built-in display.

## Language

### GPU firmware

**Firmware interface**:
Everything the GPU firmware expects the host to lay out to match its build: the InitData version, the structure layouts and the record formats.
_Avoid_: FW interface, host interface, ABI

**InitData version**:
The single 64-bit number at the start of the InitData that names which firmware interface the host is speaking.
_Avoid_: firmware version, interface version
