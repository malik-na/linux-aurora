// SPDX-License-Identifier: GPL-2.0-only OR MIT
//! A client's USC window as a mask of its first 64 pages (bit i: window base + i * 16 KiB), for
//! the T6031 USC window experiment (`asahi.t6031_usc_page`). No kernel types, so the host's
//! `rustc --test` runs the tests in this file directly.

/// Pages covered by the mask.
pub(crate) const PAGES: u32 = 64;
/// One UAT page.
pub(crate) const PAGE: u64 = 0x4000;

/// The pages to fill with zeroed read-only pages, given the pages the client has mapped.
/// Bits 1:0 of the mode pick the pages: 1 the page at +0x4000, if unmapped (boot t78); 2 every
/// unmapped page below the highest mapped one, and the page after it. Bit 2 only picks the
/// contents ([`tagged`]). Any mode above 7, or with bits 1:0 = 3: none.
pub(crate) fn fill_mask(mapped: u64, mode: u64) -> u64 {
    if mode > 7 {
        return 0;
    }
    match mode & 3 {
        1 => !mapped & 1 << 1,
        2 => match end(mapped) {
            PAGES => !mapped,
            end => !mapped & ((1u64 << end) - 1),
        },
        _ => 0,
    }
}

/// One past the page after the highest mapped page, at most [`PAGES`]; 0 when nothing is mapped.
fn end(mapped: u64) -> u32 {
    match mapped {
        0 => 0,
        m => (PAGES - m.leading_zeros() + 1).min(PAGES),
    }
}

/// Whether a mode fills with the tagged pattern of [`pattern_word`] rather than zeros (bit 2).
pub(crate) fn tagged(mode: u64) -> bool {
    mode <= 7 && mode & 4 != 0
}

/// The pattern word at `offset` from the window base. Both halves name the word's offset, the
/// low half under 0x0100_0000 and the high half under 0x0200_0000, so a read of either half as
/// an address or offset, and a 64-bit read, each point somewhere that tells which word it was.
pub(crate) fn pattern_word(offset: u32) -> u64 {
    u64::from(0x0200_0000 | offset) << 32 | u64::from(0x0100_0000 | offset)
}

/// Fill `out`, the bytes at `offset` from the window base, with [`pattern_word`]s.
pub(crate) fn fill_pattern(offset: u32, out: &mut [u8]) {
    for (i, word) in out.chunks_exact_mut(8).enumerate() {
        word.copy_from_slice(&pattern_word(offset.wrapping_add(i as u32 * 8)).to_le_bytes());
    }
}

