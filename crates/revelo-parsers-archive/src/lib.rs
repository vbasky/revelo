//! Archive, compressed-file, and executable-format parsers for the
//! [revelo](https://github.com/vbasky/revelo) media-analysis library.
//!
//! Each public function has the signature `fn(&mut FileAnalyze) -> bool`.
//! It inspects the file's magic bytes, returns `false` if the format is not
//! recognised, or fills the [`FileAnalyze`](revelo_core::FileAnalyze) General
//! stream with `Format` and related structural fields and returns `true`.
//!
//! # Supported formats
//!
//! ## Archives and compression
//!
//! | Function | Format | Detection |
//! |---|---|---|
//! | [`parse_zip`] | ZIP | `PK\x03\x04` local file header |
//! | [`parse_rar`] | RAR (v4 / v5) | `Rar!\x1A\x07` |
//! | [`parse_7z`] | 7-Zip | `7z\xBC\xAF\x27\x1C` |
//! | [`parse_tar`] | TAR (ustar / legacy) | `ustar` at offset 257, or checksum |
//! | [`parse_gzip`] | GZip | `\x1F\x8B` |
//! | [`parse_bzip2`] | BZip2 | `BZh` + block-size digit |
//! | [`parse_ace`] | ACE | `**ACE**` |
//! | [`parse_iso9660`] | ISO 9660 CD-ROM filesystem | `CD001` at sector 16 |
//!
//! ## Executables and shared objects
//!
//! | Function | Format | Detection |
//! |---|---|---|
//! | [`parse_elf`] | ELF (Linux/BSD) — 32/64-bit, LE/BE, x86/x86-64/ARM/AArch64/MIPS/PPC | `\x7FELF` |
//! | [`parse_mach_o`] | Mach-O (macOS/iOS) — 32/64-bit, LE/BE, fat/universal binary | `FEEDFACE` family |
//! | [`parse_mz_exe`] | MZ DOS / Windows PE | `MZ` + `PE\0\0` at offset from header |
//!
//! # Normal use
//!
//! Prefer the [`revelo`](https://crates.io/crates/revelo) facade, which
//! auto-detects formats and dispatches to the correct parser:
//!
//! ```no_run
//! use revelo_parsers_archive::parse_zip;
//! use revelo_core::FileAnalyze;
//!
//! let data = std::fs::read("archive.zip").unwrap();
//! let mut fa = FileAnalyze::new(&data);
//! if parse_zip(&mut fa) {
//!     // fa contains a General stream with Format = "ZIP"
//! }
//! ```
//!
//! # Safety
//!
//! `#![deny(unsafe_code)]` — zero unsafe blocks in this crate.

#![deny(unsafe_code)]

pub mod archives;

pub use archives::{
    parse_7z, parse_ace, parse_bzip2, parse_elf, parse_gzip, parse_iso9660, parse_mach_o,
    parse_mz_exe, parse_rar, parse_tar, parse_zip,
};
