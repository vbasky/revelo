//! Monkey's Audio (.ape) parser — lossless audio codec.
//!
//! Mirrors MediaInfoLib's `File_Ape.cpp`. Two header layouts exist
//! distinguished by the version field: legacy (<3.98) and modern (>=3.98).
//!
//! Common prefix:
//!   "MAC " or "MACF"      (4 bytes, magic; 'F' suffix = Float profile)
//!   uint16 LE             Version (e.g. 3990 = 3.990)
//!
//! Legacy header (Version < 3980):
//!   uint16 LE  CompressionLevel
//!   uint16 LE  FormatFlags (bit0=8-bit, bit3=24-bit, bit5=no_wav_header)
//!   uint16 LE  Channels
//!   uint32 LE  SampleRate
//!   uint32 LE  WavHeaderDataBytes
//!   uint32 LE  WavTerminatingBytes
//!   uint32 LE  TotalFrames
//!   uint32 LE  FinalFrameSamples
//!   uint32 LE  PeakLevel
//!   uint32 LE  SeekElements
//!   (optional 44-byte RIFF header)
//!   SeekElements*4 bytes seek table
//!
//! Modern header (Version >= 3980):
//!   uint16 LE  Version_High (padding)
//!   uint32 LE  DescriptorBytes
//!   uint32 LE  HeaderBytes
//!   uint32 LE  SeekTableBytes
//!   uint32 LE  WavHeaderDataBytes
//!   uint32 LE  APEFrameDataBytes
//!   uint32 LE  APEFrameDataBytesHigh
//!   uint32 LE  WavTerminatingDataBytes
//!   16 bytes   FileMD5
//!   uint16 LE  CompressionLevel
//!   uint16 LE  FormatFlags
//!   uint32 LE  BlocksPerFrame (SamplesPerFrame)
//!   uint32 LE  FinalFrameBlocks
//!   uint32 LE  TotalFrames
//!   uint16 LE  BitsPerSample
//!   uint16 LE  Channels
//!   uint32 LE  SampleRate

use revelo_core::{FileAnalyze, Reader, StreamKind};

const MAGIC_MAC_SPACE: u32 = u32::from_be_bytes(*b"MAC ");
const MAGIC_MAC_F: u32 = u32::from_be_bytes(*b"MACF");

/// Decoded APE stream header fields, shared by the legacy and modern parsers.
#[derive(Default)]
struct ApeHeader {
    compression_level: u16,
    channels: u16,
    resolution: u16,
    sample_rate: u32,
    total_frames: u32,
    final_frame_samples: u32,
    samples_per_frame: u32,
}

/// Parse Monkey's Audio (APE) lossless stream.
///
/// Detection: `MAC 3.97` magic.
/// Fills: Compression level, channels, sample rate, bit depth, APE tag.
pub fn parse_ape(fa: &mut FileAnalyze) -> bool {
    parse(fa).is_some()
}

fn parse(fa: &mut FileAnalyze) -> Option<()> {
    let r = &mut Reader::wrap(fa);
    if r.remain() < 8 {
        return None;
    }
    let head = r.peek_raw(4)?;
    if head.len() < 4 {
        return None;
    }
    let magic = u32::from_be_bytes([head[0], head[1], head[2], head[3]]);
    if magic != MAGIC_MAC_SPACE && magic != MAGIC_MAC_F {
        return None;
    }

    r.element_begin("APE");
    let identifier = r.fourcc("Identifier")?;
    let version = r.le_u16("Version")?;
    let header =
        if version < 3980 { parse_legacy_header(r, version) } else { parse_modern_header(r) };
    r.element_end();

    let h = header?;
    if h.total_frames == 0 || h.sample_rate == 0 || h.channels == 0 || h.resolution == 0 {
        return None;
    }
    let samples =
        (h.total_frames as u64 - 1) * h.samples_per_frame as u64 + h.final_frame_samples as u64;
    if samples == 0 {
        return None;
    }

    fill_streams(fa, identifier, version, &h, samples);
    Some(())
}

