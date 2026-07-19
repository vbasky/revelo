# revelo status & roadmap

revelo today is a **pure-Rust media metadata parser** covering 180+ formats
across containers, video codecs, audio codecs, tags, and archives. All parsers
are validated against the `mediainfo` oracle for byte-equal XML output. This
document tracks what's covered, what's missing, and what's planned.

Priorities are ordered by impact; checkboxes track status. Nothing here is a
commitment to a date.

## Status snapshot

**Covered:** 180+ parsers (containers, video, audio, tags, text, archives,
images); 10 export formats (XML, Text, JSON, YAML, HTML, CSV, EBUCore, MPEG-7,
PBCore, NISO, FIMS); `detect()` auto-format matching; `race + walk` parallel
detection; `#![deny(unsafe_code)]` workspace-wide; C ABI (`revelo-cdylib`, with a
`catch_unwind` panic firewall); WASM support; CLI with stream filtering, glob /
multi-file batch (NDJSON), container verification, format metadata, and log file.

**Blocked (unverifiable):** APV, AV2, Ancillary (SMPTE 436 VANC), Ikegami UMF —
no obtainable oracle samples exist for these formats, so the differential harness
can't validate a port.

**Not covered:** see tiers below.

---

## P0 — correctness fixes (small, do first)

- [x] **Duration calculation precision.** Standardized to round-to-nearest
      `duration_ms()` helper in `revelo-core` — applied to WAV and MP3 parsers.
      Ogg retains intentional truncation to match the mediainfo oracle.
- [x] **Elementary-stream extraction.** All four targets wired and validated
      against the mediainfo oracle on ffmpeg-generated samples:
      - **MP4 AV1** — `av01` sample entry + `av1C` box → sequence-header colour
        decode. av1.mp4 parity 63→94 matching lines.
      - **MPEG-TS AVC/AAC** — first-SPS/PPS/SEI scan of the PES accumulator →
        `parse_avc_sps`, plus SDT-derived Menu and PCR/PTS timing. ts.ts 59→93
        matching, **0 spurious** rust lines.
      - **MKV/WebM VP9** — first-keyframe block-payload decode via `parse_vp9`.
        vp9.mkv **byte-equal (0/0)**, vp9.webm **byte-equal (0/0)**.
      - **FLV** — header-only parser replaced with a full tag demuxer (AVC
        `avcC`→SPS, AAC AudioSpecificConfig). flv.flv 21→72 matching.
      Remaining gaps are MediaInfo-internal bitrate/StreamSize estimation
      heuristics, left unfabricated.
- [~] **Blocked field validation.** `Format_Settings_SBR` is now emitted by the
      FLV AAC path (from the AudioSpecificConfig). `FrameRate_Mode_Original`
      remains blocked: mediainfo recovers a "was VFR" signal that isn't present
      in a normalized (uniform-`stts`) sample table, so synthesizing it risks
      mislabelling genuine CFR files. The spurious equal-copy the computed layer
      used to emit was removed (see hardening below). Still needs a real-world
      VFR-normalized sample to validate a correct derivation.
- [x] **Malformed input hardening.** The read layer (`FileAnalyze`/`Reader`/
      `byte_source`) is panic-safe by construction (guarded reads degrade to
      `0`/empty + a truncated flag). Fixed the residual `flac` short-block
      unsigned-underflow; added a **`catch_unwind` panic firewall** at the
      `extern "C"` cdylib boundary (a parser panic there was UB); added a
      **fuzz/truncation sweep** that runs all 180 parsers against truncated
      magics, random, and degenerate buffers (`revelo-dispatcher` tests).
- [x] **Duration calculation precision (fragmented).** Fragmented MP4
      (`moof`/`traf`/`trun` with `mvex`/`trex` defaults, `empty_moov`) is now
      parsed: `trun` sample counts/durations/sizes are aggregated per track_ID
      and fed to tracks whose `stbl` is empty, driving Duration, FrameRate,
      FrameRate_Mode (VFR), FrameCount, and StreamSize. frag.mp4 parity 71→83
      matching lines. Remaining gaps are the x264 `Encoded_Library` SEI (needs
      an `stco`, absent in fragmented files) and exact bitrate rounding.
      Non-fragmented files are untouched (the merge only fires on an empty
      `stbl`). Segmented MXF was already unaffected.

