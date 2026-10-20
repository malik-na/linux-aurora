//! Host stand-in for the kernel crate: only what the core-only g15 files import.
pub mod prelude {}

#[macro_export]
macro_rules! static_assert {
    ($cond:expr $(, $msg:literal)?) => {
        const _: () = assert!($cond $(, $msg)?);
    };
}
