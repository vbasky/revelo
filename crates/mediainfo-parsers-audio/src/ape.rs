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

use mediainfo_core::{FileAnalyze, StreamKind};
use zenlib::{int128u, int16u, int32u};

const MAGIC_MAC_SPACE: u32 = u32::from_be_bytes(*b"MAC ");
const MAGIC_MAC_F: u32 = u32::from_be_bytes(*b"MACF");

pub fn parse_ape(fa: &mut FileAnalyze) -> bool {
    if fa.Remain() < 8 {
        return false;
    }
    let head = match fa.peek_raw(fa.Remain().min(4)) {
        Some(h) if h.len() == 4 => h,
        _ => return false,
    };
    let magic = u32::from_be_bytes([head[0], head[1], head[2], head[3]]);
    if magic != MAGIC_MAC_SPACE && magic != MAGIC_MAC_F {
        return false;
    }

    fa.Element_Begin("APE");
    let mut identifier: int32u = 0;
    fa.Get_C4(&mut identifier, "Identifier");
    let mut version: int16u = 0;
    fa.Get_L2(&mut version, "Version");

    let mut sample_rate: int32u = 0;
    let mut total_frames: int32u = 0;
    let mut final_frame_samples: int32u = 0;
    let mut samples_per_frame: int32u = 0;
    let mut compression_level: int16u = 0;
    let mut channels: int16u = 0;
    let mut resolution: int16u = 0;

    if version < 3980 {
        if !parse_legacy_header(
            fa,
            version,
            &mut compression_level,
            &mut channels,
            &mut resolution,
            &mut sample_rate,
            &mut total_frames,
            &mut final_frame_samples,
            &mut samples_per_frame,
        ) {
            fa.Element_End();
            return false;
        }
    } else if !parse_modern_header(
        fa,
        &mut compression_level,
        &mut channels,
        &mut resolution,
        &mut sample_rate,
        &mut total_frames,
        &mut final_frame_samples,
        &mut samples_per_frame,
    ) {
        fa.Element_End();
        return false;
    }

    fa.Element_End();

    if total_frames == 0 || sample_rate == 0 || channels == 0 || resolution == 0 {
        return false;
    }
    let samples: u64 =
        (total_frames as u64 - 1) * (samples_per_frame as u64) + (final_frame_samples as u64);
    if samples == 0 {
        return false;
    }

    fill_streams(
        fa,
        identifier,
        version,
        compression_level,
        channels,
        resolution,
        sample_rate,
        samples,
    );
    true
}

fn parse_legacy_header(
    fa: &mut FileAnalyze,
    version: int16u,
    compression_level: &mut int16u,
    channels: &mut int16u,
    resolution: &mut int16u,
    sample_rate: &mut int32u,
    total_frames: &mut int32u,
    final_frame_samples: &mut int32u,
    samples_per_frame: &mut int32u,
) -> bool {
    // Legacy header is 26 bytes after Version (+ optional 44-byte RIFF + seek table).
    if fa.Remain() < 32 {
        return false;
    }
    fa.Get_L2(compression_level, "CompressionLevel");
    let mut flags: int16u = 0;
    fa.Get_L2(&mut flags, "FormatFlags");
    let resolution8 = (flags & 0x0001) != 0;
    let resolution24 = (flags & 0x0008) != 0;
    let no_wav_header = (flags & 0x0020) != 0;
    *resolution = if resolution8 {
        8
    } else if resolution24 {
        24
    } else {
        16
    };
    fa.Get_L2(channels, "Channels");
    fa.Get_L4(sample_rate, "SampleRate");
    fa.Skip_L4("WavHeaderDataBytes");
    fa.Skip_L4("WavTerminatingBytes");
    fa.Get_L4(total_frames, "TotalFrames");
    fa.Get_L4(final_frame_samples, "FinalFrameSamples");
    *samples_per_frame = ape_samples_per_frame(version, *compression_level);
    fa.Skip_L4("PeakLevel");
    let mut seek_elements: int32u = 0;
    fa.Get_L4(&mut seek_elements, "SeekElements");
    if !no_wav_header {
        if fa.Remain() < 44 {
            return false;
        }
        fa.Skip_Hexa(44, "RIFF header");
    }
    let seek_bytes = (seek_elements as usize) * 4;
    if fa.Remain() < seek_bytes {
        return false;
    }
    fa.Skip_Hexa(seek_bytes, "Seek table");
    true
}