## P1 — output & reporting

- [x] **YAML export.** `--yaml`/`-y`; mirrors the JSON structure, all keys and
      values emitted as double-quoted scalars. `revelo-export::to_yaml`.
- [x] **HTML report.** `--html`; self-contained (inline CSS, no JS —
      `<details>`/`<summary>` collapsibles), summary cards + per-stream tables,
      theme-aware. `revelo-export::to_html`.
- [x] **Glob / batch processing.** `revelo --json "**/*.mp4"` walks trees;
      multi-file JSON defaults to NDJSON (one compact object per line),
      `--json-array` wraps a single array; other formats concatenate. Single-file
      output is byte-identical to before.

## P2 — extraction & diffing

- [ ] **Diff mode.** `revelo --diff a.mkv b.mkv` to show which fields differ
      between two files (user-facing, distinct from the harness-oriented
      `revelo-diff`).
- [ ] **Cover art / attachment extraction.** `--extract-attachments` flag for MKV
      Attachments, MP4 Cover boxes, and ID3 APIC frames.
- [ ] **Subtitle extraction.** Dump subtitle streams to SRT/VTT from any container.
- [ ] **Thumbnail / keyframe offset.** Report byte offset of the first keyframe
      (metadata-position only, no decoding).

## P3 — IO abstraction fit & finish

The `v0.5.0` release shipped `ReadBackend` (an enum with `Slice` and `Mapped`
variants) and a `ByteSource` trait. Two streaming-related items are planned but
require careful design.

### Streamed variant (`Read + Seek`)

- [ ] **`ReadBackend::Streamed`** — wrap a `Read + Seek` handle in a sliding
      window (~256 KiB). On every read, check whether the requested range falls
      within the current window; if not, `seek()` + `read_exact()` to shift it.

  **Effort:** ~200 lines in `byte_source.rs`. Zero parser changes — all reads
  still go through `ByteSource::slice_at()`.

  **The real cost is window sizing.** Parsers do two kinds of reads:
  1. **Sequential** (`get_b*`, `skip_*`) — advance the cursor forward, 1–8 bytes
     at a time. A 64 KiB window handles this trivially.
  2. **Random** (`peek_raw_at`, `peek_magic`) — jump to absolute file offsets
     (e.g. MP4 `stco`/`co64` → `mdat`, JPEG EXIF IFD pointer chasing, Matroska
     `SeekHead` → distributed elements). Every backwards jump triggers a window
     shift. For seekable sources this is fine (one syscall); for non-seekable it
     is a hard problem.

  **Window strategy:** start with a 256 KiB window; grow on cache miss; cap at
  some reasonable max (e.g. 8 MiB). For metadata parsing, most random accesses
  target the header region, so the window rarely shifts after the initial fill.

### Non-seekable streaming (chunked / forward-only)

- [ ] **Chunked parsing** — accept a forward-only byte source (pipe, TCP stream)
      without requiring random access. **This is a fundamentally different parser
      model** and likely not worth the complexity for this codebase.

  **Why it is hard:**

  1. `peek_raw_at(offset, n)` needs bytes that may have already passed or not yet
     arrived. Parsers that use absolute offsets break on non-seekable streams.
  2. Affected parsers: MP4 (`stco`/`co64` → `mdat`), JPEG (EXIF IFD pointer
     chasing), Matroska (`SeekHead` → elements), RIFF chunks (size-declared
     skipping), MPEG-TS (random PID selection).
  3. Solutions: (a) buffer everything before the furthest-backward jump,
     defeating the purpose of streaming, or (b) maintain per-format streaming
     variants that work forward-only, doubling the parser surface.

  **The practical answer:** metadata extraction is inherently random-access.
  Non-seekable sources should be buffered entirely first and then parsed via
  `ReadBackend::Slice`. This is MediaInfoLib's model too — it requires the
  caller to provide the full buffer.

## P4 — bindings & ecosystem

- [ ] **Python bindings** via PyO3 — natural fit for the media analysis audience.
- [ ] **NPM package** — WASM builds already compile; a documented JS API and NPM
      release would enable browser-side media inspection.
