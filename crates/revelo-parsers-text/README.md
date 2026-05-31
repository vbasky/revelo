# revelo-parsers-text

Text and subtitle format parsers for [**revelo**](https://github.com/vbasky/revelo)
— a fast, safe, pure-Rust port of [MediaInfoLib](https://mediaarea.net/en/MediaInfo).
This crate covers the probe-and-parse layer for text-based and subtitle formats:
it detects a stream's format from its header or structure, and fills the
`FileAnalyze` stream graph with format, encoding, and timing fields.

Part of the [**revelo**](https://github.com/vbasky/revelo) project — see the
[project README](https://github.com/vbasky/revelo#readme) for the full picture.

## Normal use

Most users should depend on the [`revelo`](https://crates.io/crates/revelo)
facade crate rather than this crate directly. The facade re-exports every parser
and wires them into the dispatcher automatically.

## Supported formats

| Function | Format / Standard |
| --- | --- |
| `parse_sub_rip` | SubRip (SRT) — plain-text timed subtitles |
| `parse_webvtt` | WebVTT — W3C Web Video Text Tracks |
| `parse_ttml` | TTML / DFXP — Timed Text Markup Language |
| `parse_timed_text` | 3GPP Timed Text (TX3G, QuickTime tx3g track) |
| `parse_cmml` | CMML — Continuous Media Markup Language |
| `parse_kate` | Kate — Ogg Kate overlay stream |
| `parse_pgs` | PGS — Presentation Graphic Stream (Blu-ray) |
| `parse_dvb_subtitle` | DVB subtitle (ETSI EN 300 743) |
| `parse_teletext` | Teletext subtitle (ETSI EN 300 472 / ITU-R BT.653) |
| `parse_arib_std_b24_b37` | ARIB STD-B24 / STD-B37 (Japanese digital broadcast captions) |
| `parse_eia608` | EIA-608 / CEA-608 (Line 21 closed captions) |
| `parse_eia708` | EIA-708 / CEA-708 (DTV closed captions) |
| `parse_dtvcc_transport` | DTVCC transport layer (CEA-708 packetisation) |
| `parse_scc` | SCC — Scenarist Closed Captions |
| `parse_scte20` | SCTE-20 — closed caption data in MPEG-2 user data |
| `parse_cdp` | CDP — Caption Distribution Packet (SMPTE ST 334) |
| `parse_n19` | N19 / STL — EBU Subtitling Data Exchange Format |
| `parse_pac` | PAC — Cheetah caption file |
| `parse_pdf` | PDF — basic format identification for PDF documents |
| `parse_sdp` | SDP — Session Description Protocol |
| `parse_other_text` | Generic text-stream fallback |

## Usage

```no_run
use revelo_parsers_text::parse_sub_rip;
use revelo_core::FileAnalyze;

let data: Vec<u8> = std::fs::read("subtitles.srt").unwrap();
let mut fa = FileAnalyze::new(&data);
if parse_sub_rip(&mut fa) {
    // fa now contains a Text stream with format and encoding fields
}
```

Prefer the `revelo` facade for everyday use — it handles format detection and
dispatches to the right parser automatically.

## Safety

`#![deny(unsafe_code)]` — zero unsafe blocks.

## License

BSD-2-Clause — see [LICENSE](https://github.com/vbasky/revelo/blob/main/LICENSE).
