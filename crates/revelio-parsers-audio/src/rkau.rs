//! RKAU (RK Audio) parser.
//!
//! Mirrors MediaInfoLib's `File_Rkau.cpp`. RKAU is a small lossless/lossy
//! codec whose entire stream-level metadata fits in a 15-byte header.
//!
//! Header layout (all integers little-endian):
//!   "RKA"                  (3 bytes, magic)
//!   uint8   version
//!   uint32  source_bytes
//!   uint32  sample_rate
//!   uint8   channels
//!   uint8   bits_per_sample
//!   uint8   quality            // 0 = lossless, !=0 = lossy
//!   uint8   flags              // bit0 joint_stereo, bit1 streaming, bit2 vrq_lossy

use revelio_core::{FileAnalyze, StreamKind};
use zenlib::{Int8u, Int32u};

const MAGIC_RKA: [u8; 3] = *b"RKA";
const HEADER_LEN: usize = 15;

pub fn parse_rkau(fa: &mut FileAnalyze) -> bool {
    if fa.remain() < HEADER_LEN {
        return false;
    }
    let head = match fa.peek_raw(fa.remain().min(3)) {
        Some(h) if h.len() == 3 => h,
        _ => return false,
    };
    if head != MAGIC_RKA {
        return false;
    }

    fa.element_begin("RKAU");
    fa.skip_hexa(3, "Signature");
    let mut version_bytes = [0u8; 1];
    if let Some(b) = fa.peek_raw(1) {
        version_bytes[0] = b[0];
    }
    fa.skip_l1("Version");
    let mut source_bytes: Int32u = 0;
    fa.get_l4(&mut source_bytes, "SourceBytes");
    let mut sample_rate: Int32u = 0;
    fa.get_l4(&mut sample_rate, "SampleRate");
    let mut channels: Int8u = 0;
    fa.get_l1(&mut channels, "Channels");
    let mut bits_per_sample: Int8u = 0;
    fa.get_l1(&mut bits_per_sample, "BitsPerSample");
    let mut quality: Int8u = 0;
    fa.get_l1(&mut quality, "Quality");
    let mut flags: Int8u = 0;
    fa.get_l1(&mut flags, "Flags");
    fa.element_end();

    if sample_rate == 0 || channels == 0 || bits_per_sample == 0 {
        return false;
    }
    // Mirror the C++ duration formula: (source_bytes * 1000 / 4) / sample_rate.
    let duration_ms: u64 = ((source_bytes as u64) * 1000 / 4) / sample_rate as u64;
    if duration_ms == 0 {
        return false;
    }
    let uncompressed: u64 = (channels as u64) * (bits_per_sample as u64 / 8);
    if uncompressed == 0 {
        return false;
    }

    // C++ stores the version byte as an ASCII character and concatenates it
    // with the literal "1.0" prefix to form e.g. "1.01".
    let version_str = format!("1.0{}", version_bytes[0] as char);
    let compression_mode = if quality == 0 { "Lossless" } else { "Lossy" };

    fa.stream_prepare(StreamKind::General);
    fa.fill(StreamKind::General, 0, "Format", "RKAU", false);
    fa.fill(StreamKind::General, 0, "Format_Version", version_str.clone(), false);
    fa.fill(StreamKind::General, 0, "AudioCount", "1", false);

    fa.stream_prepare(StreamKind::Audio);
    fa.fill(StreamKind::Audio, 0, "Format", "RKAU", false);
    fa.fill(StreamKind::Audio, 0, "Format_Version", version_str, false);
    fa.fill(StreamKind::Audio, 0, "Compression_Mode", compression_mode, false);
    fa.fill(StreamKind::Audio, 0, "BitRate_Mode", "VBR", false);
    fa.fill(StreamKind::Audio, 0, "Channels", channels.to_string(), false);
    fa.fill(StreamKind::Audio, 0, "SamplingRate", sample_rate.to_string(), false);
    fa.fill(StreamKind::Audio, 0, "BitDepth", bits_per_sample.to_string(), false);
    fa.fill(StreamKind::Audio, 0, "Duration", duration_ms.to_string(), false);

    let _ = flags;
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_rkau(
        version: u8,
        source_bytes: u32,
        sample_rate: u32,
        channels: u8,
        bits_per_sample: u8,
        quality: u8,
        flags: u8,
    ) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(b"RKA");
        buf.push(version);
        buf.extend_from_slice(&source_bytes.to_le_bytes());
        buf.extend_from_slice(&sample_rate.to_le_bytes());
        buf.push(channels);
        buf.push(bits_per_sample);
        buf.push(quality);
        buf.push(flags);
        buf
    }

    #[test]
    fn rejects_non_rkau_buffer() {
        let mut fa = FileAnalyze::new(b"NOT an RKAU file............");
        assert!(!parse_rkau(&mut fa));
    }

    #[test]
    fn parses_lossless_rkau_stream() {
        // 1 second of stereo 16-bit @44100: source_bytes = 44100*2*2 = 176400.
        // Duration = 176400*1000/4/44100 = 1000 ms.
        let buf = make_rkau(b'1', 176_400, 44_100, 2, 16, 0, 0);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_rkau(&mut fa));

        let g = |k: &str| fa.retrieve(StreamKind::General, 0, k).map(|z| z.as_str().to_owned());
        let a = |k: &str| fa.retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());

        assert_eq!(g("Format").as_deref(), Some("RKAU"));
        assert_eq!(g("Format_Version").as_deref(), Some("1.01"));
        assert_eq!(g("AudioCount").as_deref(), Some("1"));
        assert_eq!(a("Format").as_deref(), Some("RKAU"));
        assert_eq!(a("Format_Version").as_deref(), Some("1.01"));
        assert_eq!(a("Compression_Mode").as_deref(), Some("Lossless"));
        assert_eq!(a("BitRate_Mode").as_deref(), Some("VBR"));
        assert_eq!(a("Channels").as_deref(), Some("2"));
        assert_eq!(a("SamplingRate").as_deref(), Some("44100"));
        assert_eq!(a("BitDepth").as_deref(), Some("16"));
        assert_eq!(a("Duration").as_deref(), Some("1000"));
    }

    #[test]
    fn detects_lossy_when_quality_nonzero() {
        let buf = make_rkau(b'2', 176_400, 44_100, 2, 16, 5, 0x04);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_rkau(&mut fa));
        assert_eq!(
            fa.retrieve(StreamKind::Audio, 0, "Compression_Mode")
                .map(|z| z.as_str().to_owned())
                .as_deref(),
            Some("Lossy")
        );
        assert_eq!(
            fa.retrieve(StreamKind::Audio, 0, "Format_Version")
                .map(|z| z.as_str().to_owned())
                .as_deref(),
            Some("1.02")
        );
    }
}
