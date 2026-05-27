//! FLAC (Free Lossless Audio Codec) parser — native FLAC streams.
//!
//! Mirrors the subset of MediaInfoLib's `File_Flac.cpp` needed to fill
//! the audio + general fields the oracle emits for plain FLAC files.
//! Skips VORBIS_COMMENT/PICTURE/CUESHEET parsing for this commit;
//! Encoded_Library/Application come from VORBIS_COMMENT and will be
//! added when that parser lands.
//!
//! Layout:
//!   "fLaC"                                                              // magic
//!   metadata_block*                                                     // until is_last=true
//!     1 byte:  is_last (1 bit) | block_type (7 bits)
//!     3 bytes BE: block_length
//!     block_length bytes payload
//!   audio frames                                                        // rest of file
//!
//! STREAMINFO (block_type=0, payload 34 bytes):
//!   2 bytes BE: min_block_size (samples)
//!   2 bytes BE: max_block_size (samples)
//!   3 bytes BE: min_frame_size (bytes)
//!   3 bytes BE: max_frame_size (bytes)
//!   bit-packed:
//!     20 bits: sample_rate
//!      3 bits: channels - 1
//!      5 bits: bits_per_sample - 1
//!     36 bits: total_samples
//!   16 bytes: MD5 of unencoded audio

use mediainfo_core::{FileAnalyze, StreamKind};
use zenlib::{int128u, int16u, int32u, int64u, int8u};

const BLOCK_TYPE_STREAMINFO: u8 = 0;
const BLOCK_TYPE_VORBIS_COMMENT: u8 = 4;

#[derive(Debug, Default)]
struct StreamInfo {
    min_frame_size: int32u,
    max_frame_size: int32u,
    sample_rate: int32u,
    channels: int8u,
    bits_per_sample: int8u,
    total_samples: int64u,
    md5: int128u,
}

pub fn parse_flac(fa: &mut FileAnalyze) -> bool {
    if fa.Remain() < 4 {
        return false;
    }
    let mut magic: int32u = 0;
    fa.Peek_B4(&mut magic);
    if magic != u32::from_be_bytes(*b"fLaC") {
        return false;
    }

    fa.Element_Begin("FLAC");
    let mut magic_consume: int32u = 0;
    fa.Get_C4(&mut magic_consume, "Magic");

    let mut streaminfo: Option<StreamInfo> = None;
    let mut vendor: Option<String> = None;

    loop {
        if fa.Remain() < 4 {
            break;
        }
        let mut header: int8u = 0;
        fa.Get_B1(&mut header, "BlockHeader");
        let is_last = (header & 0x80) != 0;
        let block_type = header & 0x7F;
        let mut block_length: int32u = 0;
        fa.Get_B3(&mut block_length, "BlockLength");
        let block_len_usize = block_length as usize;

        if fa.Remain() < block_len_usize {
            break;
        }

        match block_type {
            BLOCK_TYPE_STREAMINFO => {
                fa.Element_Begin("STREAMINFO");
                streaminfo = Some(parse_streaminfo(fa));
                fa.Element_End();
                if block_len_usize > 34 {
                    fa.Skip_Hexa(block_len_usize - 34, "Extension");
                }
            }
            BLOCK_TYPE_VORBIS_COMMENT => {
                fa.Element_Begin("VORBIS_COMMENT");
                vendor = parse_vorbis_comment(fa, block_len_usize);
                fa.Element_End();
            }
            _ => {
                fa.Skip_Hexa(block_len_usize, "MetadataBlock");
            }
        }

        if is_last {
            break;
        }
    }

    let audio_stream_size = fa.Remain() as u64;
    fa.Element_End();

    if let Some(info) = streaminfo {
        fill_streams(fa, &info, audio_stream_size, vendor.as_deref());
        true
    } else {
        false
    }
}

