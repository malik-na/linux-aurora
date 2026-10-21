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

/// Bytes of each stamp page.
pub(crate) const PAGE_SIZE: usize = 0x4000;
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

    /// Byte offset of the word in either page.
    pub(crate) const fn offset(self) -> usize {
        4 * self.0 as usize
    }

    /// Submit record event byte.
    pub(crate) const fn event(self) -> u8 {
        self.0 as u8
    }

    /// The word's bit in a stage event's 64-bit flags (0 for a word above 63).
    pub(crate) const fn stage_bit(self) -> u64 {
        if self.0 < 64 {
            1 << self.0
        } else {
            0
        }
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

    /// The queue's index c, also its PB index (TA +0x1c, InitBM +0x8).
    pub(crate) const fn index(self) -> u32 {
        self.0
    }
}

/// Render queues the engine can run (`g15_render_queues` selects 1 or 2).
pub(crate) const RENDER_QUEUES: usize = 2;

/// The engine's render queue 0: every VM's renders unless one holds queue 1.
pub(crate) const RENDER: Render = match Render::new(0) {
    Some(r) => r,
    None => panic!(),
};
/// The engine's render queue 1: the renders of the VM that holds it.
pub(crate) const RENDER1: Render = match Render::new(1) {
    Some(r) => r,
    None => panic!(),
};
/// The engine's render queues by index.
pub(crate) const RENDERS: [Render; RENDER_QUEUES] = [RENDER, RENDER1];
/// The engine's compute queue: above the words of render queues 0 to 2.
pub(crate) const COMPUTE: Word = match Word::new(6) {
    Some(h) => h,
    None => panic!(),
};

const _: () = assert!(RENDER.disjoint(COMPUTE) && RENDER1.disjoint(COMPUTE));
const _: () = assert!(COMPUTE.stage_bit() != 0 && RENDER1.fs().stage_bit() != 0);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_words() {
        for (c, ta, fs) in [(0, 0, 1), (1, 2, 3), (2, 4, 5), (7, 14, 15)] {
            let r = Render::new(c).unwrap();
            assert_eq!((r.ta().index(), r.fs().index()), (ta, fs));
            assert_eq!(r.ta().offset(), 8 * c as usize);
            assert_eq!(r.fs().offset(), 8 * c as usize + 4);
            assert_eq!((r.ta().event(), r.fs().event()), (ta as u8, fs as u8));
        }
    }

    #[test]
    fn compute_word() {
        let h = Word::new(6).unwrap();
        assert_eq!((h.index(), h.offset(), h.event()), (6, 0x18, 6));
    }

    #[test]
    fn stage_bits() {
        assert_eq!(RENDER.ta().stage_bit(), 1);
        assert_eq!(RENDER.fs().stage_bit(), 2);
        assert_eq!(COMPUTE.stage_bit(), 0x40);
        assert_eq!(Word::new(63).unwrap().stage_bit(), 1 << 63);
        assert_eq!(Word::new(64).unwrap().stage_bit(), 0);
    }

    #[test]
    fn render_queue_1() {
        // Words 2/3: stamp pointers page + 8 / + 0xc, TA +0x878 = 2, +0x77c = FS +0xbc0 = 3,
        // event bytes 2/3, stage bits 2 and 3, PB index 1.
        let r = RENDERS[1];
        assert_eq!(r, RENDER1);
        assert_eq!((r.ta().index(), r.fs().index()), (2, 3));
        assert_eq!((r.ta().offset(), r.fs().offset()), (8, 0xc));
        assert_eq!((r.ta().event(), r.fs().event()), (2, 3));
        assert_eq!((r.ta().stage_bit(), r.fs().stage_bit()), (4, 8));
        assert_eq!((RENDER.index(), RENDER1.index()), (0, 1));
        assert!(RENDER.disjoint(r.ta()) && RENDER.disjoint(r.fs()));
        assert!(r.disjoint(COMPUTE));
        // Every stage bit of the engine's queues is distinct.
        let bits = [
            RENDER.ta().stage_bit(),
            RENDER.fs().stage_bit(),
            r.ta().stage_bit(),
            r.fs().stage_bit(),
            COMPUTE.stage_bit(),
        ];
        assert_eq!(bits.iter().fold(0, |a, b| a | b).count_ones(), 5);
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
        assert!(Word::new(WORDS - 1).unwrap().offset() + 4 <= PAGE_SIZE);
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
        // The compute queue leaves render queues 0 to 2 their words.
        for c in 0..3 {
            assert!(Render::new(c).unwrap().disjoint(COMPUTE));
        }
        assert!(!Render::new(3).unwrap().disjoint(COMPUTE));
    }
}
