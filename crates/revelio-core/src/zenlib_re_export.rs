//! Pin the small set of `zenlib` type aliases the crate uses internally,
//! so submodules can `use crate::zenlib_re_export::Int64u` without each
//! file pulling in the full `zenlib` prelude.

#[allow(unused_imports)]
pub(crate) use zenlib::{Float32, Float64, Float80, Int8u, Int16u, Int32u, Int64u, Int128u};