/// Parse a FLAC VORBIS_COMMENT block payload (block_len bytes after the
/// 4-byte metadata header). Returns the vendor string. Comments
/// themselves are read past but not stored — TITLE/ARTIST/etc. mapping
/// lands in a follow-up commit.
///
/// Note: lengths inside the VORBIS_COMMENT block are little-endian, even
/// though the rest of FLAC is big-endian. This is inherited from the
/// Vorbis/Ogg origin of the comment format.
fn parse_vorbis_comment(fa: &mut FileAnalyze, block_len: usize) -> Option<String> {
    let start_offset = fa.Element_Offset();
    let end_offset = start_offset + block_len;

    let mut vendor_len: int32u = 0;
    fa.Get_L4(&mut vendor_len, "vendor_length");
    let vendor_len_usize = vendor_len as usize;
    if fa.Element_Offset() + vendor_len_usize > end_offset {
        // Malformed — skip to block end.
        if fa.Remain() < end_offset - fa.Element_Offset() {
            return None;
        }
        fa.Skip_Hexa(end_offset - fa.Element_Offset(), "MalformedComment");
        return None;
    }

    let vendor_bytes = fa.read_raw(vendor_len_usize);
    let vendor = String::from_utf8_lossy(vendor_bytes).into_owned();

    // Consume remaining comments (length-prefixed UTF-8 strings).
    let mut num_comments: int32u = 0;
    if fa.Remain() >= 4 {
        fa.Get_L4(&mut num_comments, "user_comment_list_length");
        for _ in 0..num_comments {
            if fa.Element_Offset() + 4 > end_offset {
                break;
            }
            let mut comment_len: int32u = 0;
            fa.Get_L4(&mut comment_len, "comment_length");
            let cl = comment_len as usize;
            if fa.Element_Offset() + cl > end_offset {
                break;
            }
            fa.Skip_Hexa(cl, "user_comment");
        }
    }

    // If the block declared more bytes than we consumed, skip the trailer.
    if fa.Element_Offset() < end_offset {
        fa.Skip_Hexa(end_offset - fa.Element_Offset(), "Padding");
    }

    Some(vendor)
}

fn parse_streaminfo(fa: &mut FileAnalyze) -> StreamInfo {
    let mut min_block_size: int16u = 0;
    let mut max_block_size: int16u = 0;
    fa.Get_B2(&mut min_block_size, "BlockSize_Min");
    fa.Get_B2(&mut max_block_size, "BlockSize_Max");
    let mut min_frame_size: int32u = 0;
    let mut max_frame_size: int32u = 0;
    fa.Get_B3(&mut min_frame_size, "FrameSize_Min");
    fa.Get_B3(&mut max_frame_size, "FrameSize_Max");

    fa.BS_Begin();
    let mut sample_rate: int32u = 0;
    let mut channels: int8u = 0;
    let mut bps: int8u = 0;
    let mut samples: int64u = 0;
    fa.Get_S3(20, &mut sample_rate, "SampleRate");
    fa.Get_S1(3, &mut channels, "Channels");
    fa.Get_S1(5, &mut bps, "BitPerSample");
    fa.Get_S5(36, &mut samples, "Samples");
    fa.BS_End();

    let mut md5: int128u = 0;
    fa.Get_B16(&mut md5, "MD5");

    StreamInfo {
        min_frame_size,
        max_frame_size,
        sample_rate,
        channels,
        bits_per_sample: bps,
        total_samples: samples,
        md5,
    }
}

