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

use revelio_core::{FileAnalyze, Reader, StreamKind};

const MAGIC_LA: [u8; 2] = *b"LA";
const HEADER_LEN: usize = 45;

pub fn parse_la(fa: &mut FileAnalyze) -> bool {
    parse(fa).is_some()
}

fn parse(fa: &mut FileAnalyze) -> Option<()> {
    let r = &mut Reader::wrap(fa);
    if r.remain() < HEADER_LEN {
        return None;
    }
    if r.peek_raw(2)? != MAGIC_LA {
        return None;
    }

    r.element_begin("LA");
    r.le_u16("signature")?;
    let major = r.le_u8("major_version")?;
    let minor = r.le_u8("minor_version")?;
    r.le_u32("uncompressed_size")?;
    r.le_u32("chunk")?;
    r.le_u32("fmt_size")?;
    r.le_u32("fmt_chunk")?;
    r.le_u32("fmt_size")?;
    r.le_u16("raw_format")?;
    let channels = r.le_u16("channels")?;
    let sample_rate = r.le_u32("sample_rate")?;
    r.le_u32("bytes_per_second")?;
    r.le_u16("bytes_per_sample")?;
    let bits_per_sample = r.le_u16("bits_per_sample")?;
    let samples = r.le_u32("samples")?;
    r.le_u8("flags")?;
    r.le_u32("crc")?;
    r.element_end();

    if sample_rate == 0 || channels == 0 || bits_per_sample == 0 {
        return None;
    }
    // C++ reference notes samples is per-channel-pair-doubled; dividing by
    // Channels gives the correct duration.
    let duration_ms: u64 = (samples as u64 / channels as u64) * 1000 / sample_rate as u64;
    if duration_ms == 0 {
        return None;
    }
    let uncompressed: u64 = (samples as u64) * (channels as u64) * (bits_per_sample as u64 / 8);
    if uncompressed == 0 {
        return None;
    }

    let version_str = format!("{}.{}", major, minor);

    r.stream_prepare(StreamKind::General);
    r.set_field(StreamKind::General, 0, "Format", "LA");
    r.set_field(StreamKind::General, 0, "Format_Version", version_str.clone());
    r.set_field(StreamKind::General, 0, "AudioCount", "1");

    r.stream_prepare(StreamKind::Audio);
    r.set_field(StreamKind::Audio, 0, "Format", "LA");
    r.set_field(StreamKind::Audio, 0, "Format_Version", version_str);
    r.set_field(StreamKind::Audio, 0, "Codec", "LA");
    r.set_field(StreamKind::Audio, 0, "Compression_Mode", "Lossless");
    r.set_field(StreamKind::Audio, 0, "BitRate_Mode", "VBR");
    r.set_field(StreamKind::Audio, 0, "BitDepth", bits_per_sample.to_string());
    r.set_field(StreamKind::Audio, 0, "Channels", channels.to_string());
    r.set_field(StreamKind::Audio, 0, "SamplingRate", sample_rate.to_string());
    r.set_field(StreamKind::Audio, 0, "Duration", duration_ms.to_string());
    Some(())
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
