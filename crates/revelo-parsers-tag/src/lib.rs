//! Tag and metadata parsers for the [revelo](https://github.com/vbasky/revelo)
//! media-analysis library.
//!
//! This crate locates and decodes embedded metadata streams — ID3, APE,
//! Vorbis Comment, EXIF/TIFF, XMP, IIM/IPTC, ICC, C2PA, Apple PropertyList,
//! and Spherical Video — populating the corresponding streams inside a
//! [`FileAnalyze`](revelo_core::FileAnalyze) value.
//!
//! # Supported tag formats
//!
//! | Function | Tag format |
//! |---|---|
//! | [`parse_id3v1`] | ID3v1 — 128-byte trailer tag |
//! | [`parse_id3v2`] | ID3v2.2 / v2.3 / v2.4 header tag |
//! | [`parse_ape_tag`] | APEv1 / APEv2 |
//! | [`parse_vorbis_comment`] | Vorbis Comment (Ogg, FLAC, Opus) |
//! | [`parse_vorbis_comment_from_buf`] | Vorbis Comment from a byte buffer |
//! | [`parse_lyrics3`] | Lyrics3 v1/v2 |
//! | [`parse_exif`] | EXIF / TIFF IFD chain (Exif, GPS, Interop IFDs) |
//! | [`parse_xmp`] | XMP packet |
//! | [`parse_iim`] | IIM / IPTC (IPTC-NAA record 2, Photoshop IRB) |
//! | [`parse_iim_buf`] | IIM from a byte buffer |
//! | [`parse_icc`] | ICC colour profile (v2 / v4) |
//! | [`parse_c2pa`] | C2PA provenance manifest |
//! | [`parse_property_list`] | Apple PropertyList (binary / XML plist) |
//! | [`parse_spherical_video`] | Google Spherical Video v1/v2 |
//! | [`parse_tags`] | Dispatcher — tries all parsers in priority order |
//!
//! # Feature flags
//!
//! ## `exiftool-tables` (off by default)
//!
//! ```toml
//! [dependencies]
//! revelo-parsers-tag = { version = "0.4", features = ["exiftool-tables"] }
//! ```
//!
//! Enables ExifTool-grade maker-note decoding: when active, the EXIF parser
//! uses lookup tables derived from Phil Harvey's
//! [ExifTool](https://exiftool.org/) project, covering 14 camera vendors
//! (Canon, Nikon, Fujifilm, Olympus, Sony, Panasonic, and more).
//!
//! > **License:** the default build is **BSD-2-Clause**. Enabling
//! > `exiftool-tables` pulls in `revelo-exiftool-tables`, whose tables are
//! > derived from ExifTool (© Phil Harvey) and are subject to
//! > **GPL-1.0-or-later OR Artistic-1.0-Perl** terms. Any binary that links
//! > `revelo-exiftool-tables` inherits those terms. Do **not** enable this
//! > feature if your project requires a permissive-only licence.
//!
//! Without the feature, maker-note fields are decoded using hand-written,
//! clean-room tables and the crate stays BSD-2-Clause.
//!
//! # Normal use
//!
//! Prefer the [`revelo`](https://crates.io/crates/revelo) facade, which
//! auto-detects formats and dispatches to the correct parsers:
//!
//! ```no_run
//! use revelo_parsers_tag::{parse_exif, parse_xmp};
//! use revelo_core::FileAnalyze;
//!
//! let data = std::fs::read("photo.jpg").unwrap();
//! let mut fa = FileAnalyze::new(&data);
//! parse_exif(&mut fa);
//! parse_xmp(&mut fa);
//! // fa now has Exif, Xmp, and Iptc streams populated
//! ```
//!
//! # Safety
//!
//! `#![deny(unsafe_code)]` — zero unsafe blocks in this crate.

#![allow(non_snake_case)]
#![deny(unsafe_code)]

pub mod tags;

pub use tags::{
    parse_ape_tag, parse_c2pa, parse_exif, parse_icc, parse_id3v1, parse_id3v2, parse_iim,
    parse_iim_buf, parse_lyrics3, parse_property_list, parse_spherical_video, parse_tags,
    parse_vorbis_comment, parse_vorbis_comment_from_buf, parse_xmp,
};