fn fill_streams(
    fa: &mut FileAnalyze,
    info: &StreamInfo,
    audio_stream_size: u64,
    vendor: Option<&str>,
) {
    if info.sample_rate == 0 {
        return;
    }
    fa.Stream_Prepare(StreamKind::General);
    fa.Fill(StreamKind::General, 0, "Format", "FLAC", false);
    // FLAC reports general StreamSize as 0 because the file *is* the audio
    // stream + metadata, with no separate container overhead in MediaInfo's
    // accounting model. Replace=true so the diff-harness fallback can't
    // overwrite to FileSize-audio_StreamSize.
    fa.Fill(StreamKind::General, 0, "StreamSize", "0", true);

    fa.Stream_Prepare(StreamKind::Audio);
    fa.Fill(StreamKind::Audio, 0, "Format", "FLAC", false);

    let channels_count = (info.channels as u16) + 1;
    let bps = (info.bits_per_sample as u16) + 1;
    let sample_rate = info.sample_rate as u64;

    // BitRate_Mode: CBR if min_frame_size == max_frame_size and both != 0,
    // else VBR. Matches File_Flac.cpp logic.
    let bitrate_mode = if info.min_frame_size != 0 && info.min_frame_size == info.max_frame_size {
        "CBR"
    } else {
        "VBR"
    };
    fa.Fill(StreamKind::Audio, 0, "BitRate_Mode", bitrate_mode, false);

    // Duration as integer milliseconds — same C++ pattern as AIFF
    // (AfterComma=0 stored as int).
    let duration_ms_int: i64 = if info.total_samples > 0 {
        ((info.total_samples as f64) / (sample_rate as f64) * 1000.0).round() as i64
    } else {
        0
    };
    if duration_ms_int > 0 {
        fa.Fill(StreamKind::Audio, 0, "Duration", duration_ms_int.to_string(), false);
    }

    // BitRate (integer for FLAC, no decimal — matches oracle "203651").
    if duration_ms_int > 0 {
        let bitrate = ((audio_stream_size as f64) * 8.0 * 1000.0 / (duration_ms_int as f64)).round() as u64;
        fa.Fill(StreamKind::Audio, 0, "BitRate", bitrate.to_string(), false);
    }

    fa.Fill(StreamKind::Audio, 0, "Channels", channels_count.to_string(), false);
    let (positions, layout) = channel_layout(channels_count);
    if let Some(p) = positions {
        fa.Fill(StreamKind::Audio, 0, "ChannelPositions", p, false);
    }
    if let Some(l) = layout {
        fa.Fill(StreamKind::Audio, 0, "ChannelLayout", l, false);
    }
    fa.Fill(StreamKind::Audio, 0, "SamplingRate", info.sample_rate.to_string(), false);
    if info.total_samples > 0 {
        fa.Fill(StreamKind::Audio, 0, "SamplingCount", info.total_samples.to_string(), false);
    }
    fa.Fill(StreamKind::Audio, 0, "BitDepth", bps.to_string(), false);
    fa.Fill(StreamKind::Audio, 0, "Compression_Mode", "Lossless", false);
    fa.Fill(StreamKind::Audio, 0, "StreamSize", audio_stream_size.to_string(), false);

    if let Some(v) = vendor {
        fa.Fill(StreamKind::Audio, 0, "Encoded_Library", v, false);
        fa.Fill(StreamKind::General, 0, "Encoded_Application", v, false);
    }

    // MD5 of unencoded audio, rendered as 32-hex-char uppercase string.
    // Goes in the <extra> section per oracle output.
    let md5_hex = format!("{:032X}", info.md5);
    fa.Fill(StreamKind::Audio, 0, "MD5_Unencoded", md5_hex, false);

    fa.Fill(StreamKind::General, 0, "AudioCount", "1", false);
}

