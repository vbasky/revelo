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

use revelo_core::{FileAnalyze, Reader, StreamKind};

const BLOCK_TYPE_STREAMINFO: u8 = 0;
#[allow(dead_code)]
const BLOCK_TYPE_PADDING: u8 = 1;
#[allow(dead_code)]
const BLOCK_TYPE_APPLICATION: u8 = 2;
#[allow(dead_code)]
const BLOCK_TYPE_SEEKTABLE: u8 = 3;
const BLOCK_TYPE_VORBIS_COMMENT: u8 = 4;
const BLOCK_TYPE_CUESHEET: u8 = 5;
const BLOCK_TYPE_PICTURE: u8 = 6;

const FLAC_VENDOR_STRING_LIMIT: usize = 4 * 1024;
const FLAC_COMMENT_STRING_LIMIT: usize = 64 * 1024;

#[derive(Debug, Default)]
struct StreamInfo {
    min_frame_size: u32,
    max_frame_size: u32,
    sample_rate: u32,
    channels: u8,
    bits_per_sample: u8,
    total_samples: u64,
    md5: u128,
}

/// Parse FLAC (Free Lossless Audio Codec) stream.
///
/// Detection: `fLaC` marker.
/// Fills: STREAMINFO (channels, rate, depth, total samples), VorbisComment, MD5.
pub fn parse_flac(fa: &mut FileAnalyze) -> bool {
    parse(fa).is_some()
}

fn parse(fa: &mut FileAnalyze) -> Option<()> {
    let r = &mut Reader::wrap(fa);
    if r.remain() < 4 {
        return None;
    }
    if r.peek_be_u32()? != u32::from_be_bytes(*b"fLaC") {
        return None;
    }

    r.element_begin("FLAC");
    r.fourcc("Magic")?;

    let mut streaminfo: Option<StreamInfo> = None;
    let mut vorbis_comments: Option<VorbisComments> = None;

    loop {
        if r.remain() < 4 {
            break;
        }
        let header = r.be_u8("BlockHeader")?;
        let is_last = (header & 0x80) != 0;
        let block_type = header & 0x7F;
        let block_length = r.be_u24("BlockLength")?;
        let block_len_usize = block_length as usize;

        if r.remain() < block_len_usize {
            break;
        }

        match block_type {
            BLOCK_TYPE_STREAMINFO => {
                r.element_begin("STREAMINFO");
                streaminfo = parse_streaminfo(r);
                r.element_end();
                if block_len_usize > 34 {
                    r.skip(block_len_usize - 34)?; // Extension
                }
            }
            BLOCK_TYPE_VORBIS_COMMENT => {
                r.element_begin("VORBIS_COMMENT");
                vorbis_comments = parse_vorbis_comment(r, block_len_usize);
                r.element_end();
            }
            BLOCK_TYPE_CUESHEET => {
                r.element_begin("CUESHEET");
                r.skip(block_len_usize)?; // CuesheetBlock
                r.element_end();
            }
            BLOCK_TYPE_PICTURE => {
                r.element_begin("PICTURE");
                r.skip(block_len_usize)?; // PictureBlock
                r.element_end();
            }
            _ => {
                r.skip(block_len_usize)?; // MetadataBlock
            }
        }

        if is_last {
            break;
        }
    }

    let audio_stream_size = r.remain() as u64;
    r.element_end();

    let info = streaminfo?;
    fill_streams(r, &info, audio_stream_size, vorbis_comments.as_ref());
    Some(())
}

/// Parse a FLAC VORBIS_COMMENT block payload (block_len bytes after the
/// 4-byte metadata header). Returns the vendor string. Comments
/// themselves are read past but not stored — TITLE/ARTIST/etc. mapping
/// lands in a follow-up commit.
///
/// Note: lengths inside the VORBIS_COMMENT block are little-endian, even
/// though the rest of FLAC is big-endian. This is inherited from the
/// Vorbis/Ogg origin of the comment format.
#[derive(Debug, Default)]
struct VorbisComments {
    vendor: String,
    title: Option<String>,
    artist: Option<String>,
    album: Option<String>,
    date: Option<String>,
    track_number: Option<String>,
    genre: Option<String>,
    description: Option<String>,
}

