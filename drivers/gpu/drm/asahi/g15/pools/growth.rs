// SPDX-License-Identifier: GPL-2.0-only OR MIT

//! The per-PB growth ledger, the shared block pool, request routing and the idle clock.
//!
//! Each VM that renders has a PB of its own (a manager, lists and a [`Ledger`]); the firmware
//! asks for more PB with an event of type 7 naming the PB's slot (the word the host wrote at
//! InitBM +0x8: the VM's context), the context, its request counter (P+0x60 before its
//! increment), the TA position and the halt epoch. [`route`] picks the PB from the slot and
//! context words. The host appends at most ten blocks to that PB's block list and advances its
//! block_ctl+4 producer (modulo K), then replies with DEVCTL 7. At the PB's maximum the reply
//! refuses, and the firmware finishes the scene with partial renders. A request from before a
//! halt (epoch) or not matching a published render prefix of its VM is left unanswered.
//!
//! The blocks come from one [`BlockPool`] shared by every PB: chunks of [`CHUNK_BLOCKS`] blocks
//! (16 MiB) backed on demand, a run of up to ten contiguous free blocks per grant, and a reserve
//! of [`RESERVE_BLOCKS`] free blocks kept ahead of demand so the reply path never waits for an
//! allocation; only a reserve that ran dry makes a request wait ([`Answer::Wait`]). A PB's
//! re-initialization ([`Ledger::reinit`]) returns its posted count to the initial size; its
//! blocks go back to the pool, and chunks wholly free above the reserve may be unbacked.
//!
//! Core-only (no `kernel` imports): this file is compiled by the host test harness as well.

use crate::g15::fw::buffer::{
    grow_request, BLOCK_BYTES, BLOCK_LIST_RECORDS, GROW_STEP, MAX_BLOCKS, MIN_BLOCKS,
};

/// Blocks per backing chunk (16 MiB).
pub(crate) const CHUNK_BLOCKS: u32 = 128;
/// Free blocks kept in the pool ahead of demand: at 1-2 ms per request of ten blocks, 13
/// requests of lead for the allocation of the next chunk.
pub(crate) const RESERVE_BLOCKS: u32 = CHUNK_BLOCKS;
/// Chunks the pool may back at most (1 GiB): a bound, not a reservation.
pub(crate) const POOL_CHUNKS: u32 = 64;
/// Blocks of the whole pool.
pub(crate) const POOL_BLOCKS: u32 = POOL_CHUNKS * CHUNK_BLOCKS;

/// The answer to an admitted growth request.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum Answer {
    /// Post `count` blocks (at most ten) as PB blocks `[first, first + count)` and reply grown.
    Grant { first: u32, count: u32 },
    /// Reply refused: the maximum is posted.
    Refuse,
    /// No free block in the pool: leave the request queued and back a chunk.
    Wait,
}

/// One PB's ledger since its last (re)initialization.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) struct Ledger {
    /// Initial blocks (the C1 pages, InitBM blocks and TA register #33 name them).
    pub(crate) initial: u32,
    /// Maximum blocks (P+0x70/+0x74 = 4 * max pages).
    pub(crate) max: u32,
    /// Blocks posted to the block list, cumulative (the block_ctl+4 producer is this modulo K).
    pub(crate) posted: u32,
    /// Renders of this PB since its InitBM: the value of its counter record (manager +0x58),
    /// which counts the PB's own renders, not the engine's draws (the native manager's words
    /// +0x0/+0x10 follow this counter one per TA kick).
    pub(crate) renders: u32,
    /// The next expected request counter.
    pub(crate) next_request: u32,
    /// A refusal at the maximum was logged since the last initialization.
    pub(crate) refusal_logged: bool,
    /// Refusals answered at the maximum since the last initialization. After
    /// [`MAX_REFUSALS`] further requests are left unanswered: a firmware that keeps asking at
    /// the maximum instead of finishing the scene with partial renders would otherwise be
    /// answered forever, under the device lock (ticket malik-na/linux-aurora#69); unanswered,
    /// its TA stalls and the host deadline recovers.
    pub(crate) refusals: u32,
}

/// Refusals answered at the maximum before requests are left to the deadline.
pub(crate) const MAX_REFUSALS: u32 = 2;

impl Ledger {
    /// A fresh ledger. With growth, the initial size is clamped to 64..=4096 blocks and the
    /// maximum to initial..=4096 (the caller passes 4095); without, both are the whole range.
    pub(crate) const fn new(grow: bool, blocks: u32, max_blocks: u32) -> Self {
        let (initial, max) = if grow {
            let nb = clamp(blocks, MIN_BLOCKS, MAX_BLOCKS);
            (nb, clamp(max_blocks, nb, MAX_BLOCKS))
        } else {
            (MAX_BLOCKS, MAX_BLOCKS)
        };
        Self {
            initial,
            max,
            posted: initial,
            renders: 0,
            next_request: 0,
            refusal_logged: false,
            refusals: 0,
        }
    }

