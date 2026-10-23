// CDM gate walk benchmark: the actual g15/gate.rs (included by path) over worst-case streams,
// with read_user() and cover() modelled by their per-call costs on this CPU: the VM lock (a
// mutex), a 3-level page-table lookup per page, a clean+invalidate to PoC of every 64-byte line
// read, a DSB and the copy.
#![allow(dead_code, unused_imports)]
extern crate self as kernel;
use std::sync::Mutex;
use std::time::Instant;
pub mod prelude {
    pub type Error = i32;
    pub type Result<T = ()> = std::result::Result<T, Error>;
    pub const EINVAL: Error = -22;
    pub const EFAULT: Error = -14;
    pub use crate::{pr_warn, static_assert};
}
#[macro_export]
macro_rules! pr_warn { ($($arg:tt)*) => { let _ = format_args!($($arg)*); } }
#[macro_export]
macro_rules! static_assert { ($e:expr) => { const _: () = assert!($e); } }
pub mod mmu {
    use crate::prelude::*;
    use std::sync::Mutex;
    pub const UAT_PGSZ: usize = 0x4000;
    pub const UAT_PGMSK: usize = UAT_PGSZ - 1;
    pub struct Vm {
        pub mem: Vec<u8>,
        pub lock: Mutex<u64>,
        /// Three levels of table, 2048 entries each, as a 16K-page walk.
        pub l1: Vec<u64>, pub l2: Vec<u64>, pub l3: Vec<u64>,
        pub calls: std::cell::Cell<u64>,
    }
    impl Vm {
        #[inline(never)]
        pub fn lookup(&self, va: u64) -> u64 {
            let p = (va >> 14) as usize;
            let a = self.l1[(p >> 22) & 2047] as usize;
            let b = self.l2[(a + ((p >> 11) & 2047)) & 2047] as usize;
            self.l3[(b + (p & 2047)) & 2047]
        }
        pub fn read_user(&self, va: u64, out: &mut [u8]) -> Result {
            self.calls.set(self.calls.get() + 1);
            let mut g = self.lock.lock().unwrap();
            let start = va as usize;
            let end = start.checked_add(out.len()).ok_or(EFAULT)?;
            let src = self.mem.get(start..end).ok_or(EFAULT)?;
            // leaf_entry() and translate_iova(): two walks of the page.
            *g ^= self.lookup(va) ^ self.lookup(va);
            let base = src.as_ptr() as usize;
            let mut line = base & !63;
            while line < base + src.len() {
                unsafe { core::arch::asm!("dc civac, {}", in(reg) line) };
                line += 64;
            }
            unsafe { core::arch::asm!("dsb sy") };
            out.copy_from_slice(src);
            Ok(())
        }
        pub fn covers(&self, start: u64, end: u64) -> bool {
            self.calls.set(self.calls.get() + 1);
            let mut g = self.lock.lock().unwrap();
            let mut p = start & !(UAT_PGMSK as u64);
            while p < end {
                *g ^= self.lookup(p);
                p += UAT_PGSZ as u64;
            }
            end > start
        }
    }
}
mod command {
    use crate::{mmu, prelude::*};
    pub const USC_WINDOW: u64 = 1 << 32;
    pub fn cover(vm: &mmu::Vm, start: u64, end: u64, _: bool, _: bool) -> Result {
        if end <= start { return Err(EINVAL); }
        if vm.covers(start, end) { Ok(()) } else { Err(EFAULT) }
    }
}
mod params { pub fn stream_gate() -> bool { true } }
#[path = "gate.rs"]
mod gate;

const BLOCK: usize = 0x10000;
const BLOCKS: usize = 65;

