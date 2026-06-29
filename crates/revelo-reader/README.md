# revelo-reader

Input/reader registration layer for [revelo](https://github.com/vbasky/revelo)
— a fast, safe, pure-Rust port of
[MediaInfoLib](https://mediaarea.net/en/MediaInfo) for extracting technical and
tag metadata from media files.

Part of the [**revelo**](https://github.com/vbasky/revelo) project — see the
[project README](https://github.com/vbasky/revelo#readme) for the full picture.

## What it does

Before a media file reaches the format parsers, MediaInfoLib registers the
*source type* through which the bytes arrive — filesystem path, directory, HTTP
stream, or MMS URL. `revelo-reader` ports that reader-registration step: each
function receives a mutable [`FileAnalyze`](https://docs.rs/revelo-core) context,
ensures a `General` stream exists, and writes the `Reader` field that downstream
parsers can inspect to adapt their behaviour.

## Public API

| Function | `Reader` value | Source type |
| --- | --- | --- |
| `parse_file_reader` | `"File"` | Ordinary filesystem path |
| `parse_directory_reader` | `"Directory"` | Directory source |
| `parse_http_reader` | `"HTTP"` | HTTP or HTTPS URL |
| `parse_mms_reader` | `"MMS"` | Microsoft Media Server URL |

Every function returns `bool` (`true` on success), matching the reader-dispatch
contract used by `revelo-core`.

## Usage

Add the dependency:

```toml
[dependencies]
revelo-reader = "0.5"
revelo-core   = "0.5"
```

Register a reader before handing the context to the format parsers:

```rust,no_run
use revelo_core::FileAnalyze;
use revelo_reader::parse_file_reader;

let mut fa = FileAnalyze::new(&[]);
parse_file_reader(&mut fa);
// fa's General stream now has Reader = "File".
// Pass `fa` to the appropriate format parser next.
```

For network sources, swap in the matching function:

```rust,no_run
use revelo_core::FileAnalyze;
use revelo_reader::{parse_http_reader, parse_mms_reader};

let mut fa = FileAnalyze::new(&[]);
parse_http_reader(&mut fa);   // Reader = "HTTP"

let mut fa2 = FileAnalyze::new(&[]);
parse_mms_reader(&mut fa2);   // Reader = "MMS"
```

## License

BSD-2-Clause — see [LICENSE](https://github.com/vbasky/revelo/blob/main/LICENSE).
