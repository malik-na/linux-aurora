// SPDX-License-Identifier: GPL-2.0-only OR MIT

//! Which firmware render queue a VM's renders run on.
//!
//! With `g15_render_queues` 2 the engine has two render queues ([`super::fw::stamps::RENDERS`]):
//! queue 1, with a parameter buffer of its own, belongs to at most one VM at a time, the
//! *holder*; every other VM renders on queue 0 and the shared PB. A VM takes queue 1 when it
//! publishes a render prefix with nothing published (no prefix of another queue in flight, the
//! engine's flights of one VM only) while queue 1 is free; it keeps it until its client handle
//! is closed and nothing of it is published or pending. So a VM never changes queues while a
//! prefix of it is in flight, and the engine's single total order stays: the queue of the
//! newest flight is the queue of every flight.
//!
//! Core-only (no `kernel` imports): this file is compiled by the host test harness as well.

/// The render queue a prefix is published on.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum Pick {
    /// Queue 0 with the shared PB.
    Shared,
    /// Queue 1, held by the prefix's VM.
    Held,
    /// Queue 1: the prefix's VM takes it now.
    Take,
}

impl Pick {
    /// The render queue index.
    pub(crate) const fn queue(self) -> usize {
        match self {
            Pick::Shared => 0,
            Pick::Held | Pick::Take => 1,
        }
    }
}

/// The queue of a prefix of VM `vm`: `queues` render queues (`g15_render_queues`), queue 1 held
/// by `holder`, `overlap` when prefixes are in flight (then of `vm` only), `renders` when the
/// prefix's first command is a render.
pub(crate) const fn pick(
    queues: u32,
    holder: Option<u64>,
    vm: u64,
    overlap: bool,
    renders: bool,
) -> Pick {
    if queues < 2 {
        return Pick::Shared;
    }
    match holder {
        Some(h) if h == vm => Pick::Held,
        Some(_) => Pick::Shared,
        None if !overlap && renders => Pick::Take,
        None => Pick::Shared,
    }
}

/// Whether the holder gives queue 1 back: its client handle is closed (`closed`) and no prefix
/// or job of it is published or pending (`busy`).
pub(crate) const fn release_due(closed: bool, busy: bool) -> bool {
    closed && !busy
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_queue() {
        // g15_render_queues = 1: queue 0 for everything, nothing is taken.
        for holder in [None, Some(3), Some(4)] {
            for overlap in [false, true] {
                for renders in [false, true] {
                    assert_eq!(pick(1, holder, 3, overlap, renders), Pick::Shared);
                    assert_eq!(pick(0, holder, 3, overlap, renders), Pick::Shared);
                }
            }
        }
    }

    #[test]
    fn take_hold_share() {
        // Free: the first VM that renders with nothing in flight takes it.
        assert_eq!(pick(2, None, 3, false, true), Pick::Take);
        assert_eq!(Pick::Take.queue(), 1);
        // Not behind a flight (its earlier prefixes ran on queue 0), not for a compute prefix.
        assert_eq!(pick(2, None, 3, true, true), Pick::Shared);
        assert_eq!(pick(2, None, 3, false, false), Pick::Shared);
        // The holder keeps it, also behind its own flights and for computes.
        for overlap in [false, true] {
            for renders in [false, true] {
                assert_eq!(pick(2, Some(3), 3, overlap, renders), Pick::Held);
                assert_eq!(pick(2, Some(3), 4, overlap, renders), Pick::Shared);
            }
        }
        assert_eq!(Pick::Held.queue(), 1);
        assert_eq!(Pick::Shared.queue(), 0);
    }

    #[test]
    fn release() {
        assert!(release_due(true, false));
        assert!(!release_due(true, true));
        assert!(!release_due(false, false));
        assert!(!release_due(false, true));
    }

    #[test]
    fn handover() {
        // A holder that closes gives queue 1 to the next VM that renders from idle.
        let mut holder = None;
        let mut q = Vec::new();
        for (vm, overlap, closed_before) in [
            (1, false, false),
            (2, false, false),
            (1, true, false),
            (2, false, true),
            (2, true, false),
        ] {
            if closed_before && release_due(true, false) {
                holder = None;
            }
            let p = pick(2, holder, vm, overlap, true);
            if p == Pick::Take {
                holder = Some(vm);
            }
            q.push(p.queue());
        }
        assert_eq!(q, [1, 0, 1, 1, 1]);
        assert_eq!(holder, Some(2));
    }
}
