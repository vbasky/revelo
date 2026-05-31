# revelo-cdylib

A drop-in replacement for `libmediainfo` — a `cdylib` that exposes the
**`MediaInfo_*` C ABI** exactly as the upstream
[MediaInfoLib](https://mediaarea.net/en/MediaInfo) does, backed entirely by
revelo's pure-Rust engine.

Swap the shared library, keep your existing C/C++/Python/FFI consumer code
unchanged.

Part of the [**revelo**](https://github.com/vbasky/revelo) project — a fast,
safe, pure-Rust port of MediaInfoLib for extracting technical and tag metadata
from media files. See the [project README](https://github.com/vbasky/revelo#readme)
for the full picture.

## Exported C entry points

| Symbol | Description |
| --- | --- |
| `MediaInfo_New()` | Allocate a handle |
| `MediaInfo_Delete(handle)` | Free a handle |
| `MediaInfo_Open(handle, path)` | Read and parse a file; returns 1 on success, 0 on failure |
| `MediaInfo_Close(handle)` | Discard parsed state (handle is still valid) |
| `MediaInfo_Inform(handle, 0)` | Return the human-readable text report (caller must free) |
| `MediaInfo_Get(handle, stream_kind, stream_index, parameter, info_kind, search_kind)` | Retrieve a single field value |
| `MediaInfo_Count_Get(handle, stream_kind, stream_index)` | Count streams of a given kind |
| `MediaInfo_Option(handle, option, value)` | Set a runtime option (`Demux`, `TraceLevel`, `ParseSpeed`, …) |

## C usage

```c
#include "MediaInfoDLL.h"   /* or include just the prototypes below */

void *handle = MediaInfo_New();

if (MediaInfo_Open(handle, "/path/to/video.mp4")) {
    char *report = MediaInfo_Inform(handle, 0);
    printf("%s\n", report);
    free(report);

    /* Query a specific field */
    char *codec = MediaInfo_Get(handle,
                                1,          /* stream_kind = Video */
                                0,          /* stream_number */
                                "Format",   /* parameter */
                                1, 0);
    printf("Video codec: %s\n", codec);
    free(codec);

    MediaInfo_Close(handle);
}

MediaInfo_Delete(handle);
```

## Stream kind constants

These map directly to MediaInfoLib's `stream_t` enum values:

| Value | Kind | Value | Kind |
| --- | --- | --- | --- |
| 0 | General | 7 | Exif |
| 1 | Video | 8 | Iptc |
| 2 | Audio | 9 | Xmp |
| 3 | Text | 10 | Icc |
| 4 | Other | 11 | C2pa |
| 5 | Image | 12 | MakerNotes |
| 6 | Menu | | |

Values 7–12 are revelo extensions (EXIF/IPTC/XMP/ICC/C2PA/maker-note streams).
A C consumer iterating `0..Stream_Max` (MediaInfo uses `Stream_Max = 7`) will
only see the MediaInfo-compatible kinds.

## Field aliases

`MediaInfo_Get` transparently resolves common field-name variants used by
different MediaInfo versions:

| Alias | Canonical name |
| --- | --- |
| `WritingApplication` / `Writing_Application` | `Encoded_Application` |
| `WritingLibrary` / `Writing_Library` | `Encoded_Library` |
| `MimeType` | `InternetMediaType` |
| `ColorPrimaries` | `colour_primaries` |
| `Codec` | `CodecID` |
| `SampleRate` / `Sampling_Rate` | `SamplingRate` |
| `Resolution` | `BitDepth` |
| `Channel(s)` / `Channel_s_` | `Channels` |

## Build

```sh
# Produces target/release/librevelo_cdylib.so (Linux) or .dylib / .dll
cargo build --release -p revelo-cdylib
```

Link as you would `libmediainfo`:

```sh
cc myapp.c -L./target/release -lrevelo_cdylib -o myapp
```

## What it extracts

Detection and parsing are delegated to `revelo-dispatcher` (180 parser function
pointers raced via rayon) and the full `revelo-core` engine:

- **Containers & codecs** — MP4/MOV, Matroska/WebM, MPEG-TS, AVI, WAV, MXF,
  and ~200 formats total, with AVC/HEVC/AV1/VP9 and AAC/AC-3/DTS/FLAC/Opus
  codec detail.
- **Photo metadata** — EXIF, IPTC, XMP, ICC, C2PA, and (via `revelo-parsers-tag`)
  maker-notes for Canon, Nikon, Fujifilm, Sony, Panasonic, and more.
- **HDR signalling** — HDR10+, Dolby Vision, HLG/PQ, Dolby Atmos/AC-4, IAMF.

## License

BSD-2-Clause — see [LICENSE](https://github.com/vbasky/revelo/blob/main/LICENSE).
