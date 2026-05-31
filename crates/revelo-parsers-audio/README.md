# revelo-parsers-audio

Audio-codec parsers for the [**revelo**](https://github.com/vbasky/revelo)
media-analysis library — a fast, safe, pure-Rust port of
[MediaInfoLib](https://mediaarea.net/en/MediaInfo).

Each parser is a focused `fn(&mut FileAnalyze) -> bool` that identifies its
codec by magic bytes or structural heuristics, then fills the relevant
`Audio`/`General` fields on the `FileAnalyze` context. Parsers are registered
in the revelo dispatcher and are not normally called directly by application
code.

Part of the [**revelo**](https://github.com/vbasky/revelo) project — see the
[project README](https://github.com/vbasky/revelo#readme) for the full picture.

## Normal usage

Application code should go through the [`revelo`](https://crates.io/crates/revelo)
facade crate, not this crate directly:

```toml
[dependencies]
revelo = "0.4"
```

This crate is a public implementation detail of the revelo workspace, exposed
separately so that container-parser crates can depend on it without pulling in
the entire stack.

## Supported codecs

| Parser function | Codec / Format | Notes |
| --- | --- | --- |
| `parse_aac` | AAC (MPEG-4 Audio) | Raw AAC bitstream |
| `parse_aac_adts` | AAC-ADTS | AAC Audio Data Transport Stream |
| `parse_ac3` | AC-3 / Dolby Digital | Includes E-AC-3 |
| `parse_ac4` | AC-4 | Dolby AC-4 immersive audio |
| `parse_adm` | ADM | Audio Definition Model (broadcast spatial) |
| `parse_adpcm` | ADPCM | Adaptive Differential PCM |
| `parse_als` | ALS | MPEG-4 Audio Lossless Coding |
| `parse_amr` | AMR | Adaptive Multi-Rate (narrow/wideband) |
| `parse_ape` | APE | Monkey's Audio lossless |
| `parse_aptx100` | APT-X 100 | DTS theatrical sidecar format |
| `parse_au` | AU | Sun/NeXT audio (.au / .snd) |
| `parse_caf` | CAF | Apple Core Audio Format |
| `parse_celt` | CELT | Low-latency audio codec (pre-Opus) |
| `parse_channel_grouping` | Channel grouping | Multi-channel stream grouping metadata |
| `parse_channel_splitting` | Channel splitting | Multi-channel stream splitting metadata |
| `parse_dat` | DAT | Digital Audio Tape frame stream |
| `parse_dolby_audio_metadata` | Dolby audio metadata | Dolby loudness / metadata sidecar |
| `parse_dolby_e` | Dolby E | Broadcast-grade Dolby E bitstream |
| `parse_dsdiff` | DSDIFF | DSD Interchange File Format (DFF) |
| `parse_dsf` | DSF | DSD Stream File |
| `parse_dts` | DTS | DTS coherent acoustics |
| `parse_dts_uhd` | DTS-UHD / DTS-X | DTS Ultra HD |
| `parse_extended_module` | XM | FastTracker 2 Extended Module |
| `parse_flac` | FLAC | Free Lossless Audio Codec |
| `parse_iab` | IAB | Immersive Audio Bitstream |
| `parse_iamf` | IAMF | Immersive Audio Model and Formats (Eclipsa Audio) |
| `parse_impulse_tracker` | IT | Impulse Tracker module |
| `parse_la` | LA | Lossless Audio (LA codec) |
| `parse_mga` | MGA | MPEG-4 General Audio container |
| `parse_midi` | MIDI | Standard MIDI File |
| `parse_module` | MOD | ProTracker / AmigaOS tracker module |
| `parse_mp3` | MP3 | MPEG-1/2 Audio Layer III |
| `parse_mpc` | Musepack SV7 | Musepack / MPC stream version 7 |
| `parse_mpc_sv8` | Musepack SV8 | Musepack stream version 8 |
| `parse_mpegh3da` | MPEG-H 3D Audio | ISO/IEC 23008-3 |
| `parse_open_mg` | OpenMG / OMA | Sony ATRAC3 / OMA format |
| `parse_opus` | Opus | IETF Opus interactive audio codec |
| `parse_pcm` | PCM | Raw linear PCM |
| `parse_pcm_m2ts` | PCM (M2TS) | PCM as carried in MPEG-TS |
| `parse_pcm_vob` | PCM (VOB) | PCM as carried in DVD VOB streams |
| `parse_ps2_audio` | PS2 Audio | PlayStation 2 ADPCM audio |
| `parse_rkau` | RKAU | RK Audio lossless/lossy |
| `parse_scream_tracker3` | S3M | Scream Tracker 3 module |
| `parse_smpte_st0302` | SMPTE ST 0302 | AES3 audio in SMPTE 302M |
| `parse_smpte_st0331` | SMPTE ST 0331 | Essence container descriptor |
| `parse_smpte_st0337` | SMPTE ST 0337 | Non-PCM in AES3 |
| `parse_speex` | Speex | Xiph Speex speech codec |
| `parse_tak` | TAK | Tom's lossless Audio Kompressor |
| `parse_truehd` | TrueHD | Dolby TrueHD / MLP |
| `parse_tta` | TTA | True Audio lossless |
| `parse_twin_vq` | TwinVQ / VQF | NTT transform-domain weighted interleave VQ |
| `parse_usac` | USAC / xHE-AAC | Unified Speech and Audio Coding |
| `parse_vorbis` | Vorbis | Xiph Vorbis audio |
| `parse_wvpk` | WavPack | WavPack hybrid lossless |

### Utility re-exports

| Symbol | Description |
| --- | --- |
| `extract_replay_gain` | Extract ReplayGain tags from a parsed stream |
| `fill_id3_replay_gain` | Populate ReplayGain fields from ID3 tag data |

## Design

All parsers are pure Rust with `#[deny(unsafe_code)]`. They carry no system
dependencies and link no external libraries. The `FileAnalyze` type is defined
in `revelo-core` and holds the file buffer plus the stream field map that
accumulates results.

## License

BSD-2-Clause — see [LICENSE](https://github.com/vbasky/revelo/blob/main/LICENSE).