fn parse_modern_header(
    fa: &mut FileAnalyze,
    compression_level: &mut int16u,
    channels: &mut int16u,
    resolution: &mut int16u,
    sample_rate: &mut int32u,
    total_frames: &mut int32u,
    final_frame_samples: &mut int32u,
    samples_per_frame: &mut int32u,
) -> bool {
    // Descriptor (46) + header (24) bytes after the version field.
    if fa.Remain() < 70 {
        return false;
    }
    fa.Skip_L2("Version_High");
    fa.Skip_L4("DescriptorBytes");
    fa.Skip_L4("HeaderBytes");
    fa.Skip_L4("SeekTableBytes");
    fa.Skip_L4("WavHeaderDataBytes");
    fa.Skip_L4("APEFrameDataBytes");
    fa.Skip_L4("APEFrameDataBytesHigh");
    fa.Skip_L4("WavTerminatingDataBytes");
    let mut _md5: int128u = 0;
    fa.Get_L16(&mut _md5, "FileMD5");
    fa.Get_L2(compression_level, "CompressionLevel");
    let mut _flags: int16u = 0;
    fa.Get_L2(&mut _flags, "FormatFlags");
    fa.Get_L4(samples_per_frame, "BlocksPerFrame");
    fa.Get_L4(final_frame_samples, "FinalFrameBlocks");
    fa.Get_L4(total_frames, "TotalFrames");
    fa.Get_L2(resolution, "BitsPerSample");
    fa.Get_L2(channels, "Channels");
    fa.Get_L4(sample_rate, "SampleRate");
    true
}

fn ape_samples_per_frame(version: int16u, compression_level: int16u) -> int32u {
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

fn ape_codec_settings(level: int16u) -> &'static str {
    match level {
        1000 => "Fast",
        2000 => "Normal",
        3000 => "High",
        4000 => "Extra-high",
        5000 => "Insane",
        _ => "",
    }
}

fn fill_streams(
    fa: &mut FileAnalyze,
    identifier: int32u,
    version: int16u,
    compression_level: int16u,
    channels: int16u,
    resolution: int16u,
    sample_rate: int32u,
    samples: u64,
) {
    let duration_ms: u64 = samples * 1000 / (sample_rate as u64);
    let uncompressed_size: u64 = samples * (channels as u64) * (resolution as u64 / 8);
    let version_str = format!("{:.3}", (version as f64) / 1000.0);

    fa.Stream_Prepare(StreamKind::General);
    fa.Fill(StreamKind::General, 0, "Format", "Monkey's Audio", false);
    fa.Fill(StreamKind::General, 0, "Format_Version", version_str.clone(), false);
    fa.Fill(StreamKind::General, 0, "AudioCount", "1", false);
    fa.Fill(StreamKind::General, 0, "StreamSize", "0", true);

    fa.Stream_Prepare(StreamKind::Audio);
    fa.Fill(StreamKind::Audio, 0, "Format", "Monkey's Audio", false);
    fa.Fill(StreamKind::Audio, 0, "Format_Version", version_str, false);
    // "MACF" magic indicates floating-point samples.
    if identifier == MAGIC_MAC_F {
        fa.Fill(StreamKind::Audio, 0, "Format_Profile", "Float", false);
    }
    let settings = ape_codec_settings(compression_level);
    if !settings.is_empty() {
        fa.Fill(StreamKind::Audio, 0, "Encoded_Library_Settings", settings, false);
    }
    fa.Fill(StreamKind::Audio, 0, "Codec", "APE", false);
    fa.Fill(StreamKind::Audio, 0, "Compression_Mode", "Lossless", false);
    fa.Fill(StreamKind::Audio, 0, "BitRate_Mode", "VBR", false);
    fa.Fill(StreamKind::Audio, 0, "BitDepth", resolution.to_string(), false);
    fa.Fill(StreamKind::Audio, 0, "Channels", channels.to_string(), false);
    fa.Fill(StreamKind::Audio, 0, "SamplingRate", sample_rate.to_string(), false);
    fa.Fill(StreamKind::Audio, 0, "SamplingCount", samples.to_string(), false);
    fa.Fill(StreamKind::Audio, 0, "Duration", duration_ms.to_string(), false);
    let _ = uncompressed_size;
}

#[cfg(test)]
mod tests {
    use super::*;

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

        let g = |k: &str| fa.Retrieve(StreamKind::General, 0, k).map(|z| z.as_str().to_owned());
        let a = |k: &str| fa.Retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());

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

        let a = |k: &str| fa.Retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());
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

        let a = |k: &str| fa.Retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());
        let g = |k: &str| fa.Retrieve(StreamKind::General, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(g("Format_Version").as_deref(), Some("3.800"));
        assert_eq!(a("BitDepth").as_deref(), Some("16"));
        assert_eq!(a("Channels").as_deref(), Some("2"));
        assert_eq!(a("SamplingRate").as_deref(), Some("44100"));
        // samples = (100-1)*9216 + 4096 = 916480
        assert_eq!(a("SamplingCount").as_deref(), Some("916480"));
    }
}