/// The pipeline address of the first launch in a CDM stream's leading bytes, stepping over
/// barriers as `m3_client::check_compute_launches` does: `(word1[31:22] << 38) | (word2 << 6)`.
pub(crate) fn first_pipeline(stream: &[u8]) -> Option<u64> {
    let word = |o: usize| stream.get(o..o + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    let mut offset = 0;
    loop {
        let head = word(offset)?;
        match head >> 29 {
            3 => offset += 4,
            0 => return Some(u64::from(word(offset + 4)? >> 22) << 38 | u64::from(word(offset + 8)?) << 6),
            _ => return None,
        }
    }
}

/// The window's runs of mapped and unmapped pages, `(first page, end page, mapped)`, up to
/// and including the page after the highest mapped one.
pub(crate) fn runs(mapped: u64) -> Runs {
    Runs { mapped, page: 0, end: end(mapped) }
}

/// Iterator returned by [`runs`].
pub(crate) struct Runs {
    mapped: u64,
    page: u32,
    end: u32,
}

impl Iterator for Runs {
    type Item = (u32, u32, bool);
    fn next(&mut self) -> Option<Self::Item> {
        if self.page >= self.end {
            return None;
        }
        let at = |page: u32| self.mapped >> page & 1 != 0;
        let (first, mapped) = (self.page, at(self.page));
        while self.page < self.end && at(self.page) == mapped {
            self.page += 1;
        }
        Some((first, self.page, mapped))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A layout shaped like t78's. Observed: page 0 mapped, page 1 (+0x4000) unmapped, pages 2
    /// and 3 mapped, page 18 (+0x48000) unmapped (core 9's next fault), the launch's pipeline
    /// descriptor in page 21 (+0x54000). Assumed: pages 4..=17 and 19..=21 mapped; t78 logged
    /// only the first four pages.
    const LAYOUT: u64 = 0b1 | (0xffff << 2) | (0b111 << 19);

    fn bits(pages: &[u32]) -> u64 {
        pages.iter().fold(0, |m, p| m | 1 << p)
    }

    #[test]
    fn mode_1_fills_only_the_page_at_0x4000() {
        assert_eq!(fill_mask(LAYOUT, 1), bits(&[1]));
        assert_eq!(fill_mask(LAYOUT | 0b10, 1), 0);
    }

    #[test]
    fn mode_2_fills_every_gap_below_the_top_object_and_the_page_after_it() {
        assert_eq!(fill_mask(LAYOUT, 2), bits(&[1, 18, 22]));
    }

    #[test]
    fn mode_2_with_nothing_mapped_fills_nothing() {
        assert_eq!(fill_mask(0, 2), 0);
    }

    #[test]
    fn mode_2_with_the_last_page_mapped_stays_inside_the_mask() {
        assert_eq!(fill_mask(1 << 63, 2), !(1u64 << 63));
        assert_eq!(fill_mask(u64::MAX, 2), 0);
    }

    #[test]
    fn other_modes_fill_nothing() {
        assert_eq!(fill_mask(LAYOUT, 0), 0);
        assert_eq!(fill_mask(LAYOUT, 3), 0);
    }

    #[test]
    fn bit_2_tags_the_fill_without_changing_which_pages() {
        assert_eq!(fill_mask(LAYOUT, 5), bits(&[1]));
        assert_eq!(fill_mask(LAYOUT, 6), bits(&[1, 18, 22]));
        assert!(tagged(5) && tagged(6) && !tagged(1) && !tagged(2));
    }

    #[test]
    fn modes_outside_the_three_bits_fill_nothing_and_tag_nothing() {
        for mode in [3, 7, 8, 9, 0xff, u64::MAX] {
            assert_eq!(fill_mask(LAYOUT, mode), 0, "mode {mode:#x}");
        }
        assert!(!tagged(0xc) && !tagged(u64::MAX));
    }

    #[test]
    fn each_pattern_word_names_its_own_window_offset_in_both_halves() {
        assert_eq!(pattern_word(0x4000), 0x0200_4000_0100_4000);
        assert_eq!(pattern_word(0x7ff8), 0x0200_7ff8_0100_7ff8);
        let mut page = [0u8; 16];
        fill_pattern(0x4000, &mut page);
        assert_eq!(u64::from_le_bytes(page[..8].try_into().unwrap()), 0x0200_4000_0100_4000);
        assert_eq!(u64::from_le_bytes(page[8..].try_into().unwrap()), 0x0200_4008_0100_4008);
    }

    fn stream(words: &[u32]) -> Vec<u8> {
        words.iter().flat_map(|w| w.to_le_bytes()).collect()
    }

    #[test]
    fn the_t77_stream_names_its_pipeline_at_window_base_plus_0x54000() {
        let t77 = stream(&[0x0008_0000, 0x0100_0000, 0x1500, 0x40, 0x1_0000, 1, 1, 0x40, 1, 1, 0x6000_0160, 0x6002_15fe]);
        assert_eq!(first_pipeline(&t77), Some(0x100_0005_4000));
    }

    #[test]
    fn first_pipeline_steps_over_barriers_and_stops_at_other_records() {
        let barrier = 3 << 29;
        assert_eq!(first_pipeline(&stream(&[barrier, 0, 4 << 22, 0x1500])), Some(0x100_0005_4000));
        assert_eq!(first_pipeline(&stream(&[1 << 29, 0, 4 << 22, 0x1500])), None);
        assert_eq!(first_pipeline(&stream(&[0, 4 << 22])), None);
    }

    #[test]
    fn runs_cover_the_window_up_to_the_page_after_the_top_object() {
        let got: Vec<_> = runs(LAYOUT).collect();
        assert_eq!(got, [(0, 1, true), (1, 2, false), (2, 18, true), (18, 19, false), (19, 22, true), (22, 23, false)]);
    }

    #[test]
    fn runs_of_a_full_mask_are_one_run_and_of_an_empty_mask_none() {
        assert_eq!(runs(u64::MAX).collect::<Vec<_>>(), [(0, 64, true)]);
        assert_eq!(runs(0).count(), 0);
    }
}
