// SPDX-License-Identifier: GPL-2.0-only OR MIT
//! A client's USC window as a mask of its first 64 pages (bit i: window base + i * 16 KiB), for
//! the T6031 USC window experiment (`asahi.t6031_usc_page`). No kernel types, so the host's
//! `rustc --test` runs the tests in this file directly.

/// Pages covered by the mask.
pub(crate) const PAGES: u32 = 64;
/// One UAT page.
pub(crate) const PAGE: u64 = 0x4000;

/// The pages to fill with zeroed read-only pages, given the pages the client has mapped.
/// Mode 1: the page at +0x4000, if unmapped (boot t78). Mode 2: every unmapped page below the
/// highest mapped one, and the page after it. Any other mode: none.
pub(crate) fn fill_mask(mapped: u64, mode: u64) -> u64 {
    match mode {
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

    /// t78: page 0 mapped, page 1 the guard (+0x4000), pages 2..=17 mapped, page 18 unmapped
    /// (+0x48000, where core 9 faulted next), pages 19..=21 mapped (the program at +0x54000).
    const T78: u64 = 0b1 | (0xffff << 2) | (0b111 << 19);

    fn bits(pages: &[u32]) -> u64 {
        pages.iter().fold(0, |m, p| m | 1 << p)
    }

    #[test]
    fn mode_1_fills_only_the_page_at_0x4000() {
        assert_eq!(fill_mask(T78, 1), bits(&[1]));
        assert_eq!(fill_mask(T78 | 0b10, 1), 0);
    }

    #[test]
    fn mode_2_fills_every_gap_below_the_top_object_and_the_page_after_it() {
        assert_eq!(fill_mask(T78, 2), bits(&[1, 18, 22]));
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
        assert_eq!(fill_mask(T78, 0), 0);
        assert_eq!(fill_mask(T78, 3), 0);
    }

    #[test]
    fn runs_cover_the_window_up_to_the_page_after_the_top_object() {
        let got: Vec<_> = runs(T78).collect();
        assert_eq!(got, [(0, 1, true), (1, 2, false), (2, 18, true), (18, 19, false), (19, 22, true), (22, 23, false)]);
    }

    #[test]
    fn runs_of_a_full_mask_are_one_run_and_of_an_empty_mask_none() {
        assert_eq!(runs(u64::MAX).collect::<Vec<_>>(), [(0, 64, true)]);
        assert_eq!(runs(0).count(), 0);
    }
}
