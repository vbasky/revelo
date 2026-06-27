# revelo status & roadmap

revelo today is a **pure-Rust media metadata parser** covering 180+ formats
across containers, video codecs, audio codecs, tags, and archives. All parsers
are validated against the `mediainfo` oracle for byte-equal XML output. This
document tracks what's covered, what's missing, and what's planned.

Priorities are ordered by impact; checkboxes track status. Nothing here is a
commitment to a date.

## Status snapshot

**Covered:** 180+ parsers (containers, video, audio, tags, text, archives,
images); 8 export formats (XML, Text, JSON, EBUCore, MPEG-7, PBCore, NISO,
FIMS); `detect()` auto-format matching; `race + walk` parallel detection;
`#![deny(unsafe_code)]` workspace-wide; C ABI (`revelo-cdylib`); WASM support;
CLI with stream filtering, container verification, format metadata, and log file.

**Blocked (unverifiable):** APV, AV2, Ancillary (SMPTE 436 VANC), Ikegami UMF —
no obtainable oracle samples exist for these formats, so the differential harness
can't validate a port.

**Not covered:** see tiers below.

---

## P0 — correctness fixes (small, do first)

- [x] **Duration calculation precision.** Standardized to round-to-nearest
      `duration_ms()` helper in `revelo-core` — applied to WAV and MP3 parsers.
      Ogg retains intentional truncation to match the mediainfo oracle.
- [ ] **Elementary-stream extraction.** Wire PES payload parsing for MPEG-TS
      (AVC/AAC), VP9 frame headers in MKV/WebM, FLV per-tag AVC bitstream, and
      AV1 OBU sequence headers in MP4. These close the remaining ~10 divergence
      gaps against mediainfo.
- [ ] **Blocked field validation.** `FrameRate_Mode_Original` and
      `Format_Settings_SBR` need real-world test samples to validate against the
      oracle.
- [ ] **Malformed input hardening.** Audit all parsers for panic safety on
      truncated or fuzzed input — every `read_*` path must return `Err`, not
      panic.
- [ ] **Duration calculation precision.** Review `Duration` / `PlayTime` field
      computations across fragmented containers (MP4 fragmented, segmented MXF)
      for edge-case rounding mismatches against mediainfo.

## P1 — output & reporting

- [ ] **YAML export.** YAML output format for pipeline integration.
- [ ] **HTML report.** Self-contained visual report with collapsible sections and
      summary cards.
- [ ] **Glob / batch processing.** `revelo --json "**/*.mp4"` for directory trees,
      output as NDJSON or array.

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