    /// A re-initialization: the initial blocks are posted again, the request and render
    /// counters start over.
    pub(crate) const fn reinit(&mut self) {
        self.posted = self.initial;
        self.renders = 0;
        self.next_request = 0;
        self.refusal_logged = false;
        self.refusals = 0;
    }

    /// Counts `n` renders of this PB published in one prefix and returns the counter record's
    /// new value.
    pub(crate) const fn render(&mut self, n: u32) -> u32 {
        self.renders = self.renders.wrapping_add(n);
        self.renders
    }

    /// The live block_ctl+4 producer.
    pub(crate) const fn producer(&self) -> u32 {
        self.posted % BLOCK_LIST_RECORDS
    }

    /// Whether the refusals at the maximum are used up: further requests go unanswered.
    pub(crate) const fn exhausted(&self) -> bool {
        self.refusals >= MAX_REFUSALS
    }

    /// Whether `request` may be answered by this PB: growth on, an event of type 7 routed here
    /// ([`route`]), from a context with a published render prefix (`live`), a counter not below
    /// the next expected one, the current halt epoch, and refusals at the maximum not used up.
    pub(crate) fn admits(&self, request: &[u32; 14], grow: bool, live: bool, epoch: u64) -> bool {
        !self.exhausted()
            && grow
            && request[grow_request::TYPE] == 7
            && live
            && request[grow_request::COUNTER] >= self.next_request
            && request_epoch(request) == epoch
    }

    /// Accepts an admitted request with `free` blocks available in the pool. Granted or
    /// refused, the next expected counter follows it; a request that waits for backing is
    /// accepted again when the pool has a block.
    pub(crate) fn accept(&mut self, request: &[u32; 14], free: u32) -> Answer {
        let room = self.max - self.posted;
        if room != 0 && free == 0 {
            return Answer::Wait;
        }
        self.next_request = request[grow_request::COUNTER].wrapping_add(1);
        if room == 0 {
            self.refusals += 1;
            return Answer::Refuse;
        }
        Answer::Grant {
            first: self.posted,
            count: GROW_STEP.min(room).min(free),
        }
    }

    /// Records `count` blocks as posted after their records were written and returns the new
    /// producer.
    pub(crate) fn post(&mut self, count: u32) -> u32 {
        self.posted += count;
        self.producer()
    }

    /// Whether the answer is logged at info level: a failed reply, the first two requests, or
    /// the first refusal at the maximum since the initialization. Records the refusal.
    pub(crate) fn log_answer(
        &mut self,
        request: &[u32; 14],
        count: u32,
        reply_failed: bool,
    ) -> bool {
        let info = reply_failed
            || request[grow_request::COUNTER] < 2
            || (count == 0 && self.posted == self.max && !self.refusal_logged);
        self.refusal_logged |= count == 0;
        info
    }

    /// Whether this PB is to be re-initialized: its VM idle for `idle_ns` with more than
    /// `floor` blocks posted.
    pub(crate) const fn reclaim_due(&self, floor: u32, idle_ns: u64, min_idle_ns: u64) -> bool {
        self.posted > floor && idle_ns >= min_idle_ns
    }
}

/// The PB a growth request names: its slot word (the context the host wrote at InitBM +0x8),
/// or the context word if the firmware reports slot 0 for every PB; a slot that names another
/// context than the request's is nobody's.
pub(crate) const fn route(request: &[u32; 14]) -> Option<u32> {
    let slot = request[grow_request::SLOT];
    let ctx = request[grow_request::CONTEXT];
    if slot == 0 || slot == ctx {
        Some(ctx)
    } else {
        None
    }
}

/// The shared block pool: which blocks of the [`POOL_CHUNKS`] chunks are backed, and which of
/// the backed ones are posted to some PB. Block `b` is block `b % CHUNK_BLOCKS` of chunk
/// `b / CHUNK_BLOCKS`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BlockPool {
    /// Backed chunks (bit `c`).
    chunks: u64,
    /// Used blocks (bit `b`).
    used: [u64; POOL_BLOCKS as usize / 64],
}

impl BlockPool {
    pub(crate) const fn new() -> Self {
        Self {
            chunks: 0,
            used: [0; POOL_BLOCKS as usize / 64],
        }
    }

