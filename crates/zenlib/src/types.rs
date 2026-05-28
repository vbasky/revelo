//! ZenLib `Conf.h` integer aliases and basic types.
//!
//! Upstream uses macros / typedefs based on platform; here we pin them to
//! fixed-width Rust primitives. The names match the C++ side verbatim so
//! transliterated parser code reads identically.

pub type Int8u = u8;
pub type Int16u = u16;
pub type Int32u = u32;
pub type Int64u = u64;

pub type Int8s = i8;
pub type Int16s = i16;
pub type Int32s = i32;
pub type Int64s = i64;

pub type Float32 = f32;
pub type Float64 = f64;
pub type Float80 = f64;

pub type Int128u = u128;
pub type Int128s = i128;

pub type Char = char;

pub const ERROR: usize = usize::MAX;
