# Code review findings

Consolidated from a per-file read of the workspace. Each item is **agent-reported
and must be verified against source before fixing** — some may be false alarms
(guards already present). Severity tiers:

- **T1** — produces wrong output on *valid* files.
- **T2** — panic / DoS / unsafe behavior on *malformed/truncated* input (relevant to the "parses attacker-controlled data" threat model).
- **T3** — value-accuracy gaps, dead code, stubs, non-conformant output.

Status: ☐ open · ☑ fixed · ✗ not-a-bug (verified safe)

---

## Tier 1 — wrong output on valid files

- ☑ **EXIF FLOAT/DOUBLE endianness ignored** — `revelo-parsers-tag/src/tags.rs` — **FIXED**
  - ✗ `read_tiff_u16`/`read_tiff_u32` were a **false alarm** — they already branch on the `bo` byte-order arg.
  - ☑ `read_exif_val` types 11 (FLOAT) / 12 (DOUBLE) hardcoded `from_le_bytes`, ignoring `bo`, and types 11/12 were missing from the multi-value loop (fell through to `read_tiff_u32`). Added byte-order-aware, bounds-checked `read_tiff_f32`/`read_tiff_f64`; used in both paths. Test `exif_float_honors_byte_order` added.
  - Impact: big-endian (MM) TIFF float/double EXIF values read with wrong byte order.
- ☐ **ICC `desc` vs `mluc` handling** — `tags.rs` (~ICC parser): description sliced with a fixed `-12` header offset that only fits `mluc`; v4 profiles using a different desc type produce a garbled string.
- ☑ **SCTE-35 bitfield offsets shifted by a byte** — `revelo-parsers-container/src/scte35.rs` — **FIXED**
  - Confirmed against the spec: `protocol_version` is the full byte 3; `encrypted` is byte 4 bit 7; `pts_adjustment` spans bytes 4–8; `cw_index` = byte 9; tier/command_length over bytes 10–12; `splice_command_type` = byte 13; command body starts at byte 14. Rewrote all offsets and added a `len < 14` guard (section_length alone can be 4 → could panic reading byte 13).
- ◐ **VVC `remove_epb_3` no-op + profile/level stub** — `revelo-parsers-video/src/vvc.rs`
  - ☑ `remove_epb_3` **FIXED**: the old code pushed the `0x03` through unchanged, so it removed *no* emulation-prevention bytes (the RBSP came out bit-misaligned). Now strips the inserted `0x03` correctly.
  - ☐ `parse_sps` still hardcodes `profile_idc`/`level_idc` and uses a simplified chroma read — this is missing **profile_tier_level** parsing, i.e. feature work, not a one-line fix. Left open.
- ☑ **ELF endianness ignored** — `revelo-parsers-archive/src/archives.rs` — **FIXED**
  - `e_type`/`e_machine` now decode BE when `EI_DATA` (byte 5) == 2.
- ☑ **`infer_raw_format` reads `Make` from wrong stream index** — `tags.rs` — **FIXED**
  - Confirmed `fill_tags` writes EXIF (incl. `Make`) to General index 0; changed the read from index 1 → 0.
- ☑ **AVC scaling-matrix `delta_scale` uses unsigned exp-Golomb** — `revelo-parsers-video/src/avc.rs` — **FIXED**
  - Added `read_se` (signed Exp-Golomb) and used it for `delta_scale` per H.264 §9.1.1.
- ☑ **AV1 bit depth from coarse profile guess** — `revelo-parsers-video/src/av1.rs` — **FIXED**
  - Now derives bit depth from `high_bitdepth` + `twelve_bit` per `color_config()` (also clears the two unused-var warnings).

## Tier 2 — panic / DoS on malformed input

- ☑ **MP4 `parse_avcc` / `parse_hvcc` index after short `read_raw`** — `mp4.rs` — **FIXED**
  - `read_raw` returns exactly `body_size` bytes *or* an empty slice on truncation (verified). The guards checked the requested `body_size`, then indexed the empty returned slice → panic on a truncated avcC/hvcC. Added a `body.len()` guard after the read (≥4 / ≥23); after it, `body.len() == body_size`, so the existing bounds checks hold.
- ☑ **`scte35.rs` `parse_splice_insert` off-by-one** — **FIXED**
  - Guard was `*pos + 4 > buf.len()` but it reads `buf[*pos + 4]` (needs 5 bytes) → panic when `*pos + 4 == buf.len()`. Changed to `*pos + 5 > buf.len()`.
- ✗ **`revelo-core/src/data_helpers.rs`** — **not a live bug.** The free fns are unchecked, but the only caller (`dsf.rs`) guards `h.len() < 80` before slicing fixed ranges and `full.len() >= 92` for the data chunk. Documented contract, no reachable panic.
- ✗ **`truehd.rs`** — **false alarm.** `buf[6]` is inside `if buf.len() >= 8`; the scan loop bound `8..buf.len().saturating_sub(1)` keeps `i+1 < buf.len()`.
- ✗ **`gain_map.rs`** — **false alarm.** Reads are behind `h.len() < 22` and `off + 40 > h.len()` guards.
- ✗ **`bitstream.rs` `get()` / `skip()`** — the top-of-fn underrun guard (`how_many > buffer_size + last_byte_size`) bounds the byte cursor; no reachable OOB. (Re-verify if the guard is ever weakened.)
- ☑ **Box/chunk size overflow (32-bit / wasm32)** — **FIXED** (checked/saturating math; benign on 64-bit, real on wasm32):
  - `avi.rs` `data_start.checked_add(size)` folds in the past-end check → no wrap/infinite walk.
  - `mkv.rs` Vorbis `id_len` → `id_len.saturating_add(4)` in the length check.
  - `mp4.rs` `usize::try_from(size64).unwrap_or(usize::MAX)` (saturate, then the region bounds check rejects).
  - `mxf.rs` BER long-form: reject `num_bytes > 8`; `i.saturating_add(16).saturating_add(length)` in the compare.