    /// Backed chunks.
    pub(crate) const fn chunks(&self) -> u32 {
        self.chunks.count_ones()
    }

    /// Whether chunk `c` is backed.
    pub(crate) const fn backed(&self, c: u32) -> bool {
        c < POOL_CHUNKS && self.chunks & (1 << c) != 0
    }

    /// The lowest unbacked chunk index, if any.
    pub(crate) const fn next_chunk(&self) -> Option<u32> {
        let c = (!self.chunks).trailing_zeros();
        if c < POOL_CHUNKS {
            Some(c)
        } else {
            None
        }
    }

    /// Marks chunk `c` backed, all its blocks free.
    pub(crate) fn back_chunk(&mut self, c: u32) {
        self.chunks |= 1 << c;
        for w in self.words(c) {
            self.used[w] = 0;
        }
    }

    /// Unbacks chunk `c` if it is wholly free; false if a block of it is in use.
    pub(crate) fn unback_chunk(&mut self, c: u32) -> bool {
        if !self.chunk_free(c) {
            return false;
        }
        self.chunks &= !(1 << c);
        true
    }

    /// Whether backed chunk `c` has no block in use.
    pub(crate) fn chunk_free(&self, c: u32) -> bool {
        self.backed(c) && self.words(c).all(|w| self.used[w] == 0)
    }

    fn words(&self, c: u32) -> core::ops::Range<usize> {
        let first = (c * CHUNK_BLOCKS) as usize / 64;
        first..first + CHUNK_BLOCKS as usize / 64
    }

    /// Blocks in use.
    pub(crate) fn used_blocks(&self) -> u32 {
        self.used.iter().map(|w| w.count_ones()).sum()
    }

    /// Backed blocks not in use.
    pub(crate) fn free_blocks(&self) -> u32 {
        self.chunks() * CHUNK_BLOCKS - self.used_blocks()
    }

    /// Whether another chunk is due: fewer than the reserve free and the pool not at its bound.
    pub(crate) fn refill_due(&self) -> bool {
        self.free_blocks() < RESERVE_BLOCKS && self.chunks() < POOL_CHUNKS
    }

    /// Takes the lowest run of up to `max` contiguous free blocks within one chunk: the pool
    /// block index of its first block and its length. None when nothing is free.
    pub(crate) fn take_run(&mut self, max: u32) -> Option<(u32, u32)> {
        let max = max.max(1);
        let mut best: Option<(u32, u32)> = None;
        for c in 0..POOL_CHUNKS {
            if !self.backed(c) {
                continue;
            }
            let base = c * CHUNK_BLOCKS;
            let mut b = 0;
            while b < CHUNK_BLOCKS {
                if self.is_used(base + b) {
                    b += 1;
                    continue;
                }
                let start = b;
                while b < CHUNK_BLOCKS && b - start < max && !self.is_used(base + b) {
                    b += 1;
                }
                let len = b - start;
                if len == max {
                    self.mark(base + start, len, true);
                    return Some((base + start, len));
                }
                if best.is_none_or(|(_, l)| len > l) {
                    best = Some((base + start, len));
                }
            }
        }
        let (first, len) = best?;
        self.mark(first, len, true);
        Some((first, len))
    }

    /// Returns the run `[first, first + len)` to the pool.
    pub(crate) fn free_run(&mut self, first: u32, len: u32) {
        self.mark(first, len, false);
    }

    /// Chunks the pool needs for its used blocks and the reserve.
    pub(crate) fn chunks_needed(&self) -> u32 {
        (self.used_blocks() + RESERVE_BLOCKS).div_ceil(CHUNK_BLOCKS)
    }

    /// A wholly free chunk above what the used blocks and the reserve need (the highest), to
    /// be unbacked.
    pub(crate) fn releasable(&self) -> Option<u32> {
        if self.chunks() <= self.chunks_needed() {
            return None;
        }
        (0..POOL_CHUNKS).rev().find(|&c| self.chunk_free(c))
    }

    const fn is_used(&self, b: u32) -> bool {
        self.used[b as usize / 64] & (1 << (b % 64)) != 0
    }

    fn mark(&mut self, first: u32, len: u32, used: bool) {
        for b in first..first + len {
            let bit = 1u64 << (b % 64);
            if used {
                self.used[b as usize / 64] |= bit;
            } else {
                self.used[b as usize / 64] &= !bit;
            }
        }
    }
}

/// A PB's idle clock for the reclaim policy: when its VM had nothing published and nothing
/// pending, from the pass that retired its last flight on. Noted at the end of every poll pass
/// and publication (a pass that empties the flights is not re-armed, so a note only at the
/// start of a pass with empty flights depends on a backup pass happening to run) and, busy,
/// when a job is enqueued.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct IdleClock {
    /// When the idle began; 0 while busy.
    since_ns: u64,
}

