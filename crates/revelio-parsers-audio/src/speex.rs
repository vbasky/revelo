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

use revelio_core::{FileAnalyze, StreamKind};
use zenlib::int32u;

const SPEEX_MAGIC: &[u8; 8] = b"Speex   ";
const IDENTIFICATION_MIN_SIZE: usize = 80;

pub fn parse_speex(fa: &mut FileAnalyze) -> bool {
    let head = fa.peek_raw(fa.remain().min(8));
    let Some(h) = head else { return false };
    if h.len() < 8 || h != SPEEX_MAGIC {
        return false;
    }
    if fa.remain() < IDENTIFICATION_MIN_SIZE {
        return false;
    }

    fa.element_begin("Speex");
    fa.skip_hexa(8, "speex_string");

    let version_bytes = fa.read_raw(20).to_vec();
    let speex_version = parse_nul_terminated_utf8(&version_bytes);

    let mut speex_version_id: int32u = 0;
    fa.get_l4(&mut speex_version_id, "Speex_version_id");

    if speex_version_id != 1 {
        fa.element_end();
        fill_streams(fa, &speex_version, None, None, None);
        return true;
    }

    let mut header_size: int32u = 0;
    let mut rate: int32u = 0;
    let mut _mode: int32u = 0;
    let mut _mode_bs_ver: int32u = 0;
    let mut nb_channels: int32u = 0;
    let mut bitrate: int32u = 0;
    let mut _frame_size: int32u = 0;
    let mut vbr: int32u = 0;
    let mut _frames_per_packet: int32u = 0;
    let mut _extra_headers: int32u = 0;
    let mut _reserved1: int32u = 0;
    let mut _reserved2: int32u = 0;

    fa.get_l4(&mut header_size, "header_size");
    fa.get_l4(&mut rate, "rate");
    fa.get_l4(&mut _mode, "mode");
    fa.get_l4(&mut _mode_bs_ver, "mode_bitstream_version");
    fa.get_l4(&mut nb_channels, "nb_channels");
    fa.get_l4(&mut bitrate, "bitrate");
    fa.get_l4(&mut _frame_size, "frame_size");
    fa.get_l4(&mut vbr, "vbr");
    fa.get_l4(&mut _frames_per_packet, "frames_per_packet");
    fa.get_l4(&mut _extra_headers, "extra_headers");
    fa.get_l4(&mut _reserved1, "reserved1");
    fa.get_l4(&mut _reserved2, "reserved2");

    fa.element_end();

    let bitrate_opt = if bitrate == u32::MAX { None } else { Some(bitrate) };
    fill_streams(
        fa,
        &speex_version,
        Some(rate),
        Some(nb_channels),
        bitrate_opt,
    );
    // vbr field: 0 => CBR, anything else => VBR. Done after fill_streams
    // because fill_streams defaults BitRate_Mode to VBR (task spec); override
    // here when the header explicitly says CBR.
    if vbr == 0 {
        fa.fill(StreamKind::Audio, 0, "BitRate_Mode", "CBR", true);
    }
    true
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
    fa.fill(StreamKind::General, 0, "Format", "Speex", false);
    fa.fill(StreamKind::General, 0, "AudioCount", "1", false);

    fa.stream_prepare(StreamKind::Audio);
    fa.fill(StreamKind::Audio, 0, "Format", "Speex", false);
    fa.fill(StreamKind::Audio, 0, "Codec", "Speex", false);
    if !speex_version.is_empty() {
        fa.fill(StreamKind::Audio, 0, "Encoded_Library", speex_version, false);
    }
    if let Some(r) = rate {
        fa.fill(StreamKind::Audio, 0, "SamplingRate", r.to_string(), false);
    }
    if let Some(c) = channels {
        fa.fill(StreamKind::Audio, 0, "Channels", c.to_string(), false);
    }
    if let Some(b) = bitrate {
        fa.fill(StreamKind::Audio, 0, "BitRate", b.to_string(), false);
    }
    fa.fill(StreamKind::Audio, 0, "BitRate_Mode", "VBR", false);
    fa.fill(StreamKind::Audio, 0, "Compression_Mode", "Lossy", false);
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
        buf.extend_from_slice(&80u32.to_le_bytes());     // header_size
        buf.extend_from_slice(&rate.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes());      // mode
        buf.extend_from_slice(&4u32.to_le_bytes());      // mode_bitstream_version
        buf.extend_from_slice(&channels.to_le_bytes());
        buf.extend_from_slice(&bitrate.to_le_bytes());
        buf.extend_from_slice(&160u32.to_le_bytes());    // frame_size
        buf.extend_from_slice(&vbr.to_le_bytes());
        buf.extend_from_slice(&1u32.to_le_bytes());      // frames_per_packet
        buf.extend_from_slice(&0u32.to_le_bytes());      // extra_headers
        buf.extend_from_slice(&0u32.to_le_bytes());      // reserved1
        buf.extend_from_slice(&0u32.to_le_bytes());      // reserved2
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
