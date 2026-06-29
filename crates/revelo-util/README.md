# revelo-util

Low-level string, number, and bitstream primitives underpinning
[revelo](https://github.com/vbasky/revelo) — a fast, safe, pure-Rust port of
[MediaInfoLib](https://mediaarea.net/en/MediaInfo) for extracting technical and
tag metadata from media files.

Part of the [**revelo**](https://github.com/vbasky/revelo) project — see the
[project README](https://github.com/vbasky/revelo#readme) for the full picture.

## What it does

`revelo-util` is a Rust transliteration of
[MediaArea's ZenLib](https://github.com/MediaArea/ZenLib) C++ support library.
The goal is **behaviour parity** with the C++ types, not idiomatic Rust:
names follow the upstream `ZenLib::` convention so that ported parser code
reads close to the original. All code is `#[deny(unsafe_code)]`.

Three public items are exported:

### `Ztring` — Unicode string with multi-encoding I/O

A port of `ZenLib::Ztring`. Upstream uses `wchar_t` (UTF-16) on Windows and
`char` (UTF-8) elsewhere; this implementation uses a single UTF-8 `String`
internally on all platforms. The full `From_*` / `To_*` conversion surface is
preserved.

#### Construction

| Method | Description |
| --- | --- |
| `Ztring::new()` / `Default` | Empty string |
| `From_UTF8(s: &str)` | From a UTF-8 string slice |
| `From_UTF8_bytes(bytes: &[u8])` | From UTF-8 bytes; lossy on invalid input |
| `From_ISO_8859_1(bytes: &[u8])` | Latin-1 bytes → UTF-8 |
| `From_Local(bytes: &[u8])` | Alias for `From_ISO_8859_1` |
| `From_UTF16(bytes: &[u8])` | BOM-detected UTF-16 |
| `From_UTF16LE(bytes: &[u8])` | UTF-16 little-endian |
| `From_UTF16BE(bytes: &[u8])` | UTF-16 big-endian |
| `From_CC4(value: u32)` | FourCC four-byte code |
| `From_CC3(value: u32)` | Three-byte CC code |
| `From_CC2(value: u16)` | Two-byte CC code |
| `From_CC1(value: u8)` | Single-byte CC code |
| `From_Number_int{8,16,32,64,128}u(value, radix)` | Unsigned integer to string |
| `From_Number_int{8,16,32,64,128}s(value, radix)` | Signed integer to string |
| `From_Number_float{32,64}(value, after_comma)` | Float with fixed decimal places |

#### Extraction

| Method | Description |
| --- | --- |
| `as_str()` | Borrow as `&str` |
| `into_string()` | Consume into `String` |
| `To_UTF8()` | UTF-8 bytes (`Vec<u8>`) |
| `To_Local()` | Latin-1 bytes (`Vec<u8>`) |
| `To_int{8,16,32,64}u(radix)` | Parse as unsigned integer |
| `To_int{8,16,32,64}s(radix)` | Parse as signed integer |

`Ztring` derives `Clone`, `Debug`, `Default`, `PartialEq`, `Eq`, `PartialOrd`,
`Ord`, and `Hash`. It implements `From<&str>` and `From<String>`.

### `BitStream<'a>` — MSB-first bit reader

A port of `ZenLib::BitStream`. Reads up to 32 bits at a time from a `&[u8]`
in MSB-first order.

| Method | Description |
| --- | --- |
| `BitStream::new(buffer: &[u8])` | Attach to a byte slice |
| `attach(&mut self, buffer)` | Re-attach; resets all state |
| `get(how_many: usize) -> u32` | Read the next N bits (1–32); returns 0 on underrun |

Underrun behavior (returning 0 and setting an internal flag), bookmark/peek
mechanics, and `Byte_Align` (consume the remainder of the current partial byte)
all match the C++ implementation.

### `types` — ZenLib integer and float aliases

Fixed-width type aliases that match `ZenLib/Conf.h` exactly:

| Alias | Rust type |
| --- | --- |
| `Int8u` / `Int16u` / `Int32u` / `Int64u` / `Int128u` | `u8` / `u16` / `u32` / `u64` / `u128` |
| `Int8s` / `Int16s` / `Int32s` / `Int64s` / `Int128s` | `i8` / `i16` / `i32` / `i64` / `i128` |
| `Float32` / `Float64` / `Float80` | `f32` / `f64` / `f64` |
| `Char` | `char` |
| `ERROR` | `usize::MAX` |

## Usage

Add the dependency:

```toml
[dependencies]
revelo-util = "0.5"
```

```rust,no_run
use revelo_util::{BitStream, Ztring};

// Build a Ztring from a raw FourCC u32
let z = Ztring::From_CC4(0x6672_6565); // "free"
assert_eq!(z.as_str(), "free");

// Numeric conversion with radix
let hex = Ztring::From_Number_int32u(255, 16);
assert_eq!(hex.as_str(), "ff");

// Read individual bits from a byte buffer (MSB-first)
let buf = [0b1010_0000u8, 0b0000_0001u8];
let mut bs = BitStream::new(&buf);
assert_eq!(bs.get(4), 0b1010);
assert_eq!(bs.get(4), 0b0000);
```

## License

BSD-2-Clause — see [LICENSE](https://github.com/vbasky/revelo/blob/main/LICENSE).
