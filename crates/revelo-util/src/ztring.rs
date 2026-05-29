//! Transliteration of `ZenLib::Ztring`.
//!
//! The upstream C++ class extends `std::basic_string<Char>` where `Char` is
//! `wchar_t` on Windows (UTF-16) and `char` (UTF-8) elsewhere. The port uses
//! a single UTF-8 representation backed by `String` regardless of host. All
//! `From_*` / `To_*` conversions still convert from/to other encodings; only
//! the internal storage is unified.

use crate::types::{
    Float32, Float64, Int8s, Int8u, Int16s, Int16u, Int32s, Int32u, Int64s, Int64u, Int128s,
    Int128u,
};

#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Ztring(pub String);

impl Ztring {
    pub fn new() -> Self {
        Ztring(String::new())
    }

    pub fn with_capacity(n: usize) -> Self {
        Ztring(String::with_capacity(n))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }

    pub fn len(&self) -> usize {
        self.0.chars().count()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn clear(&mut self) {
        self.0.clear()
    }

    // ----------------------------------------------------------------------
    // Conversions — From
    // ----------------------------------------------------------------------

    pub fn From_UTF8(s: &str) -> Self {
        Ztring(s.to_owned())
    }

    pub fn From_UTF8_bytes(bytes: &[u8]) -> Self {
        match std::str::from_utf8(bytes) {
            Ok(s) => Ztring(s.to_owned()),
            Err(_) => Ztring(String::from_utf8_lossy(bytes).into_owned()),
        }
    }

    pub fn From_Local(bytes: &[u8]) -> Self {
        Self::From_ISO_8859_1(bytes)
    }

    pub fn From_ISO_8859_1(bytes: &[u8]) -> Self {
        Ztring(bytes.iter().map(|&b| b as char).collect())
    }

    pub fn From_UTF16LE(bytes: &[u8]) -> Self {
        let units: Vec<u16> =
            bytes.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        Ztring(String::from_utf16_lossy(&units))
    }

    pub fn From_UTF16BE(bytes: &[u8]) -> Self {
        let units: Vec<u16> =
            bytes.chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
        Ztring(String::from_utf16_lossy(&units))
    }

    pub fn From_UTF16(bytes: &[u8]) -> Self {
        if bytes.len() >= 2 && bytes[0] == 0xFE && bytes[1] == 0xFF {
            Self::From_UTF16BE(&bytes[2..])
        } else if bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] == 0xFE {
            Self::From_UTF16LE(&bytes[2..])
        } else {
            // Default to LE on Windows convention, BE for network order.
            // MediaInfoLib biases to LE; matching that.
            Self::From_UTF16LE(bytes)
        }
    }

    pub fn From_CC4(value: Int32u) -> Self {
        let bytes = value.to_be_bytes();
        Self::From_Local(&bytes)
    }

    pub fn From_CC3(value: Int32u) -> Self {
        let bytes = (value & 0x00FF_FFFF).to_be_bytes();
        Self::From_Local(&bytes[1..])
    }

    pub fn From_CC2(value: Int16u) -> Self {
        let bytes = value.to_be_bytes();
        Self::From_Local(&bytes)
    }

    pub fn From_CC1(value: Int8u) -> Self {
        Self::From_Local(&[value])
    }

    pub fn From_Number_int8u(value: Int8u, radix: u32) -> Self {
        from_number_unsigned(value as u128, radix)
    }
    pub fn From_Number_int16u(value: Int16u, radix: u32) -> Self {
        from_number_unsigned(value as u128, radix)
    }
    pub fn From_Number_int32u(value: Int32u, radix: u32) -> Self {
        from_number_unsigned(value as u128, radix)
    }
    pub fn From_Number_int64u(value: Int64u, radix: u32) -> Self {
        from_number_unsigned(value as u128, radix)
    }
    pub fn From_Number_int128u(value: Int128u, radix: u32) -> Self {
        from_number_unsigned(value, radix)
    }

    pub fn From_Number_int8s(value: Int8s, radix: u32) -> Self {
        from_number_signed(value as i128, radix)
    }
    pub fn From_Number_int16s(value: Int16s, radix: u32) -> Self {
        from_number_signed(value as i128, radix)
    }
    pub fn From_Number_int32s(value: Int32s, radix: u32) -> Self {
        from_number_signed(value as i128, radix)
    }
    pub fn From_Number_int64s(value: Int64s, radix: u32) -> Self {
        from_number_signed(value as i128, radix)
    }
    pub fn From_Number_int128s(value: Int128s, radix: u32) -> Self {
        from_number_signed(value, radix)
    }

    pub fn From_Number_float32(value: Float32, after_comma: u8) -> Self {
        Ztring(format!("{:.*}", after_comma as usize, value))
    }
    pub fn From_Number_float64(value: Float64, after_comma: u8) -> Self {
        Ztring(format!("{:.*}", after_comma as usize, value))
    }

    // ----------------------------------------------------------------------
    // Conversions — To
    // ----------------------------------------------------------------------

    pub fn To_UTF8(&self) -> Vec<u8> {
        self.0.as_bytes().to_vec()
    }

    pub fn To_Local(&self) -> Vec<u8> {
        self.0.chars().map(|c| (c as u32 & 0xFF) as u8).collect()
    }

    pub fn To_int8u(&self, radix: u32) -> Int8u {
        parse_unsigned::<u8>(&self.0, radix).unwrap_or(0)
    }
    pub fn To_int16u(&self, radix: u32) -> Int16u {
        parse_unsigned::<u16>(&self.0, radix).unwrap_or(0)
    }
    pub fn To_int32u(&self, radix: u32) -> Int32u {
        parse_unsigned::<u32>(&self.0, radix).unwrap_or(0)
    }
    pub fn To_int64u(&self, radix: u32) -> Int64u {
        parse_unsigned::<u64>(&self.0, radix).unwrap_or(0)
    }

