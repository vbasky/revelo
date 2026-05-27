//! Transliteration of MediaArea's ZenLib (C++) to Rust.
//!
//! Goal is behavior parity with the C++ types used by MediaInfoLib, not
//! idiomatic Rust. Naming follows the upstream `ZenLib::` namespace
//! convention to keep parser code visually close to the C++ original.

#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]

pub mod types;
pub mod ztring;

pub use types::*;
pub use ztring::Ztring;
