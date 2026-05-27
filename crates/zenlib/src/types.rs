//! ZenLib `Conf.h` integer aliases and basic types.
//!
//! Upstream uses macros / typedefs based on platform; here we pin them to
//! fixed-width Rust primitives. The names match the C++ side verbatim so
//! transliterated parser code reads identically.

pub type int8u = u8;
pub type int16u = u16;
pub type int32u = u32;
pub type int64u = u64;

pub type int8s = i8;
pub type int16s = i16;
pub type int32s = i32;
pub type int64s = i64;

pub type float32 = f32;
pub type float64 = f64;
pub type float80 = f64;

pub type int128u = u128;
pub type int128s = i128;

pub type Char = char;

pub const Error: usize = usize::MAX;
