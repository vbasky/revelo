//! Transliteration of MediaArea's ZenLib (C++) to Rust.
//!
//! Goal is behavior parity with the C++ types used by MediaInfoLib, not
//! idiomatic Rust. Naming follows the upstream `ZenLib::` namespace
//! convention to keep parser code visually close to the C++ original.

#![allow(non_snake_case)]
#![deny(unsafe_code)]

pub mod bitstream;
pub mod types;
pub mod ztring;

pub use bitstream::BitStream;
pub use types::*;
pub use ztring::Ztring;
