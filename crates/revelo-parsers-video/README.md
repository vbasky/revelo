# revelo-parsers-video

Video-codec parsers for the [**revelo**](https://github.com/vbasky/revelo)
media-analysis library — a fast, safe, pure-Rust port of
[MediaInfoLib](https://mediaarea.net/en/MediaInfo).

Each parser is a focused `fn(&mut FileAnalyze) -> bool` that identifies its
codec by magic bytes or structural heuristics, then fills the relevant
`Video`/`General` fields on the `FileAnalyze` context. Parsers are registered
in the revelo dispatcher and are not normally called directly by application
code.

Part of the [**revelo**](https://github.com/vbasky/revelo) project — see the
[project README](https://github.com/vbasky/revelo#readme) for the full picture.

## Normal usage

Application code should go through the [`revelo`](https://crates.io/crates/revelo)
facade crate, not this crate directly:

```toml
[dependencies]
revelo = "0.5"
```

This crate is a public implementation detail of the revelo workspace, exposed
separately so that container-parser crates can depend on it without pulling in
the entire stack.

## Supported codecs

| Parser function | Codec / Format | Notes |
| --- | --- | --- |
| `parse_afd_bar_data` | AFD/Bar Data | SMPTE ST 2016-1 active format descriptor |
| `parse_aic` | AIC | Apple Intermediate Codec |
| `parse_apv` | APV | Advanced Professional Video (intra-only, ISO/IEC 23094-10) |
| `parse_av1` | AV1 | AOMedia Video 1 |
| `parse_av1_from_codec_config` | AV1 (codec config) | AV1 from codec private / configuration record |
| `parse_avc` | AVC / H.264 | Advanced Video Coding (ISO/IEC 14496-10) |
| `parse_avc_sps` | AVC SPS | Parse a raw AVC Sequence Parameter Set NAL |
| `parse_avs` | AVS | Audio Video Standard (Chinese national standard) |
| `parse_avs3` | AVS3 | Audio Video Standard 3 (IEEE 1857.10) |
| `parse_canopus` | Canopus HQ/HQX | Grass Valley / Canopus intermediate codec |
| `parse_cineform` | CineForm | GoPro CineForm / CFHD wavelet codec |
| `parse_dirac` | Dirac | BBC Dirac / VC-2 wavelet codec |
| `parse_dolby_vision` | Dolby Vision | Dolby Vision layer metadata |
| `parse_dv_rpu` | Dolby Vision RPU | Dolby Vision Reference Processing Unit |
| `parse_ffv1` | FFV1 | FFmpeg lossless video codec |
| `parse_flic` | FLIC | Autodesk FLIC / FLI / FLC animation |
| `parse_fraps` | Fraps | Fraps real-time screen-capture codec |
| `parse_h263` | H.263 | ITU-T H.263 video |
| `parse_hdr_vivid` | HDR Vivid | Chinese HDR Vivid (UWA T/UWA 005) |
| `parse_hevc` | HEVC / H.265 | High Efficiency Video Coding (ISO/IEC 23008-2) |
| `parse_hevc_sps` | HEVC SPS | Parse a raw HEVC Sequence Parameter Set NAL |
| `parse_huffyuv` | HuffYUV | Lossless YUV codec |
| `parse_lagarith` | Lagarith | Lagarith lossless codec |
| `parse_mpeg2` | MPEG-2 Video | ISO/IEC 13818-2 |
| `parse_mpeg2_sequence_header` | MPEG-2 sequence header | Parse a raw MPEG-2 sequence header |
| `parse_mpeg4v` | MPEG-4 Visual | ISO/IEC 14496-2 (DivX, Xvid, …) |
| `parse_prores` | Apple ProRes | ProRes 422 / 4444 / RAW families |
| `parse_theora` | Theora | Xiph Theora video |
| `parse_vc1` | VC-1 | SMPTE VC-1 / WMV3 |
| `parse_vc1_codec_private` | VC-1 codec private | Parse VC-1 codec private data |
| `parse_vc1_sequence_header` | VC-1 sequence header | Parse a raw VC-1 sequence header |
| `parse_vc3` | VC-3 / DNxHD | Avid DNxHD / DNxHR |
| `parse_vp8` | VP8 | Google VP8 |
| `parse_vp9` | VP9 | Google VP9 |
| `parse_vp9_codec_config` | VP9 codec config | VP9 from a codec configuration record |
| `parse_vvc` | VVC / H.266 | Versatile Video Coding (ISO/IEC 23090-3) |
| `parse_y4m` | YUV4MPEG2 | Y4M raw video container |

### Rich info types

Several parsers expose structured info types alongside the `parse_*` function:

| Type | Codec |
| --- | --- |
| `AvcInfo`, `EncoderInfo` | AVC |
| `Av1Info` | AV1 |
| `HevcInfo` | HEVC |
| `Mpeg2Info`, `Mpeg2Profile`, `Mpeg2Level` | MPEG-2 |
| `ProResInfo` | ProRes |
| `Vc1Info`, `Vc1Profile`, `Vc1Level` | VC-1 |
| `VvcInfo` | VVC |
| `DolbyVisionRpuInfo` | Dolby Vision RPU |

Utility functions such as `extract_encoder_from_avc_sei_nalus`,
`extract_encoder_from_sei_nalus`, `gop_detect`, `fill_mpeg2_streams`,
`fill_vc1_streams`, `fill_dv_rpu_fields`, and `parse_x264_style_encoder`
are also re-exported for use by container parsers within the workspace.

## Design

All parsers are pure Rust with `#[deny(unsafe_code)]`. They carry no system
dependencies and link no external libraries. The `FileAnalyze` type is defined
in `revelo-core` and holds the file buffer plus the stream field map that
accumulates results.

## License

BSD-2-Clause — see [LICENSE](https://github.com/vbasky/revelo/blob/main/LICENSE).
