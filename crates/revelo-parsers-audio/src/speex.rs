//! Speex parser — raw Speex identification header.
//!
//! Speex frames are normally carried inside Ogg; this parser handles the
//! identification header payload (also valid as a standalone "raw" Speex
//! header). All multi-byte integers after the magic are little-endian.
//!
//! Magic: "Speex   " (0x53 0x70 0x65 0x65 0x78 0x20 0x20 0x20, 8 bytes)
//!
//! Identification header layout:
//!    8 bytes:  speex_string ("Speex   ")
//!   20 bytes:  speex_version (UTF-8, NUL-padded)
//!    4 bytes LE: Speex_version_id
//!    4 bytes LE: header_size
//!    4 bytes LE: rate
//!    4 bytes LE: mode
//!    4 bytes LE: mode_bitstream_version
//!    4 bytes LE: nb_channels
//!    4 bytes LE: bitrate          (-1 if unspecified)
//!    4 bytes LE: frame_size
//!    4 bytes LE: vbr              (0=CBR, 1=VBR)
//!    4 bytes LE: frames_per_packet
//!    4 bytes LE: extra_headers
//!    4 bytes LE: reserved1
//!    4 bytes LE: reserved2

use revelo_core::{FileAnalyze, Reader, StreamKind};

const SPEEX_MAGIC: &[u8; 8] = b"Speex   ";
const IDENTIFICATION_MIN_SIZE: usize = 80;

pub fn parse_speex(fa: &mut FileAnalyze) -> bool {
    parse(fa).is_some()
}

fn parse(fa: &mut FileAnalyze) -> Option<()> {
    let speex_version;
    // (rate, channels, bitrate, vbr) when version_id == 1.
    let params: Option<(u32, u32, u32, u32)>;
    {
        let r = &mut Reader::wrap(fa);
        let head = r.peek_raw(8)?;
        if head.len() < 8 || head != SPEEX_MAGIC {
            return None;
        }
        if r.remain() < IDENTIFICATION_MIN_SIZE {
            return None;
        }

        r.element_begin("Speex");
        r.skip(8)?; // speex_string
        speex_version = parse_nul_terminated_utf8(r.read_raw(20)?);
        let speex_version_id = r.le_u32("Speex_version_id")?;

        if speex_version_id == 1 {
            r.le_u32("header_size")?;
            let rate = r.le_u32("rate")?;
            r.le_u32("mode")?;
            r.le_u32("mode_bitstream_version")?;
            let nb_channels = r.le_u32("nb_channels")?;
            let bitrate = r.le_u32("bitrate")?;
            r.le_u32("frame_size")?;
            let vbr = r.le_u32("vbr")?;
            r.le_u32("frames_per_packet")?;
            r.le_u32("extra_headers")?;
            r.le_u32("reserved1")?;
            r.le_u32("reserved2")?;
            params = Some((rate, nb_channels, bitrate, vbr));
        } else {
            params = None;
        }
        r.element_end();
    }

    match params {
        Some((rate, nb_channels, bitrate, vbr)) => {
            let bitrate_opt = if bitrate == u32::MAX { None } else { Some(bitrate) };
            fill_streams(fa, &speex_version, Some(rate), Some(nb_channels), bitrate_opt);
            // vbr field: 0 => CBR, anything else => VBR. fill_streams defaults
            // BitRate_Mode to VBR; override when the header explicitly says CBR.
            if vbr == 0 {
                fa.force_field(StreamKind::Audio, 0, "BitRate_Mode", "CBR");
            }
        }
        None => fill_streams(fa, &speex_version, None, None, None),
    }
    Some(())
}

