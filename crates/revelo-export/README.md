# revelo-export

Output formatters for [revelo](https://github.com/vbasky/revelo) — a fast,
safe, pure-Rust port of [MediaInfoLib](https://mediaarea.net/en/MediaInfo) for
extracting technical and tag metadata from media files.

Part of the [**revelo**](https://github.com/vbasky/revelo) project — see the
[project README](https://github.com/vbasky/revelo#readme) for the full picture.

## What it does

`revelo-export` turns a parsed `StreamCollection` into one of eleven output
formats. The two primary formats — XML and JSON — are designed for
byte-for-byte compatibility with MediaInfoLib's output, enabling automated
regression diffing against the reference implementation. The remaining formats
cover human-readable reports, pipeline-friendly CSV, archival XML vocabularies,
and developer tooling.

## Formatters

| Function | Format | Notes |
| --- | --- | --- |
| `to_xml` | XML | MediaInfo schema v2.0; field order and Duration rendering match the oracle |
| `to_json` | JSON | MediaInfo JSON layout; `creatingLibrary` header omitted intentionally |
| `to_text` | Text | Human-readable; friendly labels and humanised values (`5.26 MiB`, `734 kb/s`) |
| `to_csv` | CSV | Per-kind sections; RFC 4180 escaping; pipe-friendly |
| `to_summary` | Summary | Compact aggregate: codec lists, resolution ranges, channel/sample-rate sets |
| `to_ebu_core` | EBU MXF Core Metadata | `ebucore:ebuCoreMain` XML (EBU Core 2015 namespace) |
| `to_mpeg7` | MPEG-7 | `MediaInformation` XML (MPEG-7 2004 schema) |
| `to_pbcore` | PBCore 2.x | `pbcoreDescriptionDocument` XML |
| `to_niso` | NISO Z39.87 | Stub XML skeleton |
| `to_fims` | FIMS 1.0 | Stub XML skeleton |
| `to_graph` | Graphviz DOT | Stream/field relationship graph for debugging |
| `to_revtmd` | RevTMD | Custom revelo preservation XML |

## Usage

Add the dependency:

```toml
[dependencies]
revelo-export = "0.4"
revelo-core   = "0.4"
```

Call any formatter with a `StreamCollection` and the file path:

```rust,no_run
use revelo_core::StreamCollection;
use revelo_export::{to_xml, to_json, to_text, to_csv, to_summary};

// StreamCollection comes from revelo-core after parsing a file.
let streams: StreamCollection = todo!("parse your file here");
let path = "/media/video.mp4";

// Pick any (or several) formats:
let xml     = to_xml(&streams, path);
let json    = to_json(&streams, path);
let text    = to_text(&streams, path);
let csv     = to_csv(&streams, path);
let summary = to_summary(&streams, path);
```

The archival and graph formatters follow the same signature:

```rust,no_run
use revelo_core::StreamCollection;
use revelo_export::{to_ebu_core, to_mpeg7, to_pbcore, to_revtmd, to_graph};

let streams: StreamCollection = todo!();
let path = "/media/video.mp4";

let ebu   = to_ebu_core(&streams, path);
let mpeg7 = to_mpeg7(&streams, path);
let pb    = to_pbcore(&streams, path);
let rtmd  = to_revtmd(&streams, path);
let dot   = to_graph(&streams);  // no file_path argument
```

## Format details

### XML and JSON

Field ordering per stream kind mirrors MediaInfoLib's `MediaInfo_Config_PerPackage`
definitions. `Duration` family fields (stored as integer milliseconds internally)
are emitted as decimal seconds with three fraction digits — e.g. `1492` →
`1.492`. Fields that do not appear in the canonical order fall through in
insertion order after the canonical ones. Both formats wrap secondary or
side-channel fields in an `extra` block (`<extra>` in XML, `"extra":{}` in
JSON).

### Text

Uses MediaInfo's friendly display labels and humanised value rendering:
file sizes in MiB/GiB, durations as `1 min 0 s`, bit rates as `734 kb/s`,
frame rates with `FPS` suffix, channel counts with `channels`, bit depth with
`bits`, pixel dimensions with `pixels`. The format profile and level are
combined (`Constrained Baseline@L3`); additional audio features are folded into
the Format line (`AAC LC`).

### CSV

Each stream kind gets a `# KindName` comment header, a field-name row
(`StreamIndex,Field1,Field2,...`), and one data row per stream position.
Values containing commas, quotes, or newlines are double-quote enclosed per
RFC 4180. Absent fields are emitted as empty cells.

### Summary

A compact human-readable aggregate: container format, file size, and duration
at the top, followed by per-kind sections listing unique codec names, resolution
ranges (for video), channel and sample-rate sets (for audio), and subtitle
format lists (for text tracks).

## License

BSD-2-Clause — see [LICENSE](https://github.com/vbasky/revelo/blob/main/LICENSE).