/// Map channel count → (ChannelPositions, ChannelLayout) using the same
/// standard WAVE_FORMAT_EXTENSIBLE mask conventions MediaInfoLib applies.
/// Only the common cases are populated for now; uncommon counts fall
/// through unset (matches oracle behavior on those).
fn channel_layout(channels: u16) -> (Option<&'static str>, Option<&'static str>) {
    match channels {
        1 => (Some("Front: C"), Some("C")),
        2 => (Some("Front: L R"), Some("L R")),
        3 => (Some("Front: L C R"), Some("L R C")),
        4 => (Some("Front: L R, Back: L R"), Some("L R Lb Rb")),
        5 => (Some("Front: L C R, Side: L R"), Some("L R C Ls Rs")),
        6 => (Some("Front: L C R, Side: L R, LFE"), Some("L R C LFE Ls Rs")),
        7 => (Some("Front: L C R, Side: L R, Back: C, LFE"), Some("L R C LFE Cb Ls Rs")),
        8 => (Some("Front: L C R, Side: L R, Back: L R, LFE"), Some("L R C LFE Lb Rb Ls Rs")),
        _ => (None, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pack_streaminfo_packed_field(
        sample_rate: u32,
        channels_m1: u8,
        bps_m1: u8,
        samples: u64,
    ) -> [u8; 8] {
        let mut packed: u64 = 0;
        packed |= (sample_rate as u64) << (3 + 5 + 36);
        packed |= (channels_m1 as u64) << (5 + 36);
        packed |= (bps_m1 as u64) << 36;
        packed |= samples & ((1u64 << 36) - 1);
        packed.to_be_bytes()
    }

    fn make_flac(
        sample_rate: u32,
        channels: u8,
        bps: u8,
        samples: u64,
        audio_size: usize,
    ) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(b"fLaC");
        // STREAMINFO block: is_last=1, type=0
        buf.push(0x80);
        // length = 34
        buf.extend_from_slice(&[0, 0, 34]);
        // payload
        buf.extend_from_slice(&[0, 0]); // min block size
        buf.extend_from_slice(&[0, 0]); // max block size
        buf.extend_from_slice(&[0, 0, 0]); // min frame size
        buf.extend_from_slice(&[0, 0, 0]); // max frame size
        let packed = pack_streaminfo_packed_field(sample_rate, channels - 1, bps - 1, samples);
        buf.extend_from_slice(&packed);
        buf.extend_from_slice(&[0u8; 16]); // MD5
        // Audio frames (stub bytes)
        buf.resize(buf.len() + audio_size, 0);
        buf
    }

    #[test]
    fn parse_minimal_flac() {
        let buf = make_flac(48000, 2, 16, 71638, 37981);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_flac(&mut fa));

        let g = |k: &str| fa.Retrieve(StreamKind::General, 0, k).map(|z| z.as_str().to_owned());
        let a = |k: &str| fa.Retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());

        assert_eq!(g("Format").as_deref(), Some("FLAC"));
        assert_eq!(g("StreamSize").as_deref(), Some("0"));
        assert_eq!(g("AudioCount").as_deref(), Some("1"));

        assert_eq!(a("Format").as_deref(), Some("FLAC"));
        assert_eq!(a("BitRate_Mode").as_deref(), Some("VBR"));
        assert_eq!(a("Channels").as_deref(), Some("2"));
        assert_eq!(a("ChannelPositions").as_deref(), Some("Front: L R"));
        assert_eq!(a("ChannelLayout").as_deref(), Some("L R"));
        assert_eq!(a("SamplingRate").as_deref(), Some("48000"));
        assert_eq!(a("SamplingCount").as_deref(), Some("71638"));
        assert_eq!(a("BitDepth").as_deref(), Some("16"));
        assert_eq!(a("Compression_Mode").as_deref(), Some("Lossless"));
        assert_eq!(a("StreamSize").as_deref(), Some("37981"));
        assert_eq!(a("Duration").as_deref(), Some("1492"));
        // BitRate = 37981*8*1000/1492 = 203651.47.. → 203651
        assert_eq!(a("BitRate").as_deref(), Some("203651"));
    }

    #[test]
    fn rejects_non_flac_buffer() {
        let mut fa = FileAnalyze::new(b"NOT a FLAC file at all..");
        assert!(!parse_flac(&mut fa));
    }

    #[test]
    fn detects_cbr_when_frame_sizes_equal() {
        // Build a FLAC where min == max frame size, both != 0.
        let mut buf = Vec::new();
        buf.extend_from_slice(b"fLaC");
        buf.push(0x80); // is_last + type=0
        buf.extend_from_slice(&[0, 0, 34]);
        buf.extend_from_slice(&[0, 0]);
        buf.extend_from_slice(&[0, 0]);
        // min == max == 100
        buf.extend_from_slice(&[0, 0, 100]);
        buf.extend_from_slice(&[0, 0, 100]);
        let packed = pack_streaminfo_packed_field(48000, 1, 15, 1000);
        buf.extend_from_slice(&packed);
        buf.extend_from_slice(&[0u8; 16]);
        buf.resize(buf.len() + 100, 0);

        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_flac(&mut fa));
        assert_eq!(
            fa.Retrieve(StreamKind::Audio, 0, "BitRate_Mode").map(|z| z.as_str().to_owned()).as_deref(),
            Some("CBR")
        );
    }

    #[test]
    fn skips_intermediate_padding_blocks() {
        let mut buf = Vec::new();
        buf.extend_from_slice(b"fLaC");
        // STREAMINFO (not last)
        buf.push(0x00);
        buf.extend_from_slice(&[0, 0, 34]);
        buf.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        let packed = pack_streaminfo_packed_field(44100, 1, 15, 1000);
        buf.extend_from_slice(&packed);
        buf.extend_from_slice(&[0u8; 16]);
        // PADDING block (is_last=1, type=1, length=20)
        buf.push(0x81);
        buf.extend_from_slice(&[0, 0, 20]);
        buf.extend_from_slice(&[0u8; 20]);
        buf.resize(buf.len() + 50, 0);

        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_flac(&mut fa));
        assert_eq!(
            fa.Retrieve(StreamKind::Audio, 0, "SamplingRate").map(|z| z.as_str().to_owned()).as_deref(),
            Some("44100")
        );
    }
}