impl IdleClock {
    /// Notes the VM idle or busy at `now_ns`; an idle already begun keeps its start.
    pub(crate) fn note(&mut self, idle: bool, now_ns: u64) {
        if !idle {
            self.since_ns = 0;
        } else if self.since_ns == 0 {
            self.since_ns = now_ns.max(1);
        }
    }

    /// The idle time at `now_ns`; 0 while busy.
    pub(crate) fn idle_ns(&self, now_ns: u64) -> u64 {
        if self.since_ns == 0 {
            0
        } else {
            now_ns.wrapping_sub(self.since_ns)
        }
    }
}

/// A dead PB's grave: its blocks stay out of the pool, its lists and manager untouched, until
/// `grace_ns` after the VM's death (the native driver frees a torn-down queue's lists and
/// blocks within about 30 s and keeps the manager image mapped).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) struct Grave {
    pub(crate) death_ns: u64,
}

impl Grave {
    /// Whether the grace period has passed at `now_ns`.
    pub(crate) const fn due(&self, now_ns: u64, grace_ns: u64) -> bool {
        now_ns.wrapping_sub(self.death_ns) >= grace_ns
    }
}

/// Where a granted run of pool blocks is mapped: chunk `chunk`, bytes `offset..offset + bytes`
/// of it, at the owning VM's heap GVAs `gva..gva + bytes` (heap block `heap_first` on). Computed
/// before the run's block records are written and the producer advanced, which precede the
/// reply: the firmware never sees a block whose pages are not mapped in its VM.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) struct RunMap {
    pub(crate) chunk: u32,
    pub(crate) offset: usize,
    pub(crate) bytes: usize,
    pub(crate) gva: u64,
}

impl RunMap {
    /// The mapping of pool run `[pool_first, pool_first + len)` as heap blocks
    /// `[heap_first, heap_first + len)` of a PB whose heap range starts at `heap`.
    pub(crate) const fn new(heap: u64, heap_first: u32, pool_first: u32, len: u32) -> Self {
        Self {
            chunk: pool_first / CHUNK_BLOCKS,
            offset: (pool_first % CHUNK_BLOCKS) as usize * BLOCK_BYTES as usize,
            bytes: len as usize * BLOCK_BYTES as usize,
            gva: heap + heap_first as u64 * BLOCK_BYTES,
        }
    }
}

/// The per-queue index words of a render's TA work. Natively the two queues of one context
/// carry 0/0/0 and 1/1/2 at +0x1c, register 0x1c830 (low word) and +0x878, and 1 and 3 at
/// +0x77c: an index `q` per queue, with `2q` and `2q + 1` where the queue's pair of stamp
/// cells is named. One PB per VM here: `q` is the PB's queue index.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) struct QueueWords {
    /// TA work +0x1c.
    pub(crate) index: u32,
    /// TA work +0x77c.
    pub(crate) odd: u32,
    /// TA work +0x878.
    pub(crate) even: u32,
}

impl QueueWords {
    pub(crate) const fn new(q: u32) -> Self {
        Self {
            index: q,
            odd: 2 * q + 1,
            even: 2 * q,
        }
    }
}

/// Queue indices of the live PBs: the lowest free one is taken at a PB's creation and given
/// back when the PB is buried, so a few concurrent PBs use the small indices the native driver
/// uses.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct QueueIndices {
    used: u64,
}

impl QueueIndices {
    /// Takes the lowest free index (0..64); None when all are taken.
    pub(crate) fn take(&mut self) -> Option<u32> {
        let q = (!self.used).trailing_zeros();
        if q >= 64 {
            return None;
        }
        self.used |= 1 << q;
        Some(q)
    }

    /// Gives index `q` back.
    pub(crate) fn give(&mut self, q: u32) {
        if q < 64 {
            self.used &= !(1 << q);
        }
    }
}

/// Blocks the graveyard may hold at most (one chunk): beyond it the oldest graves are buried at
/// once, so the memory of dead PBs stays bounded whatever the VM churn.
pub(crate) const GRAVE_BLOCKS: u32 = CHUNK_BLOCKS;

/// The graveyard's block account.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct GraveLedger {
    pub(crate) blocks: u32,
}

impl GraveLedger {
    /// Adds a grave of `blocks`.
    pub(crate) const fn add(&mut self, blocks: u32) {
        self.blocks += blocks;
    }