fn parse_legacy_header(r: &mut Reader<'_, '_>, version: u16) -> Option<ApeHeader> {
    // Legacy header is 26 bytes after Version (+ optional 44-byte RIFF + seek table).
    if r.remain() < 32 {
        return None;
    }
    let mut h = ApeHeader::default();
    h.compression_level = r.le_u16("CompressionLevel")?;
    let flags = r.le_u16("FormatFlags")?;
    let resolution8 = (flags & 0x0001) != 0;
    let resolution24 = (flags & 0x0008) != 0;
    let no_wav_header = (flags & 0x0020) != 0;
    h.resolution = if resolution8 {
        8
    } else if resolution24 {
        24
    } else {
        16
    };
    h.channels = r.le_u16("Channels")?;
    h.sample_rate = r.le_u32("SampleRate")?;
    r.le_u32("WavHeaderDataBytes")?;
    r.le_u32("WavTerminatingBytes")?;
    h.total_frames = r.le_u32("TotalFrames")?;
    h.final_frame_samples = r.le_u32("FinalFrameSamples")?;
    h.samples_per_frame = ape_samples_per_frame(version, h.compression_level);
    r.le_u32("PeakLevel")?;
    let seek_elements = r.le_u32("SeekElements")?;
    if !no_wav_header {
        if r.remain() < 44 {
            return None;
        }
        r.skip(44)?;
    }
    let seek_bytes = seek_elements as usize * 4;
    if r.remain() < seek_bytes {
        return None;
    }
    r.skip(seek_bytes)?;
    Some(h)
}

fn parse_modern_header(r: &mut Reader<'_, '_>) -> Option<ApeHeader> {
    // Descriptor (46) + header (24) bytes after the version field.
    if r.remain() < 70 {
        return None;
    }
    let mut h = ApeHeader::default();
    r.le_u16("Version_High")?;
    r.le_u32("DescriptorBytes")?;
    r.le_u32("HeaderBytes")?;
    r.le_u32("SeekTableBytes")?;
    r.le_u32("WavHeaderDataBytes")?;
    r.le_u32("APEFrameDataBytes")?;
    r.le_u32("APEFrameDataBytesHigh")?;
    r.le_u32("WavTerminatingDataBytes")?;
    r.skip(16)?; // FileMD5 (unused)
    h.compression_level = r.le_u16("CompressionLevel")?;
    r.le_u16("FormatFlags")?;
    h.samples_per_frame = r.le_u32("BlocksPerFrame")?;
    h.final_frame_samples = r.le_u32("FinalFrameBlocks")?;
    h.total_frames = r.le_u32("TotalFrames")?;
    h.resolution = r.le_u16("BitsPerSample")?;
    h.channels = r.le_u16("Channels")?;
    h.sample_rate = r.le_u32("SampleRate")?;
    Some(h)
}

fn ape_samples_per_frame(version: u16, compression_level: u16) -> u32 {
    if version >= 3950 {
        73728 * 4
    } else if version >= 3900 {
        73728
    } else if version >= 3800 && compression_level == 4000 {
        73728
    } else {
        9216
    }
}

fn ape_codec_settings(level: u16) -> &'static str {
    match level {
        1000 => "Fast",
        2000 => "Normal",
        3000 => "High",
        4000 => "Extra-high",
        5000 => "Insane",
        _ => "",
    }
}

