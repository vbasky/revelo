//! Image format parsers (PNG, JPEG, GIF, etc.).

#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]

pub mod bmp;
pub mod dds;
pub mod dpx;
pub mod exr;
pub mod gif;
pub mod ico;
pub mod jpeg;
pub mod png;
pub mod psd;
pub mod tiff;
pub mod webp;

pub use bmp::parse_bmp;
pub use dds::parse_dds;
pub use dpx::parse_dpx;
pub use exr::parse_exr;
pub use gif::parse_gif;
pub use ico::parse_ico;
pub use jpeg::parse_jpeg;
pub use png::parse_png;
pub use psd::parse_psd;
pub use tiff::parse_tiff;
pub use webp::parse_webp;
