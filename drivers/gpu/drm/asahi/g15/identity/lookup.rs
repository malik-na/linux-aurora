// SPDX-License-Identifier: GPL-2.0-only OR MIT

//! Pure firmware-build identification: the GKTS marker search, the lookup of a measured build in
//! the qualified list, the `g15_accept_text_prefix` escape and the refusal hint.
//!
//! A qualified build pins only through [`super::matching::exact`]: its sizes, marker and digest,
//! and the machine it names.
//!
//! Core-only (no `kernel` imports): this file is compiled by the host test harness as well.

use super::matching;

/// The GKTS marker: the 8 bytes after it are per-boot entropy.
pub(crate) const MARKER: [u8; 8] = [0x47, 0x4b, 0x54, 0x53, 0x08, 0, 0, 0];

/// Offset of the image-info header in TEXT, and its first words: a branch over the header, the
/// `uuid` magic and header version 5. Words 11 and 12 are the patchbay offset and size, word 13
/// the TEXT size.
pub(crate) const HEADER: usize = 0x4200;
const HEADER_BRANCH: u32 = 0x14000011;
const HEADER_MAGIC: u32 = 0x64697575;
const HEADER_VERSION: u32 = 5;
const HEADER_WORDS: usize = 14;

/// The patchbay record that names the tunables buffer (its firmware VA) and the one that gives
/// its size.
const TUNS: [u8; 4] = *b"SNUT";
const TUNZ: [u8; 4] = *b"ZNUT";

/// Why the boot-loader data of a TEXT image could not be delimited.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum BootData {
    /// No well-formed image-info header for this TEXT size.
    Header,
    /// The header's patchbay does not start at the GKTS marker.
    Patchbay,
    /// The patchbay is not exactly filled by `tag, length, value` records.
    Records,
    /// The tunables buffer the patchbay names is not inside TEXT after the header.
    Tunables,
}

fn word(text: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([text[off], text[off + 1], text[off + 2], text[off + 3]])
}

/// Zeroes what the boot loader writes into a loaded TEXT image, leaving the code: the value of
/// every patchbay record (the records the image-info header delimits; the first is the GKTS
/// entropy at `marker`), and the tunables buffer the `SNUT`/`ZNUT` records name, when they do.
/// The record tags and lengths stay, so the patchbay's layout is still part of the digest. The
/// values are the boot loader's: entropy, carveout physical addresses below the top of DRAM
/// (they follow the machine's memory size), the SoC revision and the tunables.
pub(crate) fn zero_boot_data(text: &mut [u8], marker: usize, text_va: u64) -> Result<(), BootData> {
    if text.len() < HEADER + HEADER_WORDS * 4 {
        return Err(BootData::Header);
    }
    let hdr = |i: usize| word(text, HEADER + i * 4);
    if hdr(0) != HEADER_BRANCH
        || hdr(1) != HEADER_MAGIC
        || hdr(2) != HEADER_VERSION
        || hdr(13) as usize != text.len()
    {
        return Err(BootData::Header);
    }
    let (start, size) = (hdr(11) as usize, hdr(12) as usize);
    if start != marker || start < HEADER + HEADER_WORDS * 4 {
        return Err(BootData::Patchbay);
    }
    let end = start
        .checked_add(size)
        .filter(|&e| e <= text.len())
        .ok_or(BootData::Patchbay)?;

    let (mut tuns, mut tunz) = (None, None);
    let mut off = start;
    while off < end {
        if end - off < 8 {
            return Err(BootData::Records);
        }
        let tag = [text[off], text[off + 1], text[off + 2], text[off + 3]];
        let len = word(text, off + 4) as usize;
        let value = off + 8;
        if !tag.iter().all(|c| c.is_ascii_graphic()) || len > end - value {
            return Err(BootData::Records);
        }
        let mut raw = [0u8; 8];
        if len <= 8 {
            raw[..len].copy_from_slice(&text[value..value + len]);
        }
        if tag == TUNS && len == 8 {
            tuns = Some(u64::from_le_bytes(raw));
        } else if tag == TUNZ && len == 4 {
            tunz = Some(u64::from_le_bytes(raw));
        }
        text[value..value + len].fill(0);
        off = value + len;
    }

    match (tuns, tunz) {
        (None, None) => Ok(()),
        (Some(va), Some(len)) => {
            let at = va
                .checked_sub(text_va)
                .and_then(|a| usize::try_from(a).ok())
                .filter(|&a| a >= HEADER + HEADER_WORDS * 4)
                .ok_or(BootData::Tunables)?;
            let len = usize::try_from(len).map_err(|_| BootData::Tunables)?;
            let stop = at
                .checked_add(len)
                .filter(|&s| s <= text.len())
                .ok_or(BootData::Tunables)?;
            text[at..stop].fill(0);
            Ok(())
        }
        _ => Err(BootData::Tunables),
    }
}

