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

## P0 — fidelity gaps

- [ ] **Elementary-stream extraction.** Wire PES payload parsing for MPEG-TS
      (AVC/AAC), VP9 frame headers in MKV/WebM, FLV per-tag AVC bitstream, and
      AV1 OBU sequence headers in MP4. These close the remaining ~10 divergence
      gaps against mediainfo.
- [ ] **Blocked field validation.** `FrameRate_Mode_Original` and
      `Format_Settings_SBR` need real-world test samples.

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

## P3 — bindings & ecosystem

- [ ] **Python bindings** via PyO3 — natural fit for the media analysis audience.
- [ ] **NPM package** — WASM builds already compile; a documented JS API and NPM
      release would enable browser-side media inspection.
