//! Image-format parsers for the revelo media-analysis library.
//!
//! This crate provides a parser for every still-image format that revelo
//! understands, including camera RAW formats and HDR formats. It is also the
//! home of EXIF/IFD parsing: the TIFF and JPEG parsers implement full IFD
//! walking per TIFF 6.0 and EXIF 2.3, surfacing GPS, datetime, camera
//! make/model, and related tags into the `Exif` stream.
//!
//! Each parser follows the same contract:
//!
//! ```text
//! fn parse_<format>(fa: &mut FileAnalyze) -> bool
//! ```
//!
//! The function inspects the byte buffer held by `fa`, returns `false` if the
//! data does not match, or fills the `Image`/`General`/`Exif` stream fields
//! and returns `true` on success. Parsers are registered in the revelo
//! dispatcher inside `revelo-core`; application code does not call them
//! directly.
//!
//! # Normal usage
//!
//! Use the [`revelo`](https://crates.io/crates/revelo) facade crate rather
//! than depending on this crate directly:
//!
//! ```ignore
//! // In your Cargo.toml: revelo = "0.5"
//! use revelo::Metadata;
//!
//! let meta = Metadata::from_file("photo.jpg").unwrap();
//! for (key, value) in meta.image() {
//!     println!("{key} = {value}");
//! }
//! for (key, value) in meta.exif() {
//!     println!("{key} = {value}");
//! }
//! ```
//!
//! # Parsers
//!
//! The full list of re-exported parser functions covers:
//!
//! - **Common web / display formats:** `parse_jpeg`, `parse_png`, `parse_gif`,
//!   `parse_webp`, `parse_bmp`, `parse_tga`, `parse_ico`, `parse_pcx`,
//!   `parse_rle`
//! - **EXIF-bearing formats (full IFD walking):** `parse_tiff`, `parse_jpeg`,
//!   `parse_cr2`, `parse_raf`
//! - **Professional / VFX:** `parse_exr`, `parse_dpx`, `parse_dds`,
//!   `parse_psd`, `parse_arriraw`
//! - **Modern / HDR:** `parse_heif`, `parse_bpg`, `parse_jp2`,
//!   `parse_gain_map`
//! - **Legacy / platform-specific:** `parse_amiga_icon`
//!
//! # EXIF / IFD parsing
//!
//! `parse_tiff` and `parse_jpeg` walk IFD chains in both little-endian
//! ("II") and big-endian ("MM") byte orders. Tags surfaced include camera
//! make/model/software/artist/copyright, `DateTime` / `DateTimeOriginal`,
//! image geometry, resolution, orientation, colour space, GPS coordinates,
//! and Exif sub-IFD exposure metadata. `parse_cr2` and `parse_raf` reuse
//! the same IFD machinery for Canon and Fujifilm RAW files.
//!
//! # Design
//!
//! All code in this crate is `#[deny(unsafe_code)]`. There are no system
//! library dependencies and no C FFI. The `FileAnalyze` type lives in
//! `revelo-core`.

#![allow(non_snake_case)]
#![deny(unsafe_code)]

pub mod amiga_icon;
pub mod arriraw;
pub mod bmp;
pub mod bpg;
pub mod cr2;
pub mod dds;
pub mod dpx;
pub mod exr;
pub mod gain_map;
pub mod gif;
pub mod heif;
pub mod ico;
pub mod jp2;
pub mod jpeg;
pub mod pcx;
pub mod png;
pub mod psd;
pub mod raf;
pub mod rle;
pub mod tga;
pub mod tiff;
pub mod webp;

pub use amiga_icon::parse_amiga_icon;
pub use arriraw::parse_arriraw;
pub use bmp::parse_bmp;
pub use bpg::parse_bpg;
pub use cr2::parse_cr2;
pub use dds::parse_dds;
pub use dpx::parse_dpx;
pub use exr::parse_exr;
pub use gain_map::parse_gain_map;
pub use gif::parse_gif;
pub use heif::parse_heif;
pub use ico::parse_ico;
pub use jp2::parse_jp2;
pub use jpeg::parse_jpeg;
pub use pcx::parse_pcx;
pub use png::parse_png;
pub use psd::parse_psd;
pub use raf::parse_raf;
pub use rle::parse_rle;
pub use tga::parse_tga;
pub use tiff::parse_tiff;
pub use webp::parse_webp;

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    fn assert_no_full_raw_scan_patterns(src_dir: &Path) {
        for entry in fs::read_dir(src_dir).expect("read parser src dir") {
            let path = entry.expect("read parser src entry").path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("rs") {
                continue;
            }

            let source = fs::read_to_string(&path).expect("read parser source");
            for forbidden in [
                concat!("peek_raw(", "fa.remain())"),
                concat!("read_raw(", "fa.remain())"),
                concat!("peek_raw(", "remain)"),
                concat!("read_raw(", "remain)"),
                concat!("peek_raw(", "total)"),
                concat!("read_raw(", "total)"),
                concat!("peek_raw(", "file_size)"),
                concat!("read_raw(", "file_size)"),
                concat!("peek_raw(", "avail)"),
                concat!("read_raw(", "avail)"),
                concat!("peek_raw_at(0, ", "fa.element_size())"),
            ] {
                assert!(
                    !source.contains(forbidden),
                    "{} contains forbidden full-scan pattern: {forbidden}",
                    path.display()
                );
            }
        }
    }

    #[test]
    fn image_parsers_do_not_reintroduce_full_raw_scans() {
        assert_no_full_raw_scan_patterns(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("src").as_path(),
        );
    }
}