fn parse_vorbis_comment(r: &mut Reader<'_, '_>, block_len: usize) -> Option<VorbisComments> {
    let start_offset = r.element_offset();
    let end_offset = start_offset + block_len;

    let vendor_len = r.le_u32("vendor_length")?;
    let vendor_len_usize = vendor_len as usize;
    if r.element_offset() + vendor_len_usize > end_offset {
        // Malformed — skip to block end.
        if r.remain() < end_offset - r.element_offset() {
            return None;
        }
        r.skip(end_offset - r.element_offset())?; // MalformedComment
        return None;
    }

    let vendor =
        read_utf8_limited(r, vendor_len_usize, FLAC_VENDOR_STRING_LIMIT).unwrap_or_default();

    let mut comments = VorbisComments { vendor, ..Default::default() };

    // Consume remaining comments (length-prefixed UTF-8 strings).
    if r.remain() >= 4 {
        let num_comments = r.le_u32("user_comment_list_length")?;
        for _ in 0..num_comments {
            if r.element_offset() + 4 > end_offset {
                break;
            }
            let comment_len = r.le_u32("comment_length")?;
            let cl = comment_len as usize;
            if r.element_offset() + cl > end_offset {
                break;
            }

            let Some(comment) = read_utf8_limited(r, cl, FLAC_COMMENT_STRING_LIMIT) else {
                continue;
            };

            // Vorbis comments are "FIELD=value" format
            if let Some(eq_pos) = comment.find('=') {
                let field = &comment[..eq_pos];
                let value = &comment[eq_pos + 1..];

                match field.to_uppercase().as_str() {
                    "TITLE" => comments.title = Some(value.to_string()),
                    "ARTIST" => comments.artist = Some(value.to_string()),
                    "ALBUM" => comments.album = Some(value.to_string()),
                    "DATE" => comments.date = Some(value.to_string()),
                    "TRACKNUMBER" => comments.track_number = Some(value.to_string()),
                    "GENRE" => comments.genre = Some(value.to_string()),
                    "DESCRIPTION" => comments.description = Some(value.to_string()),
                    _ => {}
                }
            }
        }
    }

    // If the block declared more bytes than we consumed, skip the trailer.
    if r.element_offset() < end_offset {
        r.skip(end_offset - r.element_offset())?; // Padding
    }

    Some(comments)
}

fn read_utf8_limited(r: &mut Reader<'_, '_>, len: usize, limit: usize) -> Option<String> {
    if len > limit {
        r.skip(len)?;
        return None;
    }
    Some(String::from_utf8_lossy(r.read_raw(len)?).into_owned())
}

fn parse_streaminfo(r: &mut Reader<'_, '_>) -> Option<StreamInfo> {
    r.be_u16("BlockSize_Min")?;
    r.be_u16("BlockSize_Max")?;
    let min_frame_size = r.be_u24("FrameSize_Min")?;
    let max_frame_size = r.be_u24("FrameSize_Max")?;

    let (sample_rate, channels, bps, samples) = r.bits(|b| {
        let sample_rate = b.read::<u32>(20, "SampleRate")?;
        let channels = b.read::<u8>(3, "Channels")?;
        let bps = b.read::<u8>(5, "BitPerSample")?;
        let samples = b.read::<u64>(36, "Samples")?;
        Some((sample_rate, channels, bps, samples))
    })?;

    let md5 = r.be_u128("MD5")?;

    Some(StreamInfo {
        min_frame_size,
        max_frame_size,
        sample_rate,
        channels,
        bits_per_sample: bps,
        total_samples: samples,
        md5,
    })
}

