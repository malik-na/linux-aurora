# Tests for decode_m3_fault.py. Run: python3 tools/test_decode_m3_fault.py (no pytest needed).
from decode_m3_fault import decode_fault, launch_programs, parse_log


def test_t50_bank0_fault_decodes_as_an_unmapped_read_by_ul1c_8():
    f = decode_fault(0x8069030211, 0x400000100)
    assert f["valid"] and f["read"]
    assert f["reason"] == "unmapped"
    assert f["unit"] == 0x81 and f["unit_name"] == "UL1C(8)"
    assert f["vm_slot"] == 1
    assert f["address"] == 0x10000004000


def test_t50_bank256_fault_is_ul1c_9_0x180_further():
    f = decode_fault(0x8069032211, 0x400000106)
    assert f["unit_name"] == "UL1C(9)"
    assert f["address"] == 0x10000004180


def test_clear_bank_is_not_a_fault():
    assert decode_fault(0, 0) is None


def test_launch_program_uses_word1_high_bits_and_word2_times_64():
    # head word 0 (launch), word1 carries VA[47:38] in its top ten bits, word2 is VA[37:6].
    va = (0x4 << 38) | (0x1234 << 6)
    words = [0x0, 0x4 << 22, 0x1234] + [0] * 7
    stream = b"".join(w.to_bytes(4, "little") for w in words)
    assert launch_programs(stream) == [va]


def test_parse_log_pulls_banks_and_cdm_bytes():
    log = """
[ 52.1] asahi 40a400000.gpu: M3 bank 0 fault=0x8069030211 address_word=0x400000100 address=0x10000004000
[ 52.1] asahi 40a400000.gpu: M3 bank 1 fault=0x0 address_word=0x0 address=0x0
[ 52.0] asahi 40a400000.gpu: M3 client CDM bytes=[00, 00, 00, 00, 00, 00, 00, 01, 34, 12, 00, 00]
"""
    banks, cdm = parse_log(log)
    assert banks == [(0, 0x8069030211, 0x400000100), (1, 0, 0)]
    assert cdm == bytes([0, 0, 0, 0, 0, 0, 0, 1, 0x34, 0x12, 0, 0])


if __name__ == "__main__":
    tests = [(n, f) for n, f in sorted(globals().items()) if n.startswith("test_")]
    for name, fn in tests:
        fn()
        print("ok", name)
    print(f"{len(tests)} passed")