- ☑ **`teletext.rs` partial packet returned `true` without fields** — **FIXED.** Fields are now filled once the sync bytes match (a partial packet is still teletext).
- ☑ **`pcm_vob.rs`** — **FIXED.** `LPCM` scan bounded to the first 64 bytes (avoids O(n) scan + spurious matches deep in the file).
- ✗ **`mpeg_ts.rs` `.first()/.last().unwrap()`** — guarded by the preceding `len < 2` check; not reachable.

## Tier 3 — value accuracy, dead code, stubs

### Value accuracy
- ☐ `rkau.rs:49` duration `source_bytes * 1000 / 4 / sample_rate` hardcodes 4 bytes/sample (16-bit stereo only); ignores header `channels`/`bits_per_sample`. (Matches a C++ bug.)
- ☐ `au.rs:113` duration formula correct only for 8-bit mono; wrong for multi-channel/higher bit depth.
- ☐ `mp3.rs:363` LAME nominal-bitrate byte read at a fixed offset without first detecting the `LAME`/Xing flag layout → can return nonsense bitrate.
- ☐ `aac.rs:29` sample rate + channels decoded into `_sr`/`_ch` but never written to the Audio stream.
- ☐ `dts.rs:213` frame count computed then discarded (`let _ = frame_count;`); FrameCount never emitted.
- ☐ `computed_fields.rs:283` AV1 level 6.0 `max_sps` copy-pasted from level 5.2 (too low).
- ☐ `computed_fields.rs:291` `h16 = max_dim * 9/16` assumes landscape; portrait videos underestimate `pic_size` → wrong inferred level.
- ☐ `computed_fields.rs:293` `(pic_size * fps) as u64` can silently saturate/truncate on overflow.
- ☐ `computed_fields.rs:136` `BitRate_Minimum = BitRate / 2` — speculative, no spec basis, silently surfaced as a real field.
- ☐ `mpc_sv8.rs` — no SH (stream header) packet parsing; SamplingRate/Channels/BitRate all absent.
- ☐ `scream_tracker3.rs:56` field key typo `"Paterns count"` (matches C++ source).
- ☐ `replay_gain.rs:16` gain scale (`* 0.01`) may not match the LAME spec revision; audit vs oracle.

### Export formatters
- ☑ `ebu_core.rs`, `mpeg7.rs`, `pbcore.rs`, `revtmd.rs` — **FIXED.** Added a shared `xml_escape` (`& < >`) applied to all values and file paths (+ regression test).
- ◐ Same files — element names still derived by lowercasing/stripping field keys (or `StreamKind` `Debug`); not aligned with the real ebuCore/MPEG-7/PBCore vocabularies → well-formed but **not schema-valid**. Full vocab mapping needs the schemas (left open).
- ☑ `niso.rs`, `fims.rs` — **FIXED (pragmatic).** Now emit the actual stream fields (escaped, well-formed) instead of an empty shell, with tests. Schema-conformant vocab mapping still needs the MIX/FIMS schemas (documented in-code).
- ☐ `summary.rs:95` multi-video "range" can display `min = u64::MAX` when no width/height parses.
- ☐ `csv.rs:102` per-field double scan of the extras iterator → O(n²) for streams with many extras.

### Dead code / warnings
- ☑ `hevc.rs` `parse_sps` (dead 195-line near-duplicate) — **REMOVED.**
- ☑ `av1.rs` unused `high_bitdepth` / `twelve_bit` — **RESOLVED** (now used by the bit-depth fix).
- ☐ `mpeg_ts.rs:869` `extract_avc_frame_rate` & friends are `#[allow(dead_code)]` and non-functional.
- ☐ `canopus.rs:18` detection array has `"CHQX"` three times (dead duplicate entries).
- ☐ `dolby_vision_rpu.rs:70` RPU prefix bytes read into `_` and never validated → any HEVC NAL type 62 accepted (false positives).

### FFI (cdylib)
- ☐ No `MediaInfo_*` string-free export; C callers can't know how to free returned `CString` pointers — and on Windows the Rust vs caller CRT allocator may differ, making `free()` unsafe.
- ☐ `MediaInfo_Get` / `MediaInfo_Option` use `CString::new(...).unwrap_or_default()` → an internal NUL yields an empty string indistinguishable from a genuinely empty field.

### Dev-tool harnesses
- ☐ `revelo-diff` `diff_lines` uses a `HashSet`, so duplicate identical lines are mis-counted (multiplicity collapsed); LCS `dp` is `u32` (overflow > 65 535 lines — not realistic).
- ☐ `revelo-exif-diff` sweeps `Iptc`/`General` in addition to `Exif`, which can inflate the matched-tag count and mask gaps.
