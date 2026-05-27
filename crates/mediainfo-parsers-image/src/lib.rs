//! Image format parsers (PNG, JPEG, GIF, etc.).

#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]

pub mod bmp;
pub mod gif;
pub mod jpeg;
pub mod png;

pub use bmp::parse_bmp;
pub use gif::parse_gif;
pub use jpeg::parse_jpeg;
pub use png::parse_png;
