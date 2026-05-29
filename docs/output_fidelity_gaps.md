# Output Fidelity Gaps

revelo's correctness bar is **byte-match with the `mediainfo` oracle**, not
"a parser exists" or "tests pass". This doc records the known gaps between
revelo's XML and the oracle's, grouped by root cause, so they're not
mistaken for missing parsers.

## How to measure

The differential harness diffs revelo's XML against the installed
`mediainfo`. Use `--strict` for true byte-fidelity — the default diff is
set-based and hides field-ordering / duplicate differences:

```sh
cargo run --release -p revelo-diff -- --strict /path/to/file
```

`BYTE-EQUAL N/N` means line-for-line identical. `M only in oracle, K only
in rust` is the gap to close. A sample is only a valid target if
`mediainfo --Output=XML <file>` actually recognizes the format (non-empty
`<Format>`); otherwise there's nothing to match (see the "unverifiable"
formats in the README).

## Snapshot (2026-05-28, synthetic ffmpeg sample matrix)

Byte-equal (`--strict` 0/0): images (JPEG/PNG/BMP/TIFF), Ogg/Vorbis, AC-3,
DCP PKL. The MP4 family is within a few lines:

| File | strict gap | dominant cause |
|---|---|---|
| h264_aac.mp4 | 3 | precision (StreamSize) + blocked field |
| hevc_aac.mp4 | 4 | precision + blocked field |
| audio.m4a | 2 | precision |
| h264_aac.mov | 9 | precision + a few container fields |
| mpeg4_mp3.avi | 21 | MP3-frame extraction + precision |
| vp9_opus.mkv | 20 | per-frame VP9 header + precision |
| h264_aac.ts | 49 | PES elementary-stream extraction |
| h264_aac.flv | 58 | FLV-tag elementary-stream extraction |
| av1_opus.mp4 | 66 | AV1 OBU sample parsing |

(Field *ordering* is no longer a gap for MP4/MOV — that work is done; these
remaining diffs are value/presence.)

## Category A — Precision (per-sample / frame-accurate math)

The values are close but not exact because the oracle computes them from a
full per-sample/per-frame scan that revelo approximates from headers.

- **AAC `StreamSize` (edit-list trim).** Oracle reports the elementary size
  *after* the edit list trims encoder priming/padding frames
  (e.g. 8722 vs the raw 8992; the raw value is correct as `Source_StreamSize`).
  The byte delta depends on the exact sizes of the trimmed frames, which
  needs per-sample `stsz` aligned to the edit list — revelo stores only the
  first/last sample sizes, so there's no clean formula (2-frame trim loses
  270 B here, a 1-frame trim elsewhere loses 325 B).
- **Duration / SamplingCount / BitRate** off-by-small (±1 ms, ±a few samples,
  ±tens of bps) from header-derived vs frame-counted timing.
- **AVI Video `BitRate`** differs by ~the per-frame variance (header average
  vs oracle's measured value).

## Category B — Elementary-stream extraction not yet wired

The largest gaps need pulling the codec bitstream out of the container's
data chunks and parsing it — the same pattern as the AVC-in-MP4 mdat SEI
scan, repeated per container. Until then these fields are missing/approximate:

- **MPEG-TS** — PES payload parsing for the elementary AVC/AAC streams
  (the largest non-AV1 gap).
- **VP9-in-MKV/WebM** — the VP9 uncompressed frame header (profile, real
  colour space/range, bit depth) lives in the first cluster SimpleBlock;
  revelo currently falls back to codec-private/defaults.
- **FLV** — per-tag AVC/AAC bitstream details.
- **AVI MP3** — `BitRate` when `nAvgBytesPerSec == 0`, and the LAME
  `Encoded_Library` string, both require the first MP3 frame from the movi
  chunk.
- **AV1-in-MP4** — fuller OBU sequence-header parsing from sample data.

This is meaningful infrastructure (locate the first sample/PES/block via the
index tables, then parse the elementary stream) and touches the large,
frequently-edited container parsers.

## Category C — Blocked fields (parser never fills them)

Emitted by the oracle but not derivable from what revelo currently parses:

- `FrameRate_Mode_Original` (VFR flag) on Video.
- `Format_Settings_SBR` for HE-AAC (AOT 5/29) — only the AAC-LC "No (Explicit)"
  case is emitted, since there's no HE-AAC sample to validate the others.

## Unverifiable formats

`File_Apv`, `File_Av2`, `File_Ancillary`, `File_Umf` are intentionally not
ported — no way to produce a sample *and* oracle output to diff against. See
the README "Blocked — unverifiable against the oracle" note.