fn vm(mem: Vec<u8>) -> mmu::Vm {
    let t = |s: u64| (0..2048u64).map(|i| (i * 2654435761 + s) & 2047).collect::<Vec<_>>();
    mmu::Vm { mem, lock: Mutex::new(0), l1: t(1), l2: t(2), l3: t(3), calls: std::cell::Cell::new(0) }
}
fn put(m: &mut Vec<u8>, at: usize, w: &[u32]) {
    for (i, x) in w.iter().enumerate() { m[at + 4 * i..at + 4 * i + 4].copy_from_slice(&x.to_le_bytes()); }
}
/// 65 blocks of 64 KiB: each filled with `rec` records, linked to the next at its end; the last
/// ends in TERMINATE, so the walk covers every byte of the budget.
fn stream(rec: &dyn Fn(usize, usize) -> Vec<u32>, size: usize) -> (Vec<u8>, u64) {
    let mut m = vec![0u8; BLOCKS * BLOCK + 4096];
    for b in 0..BLOCKS {
        let base = b * BLOCK;
        let n = (BLOCK - 8) / size;
        for i in 0..n { put(&mut m, base + i * size, &rec(b, i)); }
        let tail = base + n * size;
        if b + 1 < BLOCKS {
            let next = (b + 1) * BLOCK;
            put(&mut m, tail, &[0x20000000 | ((next as u64 >> 32) as u32 & 0xffff), next as u32]);
        } else {
            put(&mut m, tail, &[0x40000000]);
        }
    }
    (m, 0)
}
fn run(name: &str, rec: &dyn Fn(usize, usize) -> Vec<u32>, size: usize, usc: u64) {
    let (m, base) = stream(rec, size);
    let v = vm(m);
    assert_eq!(gate::cdm(&v, usc, base), Ok(()), "{name} not admitted");
    let mut best = f64::MAX;
    let mut calls = 0;
    for _ in 0..7 {
        v.calls.set(0);
        let t = Instant::now();
        let r = gate::cdm(&v, usc, base);
        let ms = t.elapsed().as_secs_f64() * 1e3;
        assert_eq!(r, Ok(()));
        best = best.min(ms);
        calls = v.calls.get();
    }
    let records = BLOCKS * ((BLOCK - 8) / size);
    println!("{name:<34} records {records:>8}  read/cover calls {calls:>7}  walk {best:>7.2} ms  x64 commands {:>7.1} ms", best * 64.0);
}
fn main() {
    run("barriers (4 B)", &|_, _| vec![0x60000160], 4, 0);
    // Direct launch, 40 B; program 64-aligned in the USC window, all in one page (cached check).
    let direct = |prog: u64| vec![0x00080000, ((prog >> 32) as u32) << 16, (prog >> 6) as u32, 0x40, 0, 0, 0, 0, 0, 0];
    let usc = 1u64 << 40;
    run("direct launches, one program page", &|_, _| direct(usc), 40, usc);
    // Each launch's program on its own page: one cover() per launch.
    run("direct launches, a page each", &|b, i| direct(usc + ((b * 2000 + i) as u64) * 0x4000), 40, usc);
    // Indirect launch, 24 B: one cover() of its 12-byte arguments each.
    let indirect = move |b: usize, i: usize| {
        let prog = usc;
        let arg = 0x100000u64 + ((b * BLOCK + i * 24) as u64 & !3);
        vec![0x10080000, ((prog >> 38) as u32) << 22, (prog >> 6) as u32, 0x40000000 | ((prog >> 40) & 0xff) as u32,
             (arg >> 32) as u32 & 0xffff, arg as u32]
    };
    run("indirect launches (24 B)", &indirect, 24, usc);
    // The old cap for comparison: 8192 barrier records and a terminator in one block.
    let mut m = vec![0u8; 8193 * 4 + 4096];
    for i in 0..8192 { put(&mut m, i * 4, &[0x60000160]); }
    put(&mut m, 8192 * 4, &[0x40000000]);
    let v = vm(m);
    let mut best = f64::MAX;
    for _ in 0..7 { let t = Instant::now(); assert_eq!(gate::cdm(&v, 0, 0), Ok(())); best = best.min(t.elapsed().as_secs_f64() * 1e3); }
    println!("{:<34} records {:>8}  walk {best:>7.3} ms  x64 commands {:>7.2} ms", "8192 barriers (the old cap)", 8192, best * 64.0);
    // The cost of one modelled read_user() of 256 bytes, alone.
    let v = vm(vec![0u8; 1 << 20]);
    let mut buf = [0u8; 256];
    let t = Instant::now();
    for i in 0..100000u64 { v.read_user((i * 256) % ((1 << 20) - 256), &mut buf).unwrap(); }
    println!("one modelled 256-byte read_user: {:.0} ns", t.elapsed().as_secs_f64() * 1e9 / 100000.0);
}
