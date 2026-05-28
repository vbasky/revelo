/// Safe byte-array-to-integer conversions that avoid the `.try_into().unwrap()`
/// pattern on guaranteed-correct-length byte slices.

macro_rules! define_readers {
    ($( ($name:ident, $ty:ty, $len:expr) ),* $(,)?) => {
        $(
            #[inline]
            pub fn $name(bytes: &[u8]) -> $ty {
                <$ty>::from_le_bytes(bytes[..$len].try_into().unwrap_or_default())
            }

            paste::paste! {
                #[inline]
                pub fn [<$name _be>](bytes: &[u8]) -> $ty {
                    <$ty>::from_be_bytes(bytes[..$len].try_into().unwrap_or_default())
                }
            }
        )*
    };
}

// Without paste, just define them manually
#[inline]
pub fn le_u16(bytes: &[u8]) -> u16 { u16::from_le_bytes([bytes[0], bytes[1]]) }
#[inline]
pub fn le_u32(bytes: &[u8]) -> u32 { u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) }
#[inline]
pub fn le_u64(bytes: &[u8]) -> u64 { u64::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7]]) }
#[inline]
pub fn be_u16(bytes: &[u8]) -> u16 { u16::from_be_bytes([bytes[0], bytes[1]]) }
#[inline]
pub fn be_u32(bytes: &[u8]) -> u32 { u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) }
#[inline]
pub fn be_u64(bytes: &[u8]) -> u64 { u64::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7]]) }
