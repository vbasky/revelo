# revelo-core

The parsing engine for [**revelo**](https://github.com/vbasky/revelo) — a fast,
safe, pure-Rust port of [MediaInfoLib](https://mediaarea.net/en/MediaInfo) with
optional [ExifTool](https://exiftool.org/)-grade EXIF/maker-note depth.

`revelo-core` provides every primitive that a format parser needs: a cursor-based
byte reader, big- and little-endian integer/float accessors, a bitstream mode for
codec header fields, an element trace tree, and a typed stream collection. Parsers
write results into `StreamCollection` via `FileAnalyze`; output formatters in
`revelo-export` walk that collection to produce XML, JSON, and text reports.

**New users** should consider the [`revelo`](https://crates.io/crates/revelo)
facade crate, which wraps format detection, parsing, and tag extraction into a
single `Metadata::from_bytes()` call.

Part of the [**revelo**](https://github.com/vbasky/revelo) project — see the
[project README](https://github.com/vbasky/revelo#readme) for the full picture.

## Key types

| Type | Role |
| --- | --- |
| `FileAnalyze<'a>` | Cursor over a byte buffer; the context every parser receives |
| `MediaFile<'a>` | Public type alias for `FileAnalyze` |
| `StreamCollection` | Stores all parsed fields, keyed by `(StreamKind, position)` |
| `Stream` | A single stream's fields in insertion order, plus an `<extra>` bucket |
| `StreamKind` | Discriminant enum: `General`, `Video`, `Audio`, `Text`, `Other`, `Image`, `Menu`, plus `Exif`, `Iptc`, `Xmp`, `Icc`, `C2pa`, `MakerNotes` |
| `ElementTree` | Stack-based trace tree (mirrors MediaInfoLib's `--trace` output) |
| `ElementNode` / `ElementInfo` | Nodes and annotated fields in the trace tree |
| `Reader<'_,'_>` | Fluent, `Option`-returning wrapper over `FileAnalyze` |
| `MediaConfig` | Runtime options: demux level, trace verbosity, parse speed, multi-file |

## Byte reader surface

`FileAnalyze` transliterates MediaInfoLib's `File__Analyze` read methods.
Every method advances the cursor and records an optional trace entry.

### Big-endian integers (`Get_B*`)

```text
get_b1(name) -> u8
get_b2(name) -> u16
get_b3(name) -> u32
get_b4(name) -> u32
get_b5(name) -> u64  …  get_b8(name) -> u64
get_b16(name) -> u128
```

### Little-endian integers (`Get_L*`)

```text
get_l1(name) -> u8  …  get_l8(name) -> u64
get_l16(name) -> u128
```

### Floats

```text
get_bf4(name) -> f32   get_bf8(name) -> f64   get_bf10(name) -> f64  (80-bit AIFF)
get_lf4(name) -> f32   get_lf8(name) -> f64
```

### Non-advancing peeks

```text
peek_b1()  …  peek_b16()       peek_l1()  …  peek_l16()
peek_raw(n) -> Option<&[u8]>   peek_magic::<N>(expected) -> bool
```

### 4CC

```text
get_c4(name) -> u32   peek_c4() -> u32
```

### Bitstream mode

```text
bs_begin()
get_s1(n, name) -> u8  …  get_s8(n, name) -> u64   (MSB-first, n bits)
bs_end()
```

### Stream management

```text
stream_prepare(kind) -> usize         // allocate a new stream, returns its index
set_field(kind, pos, key, value)      // first-write-wins
force_field(kind, pos, key, value)    // always overwrites
set_extra_field(...)  /  force_extra_field(...)  // <extra> bucket
retrieve(kind, pos, key) -> Option<&Ztring>
stream_count(kind) -> usize
```

## Ergonomic `Reader` API

`Reader` wraps `FileAnalyze` with a fluent, `Option`-returning interface:

```text
be_u8 / be_u16 / be_u24 / be_u32 / be_u40 / be_u48 / be_u56 / be_u64 / be_u128
le_u8 / le_u16 / le_u24 / le_u32 / le_u64
be_f32 / be_f64 / be_f80 / le_f32 / le_f64
fourcc(name) -> Option<u32>
bits(|br| { br.read::<u32>(5, "bsid") })  // typed bitstream reads
peek_be_u16 / peek_be_u32 / peek_be_u64 / peek_le_u16 / peek_le_u32
read_raw(n) / peek_raw(n) / skip(n)
```

## Usage example

```rust,no_run
use revelo_core::{FileAnalyze, StreamKind};

fn my_parser(fa: &mut FileAnalyze) -> bool {
    // Check magic bytes without advancing the cursor
    if !fa.peek_magic(b"RIFF") {
        return false;
    }

    let _size = fa.get_b4("Size");
    let tag   = fa.get_c4("Tag");

    let pos = fa.stream_prepare(StreamKind::General);
    fa.set_field(StreamKind::General, pos, "Format", "MyFormat");

    let _ = tag; // use it
    true
}
```

Or via the higher-level `Reader` API:

```rust,no_run
use revelo_core::{FileAnalyze, Reader, StreamKind};

fn my_parser(fa: &mut FileAnalyze) -> bool {
    let mut r = Reader::wrap(fa);
    let size = r.be_u32("Size")?;
    let pos  = r.stream_prepare(StreamKind::Audio);
    r.set_field(StreamKind::Audio, pos, "BitDepth", "24");
    let _ = size;
    Some(())
}
```

## Zero unsafe code

`#![deny(unsafe_code)]` is enforced. All read methods return native Rust types
(`u8`–`u128`, `f32`, `f64`) with no out-parameters or raw pointers. Truncated
reads return `0` / empty slices and set a `truncated()` flag rather than
panicking.

## License

BSD-2-Clause — see [LICENSE](https://github.com/vbasky/revelo/blob/main/LICENSE).