/// Appended to the refusal of an unknown build: what a user reports so that it can be pinned.
pub(crate) const REPORT_HINT: &str =
    "to have it supported, report this line and the macOS version of the boot chain that loaded it";

/// The identity of a loaded firmware build: segment sizes, marker offset, the TEXT digest with
/// the marker's entropy zeroed and the code digest (TEXT with all boot-loader data zeroed,
/// [`zero_boot_data`]), when the image delimits its boot-loader data.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) struct Measured {
    pub(crate) text_size: u64,
    pub(crate) data_size: u64,
    pub(crate) marker: u32,
    pub(crate) sha256: [u8; 32],
    pub(crate) code_sha256: Option<[u8; 32]>,
}

/// The outcome of identifying a measured build.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum Verdict {
    /// The qualified build at this index.
    Pinned(usize),
    /// Not qualified, admitted for this boot because its digest prefix was named.
    Admitted,
    /// Not qualified and not admitted.
    Refused,
}

/// Offsets of the 8-byte-aligned GKTS markers in `text` that have their 8 entropy bytes in
/// range: `Ok(offset)` for exactly one, `Err(count)` otherwise.
pub(crate) fn marker(text: &[u8]) -> Result<usize, usize> {
    let mut at = 0usize;
    let mut found = 0usize;
    let mut off = 0usize;
    while off + 16 <= text.len() {
        if text[off..off + 8] == MARKER {
            at = off;
            found += 1;
        }
        off += 8;
    }
    if found == 1 {
        Ok(at)
    } else {
        Err(found)
    }
}

/// The first 8 digest bytes, big-endian, as `g15_accept_text_prefix` names them.
pub(crate) fn prefix(sha256: &[u8; 32]) -> u64 {
    let mut first = [0u8; 8];
    first.copy_from_slice(&sha256[..8]);
    u64::from_be_bytes(first)
}