    /// Removes a buried grave of `blocks`.
    pub(crate) const fn remove(&mut self, blocks: u32) {
        self.blocks = self.blocks.saturating_sub(blocks);
    }

    /// Whether graves must be buried now to stay within [`GRAVE_BLOCKS`].
    pub(crate) const fn over_cap(&self) -> bool {
        self.blocks > GRAVE_BLOCKS
    }
}

/// The halt epoch a request names (u64 at words 5..6).
pub(crate) const fn request_epoch(request: &[u32; 14]) -> u64 {
    request[grow_request::EPOCH] as u64 | (request[grow_request::EPOCH + 1] as u64) << 32
}

const fn clamp(v: u32, lo: u32, hi: u32) -> u32 {
    if v < lo {
        lo
    } else if v > hi {
        hi
    } else {
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(ctx: u32, counter: u32, epoch: u64) -> [u32; 14] {
        request_slot(ctx, ctx, counter, epoch)
    }

    fn request_slot(slot: u32, ctx: u32, counter: u32, epoch: u64) -> [u32; 14] {
        let mut q = [0u32; 14];
        q[0] = 7;
        q[1] = ctx;
        q[2] = slot;
        q[3] = counter;
        q[4] = 0x100;
        q[5] = epoch as u32;
        q[6] = (epoch >> 32) as u32;
        q
    }

    #[test]
    fn clamps() {
        let l = Ledger::new(true, 1024, 2048);
        assert_eq!(
            (l.initial, l.max, l.posted, l.next_request),
            (1024, 2048, 1024, 0)
        );
        assert_eq!(l.producer(), 1024);
        assert_eq!(Ledger::new(true, 10, 20).initial, 64);
        assert_eq!(Ledger::new(true, 10, 20).max, 64);
        assert_eq!(Ledger::new(true, 5000, 100).initial, 4096);
        assert_eq!(Ledger::new(true, 512, 1024).max, 1024);
        assert_eq!(Ledger::new(true, 512, 100).max, 512);
        let fixed = Ledger::new(false, 512, 1024);
        assert_eq!((fixed.initial, fixed.max, fixed.posted), (4096, 4096, 4096));
    }

    #[test]
    fn admission_and_routing() {
        let l = Ledger::new(true, 1024, 2048);
        let q = request(5, 0, 3);
        assert!(l.admits(&q, true, true, 3));
        assert!(!l.admits(&q, false, true, 3));
        assert!(!l.admits(&q, true, false, 3));
        // Request of an earlier halt epoch (dropped at recovery, never answered).
        assert!(!l.admits(&q, true, true, 4));
        let mut bad = q;
        bad[0] = 6;
        assert!(!l.admits(&bad, true, true, 3));
        // 64-bit epochs.
        let q = request(5, 0, 0x1_0000_0002);
        assert!(l.admits(&q, true, true, 0x1_0000_0002));
        assert!(!l.admits(&q, true, true, 2));
        // Routing: the slot word names the PB (= the context); slot 0 (a firmware that does
        // not echo InitBM +0x8) falls back to the context; a slot of another context is
        // nobody's.
        assert_eq!(route(&request_slot(5, 5, 0, 0)), Some(5));
        assert_eq!(route(&request_slot(0, 7, 0, 0)), Some(7));
        assert_eq!(route(&request_slot(5, 7, 0, 0)), None);
        assert_eq!(route(&request_slot(0, 0, 0, 0)), Some(0));
    }

    #[test]
    fn growth_to_the_maximum() {
        // 512 initial and 1024 maximum blocks, the pool never short.
        let mut l = Ledger::new(true, 512, 1024);
        let mut grown = 0;
        let mut counter = 0;
        loop {
            let q = request(5, counter, 1);
            assert!(l.admits(&q, true, true, 1));
            let n = match l.accept(&q, POOL_BLOCKS) {
                Answer::Grant { first, count } => {
                    assert_eq!(first, 512 + grown);
                    count
                }
                Answer::Refuse => break,
                Answer::Wait => panic!("pool never short"),
            };
            assert!(n <= 10);
            assert_eq!(l.post(n), (512 + grown + n) % 4096);
            grown += n;
            counter += 1;
        }
        assert_eq!(l.posted, 1024);
        assert_eq!(grown, 512);
        // 51 requests of 10 blocks and one of 2.
        assert_eq!(counter, 52);
        // A stale (repeated) counter is refused, a later one accepted.
        assert!(!l.admits(&request(5, counter - 1, 1), true, true, 1));
        assert!(l.admits(&request(5, counter + 3, 1), true, true, 1));
        // One refusal was answered above; one more is, then requests go unanswered.
        assert_eq!(l.refusals, 1);
        assert!(!l.exhausted());
        let q = request(5, counter + 3, 1);
        assert_eq!(l.accept(&q, POOL_BLOCKS), Answer::Refuse);
        assert!(l.exhausted());
        assert!(!l.admits(&request(5, counter + 4, 1), true, true, 1));
        // A fresh ledger starts over, and so does a re-initialized one.
        assert!(!Ledger::new(true, 512, 1024).exhausted());
        l.reinit();
        assert_eq!((l.posted, l.next_request, l.refusals), (512, 0, 0));
        assert!(!l.exhausted() && !l.refusal_logged);
        // An empty pool makes a request wait without consuming its counter; a short pool
        // clips the grant.
        let q = request(5, 0, 1);
        assert_eq!(l.accept(&q, 0), Answer::Wait);
        assert_eq!(l.next_request, 0);
        assert_eq!(
            l.accept(&q, 3),
            Answer::Grant {
                first: 512,
                count: 3
            }
        );
    }

    #[test]
    fn render_counter_per_pb() {
        // Two PBs render interleaved: each counter follows its own renders, not the draws.
        let mut a = Ledger::new(true, 64, 4095);
        let mut b = Ledger::new(true, 64, 4095);
        assert_eq!((a.renders, b.renders), (0, 0));
        let mut draw = 0;
        for _ in 0..80 {
            draw += 1;
            assert_eq!(a.render(1), draw);
        }
        draw += 1;
        assert_eq!(b.render(1), 1);
        draw += 1;
        assert_eq!(a.render(1), 81);
        assert_eq!(draw, 82);
        // A prefix of three renders counts three; a re-initialization starts over.
        assert_eq!(b.render(3), 4);
        b.reinit();
        assert_eq!(b.renders, 0);
        assert_eq!(b.render(1), 1);
    }

    /// A grant's mapping: the run's pages map at the PB's next heap GVAs (the ledger's posted
    /// count), inside the heap range, within one chunk.
    #[test]
    fn grant_mapping() {
        let heap = 0x70c0000000u64;
        let mut l = Ledger::new(true, 64, 4095);
        let mut p = BlockPool::new();
        p.back_chunk(0);
        p.back_chunk(1);
        // The initial 64 blocks of another PB took pool blocks 0..64; ours takes the next.
        p.mark(0, 64, true);
        let mut posted_runs = alloc::vec::Vec::new();
        let mut want = 64;
        while want > 0 {
            let (f, n) = p.take_run(want.min(10)).unwrap();
            let heap_first = posted_runs
                .iter()
                .map(|m: &RunMap| m.bytes as u32 / 0x20000)
                .sum::<u32>();
            posted_runs.push(RunMap::new(heap, heap_first, f, n));
            want -= n;
        }
        assert_eq!(posted_runs[0], RunMap::new(heap, 0, 64, 10));
        assert_eq!(posted_runs[0].gva, heap);
        assert_eq!(posted_runs[0].offset, 64 * 0x20000);
        assert_eq!(posted_runs[6].gva, heap + 60 * 0x20000);
        // A grant after the initial posting: heap block 64 = the ledger's posted count, and
        // the run sits in chunk 1 when chunk 0 is full.
        p.mark(74, 54, true);
        let q = request(5, 0, 1);
        let (first, count) = match l.accept(&q, p.free_blocks()) {
            Answer::Grant { first, count } => (first, count),
            other => panic!("{other:?}"),
        };
        assert_eq!((first, count), (64, 10));
        let (f, n) = p.take_run(count).unwrap();
        let m = RunMap::new(heap, first, f, n);
        assert_eq!(m.chunk, 1);
        assert_eq!(m.offset, 0);
        assert_eq!(m.gva, heap + 64 * 0x20000);
        assert_eq!(m.bytes, 10 * 0x20000);
        assert!(m.gva + m.bytes as u64 <= heap + MAX_BLOCKS as u64 * BLOCK_BYTES);
        assert!(m.offset + m.bytes <= CHUNK_BLOCKS as usize * BLOCK_BYTES as usize);
        assert_eq!(l.post(n), 74);
    }

    #[test]
    fn queue_words_and_indices() {
        // The native pairs: queue 0 = 0/1/0, queue 1 = 1/3/2.
        assert_eq!(
            QueueWords::new(0),
            QueueWords {
                index: 0,
                odd: 1,
                even: 0
            }
        );
        assert_eq!(
            QueueWords::new(1),
            QueueWords {
                index: 1,
                odd: 3,
                even: 2
            }
        );
        assert_eq!(QueueWords::new(5).even, 10);
        // Indices: lowest free first, reused after being given back, 64 at most.
        let mut qi = QueueIndices::default();
        assert_eq!(qi.take(), Some(0));
        assert_eq!(qi.take(), Some(1));
        assert_eq!(qi.take(), Some(2));
        qi.give(1);
        assert_eq!(qi.take(), Some(1));
        for _ in 3..64 {
            assert!(qi.take().is_some());
        }
        assert_eq!(qi.take(), None);
        qi.give(63);
        assert_eq!(qi.take(), Some(63));
    }

    #[test]
    fn grave() {
        let s = 1_000_000_000;
        let g = Grave { death_ns: 100 * s };
        assert!(!g.due(100 * s, 10 * s));
        assert!(!g.due(109 * s, 10 * s));
        assert!(g.due(110 * s, 10 * s));
        assert!(g.due(u64::MAX, 10 * s));
        // The graveyard holds at most one chunk of blocks: two dead 64-block PBs fit, the
        // third pushes it over the cap until one is buried.
        let mut l = GraveLedger::default();
        l.add(64);
        l.add(64);
        assert!(!l.over_cap());
        l.add(64);
        assert!(l.over_cap());
        l.remove(64);
        assert!(!l.over_cap());
        l.remove(1000);
        assert_eq!(l.blocks, 0);
    }

    #[test]
    fn producer_wraps_at_k() {
        let mut l = Ledger::new(true, 2048, 2048);
        assert_eq!(l.producer(), 2048);
        l.posted = 4095;
        assert_eq!(l.post(1), 0);
        assert_eq!(l.post(10), 10);
    }

    #[test]
    fn pool_accounting_across_two_pbs() {
        let mut p = BlockPool::new();
        assert_eq!(
            (p.chunks(), p.free_blocks(), p.next_chunk()),
            (0, 0, Some(0))
        );
        assert!(p.refill_due());
        assert_eq!(p.take_run(10), None);
        // Boot: two chunks.
        p.back_chunk(0);
        p.back_chunk(1);
        assert_eq!(
            (p.chunks(), p.free_blocks(), p.next_chunk()),
            (2, 256, Some(2))
        );
        assert!(!p.refill_due());
        // VM A's initial posting: 64 blocks as runs of ten (and one of four), contiguous.
        let mut a = alloc::vec::Vec::new();
        let mut want = 64;
        while want > 0 {
            let (first, len) = p.take_run(want.min(10)).unwrap();
            a.push((first, len));
            want -= len;
        }
        assert_eq!(a[0], (0, 10));
        assert_eq!(a[6], (60, 4));
        assert_eq!((p.used_blocks(), p.free_blocks()), (64, 192));
        // VM B's initial posting takes the next 64.
        let mut b = alloc::vec::Vec::new();
        for _ in 0..7 {
            b.push(p.take_run(10).unwrap());
        }
        assert_eq!(b[0], (64, 10));
        // A run never crosses a chunk: the seventh run skips blocks 124..128 for chunk 1.
        assert_eq!(b[6], (128, 10));
        assert_eq!(p.free_blocks(), 122);
        assert!(p.refill_due());
        // A's blocks go back (a reclaim or its destruction): the holes are reused first, and
        // neither chunk is wholly free while B's runs are in both.
        for (first, len) in a.iter() {
            p.free_run(*first, *len);
        }
        assert_eq!(p.free_blocks(), 186);
        assert_eq!(p.take_run(10), Some((0, 10)));
        p.free_run(0, 10);
        assert!(!p.chunk_free(0));
        assert!(!p.chunk_free(1));
        assert_eq!(p.releasable(), None);
        // B gone too: both chunks free; one above the reserve's need is releasable.
        for (first, len) in b.iter() {
            p.free_run(*first, *len);
        }
        assert_eq!(p.used_blocks(), 0);
        assert_eq!(p.chunks_needed(), 1);
        assert_eq!(p.releasable(), Some(1));
        assert!(p.unback_chunk(1));
        assert_eq!((p.chunks(), p.free_blocks()), (1, 128));
        assert_eq!(p.releasable(), None);
        // A run never crosses a chunk: with 3 free at the end of chunk 0 and chunk 2 backed,
        // the request of ten gets ten from chunk 2.
        p.back_chunk(2);
        p.mark(0, 125, true);
        assert_eq!(p.take_run(10), Some((256, 10)));
        // With no full run anywhere the longest run is granted.
        p.mark(256 + 10, 128 - 10, true);
        assert_eq!(p.take_run(10), Some((125, 3)));
        assert_eq!(p.take_run(10), None);
        assert!(!p.unback_chunk(2));
        // The bound.
        for c in 0..POOL_CHUNKS {
            if !p.backed(c) {
                p.back_chunk(c);
            }
        }
        assert_eq!(p.next_chunk(), None);
        assert!(!p.refill_due());
    }

    #[test]
    fn reclaim() {
        let mut l = Ledger::new(true, 64, 4095);
        let s = 1_000_000_000;
        assert!(!l.reclaim_due(1024, 5 * s, s));
        l.posted = 1024;
        assert!(!l.reclaim_due(1024, 5 * s, s));
        l.posted = 1025;
        assert!(l.reclaim_due(1024, s, s));
        assert!(!l.reclaim_due(1024, s - 1, s));
        // The clock: busy is 0, an idle keeps its start, busy ends it.
        let mut c = IdleClock::default();
        assert_eq!(c.idle_ns(10 * s), 0);
        c.note(true, 10 * s);
        c.note(true, 12 * s);
        assert_eq!(c.idle_ns(15 * s), 5 * s);
        c.note(false, 16 * s);
        assert_eq!(c.idle_ns(20 * s), 0);
        // A clock started at 0 ns still counts as idle.
        c.note(true, 0);
        assert_eq!(c.idle_ns(s), s - 1);
    }

    /// Two PBs over one pool: a heavy VM grows while a light one renders on; the heavy VM's
    /// idle reclaim returns its blocks and frees chunks while the light VM keeps its blocks;
    /// the heavy VM's next scene is served from the reclaimed blocks.
    #[test]
    fn reclaim_of_one_pb_while_another_renders() {
        let s = 1_000_000_000;
        let (floor, min_idle) = (1024, s);
        let mut p = BlockPool::new();
        p.back_chunk(0);
        p.back_chunk(1);
        let take = |p: &mut BlockPool, want: u32| -> alloc::vec::Vec<(u32, u32)> {
            let mut runs = alloc::vec::Vec::new();
            let mut left = want;
            while left > 0 {
                while p.refill_due() {
                    let c = p.next_chunk().unwrap();
                    p.back_chunk(c);
                }
                let (f, l) = p.take_run(left.min(10)).unwrap();
                runs.push((f, l));
                left -= l;
            }
            runs
        };
        let mut heavy = Ledger::new(true, 64, 4095);
        let light = Ledger::new(true, 64, 4095);
        let mut heavy_runs = take(&mut p, 64);
        let light_runs = take(&mut p, 64);
        let mut heavy_clock = IdleClock::default();
        let mut light_clock = IdleClock::default();
        // The heavy scene: grows to 1484 while the light VM renders (busy).
        heavy_clock.note(false, 100 * s);
        light_clock.note(false, 100 * s);
        let mut counter = 0;
        while heavy.posted < 1484 {
            while p.refill_due() {
                let c = p.next_chunk().unwrap();
                p.back_chunk(c);
            }
            match heavy.accept(&request(5, counter, 0), p.free_blocks()) {
                Answer::Grant { count, .. } => {
                    let (f, l) = p.take_run(count).unwrap();
                    assert_eq!(l, count);
                    heavy_runs.push((f, l));
                    heavy.post(count);
                }
                other => panic!("{other:?}"),
            }
            counter += 1;
        }
        assert_eq!(p.used_blocks(), 1484 + 64);
        assert_eq!(
            p.chunks(),
            (1484 + 64 + RESERVE_BLOCKS).div_ceil(CHUNK_BLOCKS)
        );
        // The heavy VM goes idle, the light one does not.
        heavy_clock.note(true, 102 * s);
        light_clock.note(false, 102 * s);
        let t = 108 * s;
        assert!(heavy.reclaim_due(floor, heavy_clock.idle_ns(t), min_idle));
        assert!(!light.reclaim_due(floor, light_clock.idle_ns(t), min_idle));
        for (f, l) in heavy_runs.drain(..) {
            p.free_run(f, l);
        }
        heavy.reinit();
        while let Some(c) = p.releasable() {
            assert!(p.unback_chunk(c));
        }
        // The light VM's 64 blocks are untouched and the pool kept them plus the reserve.
        assert_eq!(p.used_blocks(), 64);
        for (f, l) in light_runs.iter() {
            for b in *f..*f + *l {
                assert!(p.is_used(b));
            }
        }
        assert_eq!(p.chunks(), 2);
        assert_eq!(light.posted, 64);
        // The heavy VM's next render re-posts its 64 from the pool.
        heavy_clock.note(false, 110 * s);
        let again = take(&mut p, heavy.initial);
        assert_eq!(again.iter().map(|(_, l)| l).sum::<u32>(), 64);
        assert_eq!(p.used_blocks(), 128);
    }
}
