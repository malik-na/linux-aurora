// SPDX-License-Identifier: GPL-2.0-only OR MIT

//! Stamp words and queue indices.
//!
//! The stamps of every queue are u32 words of two shared 16 KiB pages with the same layout, a
//! user page and a firmware page: word i is at byte 4i of each. A render queue with index c owns
//! words 2c (TA) and 2c + 1 (FS); a compute queue owns one word h. A queue's word index is also
//! the event byte of its submit records, the producer word of a barrier waiting on its stamp, and
//! the index words of its work images (TA +0x878 = 2c and +0x77c = 2c + 1, FS +0xbc0 = 2c + 1,
//! compute +0x7fc = h). Indices are unique across queue types. Event bytes bound the usable
//! words to the first [`WORDS`].
//!
//! Core-only (no `kernel` imports): this file is compiled by the host test harness as well.

/// Stamp words an index can name (an event byte).
pub(crate) const WORDS: u32 = 0x100;

/// One stamp word index, shared by the user and the firmware page.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Word(u32);

impl Word {
    /// Word `i`, if an event byte can name it.
    pub(crate) const fn new(i: u32) -> Option<Self> {
        if i < WORDS {
            Some(Self(i))
        } else {
            None
        }
    }

    /// The word index (index words of the work images, barrier producer word).
    pub(crate) const fn index(self) -> u32 {
        self.0
    }

    /// Submit record event byte.
    pub(crate) const fn event(self) -> u8 {
        self.0 as u8
    }
}

/// A render queue's index c: its TA owns word 2c, its FS word 2c + 1.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Render(u32);

impl Render {
    /// Render queue `c`, if both of its words can be named.
    pub(crate) const fn new(c: u32) -> Option<Self> {
        if c < WORDS / 2 {
            Some(Self(c))
        } else {
            None
        }
    }

    /// The TA's word, 2c.
    pub(crate) const fn ta(self) -> Word {
        Word(2 * self.0)
    }

    /// The FS's word, 2c + 1.
    pub(crate) const fn fs(self) -> Word {
        Word(2 * self.0 + 1)
    }

    /// Whether word `h` is neither of this queue's words.
    pub(crate) const fn disjoint(self, h: Word) -> bool {
        h.0 / 2 != self.0
    }
}

/// The engine's render queue.
pub(crate) const RENDER: Render = match Render::new(0) {
    Some(r) => r,
    None => panic!(),
};
/// The engine's compute queue.
pub(crate) const COMPUTE: Word = match Word::new(2) {
    Some(h) => h,
    None => panic!(),
};

const _: () = assert!(RENDER.disjoint(COMPUTE));

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_words() {
        for (c, ta, fs) in [(0, 0, 1), (1, 2, 3), (2, 4, 5), (7, 14, 15)] {
            let r = Render::new(c).unwrap();
            assert_eq!((r.ta().index(), r.fs().index()), (ta, fs));
            assert_eq!((r.ta().event(), r.fs().event()), (ta as u8, fs as u8));
        }
    }

    #[test]
    fn compute_word() {
        let h = Word::new(6).unwrap();
        assert_eq!((h.index(), h.event()), (6, 6));
    }

    #[test]
    fn bounds() {
        assert_eq!(Word::new(WORDS - 1).map(Word::event), Some(0xff));
        assert_eq!(Word::new(WORDS), None);
        assert_eq!(
            Render::new(WORDS / 2 - 1).map(|r| r.fs().index()),
            Some(WORDS - 1)
        );
        assert_eq!(Render::new(WORDS / 2), None);
    }

    #[test]
    fn indices_unique() {
        // Distinct render queues never share a word.
        for a in 0..8 {
            for b in 0..8 {
                let (ra, rb) = (Render::new(a).unwrap(), Render::new(b).unwrap());
                assert_eq!(a == b, !rb.disjoint(ra.ta()));
                assert_eq!(a == b, !rb.disjoint(ra.fs()));
            }
        }
        // Compute word 2 is render queue 1's TA word.
        assert!(!Render::new(1).unwrap().disjoint(Word::new(2).unwrap()));
        assert!(RENDER.disjoint(COMPUTE));
        assert_eq!(RENDER, Render::new(0).unwrap());
    }
}