    pub fn To_int8s(&self, radix: u32) -> Int8s {
        parse_signed::<i8>(&self.0, radix).unwrap_or(0)
    }
    pub fn To_int16s(&self, radix: u32) -> Int16s {
        parse_signed::<i16>(&self.0, radix).unwrap_or(0)
    }
    pub fn To_int32s(&self, radix: u32) -> Int32s {
        parse_signed::<i32>(&self.0, radix).unwrap_or(0)
    }
    pub fn To_int64s(&self, radix: u32) -> Int64s {
        parse_signed::<i64>(&self.0, radix).unwrap_or(0)
    }

    pub fn To_float64(&self) -> Float64 {
        self.0.trim().parse::<f64>().unwrap_or(0.0)
    }
}

impl From<&str> for Ztring {
    fn from(s: &str) -> Self {
        Ztring(s.to_owned())
    }
}

impl From<String> for Ztring {
    fn from(s: String) -> Self {
        Ztring(s)
    }
}

impl AsRef<str> for Ztring {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Ztring {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

fn from_number_unsigned(mut value: u128, radix: u32) -> Ztring {
    assert!((2..=36).contains(&radix), "radix must be 2..=36");
    if value == 0 {
        return Ztring("0".to_owned());
    }
    let mut digits = Vec::with_capacity(40);
    while value > 0 {
        let d = (value % radix as u128) as u32;
        digits.push(std::char::from_digit(d, radix).unwrap());
        value /= radix as u128;
    }
    Ztring(digits.into_iter().rev().collect())
}

fn from_number_signed(value: i128, radix: u32) -> Ztring {
    if value < 0 {
        let abs = value.unsigned_abs();
        let mut s = String::from("-");
        s.push_str(&from_number_unsigned(abs, radix).0);
        Ztring(s)
    } else {
        from_number_unsigned(value as u128, radix)
    }
}

fn parse_unsigned<T>(s: &str, radix: u32) -> Option<T>
where
    T: std::str::FromStr + num_from_radix::FromRadix,
{
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    T::from_radix(s, radix)
}

fn parse_signed<T>(s: &str, radix: u32) -> Option<T>
where
    T: std::str::FromStr + num_from_radix::FromRadixSigned,
{
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    T::from_radix_signed(s, radix)
}

mod num_from_radix {
    pub(super) trait FromRadix: Sized {
        fn from_radix(s: &str, radix: u32) -> Option<Self>;
    }
    pub(super) trait FromRadixSigned: Sized {
        fn from_radix_signed(s: &str, radix: u32) -> Option<Self>;
    }
    macro_rules! impl_unsigned {
        ($($t:ty),*) => { $(
            impl FromRadix for $t {
                fn from_radix(s: &str, radix: u32) -> Option<Self> {
                    <$t>::from_str_radix(s, radix).ok()
                }
            }
        )* };
    }
    macro_rules! impl_signed {
        ($($t:ty),*) => { $(
            impl FromRadixSigned for $t {
                fn from_radix_signed(s: &str, radix: u32) -> Option<Self> {
                    <$t>::from_str_radix(s, radix).ok()
                }
            }
        )* };
    }
    impl_unsigned!(u8, u16, u32, u64, u128);
    impl_signed!(i8, i16, i32, i64, i128);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_round_trip() {
        let z = Ztring::From_UTF8("hello world");
        assert_eq!(z.as_str(), "hello world");
        assert_eq!(z.To_UTF8(), b"hello world");
    }

    #[test]
    fn utf16le_with_bom() {
        let bytes: &[u8] = &[0xFF, 0xFE, 0x48, 0x00, 0x69, 0x00];
        assert_eq!(Ztring::From_UTF16(bytes).as_str(), "Hi");
    }

    #[test]
    fn utf16be_with_bom() {
        let bytes: &[u8] = &[0xFE, 0xFF, 0x00, 0x48, 0x00, 0x69];
        assert_eq!(Ztring::From_UTF16(bytes).as_str(), "Hi");
    }

    #[test]
    fn cc4_packs_big_endian() {
        // "moov" atom in MP4
        let val: Int32u = 0x6D6F6F76;
        assert_eq!(Ztring::From_CC4(val).as_str(), "moov");
    }

    #[test]
    fn from_number_decimal() {
        assert_eq!(Ztring::From_Number_int32u(12345, 10).as_str(), "12345");
        assert_eq!(Ztring::From_Number_int32s(-42, 10).as_str(), "-42");
        assert_eq!(Ztring::From_Number_int8u(0, 10).as_str(), "0");
    }

    #[test]
    fn from_number_hex() {
        assert_eq!(Ztring::From_Number_int32u(0xDEAD_BEEF, 16).as_str(), "deadbeef");
    }

    #[test]
    fn to_int_round_trip() {
        let z = Ztring::From_Number_int64u(9_000_000_000, 10);
        assert_eq!(z.To_int64u(10), 9_000_000_000);
        let z = Ztring::From_Number_int32s(-12345, 10);
        assert_eq!(z.To_int32s(10), -12345);
    }

    #[test]
    fn to_int_invalid_returns_zero() {
        let z = Ztring::From_UTF8("not a number");
        assert_eq!(z.To_int32u(10), 0);
    }

    #[test]
    fn float_formatting() {
        assert_eq!(Ztring::From_Number_float64(std::f64::consts::PI, 2).as_str(), "3.14");
        assert_eq!(Ztring::From_Number_float64(0.0, 3).as_str(), "0.000");
    }
}