fn fill_streams(fa: &mut FileAnalyze, identifier: u32, version: u16, h: &ApeHeader, samples: u64) {
    let &ApeHeader { compression_level, channels, resolution, sample_rate, .. } = h;
    let duration_ms: u64 = samples * 1000 / (sample_rate as u64);
    let uncompressed_size: u64 = samples * (channels as u64) * (resolution as u64 / 8);
    let version_str = format!("{:.3}", (version as f64) / 1000.0);

    fa.stream_prepare(StreamKind::General);
    fa.set_field(StreamKind::General, 0, "Format", "Monkey's Audio");
    fa.set_field(StreamKind::General, 0, "Format_Version", version_str.clone());
    fa.set_field(StreamKind::General, 0, "AudioCount", "1");
    fa.force_field(StreamKind::General, 0, "StreamSize", "0");

    fa.stream_prepare(StreamKind::Audio);
    fa.set_field(StreamKind::Audio, 0, "Format", "Monkey's Audio");
    fa.set_field(StreamKind::Audio, 0, "Format_Version", version_str);
    // "MACF" magic indicates floating-point samples.
    if identifier == MAGIC_MAC_F {
        fa.set_field(StreamKind::Audio, 0, "Format_Profile", "Float");
    }
    let settings = ape_codec_settings(compression_level);
    if !settings.is_empty() {
        fa.set_field(StreamKind::Audio, 0, "Encoded_Library_Settings", settings);
    }
    fa.set_field(StreamKind::Audio, 0, "Codec", "APE");
    fa.set_field(StreamKind::Audio, 0, "Compression_Mode", "Lossless");
    fa.set_field(StreamKind::Audio, 0, "BitRate_Mode", "VBR");
    fa.set_field(StreamKind::Audio, 0, "BitDepth", resolution.to_string());
    fa.set_field(StreamKind::Audio, 0, "Channels", channels.to_string());
    fa.set_field(StreamKind::Audio, 0, "SamplingRate", sample_rate.to_string());
    fa.set_field(StreamKind::Audio, 0, "SamplingCount", samples.to_string());
    fa.set_field(StreamKind::Audio, 0, "Duration", duration_ms.to_string());
    let _ = uncompressed_size;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(clippy::too_many_arguments)] // fixture builder mirrors the binary header layout
    fn make_modern_ape(
        magic: &[u8; 4],
        version: u16,
        compression_level: u16,
        samples_per_frame: u32,
        final_frame_blocks: u32,
        total_frames: u32,
        bits_per_sample: u16,
        channels: u16,
        sample_rate: u32,
    ) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(magic);
        buf.extend_from_slice(&version.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes()); // Version_High
        buf.extend_from_slice(&0u32.to_le_bytes()); // DescriptorBytes
        buf.extend_from_slice(&0u32.to_le_bytes()); // HeaderBytes
        buf.extend_from_slice(&0u32.to_le_bytes()); // SeekTableBytes
        buf.extend_from_slice(&0u32.to_le_bytes()); // WavHeaderDataBytes
        buf.extend_from_slice(&0u32.to_le_bytes()); // APEFrameDataBytes
        buf.extend_from_slice(&0u32.to_le_bytes()); // APEFrameDataBytesHigh
        buf.extend_from_slice(&0u32.to_le_bytes()); // WavTerminatingDataBytes
        buf.extend_from_slice(&[0u8; 16]); // MD5
        buf.extend_from_slice(&compression_level.to_le_bytes());
        buf.extend_from_slice(&0u16.to_le_bytes()); // FormatFlags
        buf.extend_from_slice(&samples_per_frame.to_le_bytes());
        buf.extend_from_slice(&final_frame_blocks.to_le_bytes());
        buf.extend_from_slice(&total_frames.to_le_bytes());
        buf.extend_from_slice(&bits_per_sample.to_le_bytes());
        buf.extend_from_slice(&channels.to_le_bytes());
        buf.extend_from_slice(&sample_rate.to_le_bytes());
        buf
    }

    fn make_legacy_ape(
        version: u16,
        compression_level: u16,
        flags: u16,
        channels: u16,
        sample_rate: u32,
        total_frames: u32,
        final_frame_samples: u32,
    ) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(b"MAC ");
        buf.extend_from_slice(&version.to_le_bytes());
        buf.extend_from_slice(&compression_level.to_le_bytes());
        buf.extend_from_slice(&flags.to_le_bytes());
        buf.extend_from_slice(&channels.to_le_bytes());
        buf.extend_from_slice(&sample_rate.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes()); // WavHeaderDataBytes
        buf.extend_from_slice(&0u32.to_le_bytes()); // WavTerminatingBytes
        buf.extend_from_slice(&total_frames.to_le_bytes());
        buf.extend_from_slice(&final_frame_samples.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes()); // PeakLevel
        buf.extend_from_slice(&0u32.to_le_bytes()); // SeekElements = 0
        // flag bit5 = no_wav_header set → no RIFF block follows
        buf
    }

    #[test]
    fn rejects_non_ape_buffer() {
        let mut fa = FileAnalyze::new(b"NOT an APE file..");
        assert!(!parse_ape(&mut fa));
    }

    #[test]
    fn parses_modern_ape_3990() {
        let buf = make_modern_ape(b"MAC ", 3990, 2000, 73728 * 4, 1000, 10, 16, 2, 44100);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_ape(&mut fa));

        let g = |k: &str| fa.retrieve(StreamKind::General, 0, k).map(|z| z.as_str().to_owned());
        let a = |k: &str| fa.retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());

        assert_eq!(g("Format").as_deref(), Some("Monkey's Audio"));
        assert_eq!(g("Format_Version").as_deref(), Some("3.990"));
        assert_eq!(a("Format").as_deref(), Some("Monkey's Audio"));
        assert_eq!(a("Format_Version").as_deref(), Some("3.990"));
        assert_eq!(a("Compression_Mode").as_deref(), Some("Lossless"));
        assert_eq!(a("BitRate_Mode").as_deref(), Some("VBR"));
        assert_eq!(a("BitDepth").as_deref(), Some("16"));
        assert_eq!(a("Channels").as_deref(), Some("2"));
        assert_eq!(a("SamplingRate").as_deref(), Some("44100"));
        assert_eq!(a("Codec").as_deref(), Some("APE"));
        assert_eq!(a("Encoded_Library_Settings").as_deref(), Some("Normal"));
        // samples = (10-1)*294912 + 1000 = 2655208
        assert_eq!(a("SamplingCount").as_deref(), Some("2655208"));
        // duration = 2655208*1000/44100 = 60208
        assert_eq!(a("Duration").as_deref(), Some("60208"));
        assert!(a("Format_Profile").is_none());
    }

    #[test]
    fn parses_macf_float_profile() {
        let buf = make_modern_ape(b"MACF", 3990, 3000, 73728 * 4, 500, 5, 24, 2, 48000);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_ape(&mut fa));

        let a = |k: &str| fa.retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(a("Format_Profile").as_deref(), Some("Float"));
        assert_eq!(a("BitDepth").as_deref(), Some("24"));
        assert_eq!(a("Encoded_Library_Settings").as_deref(), Some("High"));
    }

    #[test]
    fn parses_legacy_ape_3800() {
        // version 3800, compression 2000 → SamplesPerFrame = 9216
        // flags bit5 set (no_wav_header), 16-bit (no bit0/bit3)
        let buf = make_legacy_ape(3800, 2000, 0x0020, 2, 44100, 100, 4096);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_ape(&mut fa));

        let a = |k: &str| fa.retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());
        let g = |k: &str| fa.retrieve(StreamKind::General, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(g("Format_Version").as_deref(), Some("3.800"));
        assert_eq!(a("BitDepth").as_deref(), Some("16"));
        assert_eq!(a("Channels").as_deref(), Some("2"));
        assert_eq!(a("SamplingRate").as_deref(), Some("44100"));
        // samples = (100-1)*9216 + 4096 = 916480
        assert_eq!(a("SamplingCount").as_deref(), Some("916480"));
    }
}
