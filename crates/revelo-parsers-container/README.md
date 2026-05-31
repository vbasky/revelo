# revelo-parsers-container

Container-format parsers for [**revelo**](https://github.com/vbasky/revelo) — a fast, safe,
pure-Rust port of [MediaInfoLib](https://mediaarea.net/en/MediaInfo). This crate
covers the probe-and-parse layer for multimedia container formats: it detects a
file's format from its binary header, walks its structure, and fills the
`FileAnalyze` stream graph with codec, timing, track, and metadata fields.

Part of the [**revelo**](https://github.com/vbasky/revelo) project — see the
[project README](https://github.com/vbasky/revelo#readme) for the full picture.

## Normal use

Most users should depend on the [`revelo`](https://crates.io/crates/revelo)
facade crate rather than this crate directly. The facade re-exports every
parser and wires them into the dispatcher automatically.

## Supported formats

Each row corresponds to one public `parse_*` function registered in the
dispatcher.

| Function | Format / Standard |
| --- | --- |
| `parse_mp4` | MP4 / MOV / QuickTime (ISO Base Media) |
| `parse_mkv` | Matroska / WebM (EBML) |
| `parse_mpeg_ts` | MPEG Transport Stream (TS / M2TS, ITU-T H.222.0) |
| `parse_mpeg_ps` | MPEG Program Stream (VOB, SVCD) |
| `parse_avi` | AVI / RIFF |
| `parse_wav` | WAV / RIFF audio |
| `parse_aiff` | AIFF / AIFF-C |
| `parse_ogg` | Ogg (Vorbis, FLAC, Opus, Theora, …) |
| `parse_flv` | Flash Video (FLV, F4V) |
| `parse_rm` | RealMedia (RM / RMVB) |
| `parse_wm` | Windows Media (WMV / WMA / ASF) |
| `parse_wtv` | Windows Recorded TV (WTV) |
| `parse_mxf` | Material Exchange Format (MXF, SMPTE 377) |
| `parse_gxf` | General eXchange Format (GXF, SMPTE 360) |
| `parse_lxf` | Leitch/Harris eXchange Format (LXF) |
| `parse_nsv` | Nullsoft Streaming Video (NSV) |
| `parse_nut` | NUT multimedia container |
| `parse_ivf` | IVF (VP8/VP9/AV1 raw bitstream container) |
| `parse_swf` | Adobe/Macromedia SWF (Flash) |
| `parse_aaf` | Advanced Authoring Format (AAF) |
| `parse_amv` | AMV video (Action camera) |
| `parse_dpg` | DPG video (Nintendo DS) |
| `parse_pmp` | PMP video (PSP) |
| `parse_skm` | SKM video (Samsung) |
| `parse_ptx` | PTX video |
| `parse_dxw` | DXW video |
| `parse_dv_dif` | DV / DIF raw bitstream |
| `parse_cdxa` | CD-XA / CDROM/XA sectors |
| `parse_vbi` | VBI (Vertical Blanking Interval) data |
| `parse_ibi` | Index-Byte Information (IBI) |
| `parse_bdmv` | Blu-ray BDMV / BDAV playlist |
| `parse_dvdv` | DVD Video |
| `parse_hls` | HLS / M3U8 playlist |
| `parse_dash_mpd` | MPEG-DASH MPD manifest |
| `parse_hds_f4m` | Adobe HDS F4M manifest |
| `parse_ism` | Smooth Streaming ISM manifest |
| `parse_scte35` | SCTE-35 splice-info section |
| `parse_sequence_info` | Sequence-info sidecar (multi-file sequences) |
| `parse_dcp_am` | DCP Asset Map (digital cinema) |
| `parse_dcp_cpl` | DCP Composition Playlist (digital cinema) |
| `parse_dcp_pkl` | DCP Packing List (digital cinema) |
| `parse_p2_clip` | Panasonic P2 clip XML |
| `parse_xdcam_clip` | Sony XDCAM clip XML |
| `parse_mi_xml` | MediaInfoLib XML re-import |

## Usage

```no_run
use revelo_parsers_container::parse_mp4;
use revelo_core::FileAnalyze;

let data: Vec<u8> = std::fs::read("video.mp4").unwrap();
let mut fa = FileAnalyze::new(&data);
if parse_mp4(&mut fa) {
    // fa now contains General, Video, Audio, … streams
}
```

Prefer the `revelo` facade for everyday use — it handles format detection and
dispatches to the right parser automatically.

## Safety

`#![deny(unsafe_code)]` — zero unsafe blocks.

## License

BSD-2-Clause — see [LICENSE](https://github.com/vbasky/revelo/blob/main/LICENSE).
