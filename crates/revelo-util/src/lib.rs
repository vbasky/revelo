//! Low-level primitives underpinning the
//! [revelo](https://github.com/vbasky/revelo) media-metadata library — a
//! Rust transliteration of [MediaArea](https://mediaarea.net/)'s
//! [ZenLib](https://github.com/MediaArea/ZenLib) C++ support library.
//!
//! The goal is **behaviour parity** with the C++ types used by MediaInfoLib,
//! not idiomatic Rust. Naming follows the upstream `ZenLib::` namespace
//! convention so that transliterated parser code reads as close to the C++
//! original as possible. All code is `#[deny(unsafe_code)]`.
//!
//! # Modules
//!
//! ## [`Ztring`] — Unicode string with multi-encoding I/O
//!
//! A transliteration of `ZenLib::Ztring`. Upstream uses `wchar_t` (UTF-16) on
//! Windows and `char` (UTF-8) elsewhere; this port uses a single UTF-8
//! `String` internally regardless of host. The full set of `From_*` / `To_*`
//! conversions is preserved:
//!
//! - **Encoding constructors**: `From_UTF8`, `From_UTF8_bytes`,
//!   `From_ISO_8859_1`, `From_Local`, `From_UTF16`, `From_UTF16LE`,
//!   `From_UTF16BE`
//! - **FourCC / short-code constructors**: `From_CC4`, `From_CC3`, `From_CC2`,
//!   `From_CC1`
//! - **Numeric constructors** (with radix): `From_Number_int8u` …
//!   `From_Number_int128u`, `From_Number_int8s` … `From_Number_int128s`,
//!   `From_Number_float32`, `From_Number_float64`
//! - **Extraction**: `To_UTF8`, `To_Local`, `To_int8u` … `To_int64u`,
//!   `To_int8s` … (with radix), `as_str`, `into_string`
//!
//! `Ztring` implements `Clone`, `Debug`, `Default`, `PartialEq`, `Eq`,
//! `PartialOrd`, `Ord`, and `Hash`. It also implements `From<&str>` and
//! `From<String>` for ergonomic construction.
//!
//! ## [`BitStream`] — MSB-first bit reader
//!
//! A transliteration of `ZenLib::BitStream`. Reads up to 32 bits at a time
//! from a byte slice in MSB-first order. Key API:
//!
//! - `BitStream::new(buffer: &[u8])` — attach to a slice
//! - `get(how_many: usize) -> u32` — read the next N bits; returns 0 on
//!   underrun and sets an internal `buffer_under_run` flag
//! - `attach(&mut self, buffer: &[u8])` — re-attach to a new slice
//! - Bookmark / peek support and `Byte_Align` for partial-byte skipping
//!
//! ## [`types`] — ZenLib integer and float aliases
//!
//! Fixed-width type aliases that match `ZenLib/Conf.h` verbatim so
//! transliterated code compiles without renaming:
//!
//! `Int8u`, `Int16u`, `Int32u`, `Int64u`, `Int128u` (unsigned),
//! `Int8s`, `Int16s`, `Int32s`, `Int64s`, `Int128s` (signed),
//! `Float32`, `Float64`, `Float80` (= `f64`), `Char`, and the sentinel
//! constant `ERROR` (= `usize::MAX`).
//!
//! # Example
//!
//! ```no_run
//! use revelo_util::{BitStream, Ztring};
//!
//! // Build a Ztring from a raw FourCC u32
//! let z = Ztring::From_CC4(0x6672_6565); // "free"
//! assert_eq!(z.as_str(), "free");
//!
//! // Read individual bits from a byte buffer
//! let buf = [0b1010_0000u8, 0b0000_0001u8];
//! let mut bs = BitStream::new(&buf);
//! assert_eq!(bs.get(4), 0b1010);
//! assert_eq!(bs.get(4), 0b0000);
//! ```

#![allow(non_snake_case)]
#![deny(unsafe_code)]

pub mod bitstream;
pub mod types;
pub mod ztring;

pub use bitstream::BitStream;
pub use types::*;
pub use ztring::Ztring;