fn fill_streams(
    r: &mut Reader<'_, '_>,
    info: &StreamInfo,
    audio_stream_size: u64,
    vorbis_comments: Option<&VorbisComments>,
) {
    if info.sample_rate == 0 {
        return;
    }
    r.stream_prepare(StreamKind::General);
    r.set_field(StreamKind::General, 0, "Format", "FLAC");
    // FLAC reports general StreamSize as 0 because the file *is* the audio
    // stream + metadata, with no separate container overhead in MediaInfo's
    // accounting model. Replace=true so the revelo-diff fallback can't
    // overwrite to FileSize-audio_StreamSize.
    r.force_field(StreamKind::General, 0, "StreamSize", "0");

    r.stream_prepare(StreamKind::Audio);
    r.set_field(StreamKind::Audio, 0, "Format", "FLAC");

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
    r.set_field(StreamKind::Audio, 0, "BitRate_Mode", bitrate_mode);

    // Duration as integer milliseconds — same C++ pattern as AIFF
    // (AfterComma=0 stored as int).
    let duration_ms_int: i64 = if info.total_samples > 0 {
        ((info.total_samples as f64) / (sample_rate as f64) * 1000.0).round() as i64
    } else {
        0
    };
    if duration_ms_int > 0 {
        r.set_field(StreamKind::Audio, 0, "Duration", duration_ms_int.to_string());
    }

    // BitRate (integer for FLAC, no decimal — matches oracle "203651").
    if duration_ms_int > 0 {
        let bitrate =
            ((audio_stream_size as f64) * 8.0 * 1000.0 / (duration_ms_int as f64)).round() as u64;
        r.set_field(StreamKind::Audio, 0, "BitRate", bitrate.to_string());
    }

    r.set_field(StreamKind::Audio, 0, "Channels", channels_count.to_string());
    let (positions, layout) = channel_layout(channels_count);
    if let Some(p) = positions {
        r.set_field(StreamKind::Audio, 0, "ChannelPositions", p);
    }
    if let Some(l) = layout {
        r.set_field(StreamKind::Audio, 0, "ChannelLayout", l);
    }
    r.set_field(StreamKind::Audio, 0, "SamplingRate", info.sample_rate.to_string());
    if info.total_samples > 0 {
        r.set_field(StreamKind::Audio, 0, "SamplingCount", info.total_samples.to_string());
    }
    r.set_field(StreamKind::Audio, 0, "BitDepth", bps.to_string());
    r.set_field(StreamKind::Audio, 0, "Compression_Mode", "Lossless");
    r.set_field(StreamKind::Audio, 0, "StreamSize", audio_stream_size.to_string());

    if let Some(vc) = vorbis_comments {
        if !vc.vendor.is_empty() {
            r.set_field(StreamKind::Audio, 0, "Encoded_Library", vc.vendor.as_str());
            r.set_field(StreamKind::General, 0, "Encoded_Application", vc.vendor.as_str());
        }

        // Emit standard metadata fields from Vorbis comments
        if let Some(ref title) = vc.title {
            r.set_field(StreamKind::General, 0, "Track", title.as_str());
        }
        if let Some(ref artist) = vc.artist {
            r.set_field(StreamKind::General, 0, "Performer", artist.as_str());
        }
        if let Some(ref album) = vc.album {
            r.set_field(StreamKind::General, 0, "Album", album.as_str());
        }
        if let Some(ref date) = vc.date {
            r.set_field(StreamKind::General, 0, "Recorded_Date", date.as_str());
        }
        if let Some(ref track_num) = vc.track_number {
            r.set_field(StreamKind::General, 0, "Track/Position", track_num.as_str());
        }
        if let Some(ref genre) = vc.genre {
            r.set_field(StreamKind::General, 0, "Genre", genre.as_str());
        }
        if let Some(ref desc) = vc.description {
            r.set_field(StreamKind::General, 0, "Description", desc.as_str());
        }
    }

    // MD5 of unencoded audio, rendered as 32-hex-char uppercase string.
    // Goes in the <extra> section per oracle output.
    let md5_hex = format!("{:032X}", info.md5);
    r.set_field(StreamKind::Audio, 0, "MD5_Unencoded", md5_hex);

    r.set_field(StreamKind::General, 0, "AudioCount", "1");
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

    const FLAC_METADATA_ONLY_BUDGET: u64 = 8 * 1024 * 1024;

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

    fn append_streaminfo(buf: &mut Vec<u8>, is_last: bool) {
        buf.push(if is_last { 0x80 } else { 0x00 });
        buf.extend_from_slice(&[0, 0, 34]);
        buf.extend_from_slice(&[0, 0]);
        buf.extend_from_slice(&[0, 0]);
        buf.extend_from_slice(&[0, 0, 0]);
        buf.extend_from_slice(&[0, 0, 0]);
        let packed = pack_streaminfo_packed_field(48000, 1, 15, 1000);
        buf.extend_from_slice(&packed);
        buf.extend_from_slice(&[0u8; 16]);
    }

    #[test]
    fn parse_minimal_flac() {
        let buf = make_flac(48000, 2, 16, 71638, 37981);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_flac(&mut fa));

        let g = |k: &str| fa.retrieve(StreamKind::General, 0, k).map(|z| z.as_str().to_owned());
        let a = |k: &str| fa.retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());

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
            fa.retrieve(StreamKind::Audio, 0, "BitRate_Mode")
                .map(|z| z.as_str().to_owned())
                .as_deref(),
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
            fa.retrieve(StreamKind::Audio, 0, "SamplingRate")
                .map(|z| z.as_str().to_owned())
                .as_deref(),
            Some("44100")
        );
    }

    #[test]
    fn oversized_vorbis_vendor_is_skipped() {
        let vendor_len = FLAC_VENDOR_STRING_LIMIT + 1;
        let block_len = 4 + vendor_len + 4;
        let mut buf = Vec::new();
        buf.extend_from_slice(b"fLaC");
        append_streaminfo(&mut buf, false);
        buf.push(0x80 | BLOCK_TYPE_VORBIS_COMMENT);
        buf.extend_from_slice(&[
            ((block_len >> 16) & 0xff) as u8,
            ((block_len >> 8) & 0xff) as u8,
            (block_len & 0xff) as u8,
        ]);
        buf.extend_from_slice(&(vendor_len as u32).to_le_bytes());
        buf.resize(buf.len() + vendor_len, b'v');
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.resize(buf.len() + 16, 0);

        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_flac(&mut fa));
        assert!(fa.access_stats().max_request_len < vendor_len);
    }

    #[test]
    fn large_padding_block_access_stays_bounded() {
        let padding_len = FLAC_METADATA_ONLY_BUDGET as usize + 1024;
        let mut buf = Vec::new();
        buf.extend_from_slice(b"fLaC");
        append_streaminfo(&mut buf, false);
        buf.push(0x80 | BLOCK_TYPE_PADDING);
        buf.extend_from_slice(&[
            ((padding_len >> 16) & 0xff) as u8,
            ((padding_len >> 8) & 0xff) as u8,
            (padding_len & 0xff) as u8,
        ]);
        buf.resize(buf.len() + padding_len, 0);

        assert!(buf.len() as u64 > FLAC_METADATA_ONLY_BUDGET);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_flac(&mut fa));

        let stats = fa.access_stats();
        assert!(stats.bytes_requested < FLAC_METADATA_ONLY_BUDGET, "{stats:?}");
        assert!(stats.bytes_returned < FLAC_METADATA_ONLY_BUDGET, "{stats:?}");
        assert!(stats.max_request_len <= 34, "{stats:?}");
    }
}
