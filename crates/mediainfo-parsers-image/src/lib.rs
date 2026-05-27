//! Image format parsers (PNG, JPEG, GIF, etc.).

#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]

pub mod png;

pub use png::parse_png;
