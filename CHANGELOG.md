# Changelog

## [0.4.4] - 2026-06-06

### Added

- **`duration_ms()` helper** in `revelo-core` — round-to-nearest duration
  computation from sample counts. Applied to WAV and MP3 parsers for more
  precise output. Ogg retains intentional truncation to match the mediainfo
  oracle.
- **STATUS.md** — project roadmap and status tracker, linked from the README.
- **`## Status` and `## License` sections** added to README.

### Fixed

- **Banner image URL** — crates.io now shows the revelo banner (was a broken
  relative path).

## [0.4.3] - 2026-05-31

### Added

- **Apple MakerNote support** — ~17 fields parsed clean-room from the on-disk
  IFD (cross-referenced against ExifTool's printed output, not its tables):
  `MakerNoteVersion`, `AEStable`/`AETarget`/`AEAverage`, `AFStable`,
  `AccelerationVector`, `FocusDistanceRange`, `ContentIdentifier`,
  `ImageCaptureType`, `LivePhotoVideoIndex`, `HDRHeadroom`,
  `SignalToNoiseRatio`, `PhotoIdentifier`, `ColorTemperature`, `CameraType`,
  `FocusPosition`. The default (BSD) build is unaffected by the GPL
  `exiftool-tables` feature.
- **ICC profile header fields** — `ProfileCMMType`, `ProfileVersion`,
  `ProfileClass`, `ColorSpaceData`, `ProfileConnectionSpace`, `ProfileDateTime`,
  `PrimaryPlatform`, `RenderingIntent`, `DeviceManufacturer`/`DeviceModel`,
  `ProfileID`/`Description`/`Copyright`, `MediaWhitePoint`, and the RGB matrix
  columns. The profile is located by its `acsp` signature, so embedded profiles
  (JPEG `APP2`, HEIC, raw `.icc`) are read correctly rather than from offset 0.
- **Composite/derived fields** — `ScaleFactor35efl`, `CircleOfConfusion`,
  `FieldOfView`, `HyperfocalDistance`, `LightValue`.
- **Extra GPS fields** surfaced in text output — `GPSSpeed`, `GPSImgDirection`,
  `GPSDestBearing`, `GPSHPositioningError` (and their reference tags).

### Fixed

- `parse_icc` read the wrong offset and emitted an empty `ICC_Profile : ()`;
  it now locates the profile correctly and no longer emits a spurious ICC
  section for files without a profile.
- Apple maker notes produced no output because the parser never skipped the
  14-byte `Apple iOS` header and the hand-written tag table had incorrect IDs;
  both are fixed.
- `parse_xmp` failed to find the XMP packet embedded in JPEGs (it read from the
  cursor over binary data); it now scans the whole file.
- JPEG camera `Model` was read past its NUL terminator (e.g. `ION230 F` →
  `ION230`).

### Changed

- **Single EXIF walker.** `revelo-parsers-tag` is now the sole EXIF / GPS /
  maker-note / ICC parser. The JPEG container parser no longer carries a
  duplicate EXIF/TIFF walk — it parses only JPEG structure (geometry, chroma
  subsampling, thumbnail), and the MediaInfo-vocabulary fields
  (`Recorded_Date`, `Encoded_Hardware_*`, the photographic `<extra>` block) are
  derived from the shared tag parse. Verified net-zero against a 104-file
  sample corpus.

## [0.4.2] - 2026-05-31

### Fixed

- **Panic-safety on malformed/truncated input**: guard MP4 `avcC`/`hvcC` parsing
  against truncated boxes; correct the SCTE-35 splice-header byte offsets and a
  `parse_splice_insert` off-by-one; checked/saturating size arithmetic in
  AVI/MKV/MP4/MXF (safe on 32-bit `wasm32`).
- **Parsing correctness on valid files**: honor TIFF byte order for EXIF
  FLOAT/DOUBLE values; decode ELF `e_type`/`e_machine` per the file's endianness;
  read AVC scaling-list `delta_scale` as signed Exp-Golomb; derive AV1 bit depth
  from `color_config()`; parse real VVC profile/tier/level and fix VVC RBSP
  emulation-prevention removal; fix `infer_raw_format` reading `Make` from the
  wrong stream.
- **Exporters**: XML-escape values and paths in ebuCore/MPEG-7/PBCore/RevTMD;
  NISO/FIMS now emit the parsed stream fields instead of an empty shell; teletext
  fills fields on partial packets; bound the VOB-PCM `LPCM` scan.

### Changed

- Rewrote per-crate READMEs and added crate-level docs across the workspace; added
  `exif`/`exiftool` keywords and richer descriptions for crates.io discoverability.
- Release pipeline now publishes the `revelo` facade and `revelo-exiftool-tables`
  (previously omitted from the publish list).

### Note

- First crates.io publish of the **`revelo`** facade crate.

## [0.4.1] - 2026-05-31

### Added

- **`revelo` facade crate** — a single `revelo` dependency replaces manually wiring
  `revelo-core`, `revelo-dispatcher`, and `revelo-parsers-tag`. The crate exposes:
  - `Metadata::from_file(path)` and `Metadata::from_bytes(bytes)` — detect format,
    parse container/codec metadata, and run EXIF/IPTC/XMP/ICC/C2PA tag extraction
    in one call
  - Typed per-stream accessors: `.general()`, `.video()`, `.audio()`, `.text()`,
    `.image()`, `.exif()`, `.iptc()`, `.xmp()` — each yielding `(&str, &str)` pairs
  - `MediaFile` type alias for `FileAnalyze` (the old name continues to work)
  - `Metadata::streams()` and `into_streams()` for access to the raw
    `StreamCollection`
  - Re-exports of `revelo_core`, `revelo_dispatcher`, and `revelo_parsers_tag`
    so dependents get the full API from one crate
  - Pass-through `exiftool-tables` feature that pulls the GPL/Artistic maker-note
    tables crate

### Changed

- README parser and crate counts are now auto-updated as part of the release
  workflow (script runs `cargo metadata` and counts `#[test]` annotations).
- Release notes generation strips the `v` prefix from the tag name before passing
  it to the GitHub CLI.

### Fixed

- Homebrew formula pushed to personal tap (`vbasky/homebrew-revelo`) instead of
  attempting an upstream `homebrew-core` PR (which cannot be automated reliably).
- Homebrew install command corrected in README to match the personal tap URL.

## [0.4.0] - 2026-05-31

### Added

- **EXIF/IPTC/XMP/ICC/C2PA/MakerNotes promoted to their own stream sections**
  (revelo extensions beyond MediaInfo's `stream_t`), matching how exiftool groups
  metadata by family-0 type rather than folding everything into General. EXIF
  streams from the container parser and the IFD walker are now merged into one.
- **IPTC IIM now anchored to the Photoshop 8BIM Image Resource Block** (resource
  `0x0404`) instead of scanning raw bytes — fixes false-positive datasets (e.g. a
  garbage `Headline`) matched in compressed image data.
- **Optional `exiftool-tables` feature** (off by default) — opt in to richer
  maker-note tag names and PrintConv value decoding sourced from ExifTool, for 14
  vendors (Apple, Canon, Casio, DJI, FLIR, FujiFilm, Minolta, Nikon, Olympus,
  Panasonic, Pentax, Samsung, Sigma, Sony). The tables live in a separate
  **GPL/Artistic** crate `revelo-exiftool-tables` (generated from ExifTool via
  `codegen/extract.pl`); the default revelo build stays BSD-2-Clause and uses the
  hand-written clean-room tables. Enabling the feature subjects the resulting
  binary to GPL/Artistic terms.

- **Canon maker-note decoding (under `exiftool-tables`)** — ExifTool-derived
  `ProcessBinaryData` sub-tables (CameraSettings, ShotInfo) are now decoded into
  individual named, value-converted tags. Two correctness fixes make this work on
  real files: the bare-IFD offset is no longer mis-detected when the first tag id
  is `0x0000` (broke every Canon file whose maker note starts that way, e.g. the
  IXUS), and a cross-validated **FixBase** recovers mis-based maker-note offsets in
  edited/re-muxed files (matching exiftool's "Adjusted MakerNotes base by N").
  Records are accepted only when their self-describing byte-count header validates,
  so a wrong offset decodes nothing rather than garbage. The header-less
  `FIRST_ENTRY 0` FocalLength sub-table is decoded only when FixBase confirmed the
  base. The variable-length AFInfo (0x0012) record (NumAFPoints-driven
  `AFAreaXPositions[N]` etc.) is hand-walked behind an exact total-size match, so a
  mis-laid record decodes nothing. The newer AFInfo2 (0x0026) / AFInfo3 (0x003C,
  SerialData) records — which lead with a self-validating `AFInfoSize` header and
  carry an `AFAreaMode` enum plus four `[N]` arrays — are decoded the same way. For
  Main tags revelo's older bespoke table lacks, the fuller ExifTool Main table is
  consulted under the feature. Further `ProcessBinaryData` sub-tables are decoded
  with per-table element size — MyColors and ContrastInfo (`int16`), TimeInfo and
  AspectInfo (`int32`), and FaceDetect3. ImageUniqueID (16-byte hex) and DateStampMode
  decode too. Validated against real camera samples: IXUS 400 31%→99%, PowerShot S40
  →99%, edited HDR files (AFInfo3, newer body) →100%, all with exact value matches.
- **EXIF / Interop / Canon Main tag names aligned to ExifTool** under
  `exiftool-tables` for the handful of divergences (`DateTime`→`ModifyDate`,
  `DateTimeDigitized`→`CreateDate`, `PhotographicSensitivity`→`ISO`,
  `PixelX/YDimension`→`ExifImage{Width,Height}`,
  `Interoperability{Index,Version}`→`Interop{Index,Version}`,
  `CanonOwnerName`→`OwnerName`, `CanonImageNumber`→`FileNumber`,
  `CanonThumbnailValidArea`→`ThumbnailImageValidArea`). Lens fields are
  deliberately not remapped (the lens-formatting post-pass keys off the names).
  With all of the above, clean Canon samples reach 99% tag parity with exiftool.

### Fixed

- **Maker-note parsing repaired for several vendors** (BSD core, all builds). The
  Fujifilm maker note read its IFD offset from the wrong byte (12 instead of 8) and
  mis-resolved value offsets — it now parses against the full block from the correct
  offset. Nikon Type 2/3 (embedded TIFF header at offset 10) is detected so COOLPIX
  bodies parse. Header-less big-endian Konica-Minolta IFDs are now byte-order
  detected. Under `exiftool-tables`, Nikon names are aligned to ExifTool. Validated
  against real samples: Fujifilm 68%→98-100%, Nikon COOLPIX 63%→95%, Konica-Minolta
  76%→97%.
- **Olympus "type-2" maker notes** (`OLYMPUS\0II…`) are now parsed: the main IFD plus
  its nested Equipment / CameraSettings / RawDevelopment / ImageProcessing / FocusInfo
  sub-IFDs (decoded via generated ExifTool tables under the feature).
- **Nikon AFInfo (0x0088) / FlashInfo (0x00A8)** decoded on COOLPIX bodies.
- **UNDEFINED (type 7) values** are read as a single trimmed blob instead of a string
  repeated from every byte offset (fixes e.g. Panasonic `InternalSerialNumber`).
- **Canon maker-note Main tag ids corrected** (BSD core, all builds): `0x000C` is
  the serial number (was mislabelled `CanonModelID`), `0x0010` is the model id
  (was `CanonThumbnailValidArea`), `0x0013` is the thumbnail valid area (was
  missing), `0x0015` is the serial-number format, `0x001C` is `DateStampMode` (was
  `CanonAFInfo`), and `0x0028` is `ImageUniqueID` rendered as 16-byte hex (was
  `CanonCRWParam`). The model-name lookup now applies to `0x0010` instead of the
  serial number at `0x000C`. Verified against exiftool's raw values.

### Changed

- **Dropped the `creatingLibrary` header from JSON and XML output.** The block
  (`name`/`version`/`url`) only identified the producing tool and carried nothing
  about the parsed file, so it is no longer emitted. JSON now opens directly at
  `"media"`; XML goes straight from the `<MediaInfo>` header to `<media>`. This is
  a deliberate divergence from `mediainfo`'s wire format — consumers that validate
  against the published mediainfo schema or read `creatingLibrary.version` will see
  a missing key. `revelo-diff` strips the oracle's `creatingLibrary` line before
  comparing, so the diff harness stays focused on per-stream differences.

## [0.3.1] - 2026-05-31

### Fixed

- **JSON output** now escapes control characters (U+0000–U+001F) as `\u00xx`
  instead of emitting raw bytes, producing valid JSON when binary metadata is
  present.
- **JFIFVersion** extracted from the JPEG APP0 segment (e.g. `1.01`).

## [0.3.0] - 2026-05-30

### Added

- **EXIF rewrite** — full IFD chain traversal (IFD0, ExifIFD, GPS IFD, InteropIFD,
  IFD1), 170+ tag names including EXIF, GPS, and maker-note tags parsed via IFD
  iteration; GPS coordinates computed as decimal degrees (`GPSLatitudeRef` /
  `GPSLongitudeRef`)
- **Lens specification formatting** — `LensSpec` field computed from
  `LensModel`, `MaxApertureAtMinFocal`, `MaxApertureAtMaxFocal`, `MinFocalLength`,
  `MaxFocalLength`, and `LensType`; `LensType` hex formatting
- **IPTC IIM enhanced** — full 0x1C-delimited IIM dataset parsing, cross-reference
  with IPTC-NAA tag keys via `parse_iim_buf` for datasets > 0x80 bytes on 0x83BB
  records
- **XMP enhanced** — additional EXIF field name mappings (ExifIFD, GPS, IIM
  cross-references) and `XMP_xmpMM_*` namespace tags
- **MakerNote stub** — `parse_makernote` infrastructure hook, returns `()` for now

## [0.2.3] - 2026-05-29

### Added

- CSV output format (`--csv`)
- Summary output mode (`--summary`) with aggregate stream statistics
- `--inform-version` and `--inform-timestamp` flags for text output provenance
- `--log-file <FILE>` flag for writing output to file
- `Format_Level_Inferred` computed field for AV1 — shows the minimum required
  level based on resolution and frame rate (e.g. `4.1` for 1080p60)

### Fixed

- AV1 `Format_Profile` now includes level (e.g. `Main@L5.3` instead of just `Main`)
- AV1 `Format_Info` now shows `AOMedia Video 1`
- AV1 sequence header parser no longer returns `None` when timing info present
- AV1 level now reads from codec config record instead of last operating point
- BPG test helper `unimplemented!()` panic for VSI values 21-34 bits
- MKV `Title` field now populated from `HANDLER_NAME` and `TITLE` tags

## [0.2.2] - 2026-05-29

### Added

- CLI migration to clap derive API with `--help`, `--version`, `-h`, `-V`, subcommand skeleton (inspect/diff/batch/verify/extract)
- `--video-only` and `--audio-only` flags for stream filtering
- `--stream KIND:INDEX` flag for selecting specific streams (repeatable)
- `--verify` flag with `IsComplete` field derived from parser truncation detection
- `--text` flag (previously documented but missing from the parser)
- `StreamCollection::filter_keep()` method in `revelo-core`
- `IsComplete` field in text output display
- Future improvement roadmap in README

### Fixed

- `revelo --help` and `revelo -h` now print help instead of falling through as a file path
- Bare `revelo` with no arguments prints help and exits 0 instead of erroring
- Unknown flags like `--foobar` are rejected with a proper error message
- `--demux` and `--trace` values are validated at parse time

### Changed

- Stream filtering and verify flags added to CLI struct
- `StreamCollection` gains `filter_keep()` for removing streams by kind and position
- Bumped MSRV to 1.85 in workspace Cargo.toml (already the minimum Rust version)

## [0.2.0] - 2026-05-29

### Added

- APV (Advanced Professional Video) parser (`revelo_parsers_video::apv`) — detects the `aPv1` signature, walks to the first frame/access-unit PBU, and fills profile\@level, dimensions, chroma format, bit depth, and colour description. Ported from MediaInfoLib's `File_Apv.cpp`.
- Ergonomic `Reader` API (`revelo_core::reader`) — fluent, value-returning byte/bits reader wrapping `FileAnalyze` (native `u8`/`u16`/`u32` types, `Option`/`?`-based reads, no out-parameters)
- `revelo_core::prelude` module
- Workspace lints table (`Cargo.toml`) — `unreachable_pub`, `clippy::unwrap_used`, `clippy::doc_markdown`, `clippy::cast_possible_truncation`, `clippy::cast_sign_loss`
- `rustfmt.toml` — edition 2024, max_width 100, reorder_imports
- `#![deny(unsafe_code)]` on all 12 non-cdylib crate roots
- Intra-doc links between `FileAnalyze`, `StreamCollection`, `ElementTree`, `StreamKind`
- `rust-toolchain.toml` — stable channel with `clippy`/`rustfmt`/`rust-docs`
- GitHub Actions CI workflow (`.github/workflows/ci.yml`) — fmt, check, clippy, test, doc
- `LICENSE` — BSD-2-Clause
- `.cargo/config.toml` — build configuration
- `justfile` — developer task runner
- `deny.toml` — license audit configuration
- `CHANGELOG.md`

### Changed

- Migrated parsers off the C++-style `FileAnalyze::get_*` out-parameter API to the ergonomic `Reader` API across the audio, video, image, text, and container crates (including the 3.4k-line `mp4`); behavior-preserving, verified by the workspace test suite
- Toolchain switched from nightly to **stable** — edition 2024 and let-chains are both stabilized, so no nightly features are required
- Renamed `diff-harness` → `revelo-diff`; it now uses `revelo_dispatcher::detect()` instead of a hand-maintained parser table
- Parser dispatch table (`revelo-dispatcher`) now carries inline format-name comments
- `revelo-util` `FromRadix`/`FromRadixSigned` traits changed to `pub(super)`; `revelo_util_re_export` re-exports to `pub(crate)`
- `[workspace.lints.rustdoc]` allows intentional byte-layout notation (`invalid_html_tags`, `broken_intra_doc_links`)
- Bare URLs in doc comments wrapped in `<>`
- CI no longer fails the build on warnings (`-D warnings` dropped); `cargo fmt --check` is reported but non-blocking

### Fixed

- IAMF parser: reject OBU headers that claim to extend past the buffer instead of panicking on an out-of-bounds payload slice

### Removed

- Dead `fill_str` method from `FileAnalyze`
- `try_into().unwrap()` in `ac3.rs` — replaced with direct indexing

## 0.1.0

- 177 parsers across 7 domain crates (archive, audio, container, image, tag, text, video)
- Core engine (`revelo-core`): `FileAnalyze` byte reader, `StreamCollection`, `ElementTree`, bitstream mode
- Parallel parser dispatch via `rayon` (`revelo-dispatcher`)
- 10 output formatters: XML, JSON, Text, EBUCore, MPEG-7, PBCore, NISO, FIMS, Graph, RevTMD
- C ABI shim (`revelo-cdylib`) — drop-in replacement for `libmediainfo`
- CLI (`revelo-cli`) with `--xml`, `--json`, `--multi-file`, `--demux`, `--trace`
- 579 unit tests
- Differential testing harness against `mediainfo` oracle