fn parse_nul_terminated_utf8(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

fn fill_streams(
    fa: &mut FileAnalyze,
    speex_version: &str,
    rate: Option<u32>,
    channels: Option<u32>,
    bitrate: Option<u32>,
) {
    fa.stream_prepare(StreamKind::General);
    fa.set_field(StreamKind::General, 0, "Format", "Speex");
    fa.set_field(StreamKind::General, 0, "AudioCount", "1");

    fa.stream_prepare(StreamKind::Audio);
    fa.set_field(StreamKind::Audio, 0, "Format", "Speex");
    fa.set_field(StreamKind::Audio, 0, "Codec", "Speex");
    if !speex_version.is_empty() {
        fa.set_field(StreamKind::Audio, 0, "Encoded_Library", speex_version);
    }
    if let Some(r) = rate {
        fa.set_field(StreamKind::Audio, 0, "SamplingRate", r.to_string());
    }
    if let Some(c) = channels {
        fa.set_field(StreamKind::Audio, 0, "Channels", c.to_string());
    }
    if let Some(b) = bitrate {
        fa.set_field(StreamKind::Audio, 0, "BitRate", b.to_string());
    }
    fa.set_field(StreamKind::Audio, 0, "BitRate_Mode", "VBR");
    fa.set_field(StreamKind::Audio, 0, "Compression_Mode", "Lossy");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_speex_header(
        version: &str,
        version_id: u32,
        rate: u32,
        channels: u32,
        bitrate: u32,
        vbr: u32,
    ) -> Vec<u8> {
        let mut buf = Vec::with_capacity(IDENTIFICATION_MIN_SIZE);
        buf.extend_from_slice(SPEEX_MAGIC);
        let mut ver = [0u8; 20];
        let vb = version.as_bytes();
        let n = vb.len().min(20);
        ver[..n].copy_from_slice(&vb[..n]);
        buf.extend_from_slice(&ver);
        buf.extend_from_slice(&version_id.to_le_bytes());
        buf.extend_from_slice(&80u32.to_le_bytes()); // header_size
        buf.extend_from_slice(&rate.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes()); // mode
        buf.extend_from_slice(&4u32.to_le_bytes()); // mode_bitstream_version
        buf.extend_from_slice(&channels.to_le_bytes());
        buf.extend_from_slice(&bitrate.to_le_bytes());
        buf.extend_from_slice(&160u32.to_le_bytes()); // frame_size
        buf.extend_from_slice(&vbr.to_le_bytes());
        buf.extend_from_slice(&1u32.to_le_bytes()); // frames_per_packet
        buf.extend_from_slice(&0u32.to_le_bytes()); // extra_headers
        buf.extend_from_slice(&0u32.to_le_bytes()); // reserved1
        buf.extend_from_slice(&0u32.to_le_bytes()); // reserved2
        buf
    }

    #[test]
    fn rejects_non_speex() {
        let mut fa = FileAnalyze::new(b"NOT a Speex header at all....................");
        assert!(!parse_speex(&mut fa));
    }

    #[test]
    fn parses_vbr_narrowband_mono() {
        let buf = build_speex_header("speex-1.2rc1", 1, 8000, 1, 15000, 1);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_speex(&mut fa));

        let g = |k: &str| fa.retrieve(StreamKind::General, 0, k).map(|z| z.as_str().to_owned());
        let a = |k: &str| fa.retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());

        assert_eq!(g("Format").as_deref(), Some("Speex"));
        assert_eq!(g("AudioCount").as_deref(), Some("1"));
        assert_eq!(a("Format").as_deref(), Some("Speex"));
        assert_eq!(a("Codec").as_deref(), Some("Speex"));
        assert_eq!(a("Encoded_Library").as_deref(), Some("speex-1.2rc1"));
        assert_eq!(a("SamplingRate").as_deref(), Some("8000"));
        assert_eq!(a("Channels").as_deref(), Some("1"));
        assert_eq!(a("BitRate").as_deref(), Some("15000"));
        assert_eq!(a("BitRate_Mode").as_deref(), Some("VBR"));
        assert_eq!(a("Compression_Mode").as_deref(), Some("Lossy"));
    }

    #[test]
    fn cbr_overrides_default_mode_and_skips_sentinel_bitrate() {
        // bitrate = 0xFFFFFFFF means "unspecified" per the spec.
        let buf = build_speex_header("speex-1.2.0", 1, 16000, 2, u32::MAX, 0);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_speex(&mut fa));

        let a = |k: &str| fa.retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(a("SamplingRate").as_deref(), Some("16000"));
        assert_eq!(a("Channels").as_deref(), Some("2"));
        assert_eq!(a("BitRate"), None);
        assert_eq!(a("BitRate_Mode").as_deref(), Some("CBR"));
    }
}
