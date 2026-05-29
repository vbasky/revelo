# Media Format Reference

A living document cataloging every format revelio parses, organized by family.
Each entry covers: what the format is, where it appears, how revelio detects it,
and references the relevant specification.

---

## Containers

### ISO Base Media File Format (MP4/MOV/3GP)
**Spec:** ISO/IEC 14496-12, ISO/IEC 14496-14, Apple QuickTime File Format
**Detection:** Walk `ftyp` box → major_brand identifies variant (`mp42`, `qt  `, `3gp4`, `mif1`, `heic`).
Inner boxes walked recursively: `moov` → `trak` → `mdia` → `minf` → `stbl`.
Key sub-boxes: `stsd` (codec entries), `stsz`/`stts` (frame tables), `mdhd` (duration),
`mvhd` (timescale), `tkhd` (dimensions), `udta` (iTunes metadata).
**Deep analysis:** AVC/HEVC SPS VUI colour info from avcC/hvcC, Dolby Vision dvcC/dvvC
HDR metadata, AAC esds descriptor parsing, x264/x265 encoder SEI extraction,
iTunes ilst/QuickTime mdta metadata, Nero chpl chapters.

### Matroska / WebM (MKV)
**Spec:** [matroska.org/technical/elements.html](https://www.matroska.org/technical/elements.html)
**Detection:** EBML header → `DocType` = `matroska` or `webm`.
Segment elements: Info, Tracks (CodecID + CodecPrivate), Chapters, Tags, Attachments.
CodecPrivate decoding for AVC/HEVC SPS, AV1 OBU header, OpusHead, Vorbis headers.
**Deep analysis:** Dolby Vision codec ID handling (V_DOLBYVISION, V_DOLBYVISION/AVC,
V_DOLBYVISION/HEVC), CRC-32 detection, Colour elements, UniqueIDs.

### MPEG Transport Stream (MPEG-TS)
**Spec:** ITU-T H.222.0 (ISO/IEC 13818-1), ATSC A/53, DVB SI (ETSI EN 300 468)
**Detection:** Sync byte `0x47` every 188 bytes. PAT (PID 0x0000) → PMT PIDs →
stream_type + ES_PID + descriptors.
**Descriptor parsing:** Registration (0x05 — HDMV/GA94/SCTE format identifiers),
ISO 639 language (0x0A — 3-letter codes), AC-3 descriptor (0x6A/0x7A).

### MPEG Program Stream (MPEG-PS) / VOB
**Spec:** ISO/IEC 13818-1, DVD-Video specifications
**Detection:** Pack start code `0x000001BA` + PES packets (stream_id 0xE0=video, 0xC0=audio).
**Output:** System header info, MPEG-2 Sequence Header sniffing, AAC/M2TS payloads.

### AVI (Audio Video Interleave)
**Spec:** [Microsoft AVI RIFF File Reference](https://docs.microsoft.com/en-us/windows/win32/directshow/avi-riff-file-reference)
**Detection:** `RIFF` + `AVI ` → hdrl (stream headers with strh/strf) + movi (interleaved data).
Two-pass: header pass for BITMAPINFOHEADER/WAVEFORMATEX, movi pass for frame counts.

### WAV (Waveform Audio)
**Spec:** Multimedia Programming Interface and Data Specifications 1.0 (IBM/Microsoft),
EBU Tech 3285 (BWF), EBU Tech 3293 (iXML/aXML)
**Detection:** `RIFF` + `WAVE` → `fmt ` (WAVEFORMATEX) + `data` chunks.
**BWF (Broadcast Wave):** `bext` chunk → Description, Originator, OriginationDate/Time,
TimeReference (sample count), BWFVersion, UMID, LoudnessValue/Range/MaxTruePeak.
`iXML`/`aXML` chunks → embedded XML metadata with scene/take/note fields.

### Ogg
**Spec:** RFC 3533
**Detection:** `OggS` page headers with serial numbers and granule positions.
Vorbis/Opus/Theora/FLAC/Speex identification header parsing.

### Other Containers
| Format | Magic | Notes |
|---|---|---|
| FLV | `FLV\x01` | Adobe Flash Video, AMF metadata |
| MXF | KLV structure | SMPTE material exchange format |
| ASF/WM | ASF GUID objects | Windows Media |
| RealMedia | `.RMF` | RealNetworks container |
| WTV | Recorded TV GUID | Microsoft DVR-MS successor |
| DV-DIF | DIF block header | Sony DVCPRO/DVCAM |
| IVF | `DKIF` | VP8/VP9/AV1 elementary stream |
| SWF | `FWS`/`CWS`/`ZWS` | Adobe Flash |

### SCTE-35 (Digital Program Insertion / Ad Cueing)
**Spec:** ANSI/SCTE 35
**Detection:** Table ID `0xFC` (splice_info_section) in MPEG-TS or raw binary.
Commands: splice_insert (0x05), time_signal (0x06). Segmentation descriptors
extract seg_type_id/name, segmentation_duration, and UPID (Unique Program
Identifier) with UPID type names (Ad ID, ISAN, EIDR, URI, UUID, etc.).

---

## Video Codecs

### AVC/H.264 (MPEG-4 Part 10)
**Spec:** ITU-T H.264, ISO/IEC 14496-10
**Detection:** Annex B start codes → NAL parsing. SPS type 7 → profile_idc, level_idc,
frame dimensions, chroma_format_idc, bit_depth. PPS type 8 → CABAC flag.
SEI type 6 → user_data_unregistered (x264/x265 UUID dispatch).
**Deep analysis:** Full SPS VUI (colour_primaries, transfer_characteristics,
matrix_coefficients, aspect_ratio, video_full_range, chroma_sample_loc).
x264 SEI → EncoderInfo struct (library/name/version/settings). GOP detection:
slice type sequence → M (P-frame distance) + N (GOP length).

### HEVC/H.265 (MPEG-H Part 2)
**Spec:** ITU-T H.265, ISO/IEC 23008-2
**Detection:** Annex B NALs → VPS(32)/SPS(33)/PPS(34)/SEI(39/40). SPS →
profile_tier_level(1,max_sublayers-1), pic dimensions, conformance window.
**Deep analysis:** Full VUI colour parsing. HDR10 Mastering Display SEI (type 137)
→ primaries, white point, luminance (0.00002/0.0001 cd/m² units). Content Light
Level SEI (type 144) → MaxCLL, MaxFALL (cd/m²). x265 encoder UUID dispatch.
**HDR10+:** SEI payload type 4, country code `0xB5`, ITU-T provider `0x003C`,
application identifier 4 → `HDR_Format: ST 2094-40`, summary string.
**SL-HDR1:** SEI payload types 172 (HDR metadata), 173 (HDR/WCG tone-mapping),
174 (colour mapping table) → `HDR_Format: ETSI TS 103 433`, `HDR_Format_Compatibility: SL-HDR1`.
**CTA-861:** SEI user_data_registered (country `0xB5`, provider `0x003C`) →
CEA-861 auxiliary data parsing.
**Dolby Vision RPU:** NAL unit type 62 — full RPU header parsing: RPU type,
format version, L1 extension metadata (target luminance), L5 reshaping
parameters, L8 VDR metadata → per-frame HDR luminance values.
**HDR output:** `MasteringDisplay_ColorPrimaries`, `MasteringDisplay_Luminance`,
`MaxCLL`, `MaxFALL`, `HDR_Format: SMPTE ST 2086`, `HDR_Format_Compatibility: HDR10`.

### AV1 (AOMedia Video 1)
**Spec:** [AOMedia AV1 Specification](https://aomediacodec.github.io/av1-spec/)
**Detection:** OBU sequence header (type=1). Profile-based bit depth (0→8-bit,
1→10-bit, 2→12-bit) and chroma subsampling (0-1→4:2:0, 2→4:2:2).
Level from operating point.
**HDR10+:** Metadata OBU (type=8, metadata_type=1) → ITU-T T.35 with country
`0xB5`, provider `0x003C`, application `4` → `HDR_Format: ST 2094-40`.

### VP8 / VP9
**Spec:** RFC 6386 (VP8), WebM VP9 spec
**Detection:** VP8 keyframe magic `0x9D012A`. VP9 frame marker + profile bits.
VP9 CodecPrivate (vpcC) → profile, bit_depth, chroma_subsampling, color_space.

### Others
| Codec | Magic/Detection | Notes |
|---|---|---|
| VVC/H.266 | Annex B SPS type 15 | ITU-T H.266 |
| MPEG-2 | Sequence header 0x000001B3 | ITU-T H.262 |
| VC-1 | 0x0000010F start code | SMPTE 421M |
| ProRes | icpf/apcn frame magic | Apple intermediate |
| VC-3/DNxHD | 0x00000280 prefix | SMPTE ST 2019 |
| Theora | Ogg packet 0x80 + "theora" | Xiph codec |
| H.263 | Picture start 0x000080 | 3GPP video |
| MPEG-4V | VOS start 0x000001B0 | DivX/Xvid |
| FFV1 | "FFV1" magic | Archival lossless |
| Canopus | CHQX/CHQH 4CC | Grass Valley |
| CineForm | CFHD magic | GoPro wavelet |
| Dolby Vision | dvcC/dvvC boxes, XML, NAL type 62 RPU | HDR profiles 5/7/8.1, L1 luminance metadata |
| HDR10+ (ST 2094-40) | HEVC SEI type 4, AV1 metadata OBU | Per-frame HDR metadata |
| SL-HDR1 | HEVC SEI payload 172/173/174 | ETSI TS 103 433 HDR/WCG |
| HLG | CICP transfer=18 (MP4) / ColourPrimaries (MKV) | ARIB STD-B67 |
| PQ | CICP transfer=16 (MP4) / ColourPrimaries (MKV) | SMPTE ST 2084 |
| CTA-861 | HEVC SEI user_data_registered | Auxiliary data parsing |
| Fraps | FPS1 4CC | Game capture |
| FLIC | 0xAF11/0xAF12 | Autodesk Animator |
| HuffYUV | HFYU 4CC | Lossless YUV |
| Lagarith | LAGS 4CC | Lossless |
| Dirac | BBCD magic | BBC wavelet |
| AVS/AVS3 | AVS 4CC / Annex B | Chinese standard |
| HDR Vivid | HDRV/HVIV | Chinese HDR |
| AIC | aic/AIC 4CC | Apple intermediate |
| AFD/Bar | AFBd/BARD | SMPTE 2016-1 |

### Container-level HDR signalling

| Container | Detection | HDR_Format output |
|---|---|---|
| MP4 | `colr` box `transfer_characteristics` = 16 (PQ) / 18 (HLG) | `SMPTE ST 2084` / `ARIB STD-B67` |
| Matroska | `Colour` element `TransferCharacteristics` | `SMPTE ST 2084` / `ARIB STD-B67` |

---

## Audio Codecs

### AAC / ADTS (Advanced Audio Coding)
**Spec:** ISO/IEC 14496-3, ISO/IEC 13818-7
**Detection:** Raw ADTS: sync `0xFFF` → MPEG version, layer, profile, SR, channels.
MP4 esds: AudioSpecificConfig → audioObjectType, frequency index, channel config.
**Output:** CodecID, SamplingRate, Channels, Format_Profile (LC/HE-AAC/HE-AACv2), SBR/PS signaling.

### ADM (Audio Definition Model)
**Spec:** SMPTE ST 2076
**Detection:** `ADM` magic or `axml` chunk. Next-gen broadcast audio with object-based,
scene-based, and channel-based representations.

### Dolby Audio Metadata
**Spec:** Dolby DAM format. Detection: RIFF `DAM` or `DAMG` form types, or raw `DAM` magic.

### PcmVob
**Detection:** `DVD` magic or `LPCM` chunk. Big-endian PCM in DVD-Video VOB files.

### PcmM2ts
**Detection:** `HDMV` magic. Big-endian LPCM embedded in Blu-ray M2TS streams.

### MGA (MPEG-4 General Audio)
**Detection:** `MGA` magic. Generic MPEG-4 audio container (pre-AAC era). MP4 esds: AudioSpecificConfig.
**Output:** CodecID (mp4a.40.2), SamplingRate, Channels, Format_Profile (LC/HE-AAC/HE-AACv2).

### MP3 (MPEG Audio Layer III)
**Spec:** ISO/IEC 11172-3, ISO/IEC 13818-3
**Detection:** Frame sync `0xFFF` + layer=3. Xing/VBRI headers → VBR/CBR, frame count.
**Output:** Duration, BitRate, BitRate_Mode, Encoded_Library (LAME), stereo mode.

### AC-3 / E-AC-3 (Dolby Digital + Dolby Digital Plus)
**Spec:** ATSC A/52:2018
**Detection:** Sync word `0x0B77`. bsid → AC-3(8-10), E-AC-3(16).
**Output:** BitRate, Channels+LFE, dialnorm, dsurmod, bitstream mode.
**Atmos (E-AC-3):** `strmtyp` field (bits 16–17) — dependent substream
(strmtyp=1) signals Dolby Atmos → `Format_AdditionalFeatures: Atmos`,
`HDR_Format: Dolby Atmos`.

### DTS / DTS-UHD
**Spec:** ETSI TS 102 114
**Detection:** Sync word `0x7FFE8001`. Core + XLL/XLL2 profile for lossless UHD.
**Output:** BitRate, Channels, SamplingRate, Format_Profile (Core/UHD/HD MA).

### FLAC (Free Lossless Audio Codec)
**Spec:** [xiph.org/flac/format.html](https://xiph.org/flac/format.html)
**Detection:** `fLaC` marker → STREAMINFO metadata block (20+3+5+36 bit-packed fields).
**Output:** Channels, SamplingRate, BitDepth, Duration, VorbisComment metadata.

### Opus
**Spec:** RFC 6716, RFC 7845
**Detection:** `OpusHead` packet → version, channel_count, preskip, sample_rate.
Channel mapping family 0 (mono/stereo) / 1 (Vorbis order, table lookup).

### Vorbis
**Spec:** [xiph.org/vorbis/doc/Vorbis_I_spec.html](https://xiph.org/vorbis/doc/Vorbis_I_spec.html)
**Detection:** Packet type 1 + "vorbis" magic. Version 0 only.
**Output:** Channels, SamplingRate, BitRate, Mode (CBR/VBR), VorbisComment metadata.

### TrueHD / MLP (Dolby TrueHD + Dolby Atmos)
**Spec:** Dolby TrueHD Bitstream Specification
**Detection:** Sync `0xF8726FBA` (TrueHD) / `0xF8726FBB` (AC-3 core + TrueHD).
SR index, channel count, bit depth lookup. Output: Lossless, VBR.
**Atmos:** MAT (MLP Audio Transfer) frame header `0x0003` detection + substream
type sniffing → `Format_AdditionalFeatures: Atmos`, `HDR_Format: Dolby Atmos`.

### AC-4 (Dolby AC-4)
**Spec:** ETSI TS 103 190, ATSC A/342
**Detection:** Sync word `0xAC40` (with CRC) / `0xAC41` (without CRC).
**Frame header:** CBR/VBR flag, frame length, substream table (up to 16 substreams)
with per-substream sampling rate, channel mode, IMS (Immersive Stereo) flag,
JOC (Joint Object Coding) object count, loudness/dialnorm.
**Output:** `Format: AC-4`, `BitRate_Mode: CBR/VBR`, `SamplingRate`, `Channels`,
`Dialnorm`. IMS → `Format_Commercial: Dolby AC-4 Immersive`.

### IAMF / Eclipsa Audio
**Spec:** [AOMedia IAMF Specification](https://aomediacodec.github.io/iamf/)
**Detection:** OBU sequence magic — IA Sequence Header OBU (type=0) with codec
config, followed by Audio Element OBUs (type=2).
**OBU parsing:** Codec Config OBU — codec_id (Opus/AAC/LPCM), num_samples_per_frame,
audio_roll_distance. Audio Element OBU — scalable channel layout (number of layers,
loudspeaker/channel groups), ambisonics mode (mono/1/2/3), multi-language
labels. Stream parameters merged across all elements.
**Output:** `Format: IAMF`, `CodecID`, `Channels` (total across all layers),
`SamplingRate`, `Format_Commercial: Eclipsa Audio`.

### Other Codecs
- USAC/xHE-AAC (MPEG-D), MPEG-H 3D Audio
- CELT (ultra-low-delay), PCM (WAVEFORMATEX), ADPCM, WavPack, TAK, TTA
- Musepack SV7/SV8, Monkey's Audio (APE), Speex, ALAC (CAF), ALS
- AU, AIFF/AIFC, DSD/DSDIFF, MIDI, Module formats (MOD/XM/IT/S3M), IAB
- Dolby E (SMPTE 337M 0x9669), SMPTE ST 302/331/337 (AES3 transport)

---

## Image Formats

| Format | Detection | Spec |
|---|---|---|
| JPEG | SOI 0xFFD8 | ISO/IEC 10918-1 |
| PNG | `\x89PNG\r\n\x1A\n` | ISO/IEC 15948:2004 |
| GIF | `GIF87a`/`GIF89a` | CompuServe GIF89a |
| WebP | RIFF+WEBP+VP8/VP8L/VP8X | Google WebP spec |
| TIFF | `II`/`MM` + 42 | Adobe TIFF 6.0 |
| BMP | `BM` magic | Windows BMP |
| ICO/CUR | 0x00010000 | Windows icon |
| PSD | `8BPS` | Adobe Photoshop |
| DPX | SDPX/XDPX | SMPTE 268M |
| EXR | `v/1\x01` | OpenEXR |
| DDS | `DDS ` | DirectDraw Surface |
| BPG | `BPG\xFB` | HEVC intra |
| PCX | 0x0A+version | ZSoft PCX |
| TGA | Image ID field | Truevision TARGA |
| JPEG 2000 | `\x00\x00\x00\x0C\x6A\x50\x20\x20...` (JP2) or `\xFF\x4F` (J2K) | ISO/IEC 15444-1 |
| HEIF | ftyp+mif1/heic/hevc | ISO/IEC 23008-12 |
| ArriRaw | ARRIRAW | Arri Alexa |
| Amiga Icon | 0xE310 | AmigaOS |
| RLE | Run-length | Utah RLE |
| Gain Map | AVIF gain map | HDR gain map |

---

## Text / Subtitles

| Format | Magic/Detection | Spec |
|---|---|---|
| SubRip | `-->` timecode separator | De facto |
| WebVTT | `WEBVTT\n` header | W3C WebVTT |
| TTML | `<tt>` XML root | W3C TTML |
| PGS | Segment descriptor 0x16 | Blu-ray |
| DVB Subtitle | Segment sync byte | ETSI EN 300 743 |
| Teletext | 0x55 0x55 0x27 sync | ETSI EN 300 706 |
| EIA-608 | cc_data() | CEA-608 |
| EIA-708 | DTVCC transport | CEA-708 |
| CDP | CDP packet | SMPTE 334-2 |
| SCC | "Scenarist_SCC V1.0" | Scenarist |
| N19 | STL header | EBU N19 |
| ARIB B24/B37 | ARIB data group | ARIB STD-B24 |
| CMML | `<cmml>` XML | CMML |
| Kate | `kate\0\0\0\x80` | OggKate |
| Timed Text | 16-bit BE + UTF-8 | 3GPP TS 26.245 |
| Other Text | Various | Generic |
| PDF | `%PDF-` magic | Adobe PDF, ISO 32000 |
| SDP | `v=0` + `m=` lines | RFC 4566 Session Description |
| PAC | `PAC` magic | PAC subtitle format |
| DTvCC Transport | CC_type_1 0x03 0x00 | CTA-708 DTVCC transport |
| SCTE-20 | `SCTE` magic | SCTE 20 closed captioning |

---

## Archives

ZIP (`PK\x03\x04`), RAR (`Rar!\x1A`), 7z (`7z\xBC\xAF`), TAR (`ustar\x00`),
GZip (`\x1F\x8B`), BZip2 (`BZh`), ACE (`**ACE**`),
ISO 9660 (`CD001` at sector 16), ELF (`\x7FELF`), Mach-O (FEEDFACE/CEFAEDFE),
MZ/PE (`MZ` + PE at 0x3C).

---

## Tags / Metadata

ID3v1 (last 128 bytes, `TAG`), ID3v2 (front, `ID3` + synch-safe size, 10+ frame types),
APE Tag (APETAGEX footer), Vorbis Comment (`KEY=value` pairs), Lyrics3 (LYRICSBEGIN/LYR200),
EXIF (TIFF IFD, 13 tag types), XMP (RDF/XML), ICC (profile header + desc tag),
IIM/IPTC (record 2 datasets), C2PA (JUMBF c2pa/c2ma atoms),
Apple PropertyList (plist XML), SphericalVideo (ProjectionType/StereoMode XML).

---

## Core Infrastructure

### IBI (Index of Binary Information)
Frame-accurate seek table mapping byte offsets → timestamps.
Used by MPEG-TS/PS to enable random access in transport streams.

### MIME Type Mapping
Container-to-MIME and codec-to-MIME lookup tables covering all 185 formats.
Examples: `mp42→video/mp4`, `av01→video/AV1`, `opus→audio/opus`.

### Reference File Tracker
Tracks multi-file references (BDMV playlists, segmented MP4, SMPTE interop
packages) linking primary media to companion files.

### Channel Splitting / Grouping (SMPTE ST 337)
Deinterleaves multi-channel PCM into independent AES3 channel pairs
for downstream SMPTE 337M/338M/339M parsing. 4-channel → 2 stereo pairs,
6-channel → 3 stereo pairs. Reverse direction merges mono streams
into interleaved output (16/20/24-bit support).

### Demux / Event Framework
4-level demux bitmask: Frame(1), Container(2), Elementary(4), Ancillary(8).
DemuxState tracks events per stream with PTS/DTS, stream IDs, offsets,
random_access flags. DemuxEvent emitted per frame/packet for downstream
consumers (CDI, DTVCC, PCM un-packetizing).

### Trace System
4 output formats: Tree (MediaInfo-like hierarchy), CSV, XML, MicroXml.
TraceNode with hierarchical parent/child structure, file offsets, sizes,
named values, and info sections. Used by all container parsers for
trace/debug output.

### Field Ordering / Interlacement
FieldTracker counts top/bottom/progressive fields to infer ScanOrder
(Progressive/TFF/BFF/Mixed) and InterlacementMode (PPF/Interlaced/
TFF/BFF/PsF). Maps to Video_ScanOrder/Video_Interlacement output fields.

## References

### Container
- ISO/IEC 14496-12, [Matroska](https://www.matroska.org/technical/elements.html),
  ITU-T H.222.0, [AVI](https://docs.microsoft.com/en-us/windows/win32/directshow/avi-riff-file-reference), RFC 3533

### Video
- ITU-T H.264, H.265, H.266, [AV1](https://aomediacodec.github.io/av1-spec/),
  SMPTE 421M (VC-1), SMPTE ST 2019 (VC-3), Apple ProRes (2018)

### Audio
- ISO/IEC 14496-3, ATSC A/52, RFC 6716 (Opus),
  [FLAC](https://xiph.org/flac/format.html), [Vorbis](https://xiph.org/vorbis/doc/Vorbis_I_spec.html)

### Image
- ISO/IEC 10918-1 (JPEG), ISO/IEC 15948 (PNG), EXIF 2.32, ICC.1:2010, TIFF 6.0 (Adobe)

### HDR
- SMPTE ST 2084 (PQ), SMPTE ST 2086, SMPTE ST 2094-40 (HDR10+), CTA-861-G,
  Dolby Vision Profiles, ARIB STD-B67 (HLG), ETSI TS 103 433 (SL-HDR1)

### Text
- [W3C WebVTT](https://www.w3.org/TR/webvtt1/), W3C TTML1/2, SMPTE ST 2052-1
