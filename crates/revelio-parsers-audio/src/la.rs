//! LA (Lossless Audio) parser.
//!
//! Mirrors MediaInfoLib's `File_La.cpp`. The LA stream embeds a stripped
//! WAVE `fmt ` chunk inside its own header.
//!
//! Header layout (all integers little-endian):
//!   "LA"                  (2 bytes, magic)
//!   uint8   major_version
//!   uint8   minor_version
//!   uint32  uncompressed_size
//!   uint32  WAVE_chunk             ("WAVE")
//!   uint32  fmt_size_outer
//!   uint32  fmt_chunk              ("fmt ")
//!   uint32  fmt_size
//!   uint16  raw_format
//!   uint16  channels
//!   uint32  sample_rate
//!   uint32  bytes_per_second
//!   uint16  bytes_per_sample
//!   uint16  bits_per_sample
//!   uint32  samples
//!   uint8   flags
//!   uint32  crc32

use revelio_core::{FileAnalyze, StreamKind};
use zenlib::{Int8u, Int16u, Int32u};

const MAGIC_LA: [u8; 2] = *b"LA";
const HEADER_LEN: usize = 45;

pub fn parse_la(fa: &mut FileAnalyze) -> bool {
    if fa.remain() < HEADER_LEN {
        return false;
    }
    let head = match fa.peek_raw(fa.remain().min(2)) {
        Some(h) if h.len() == 2 => h,
        _ => return false,
    };
    if head != MAGIC_LA {
        return false;
    }

    fa.element_begin("LA");
    let mut signature: Int16u = 0;
    fa.get_l2(&mut signature, "signature");
    let mut major: Int8u = 0;
    let mut minor: Int8u = 0;
    fa.get_l1(&mut major, "major_version");
    fa.get_l1(&mut minor, "minor_version");
    let mut uncompressed_size: Int32u = 0;
    fa.get_l4(&mut uncompressed_size, "uncompressed_size");
    fa.skip_l4("chunk");
    fa.skip_l4("fmt_size");
    fa.skip_l4("fmt_chunk");
    fa.skip_l4("fmt_size");
    let mut raw_format: Int16u = 0;
    fa.get_l2(&mut raw_format, "raw_format");
    let mut channels: Int16u = 0;
    fa.get_l2(&mut channels, "channels");
    let mut sample_rate: Int32u = 0;
    fa.get_l4(&mut sample_rate, "sample_rate");
    fa.skip_l4("bytes_per_second");
    fa.skip_l2("bytes_per_sample");
    let mut bits_per_sample: Int16u = 0;
    fa.get_l2(&mut bits_per_sample, "bits_per_sample");
    let mut samples: Int32u = 0;
    fa.get_l4(&mut samples, "samples");
    fa.skip_l1("flags");
    fa.skip_l4("crc");
    fa.element_end();

    if sample_rate == 0 || channels == 0 || bits_per_sample == 0 {
        return false;
    }
    // C++ reference notes samples is per-channel-pair-doubled; dividing by
    // Channels gives the correct duration.
    let duration_ms: u64 = (samples as u64 / channels as u64) * 1000 / sample_rate as u64;
    if duration_ms == 0 {
        return false;
    }
    let uncompressed: u64 = (samples as u64) * (channels as u64) * (bits_per_sample as u64 / 8);
    if uncompressed == 0 {
        return false;
    }

    let version_str = format!("{}.{}", major, minor);

    fa.stream_prepare(StreamKind::General);
    fa.fill(StreamKind::General, 0, "Format", "LA", false);
    fa.fill(StreamKind::General, 0, "Format_Version", version_str.clone(), false);
    fa.fill(StreamKind::General, 0, "AudioCount", "1", false);

    fa.stream_prepare(StreamKind::Audio);
    fa.fill(StreamKind::Audio, 0, "Format", "LA", false);
    fa.fill(StreamKind::Audio, 0, "Format_Version", version_str, false);
    fa.fill(StreamKind::Audio, 0, "Codec", "LA", false);
    fa.fill(StreamKind::Audio, 0, "Compression_Mode", "Lossless", false);
    fa.fill(StreamKind::Audio, 0, "BitRate_Mode", "VBR", false);
    fa.fill(StreamKind::Audio, 0, "BitDepth", bits_per_sample.to_string(), false);
    fa.fill(StreamKind::Audio, 0, "Channels", channels.to_string(), false);
    fa.fill(StreamKind::Audio, 0, "SamplingRate", sample_rate.to_string(), false);
    fa.fill(StreamKind::Audio, 0, "Duration", duration_ms.to_string(), false);

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_la(
        major: u8,
        minor: u8,
        channels: u16,
        sample_rate: u32,
        bits_per_sample: u16,
        samples: u32,
    ) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(b"LA");
        buf.push(major);
        buf.push(minor);
        buf.extend_from_slice(&0u32.to_le_bytes()); // uncompressed_size
        buf.extend_from_slice(b"WAVE");
        buf.extend_from_slice(&0u32.to_le_bytes()); // fmt_size outer
        buf.extend_from_slice(b"fmt ");
        buf.extend_from_slice(&16u32.to_le_bytes()); // fmt_size
        buf.extend_from_slice(&1u16.to_le_bytes()); // raw_format = PCM
        buf.extend_from_slice(&channels.to_le_bytes());
        buf.extend_from_slice(&sample_rate.to_le_bytes());
        let bytes_per_sec = sample_rate * (channels as u32) * (bits_per_sample as u32 / 8);
        buf.extend_from_slice(&bytes_per_sec.to_le_bytes());
        let bytes_per_sample = channels * (bits_per_sample / 8);
        buf.extend_from_slice(&bytes_per_sample.to_le_bytes());
        buf.extend_from_slice(&bits_per_sample.to_le_bytes());
        buf.extend_from_slice(&samples.to_le_bytes());
        buf.push(0); // flags
        buf.extend_from_slice(&0u32.to_le_bytes()); // crc
        buf
    }

    #[test]
    fn rejects_non_la_buffer() {
        let mut fa = FileAnalyze::new(b"NOT an LA file at all..........");
        assert!(!parse_la(&mut fa));
    }

    #[test]
    fn parses_basic_la_stream() {
        // samples=88200 stereo 16-bit @44100 → duration = 88200/2*1000/44100 = 1000 ms
        let buf = make_la(0, 4, 2, 44100, 16, 88200);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_la(&mut fa));

        let g = |k: &str| fa.retrieve(StreamKind::General, 0, k).map(|z| z.as_str().to_owned());
        let a = |k: &str| fa.retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());

        assert_eq!(g("Format").as_deref(), Some("LA"));
        assert_eq!(g("Format_Version").as_deref(), Some("0.4"));
        assert_eq!(g("AudioCount").as_deref(), Some("1"));
        assert_eq!(a("Format").as_deref(), Some("LA"));
        assert_eq!(a("Format_Version").as_deref(), Some("0.4"));
        assert_eq!(a("Codec").as_deref(), Some("LA"));
        assert_eq!(a("Compression_Mode").as_deref(), Some("Lossless"));
        assert_eq!(a("BitRate_Mode").as_deref(), Some("VBR"));
        assert_eq!(a("BitDepth").as_deref(), Some("16"));
        assert_eq!(a("Channels").as_deref(), Some("2"));
        assert_eq!(a("SamplingRate").as_deref(), Some("44100"));
        assert_eq!(a("Duration").as_deref(), Some("1000"));
    }

    #[test]
    fn rejects_la_with_zero_sample_rate() {
        let buf = make_la(0, 4, 2, 0, 16, 88200);
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_la(&mut fa));
    }
}
