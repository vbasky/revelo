//! TTA (True Audio) lossless codec parser.
//!
//! Mirrors MediaInfoLib's `File_Tta.cpp`. TTA is a lossless audio codec
//! whose stream begins with a fixed-size header.
//!
//! Header layout (all multi-byte integers are little-endian):
//!   "TTA1"  (4 bytes, magic)
//!   uint16  AudioFormat
//!   uint16  NumChannels
//!   uint16  BitsPerSample
//!   uint32  SampleRate
//!   uint32  DataLength      (total decoded sample count per channel)
//!   uint32  CRC32

use revelio_core::{FileAnalyze, StreamKind};
use zenlib::{int16u, int32u};

const MAGIC_TTA1: [u8; 4] = *b"TTA1";
const HEADER_LEN: usize = 22;

pub fn parse_tta(fa: &mut FileAnalyze) -> bool {
    if fa.Remain() < HEADER_LEN {
        return false;
    }
    let head = match fa.peek_raw(fa.Remain().min(4)) {
        Some(h) if h.len() == 4 => h,
        _ => return false,
    };
    if head != MAGIC_TTA1 {
        return false;
    }

    fa.Element_Begin("TTA");
    let mut signature: int32u = 0;
    fa.Get_C4(&mut signature, "Signature");
    let mut audio_format: int16u = 0;
    fa.Get_L2(&mut audio_format, "AudioFormat");
    let mut channels: int16u = 0;
    fa.Get_L2(&mut channels, "NumChannels");
    let mut bits_per_sample: int16u = 0;
    fa.Get_L2(&mut bits_per_sample, "BitsPerSample");
    let mut sample_rate: int32u = 0;
    fa.Get_L4(&mut sample_rate, "SampleRate");
    let mut samples: int32u = 0;
    fa.Get_L4(&mut samples, "DataLength");
    fa.Skip_L4("CRC32");
    fa.Element_End();

    // Reject obviously-broken headers: divisions below would panic and the
    // resulting metadata would be useless anyway.
    if sample_rate == 0 || channels == 0 || bits_per_sample == 0 || samples == 0 {
        return false;
    }

    // C++ computes Duration via CalcDurationUncompressedSize:
    //   Duration = Samples * 1000 / SampleRate
    let duration_ms: u64 = (samples as u64) * 1000 / (sample_rate as u64);

    fa.Stream_Prepare(StreamKind::General);
    fa.Fill(StreamKind::General, 0, "Format", "TTA", false);
    fa.Fill(StreamKind::General, 0, "AudioCount", "1", false);

    fa.Stream_Prepare(StreamKind::Audio);
    fa.Fill(StreamKind::Audio, 0, "Format", "TTA", false);
    fa.Fill(StreamKind::Audio, 0, "Codec", "TTA ", false);
    fa.Fill(StreamKind::Audio, 0, "Compression_Mode", "Lossless", false);
    fa.Fill(StreamKind::Audio, 0, "BitRate_Mode", "VBR", false);
    fa.Fill(StreamKind::Audio, 0, "BitDepth", bits_per_sample.to_string(), false);
    fa.Fill(StreamKind::Audio, 0, "Channels", channels.to_string(), false);
    fa.Fill(StreamKind::Audio, 0, "SamplingRate", sample_rate.to_string(), false);
    fa.Fill(StreamKind::Audio, 0, "SamplingCount", samples.to_string(), false);
    fa.Fill(StreamKind::Audio, 0, "Duration", duration_ms.to_string(), false);

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tta(
        audio_format: u16,
        channels: u16,
        bits_per_sample: u16,
        sample_rate: u32,
        samples: u32,
    ) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(b"TTA1");
        buf.extend_from_slice(&audio_format.to_le_bytes());
        buf.extend_from_slice(&channels.to_le_bytes());
        buf.extend_from_slice(&bits_per_sample.to_le_bytes());
        buf.extend_from_slice(&sample_rate.to_le_bytes());
        buf.extend_from_slice(&samples.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes()); // CRC32
        buf
    }

    #[test]
    fn rejects_non_tta_buffer() {
        let mut fa = FileAnalyze::new(b"NOT a TTA file at all............");
        assert!(!parse_tta(&mut fa));
    }

    #[test]
    fn parses_basic_tta_stream() {
        // samples=44100 stereo 16-bit @44100 -> duration = 1000 ms
        let buf = make_tta(1, 2, 16, 44100, 44100);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_tta(&mut fa));

        let g = |k: &str| fa.Retrieve(StreamKind::General, 0, k).map(|z| z.as_str().to_owned());
        let a = |k: &str| fa.Retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());

        assert_eq!(g("Format").as_deref(), Some("TTA"));
        assert_eq!(g("AudioCount").as_deref(), Some("1"));
        assert_eq!(a("Format").as_deref(), Some("TTA"));
        assert_eq!(a("Compression_Mode").as_deref(), Some("Lossless"));
        assert_eq!(a("BitRate_Mode").as_deref(), Some("VBR"));
        assert_eq!(a("BitDepth").as_deref(), Some("16"));
        assert_eq!(a("Channels").as_deref(), Some("2"));
        assert_eq!(a("SamplingRate").as_deref(), Some("44100"));
        assert_eq!(a("SamplingCount").as_deref(), Some("44100"));
        assert_eq!(a("Duration").as_deref(), Some("1000"));
    }

    #[test]
    fn rejects_tta_with_zero_sample_rate() {
        let buf = make_tta(1, 2, 16, 0, 44100);
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_tta(&mut fa));
    }
}
