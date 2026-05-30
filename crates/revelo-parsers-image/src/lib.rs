//! Image format parsers (PNG, JPEG, GIF, etc.).

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
