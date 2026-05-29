//! MPEG-4 Audio Lossless Coding (ALS) parser — native ALS streams.
//!
//! Mirrors MediaInfoLib's `File_Als.cpp`. Header layout (all big-endian,
//! 16 bytes minimum):
//!   0x00  magic         u32  = "ALS\0" (0x414C5300)
//!   0x04  sample_rate   u32  Hz
//!   0x08  samples       u32  total sample count
//!   0x0C  channels-1    u16  stored value is channel_count - 1
//!   0x0E  bitfield      u8:
//!         3 bits  file_type           (WAV/RIFF/AIFF)
//!         3 bits  bits_per_sample     stored as (bits/8 - 1), so 0..=3 → 8/16/24/32
//!         1 bit   floating_point
//!         1 bit   samples_big_endian

use revelio_core::{FileAnalyze, StreamKind};

const ALS_MAGIC: [u8; 4] = [b'A', b'L', b'S', 0x00];

pub fn parse_als(fa: &mut FileAnalyze) -> bool {
    let head = fa.peek_raw(fa.remain().min(16));
    let Some(h) = head else { return false };
    if h.len() < 16 || h[..4] != ALS_MAGIC {
        return false;
    }

    let sample_rate = u32::from_be_bytes([h[4], h[5], h[6], h[7]]);
    let samples = u32::from_be_bytes([h[8], h[9], h[10], h[11]]);
    let channels_m1 = u16::from_be_bytes([h[12], h[13]]);
    let packed = h[14];
    // 3:3:1:1 MSB-first packing — see header doc above.
    let bps_field = (packed >> 2) & 0x07;
    let bit_depth: u16 = ((bps_field as u16) + 1) * 8;
    let channels: u32 = (channels_m1 as u32) + 1;

    // Match C++ CalcDurationUncompressedSize early-out: reject if any
    // derived value would be zero, since ALS without these is unusable.
    if sample_rate == 0 {
        return false;
    }
    let duration_ms = (samples as u64) * 1000 / (sample_rate as u64);
    if duration_ms == 0 {
        return false;
    }

    fa.stream_prepare(StreamKind::General);
    fa.set_field(StreamKind::General, 0, "Format", "ALS");
    fa.set_field(StreamKind::General, 0, "AudioCount", "1");

    fa.stream_prepare(StreamKind::Audio);
    fa.set_field(StreamKind::Audio, 0, "Format", "ALS");
    fa.set_field(StreamKind::Audio, 0, "Codec", "ALS");
    fa.set_field(StreamKind::Audio, 0, "BitDepth", bit_depth.to_string());
    fa.set_field(StreamKind::Audio, 0, "Channels", channels.to_string());
    fa.set_field(StreamKind::Audio, 0, "SamplingRate", sample_rate.to_string());
    fa.set_field(StreamKind::Audio, 0, "SamplingCount", samples.to_string());
    fa.set_field(StreamKind::Audio, 0, "Duration", duration_ms.to_string());
    fa.set_field(StreamKind::Audio, 0, "Compression_Mode", "Lossless");
    fa.set_field(StreamKind::Audio, 0, "BitRate_Mode", "VBR");

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_als(sample_rate: u32, samples: u32, channels: u16, bps_field: u8) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&ALS_MAGIC);
        v.extend_from_slice(&sample_rate.to_be_bytes());
        v.extend_from_slice(&samples.to_be_bytes());
        v.extend_from_slice(&(channels - 1).to_be_bytes());
        // Pack bitfield: file_type=0 (3b), bps_field (3b), floating=0, big_endian=0.
        let packed = (bps_field & 0x07) << 2;
        v.push(packed);
        v.push(0); // trailing byte to round out the 16-byte header window
        v
    }

    #[test]
    fn rejects_non_als() {
        let mut fa = FileAnalyze::new(b"NOT AN ALS FILE!!!!!!!!!");
        assert!(!parse_als(&mut fa));
    }

    #[test]
    fn parses_stereo_44100_16bit() {
        // 1-second stereo @ 44100 Hz, 16-bit (bps_field=1 → 16).
        let buf = build_als(44100, 44100, 2, 1);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_als(&mut fa));
        let g = |k: &str| fa.retrieve(StreamKind::General, 0, k).map(|z| z.as_str().to_owned());
        let a = |k: &str| fa.retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(g("Format").as_deref(), Some("ALS"));
        assert_eq!(g("AudioCount").as_deref(), Some("1"));
        assert_eq!(a("Format").as_deref(), Some("ALS"));
        assert_eq!(a("Channels").as_deref(), Some("2"));
        assert_eq!(a("SamplingRate").as_deref(), Some("44100"));
        assert_eq!(a("SamplingCount").as_deref(), Some("44100"));
        assert_eq!(a("BitDepth").as_deref(), Some("16"));
        assert_eq!(a("Duration").as_deref(), Some("1000"));
        assert_eq!(a("Compression_Mode").as_deref(), Some("Lossless"));
        assert_eq!(a("BitRate_Mode").as_deref(), Some("VBR"));
    }

    #[test]
    fn parses_6ch_48000_24bit() {
        // 2 seconds 5.1 @ 48 kHz, 24-bit (bps_field=2 → 24).
        let buf = build_als(48000, 96000, 6, 2);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_als(&mut fa));
        let a = |k: &str| fa.retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(a("Channels").as_deref(), Some("6"));
        assert_eq!(a("SamplingRate").as_deref(), Some("48000"));
        assert_eq!(a("BitDepth").as_deref(), Some("24"));
        assert_eq!(a("Duration").as_deref(), Some("2000"));
    }
}