/// Looks `m` up in `qualified`, each build with whether this machine is the one it names; an
/// unqualified build is admitted only when `accept_prefix` is nonzero and equals its digest
/// prefix.
pub(crate) fn verdict(
    m: &Measured,
    qualified: impl IntoIterator<Item = (Measured, bool)>,
    accept_prefix: u64,
) -> Verdict {
    if let Some(i) = qualified.into_iter().position(|(q, machine)| {
        let by = |expected: &[u8; 32], actual: &[u8; 32]| {
            matching::exact(
                (q.text_size, q.data_size),
                (m.text_size, m.data_size),
                q.marker,
                m.marker as usize,
                expected,
                actual,
                machine,
            )
        };
        by(&q.sha256, &m.sha256)
            || matches!((&q.code_sha256, &m.code_sha256), (Some(e), Some(a)) if by(e, a))
    }) {
        return Verdict::Pinned(i);
    }
    if accept_prefix != 0 && accept_prefix == prefix(&m.sha256) {
        Verdict::Admitted
    } else {
        Verdict::Refused
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(n: u8) -> Measured {
        let mut sha256 = [0u8; 32];
        sha256[0] = 0x1b;
        sha256[7] = n;
        sha256[31] = n;
        Measured {
            text_size: 0x64000,
            data_size: 0x140000,
            marker: 0x5cf28,
            sha256,
            code_sha256: None,
        }
    }

    const VA: u64 = 0xfffffc0000000000;

    fn put(text: &mut [u8], off: usize, bytes: &[u8]) -> usize {
        text[off..off + bytes.len()].copy_from_slice(bytes);
        off + bytes.len()
    }

    fn record(text: &mut [u8], off: usize, tag: &[u8; 4], value: &[u8]) -> usize {
        let off = put(text, off, tag);
        let off = put(text, off, &(value.len() as u32).to_le_bytes());
        put(text, off, value)
    }

    /// A TEXT image with a header, a patchbay at 0x4400 (GKTS, a carveout address, the tunables
    /// records) and a tunables buffer right after it; returns it with the patchbay end.
    fn image(tunables: bool) -> (Vec<u8>, usize) {
        let mut text = vec![0xc3u8; 0x8000];
        let start = 0x4400;
        let mut off = record(&mut text, start, b"GKTS", &[0x11; 8]);
        off = record(&mut text, off, b"BPTP", &0x10bfff78000u64.to_le_bytes());
        off = record(&mut text, off, b"LRSD", &[1]);
        let end = if tunables { off + 12 + 16 } else { off };
        if tunables {
            off = record(&mut text, off, b"SNUT", &(VA + end as u64).to_le_bytes());
            off = record(&mut text, off, b"ZNUT", &0x40u32.to_le_bytes());
        }
        assert_eq!(off, end);
        let words = [
            HEADER_BRANCH,
            HEADER_MAGIC,
            HEADER_VERSION,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            start as u32,
            (end - start) as u32,
            text.len() as u32,
        ];
        for (i, w) in words.iter().enumerate() {
            put(&mut text, HEADER + i * 4, &w.to_le_bytes());
        }
        (text, end)
    }

    #[test]
    fn zeroes_record_values_and_tunables_only() {
        let (mut text, end) = image(true);
        let before = text.clone();
        assert_eq!(zero_boot_data(&mut text, 0x4400, VA), Ok(()));
        // Tags and lengths stay; values are zero.
        assert_eq!(&text[0x4400..0x4408], &MARKER);
        assert_eq!(&text[0x4408..0x4410], &[0; 8]);
        assert_eq!(&text[0x4410..0x4414], b"BPTP");
        assert_eq!(&text[0x4418..0x4420], &[0; 8]);
        // The tunables buffer is zero, the code after it is not touched.
        assert!(text[end..end + 0x40].iter().all(|&b| b == 0));
        assert_eq!(text[end + 0x40..], before[end + 0x40..]);
        assert_eq!(text[..0x4400], before[..0x4400]);
    }

    #[test]
    fn boot_data_values_do_not_change_the_code_digest_input() {
        let (mut a, end) = image(true);
        let (mut b, _) = image(true);
        b[0x4408] ^= 0xff; // entropy
        b[0x4419] ^= 0x10; // carveout address
        b[end + 3] ^= 1; // tunables
        zero_boot_data(&mut a, 0x4400, VA).unwrap();
        zero_boot_data(&mut b, 0x4400, VA).unwrap();
        assert_eq!(a, b);
        // A code byte still differs.
        let (mut c, _) = image(true);
        c[0x5000] ^= 1;
        zero_boot_data(&mut c, 0x4400, VA).unwrap();
        assert_ne!(a, c);
    }

    #[test]
    fn a_patchbay_without_tunables_is_accepted() {
        let (mut text, _) = image(false);
        assert_eq!(zero_boot_data(&mut text, 0x4400, VA), Ok(()));
    }

    #[test]
    fn refuses_what_it_cannot_delimit() {
        let (mut text, _) = image(true);
        assert_eq!(
            zero_boot_data(&mut text, 0x4408, VA),
            Err(BootData::Patchbay)
        );
        let (mut text, _) = image(true);
        text[HEADER + 4] ^= 1;
        assert_eq!(zero_boot_data(&mut text, 0x4400, VA), Err(BootData::Header));
        let (mut text, _) = image(true);
        let mut short = text[..0x7000].to_vec();
        assert_eq!(
            zero_boot_data(&mut short, 0x4400, VA),
            Err(BootData::Header)
        );
        // A record running past the patchbay end.
        text[0x4414] = 0x40;
        assert_eq!(
            zero_boot_data(&mut text, 0x4400, VA),
            Err(BootData::Records)
        );
        // A tunables buffer outside TEXT.
        let (mut text, _) = image(true);
        assert_eq!(
            zero_boot_data(&mut text, 0x4400, VA + 0x100000),
            Err(BootData::Tunables)
        );
    }

    #[test]
    fn pins_by_the_code_digest_when_the_full_digest_differs() {
        let mut q = build(1);
        q.code_sha256 = Some([0xc0; 32]);
        let mut m = build(2);
        m.code_sha256 = Some([0xc0; 32]);
        assert_eq!(verdict(&m, [(q, true)], 0), Verdict::Pinned(0));
        // Not on another machine, and not with another code digest or none.
        assert_eq!(verdict(&m, [(q, false)], 0), Verdict::Refused);
        m.code_sha256 = Some([0xc1; 32]);
        assert_eq!(verdict(&m, [(q, true)], 0), Verdict::Refused);
        m.code_sha256 = None;
        assert_eq!(verdict(&m, [(q, true)], 0), Verdict::Refused);
        // A build pinned without a code digest still pins by its full digest only.
        let legacy = build(1);
        let mut m = build(1);
        m.code_sha256 = Some([0xc0; 32]);
        assert_eq!(verdict(&m, [(legacy, true)], 0), Verdict::Pinned(0));
        let mut other = build(2);
        other.code_sha256 = Some([0xc0; 32]);
        assert_eq!(verdict(&other, [(legacy, true)], 0), Verdict::Refused);
    }

    #[test]
    fn finds_one_marker() {
        let mut text = [0u8; 64];
        text[16..24].copy_from_slice(&MARKER);
        assert_eq!(marker(&text), Ok(16));
    }

    #[test]
    fn refuses_none_two_or_misaligned_markers() {
        let mut text = [0u8; 64];
        assert_eq!(marker(&text), Err(0));
        text[12..20].copy_from_slice(&MARKER);
        assert_eq!(marker(&text), Err(0));
        text = [0u8; 64];
        text[8..16].copy_from_slice(&MARKER);
        text[32..40].copy_from_slice(&MARKER);
        assert_eq!(marker(&text), Err(2));
    }

    #[test]
    fn needs_the_entropy_bytes_in_range() {
        let mut text = [0u8; 64];
        text[56..64].copy_from_slice(&MARKER);
        assert_eq!(marker(&text), Err(0));
    }

    #[test]
    fn prefix_is_the_first_eight_bytes_big_endian() {
        let mut sha256 = [0u8; 32];
        sha256[..8].copy_from_slice(&[0x1b, 0xf2, 0xfd, 0x34, 0x76, 0xbe, 0xfe, 0x73]);
        sha256[8] = 0xff;
        assert_eq!(prefix(&sha256), 0x1bf2fd3476befe73);
    }

    #[test]
    fn pins_the_matching_build() {
        assert_eq!(
            verdict(&build(2), [(build(1), true), (build(2), true)], 0),
            Verdict::Pinned(1)
        );
    }

    #[test]
    fn pinned_wins_over_the_prefix() {
        let m = build(1);
        assert_eq!(
            verdict(&m, [(m, true)], prefix(&m.sha256)),
            Verdict::Pinned(0)
        );
    }

    #[test]
    fn every_field_must_match() {
        let q = build(1);
        let mut m = q;
        m.text_size += 0x1000;
        assert_eq!(verdict(&m, [(q, true)], 0), Verdict::Refused);
        m = q;
        m.data_size += 0x1000;
        assert_eq!(verdict(&m, [(q, true)], 0), Verdict::Refused);
        m = q;
        m.marker += 8;
        assert_eq!(verdict(&m, [(q, true)], 0), Verdict::Refused);
        m = q;
        m.sha256[31] ^= 1;
        assert_eq!(verdict(&m, [(q, true)], 0), Verdict::Refused);
    }

    #[test]
    fn another_machines_build_is_not_pinned() {
        let m = build(1);
        assert_eq!(verdict(&m, [(m, false)], 0), Verdict::Refused);
        assert_eq!(verdict(&m, [(m, false), (m, true)], 0), Verdict::Pinned(1));
        // The digest-prefix escape still applies to it, as to any unqualified build.
        assert_eq!(
            verdict(&m, [(m, false)], prefix(&m.sha256)),
            Verdict::Admitted
        );
    }

    #[test]
    fn admits_only_the_named_prefix() {
        let m = build(3);
        assert_eq!(
            verdict(&m, [(build(1), true)], prefix(&m.sha256)),
            Verdict::Admitted
        );
        assert_eq!(
            verdict(&m, [(build(1), true)], prefix(&build(4).sha256)),
            Verdict::Refused
        );
    }

    #[test]
    fn zero_prefix_admits_nothing() {
        let m = Measured {
            sha256: [0u8; 32],
            ..build(0)
        };
        assert_eq!(verdict(&m, [(build(1), true)], 0), Verdict::Refused);
    }

    #[test]
    fn hint_says_what_to_report() {
        assert!(REPORT_HINT.contains("report this line"));
        assert!(REPORT_HINT.contains("macOS version"));
    }
}
