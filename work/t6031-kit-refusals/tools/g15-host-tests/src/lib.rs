//! Host test harness: compiles the core-only g15 files in place through the `src/g15` symlink
//! that `run.sh` points at a kernel tree's `drivers/gpu/drm/asahi/g15`.
#![allow(dead_code)]
#[macro_use]
extern crate kernel;

pub mod g15 {
    pub mod fw {
        #[path = "../../g15/fw/buffer.rs"]
        pub mod buffer;
        #[path = "../../g15/fw/channels.rs"]
        pub mod channels;
    }
    pub mod pools {
        #[path = "../../g15/pools/growth.rs"]
        pub mod growth;
    }
}
