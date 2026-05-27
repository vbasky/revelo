//! MPEG Audio (MP1/MP2/MP3) parser — sync-based frame stream.
//!
//! Mirrors the subset of MediaInfoLib's `File_Mpega.cpp` needed for
//! plain MP3 files. No chunked container — instead a stream of frames
//! each starting with an 11-bit sync word. May be preceded by an ID3v2
//! tag and/or contain a Xing/Info/VBRI metadata frame at the start.
//!
//! Layout:
//!   [ID3v2 header + tag bytes]?
//!   frame*
//!     4 bytes frame header (sync<11> + version<2> + layer<2> + crc<1>
//!                          + bitrate<4> + samplerate<2> + padding<1>
//!                          + private<1> + chan_mode<2> + mode_ext<2>
//!                          + copyright<1> + original<1> + emphasis<2>)
//!     side info (8/17/32 bytes depending on version/channel mode)
//!     frame data
//!
//! First frame may carry a Xing/Info magic right after side info ⇒ it's
//! the VBR/CBR-LAME info frame, not an encoded audio frame; we skip it
//! and parse the next frame for the audio params.

use mediainfo_core::{FileAnalyze, StreamKind};

/// Lookup: [version_index][layer_index][bitrate_index] → kbps. Indexed by
/// the *bitstream* values: version 0..=3 (2.5,X,2,1), layer 0..=3
/// (X,3,2,1), bitrate 0..=15.
const BITRATES_KBPS: [[[u16; 16]; 4]; 4] = [
    // MPEG 2.5
    [
        [0; 16],
        [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160, 0],
        [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160, 0],
        [0, 32, 48, 56, 64, 80, 96, 112, 128, 144, 160, 176, 192, 224, 256, 0],
    ],
    // Reserved
    [[0; 16]; 4],
    // MPEG 2
    [
        [0; 16],
        [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160, 0],
        [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160, 0],
        [0, 32, 48, 56, 64, 80, 96, 112, 128, 144, 160, 176, 192, 224, 256, 0],
    ],
    // MPEG 1
    [
        [0; 16],
        [0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 0],
        [0, 32, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 384, 0],
        [0, 32, 64, 96, 128, 160, 192, 224, 256, 288, 320, 352, 384, 416, 448, 0],
    ],
];

const SAMPLE_RATES: [[u32; 4]; 4] = [
    [11025, 12000, 8000, 0], // MPEG 2.5
    [0, 0, 0, 0],            // Reserved
    [22050, 24000, 16000, 0], // MPEG 2
    [44100, 48000, 32000, 0], // MPEG 1
];

const SAMPLES_PER_FRAME: [[u16; 4]; 4] = [
    [0, 576, 1152, 384], // MPEG 2.5: Layer X, III, II, I
    [0, 0, 0, 0],        // Reserved
    [0, 576, 1152, 384], // MPEG 2
    [0, 1152, 1152, 384], // MPEG 1
];

/// Coefficient × bitrate_kbps × 1000 / sample_rate = frame size in bytes
/// (before padding). MPEG-1 Layer I needs special handling.
fn frame_size_bytes(version: u8, layer: u8, bitrate_kbps: u16, sample_rate: u32, padding: u8) -> u32 {
    if sample_rate == 0 || bitrate_kbps == 0 {
        return 0;
    }
    let bitrate_bps = bitrate_kbps as u32 * 1000;
    match (version, layer) {
        // MPEG-1 Layer I
        (3, 3) => (12 * bitrate_bps / sample_rate + padding as u32) * 4,
        // MPEG-2/2.5 Layer I
        (_, 3) => (12 * bitrate_bps / sample_rate + padding as u32) * 4,
        // MPEG-1 Layer II/III
        (3, _) => 144 * bitrate_bps / sample_rate + padding as u32,
        // MPEG-2/2.5 Layer III
        (_, 1) => 72 * bitrate_bps / sample_rate + padding as u32,
        // MPEG-2/2.5 Layer II
        (_, _) => 144 * bitrate_bps / sample_rate + padding as u32,
    }
}

const VERSION_NAMES: [&str; 4] = ["2.5", "?", "2", "1"];
const LAYER_NAMES: [&str; 4] = ["", "Layer 3", "Layer 2", "Layer 1"];
const CHANNEL_MODE_NAMES: [&str; 4] = ["Stereo", "Joint stereo", "Dual channel", "Single channel"];

fn channel_mode_to_count(mode: u8) -> u16 {
    if mode == 3 { 1 } else { 2 }
}

/// Layer III mode extension semantics (only meaningful for joint stereo):
/// bit 4 = intensity stereo, bit 5 = MS stereo. The C++ side displays
/// using the same combinations.
fn mode_extension_name(layer: u8, mode_ext: u8) -> &'static str {
    if layer == 1 {
        // Layer 3
        match mode_ext & 0x3 {
            0 => "",
            1 => "Intensity Stereo",
            2 => "MS Stereo",
            3 => "Intensity Stereo + MS Stereo",
            _ => "",
        }
    } else {
        // Layer 1 / 2: bounds (sub-bands)
        match mode_ext & 0x3 {
            0 => "Bound 4",
            1 => "Bound 8",
            2 => "Bound 12",
            3 => "Bound 16",
            _ => "",
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct FrameHeader {
    version: u8, // 0=2.5, 2=2, 3=1
    layer: u8,   // 1=L3, 2=L2, 3=L1
    bitrate_kbps: u16,
    sample_rate: u32,
    #[allow(dead_code)]
    padding: u8,
    channel_mode: u8,
    mode_ext: u8,
    samples_per_frame: u16,
    frame_size: u32,
}

fn parse_frame_header(bytes: &[u8]) -> Option<FrameHeader> {
    if bytes.len() < 4 {
        return None;
    }
    // Sync: 11 bits of 1s. Allow both 0xFFE (lax) and 0xFFF (strict).
    if bytes[0] != 0xFF || (bytes[1] & 0xE0) != 0xE0 {
        return None;
    }
    let version = (bytes[1] >> 3) & 0x3;
    let layer = (bytes[1] >> 1) & 0x3;
    let bitrate_idx = (bytes[2] >> 4) & 0xF;
    let sr_idx = (bytes[2] >> 2) & 0x3;
    let padding = (bytes[2] >> 1) & 0x1;
    let channel_mode = (bytes[3] >> 6) & 0x3;
    let mode_ext = (bytes[3] >> 4) & 0x3;

    if version == 1 || layer == 0 || bitrate_idx == 0 || bitrate_idx == 15 || sr_idx == 3 {
        return None;
    }

    let bitrate_kbps = BITRATES_KBPS[version as usize][layer as usize][bitrate_idx as usize];
    let sample_rate = SAMPLE_RATES[version as usize][sr_idx as usize];
    let samples_per_frame = SAMPLES_PER_FRAME[version as usize][layer as usize];
    let frame_size = frame_size_bytes(version, layer, bitrate_kbps, sample_rate, padding);

    Some(FrameHeader {
        version,
        layer,
        bitrate_kbps,
        sample_rate,
        padding,
        channel_mode,
        mode_ext,
        samples_per_frame,
        frame_size,
    })
}

/// Offset (from frame start) where Xing/Info/VBRI magic appears, if the
/// frame is an info frame. Depends on version + channel mode.
fn info_magic_offset(version: u8, channel_mode: u8) -> usize {
    let mono = channel_mode == 3;
    match (version, mono) {
        (3, false) => 36,
        (3, true) => 21,
        (_, false) => 21,
        (_, true) => 13,
    }
}

fn looks_like_info_frame(frame_bytes: &[u8], version: u8, channel_mode: u8) -> bool {
    let off = info_magic_offset(version, channel_mode);
    if frame_bytes.len() < off + 4 {
        return false;
    }
    let magic = &frame_bytes[off..off + 4];
    magic == b"Xing" || magic == b"Info" || magic == b"VBRI"
}

pub fn parse_mp3(fa: &mut FileAnalyze) -> bool {
    // Sync must be either at byte 0 OR immediately after an ID3v2
    // header. Scanning for sync further into the file produces false
    // positives on any container that happens to contain 0xFF 0xE/F
    // bytes (which is most of them). Container parsers run before
    // this so by the time we're here it's not been claimed.
    let id3v2_size = detect_id3v2(fa);
    if id3v2_size > 0 {
        fa.Skip_Hexa(id3v2_size, "ID3v2");
    }

    let head = fa.peek_raw(4);
    let Some(h) = head else { return false };
    if h[0] != 0xFF || (h[1] & 0xE0) != 0xE0 {
        return false;
    }
    if parse_frame_header(&h[..4]).is_none() {
        return false;
    }

    // Peek the first frame header.
    let first_header_bytes = fa.peek_raw(4);
    if first_header_bytes.is_none() {
        return false;
    }
    let first_header = match parse_frame_header(first_header_bytes.unwrap()) {
        Some(h) => h,
        None => return false,
    };

    fa.Element_Begin("MPEG Audio");

    // If the first frame is an Xing/Info/VBRI info frame, skip it and
    // re-parse the next frame for audio params.
    let first_frame_bytes = fa.peek_raw(first_header.frame_size as usize);
    let is_info_frame = first_frame_bytes
        .map(|b| looks_like_info_frame(b, first_header.version, first_header.channel_mode))
        .unwrap_or(false);

    let audio_frame_start;
    let audio_frame_header;
    if is_info_frame {
        fa.Skip_Hexa(first_header.frame_size as usize, "InfoFrame");
        audio_frame_start = fa.Element_Offset();
        let next_bytes = fa.peek_raw(4);
        let Some(b) = next_bytes else {
            fa.Element_End();
            return false;
        };
        let Some(h) = parse_frame_header(b) else {
            fa.Element_End();
            return false;
        };
        audio_frame_header = h;
    } else {
        audio_frame_start = fa.Element_Offset();
        audio_frame_header = first_header;
    }

    // Walk frames to count them.
    let (frame_count, audio_bytes) = scan_frames(fa, audio_frame_start);
    fa.Element_End();

    fill_streams(
        fa,
        &audio_frame_header,
        frame_count,
        audio_bytes,
        is_info_frame,
        id3v2_size,
    );
    true
}

fn detect_id3v2(fa: &mut FileAnalyze) -> usize {
    let bytes = fa.peek_raw(10);
    let Some(b) = bytes else { return 0 };
    if &b[0..3] != b"ID3" {
        return 0;
    }
    // Bytes 6..10 = syncsafe size: each byte's top bit is 0; lower 7
    // bits significant. Total tag size = 10 (header) + size_bytes.
    let size = ((b[6] as usize) << 21)
        | ((b[7] as usize) << 14)
        | ((b[8] as usize) << 7)
        | (b[9] as usize);
    10 + size
}


/// Scan frames sequentially from the current position, returning
/// (frame_count, total_bytes_consumed). Stops at the first unparseable
/// frame or end of buffer.
fn scan_frames(fa: &mut FileAnalyze, audio_frame_start: usize) -> (u32, u64) {
    let mut frame_count: u32 = 0;
    let starting_remain = fa.Remain();
    loop {
        let header_bytes = fa.peek_raw(4);
        let Some(b) = header_bytes else { break };
        let Some(h) = parse_frame_header(b) else {
            // Allow ID3v1 (128 bytes ending the file, magic "TAG") to
            // terminate the scan cleanly.
            break;
        };
        if h.frame_size == 0 || fa.Remain() < h.frame_size as usize {
            break;
        }
        fa.Skip_Hexa(h.frame_size as usize, "Frame");
        frame_count += 1;
    }
    let consumed = starting_remain.saturating_sub(fa.Remain()) as u64;
    let _ = audio_frame_start;
    (frame_count, consumed)
}

fn fill_streams(
    fa: &mut FileAnalyze,
    h: &FrameHeader,
    audio_frame_count: u32,
    audio_bytes_consumed: u64,
    had_info_frame: bool,
    id3v2_size: usize,
) {
    fa.Stream_Prepare(StreamKind::General);
    fa.Fill(StreamKind::General, 0, "Format", "MPEG Audio", false);

    fa.Stream_Prepare(StreamKind::Audio);
    fa.Fill(StreamKind::Audio, 0, "Format", "MPEG Audio", false);
    fa.Fill(StreamKind::Audio, 0, "Format_Version", VERSION_NAMES[h.version as usize], false);
    fa.Fill(StreamKind::Audio, 0, "Format_Profile", LAYER_NAMES[h.layer as usize], false);
    fa.Fill(
        StreamKind::Audio,
        0,
        "Format_Settings_Mode",
        CHANNEL_MODE_NAMES[h.channel_mode as usize],
        false,
    );
    if h.channel_mode == 1 {
        let ext = mode_extension_name(h.layer, h.mode_ext);
        if !ext.is_empty() {
            fa.Fill(StreamKind::Audio, 0, "Format_Settings_ModeExtension", ext, false);
        }
    }
    fa.Fill(StreamKind::Audio, 0, "BitRate_Mode", "CBR", false);
    let bitrate_bps = (h.bitrate_kbps as u32) * 1000;
    fa.Fill(StreamKind::Audio, 0, "BitRate", bitrate_bps.to_string(), false);
    fa.Fill(
        StreamKind::Audio,
        0,
        "Channels",
        channel_mode_to_count(h.channel_mode).to_string(),
        false,
    );
    fa.Fill(StreamKind::Audio, 0, "SamplesPerFrame", h.samples_per_frame.to_string(), false);
    fa.Fill(StreamKind::Audio, 0, "SamplingRate", h.sample_rate.to_string(), false);

    // SamplingCount counts ALL frames including the info frame's nominal
    // samples — matches oracle.
    let total_frame_count = audio_frame_count + had_info_frame as u32;
    let sampling_count = (total_frame_count as u64) * (h.samples_per_frame as u64);
    if sampling_count > 0 {
        fa.Fill(StreamKind::Audio, 0, "SamplingCount", sampling_count.to_string(), false);
    }

    // FrameRate = sample_rate / samples_per_frame, 3 decimal places.
    if h.samples_per_frame > 0 {
        let frame_rate = (h.sample_rate as f64) / (h.samples_per_frame as f64);
        fa.Fill(StreamKind::Audio, 0, "FrameRate", format!("{:.3}", frame_rate), false);
    }
    fa.Fill(StreamKind::Audio, 0, "FrameCount", total_frame_count.to_string(), false);
    fa.Fill(StreamKind::Audio, 0, "Compression_Mode", "Lossy", false);
    fa.Fill(StreamKind::Audio, 0, "StreamSize", audio_bytes_consumed.to_string(), false);

    // Audio Duration: frames-based.
    if h.sample_rate > 0 && sampling_count > 0 {
        let duration_ms = (sampling_count * 1000) / (h.sample_rate as u64);
        fa.Fill(StreamKind::Audio, 0, "Duration", duration_ms.to_string(), false);
    }

    // General-stream fields that aren't file-level but need the parser's
    // knowledge of ID3v2/audio_bytes vs the harness's FileSize-Audio.StreamSize
    // fallback. Use replace=true so the harness can't overwrite.
    fa.Fill(StreamKind::General, 0, "StreamSize", id3v2_size.to_string(), true);
    if bitrate_bps > 0 {
        // General Duration = Audio.StreamSize * 8000 / BitRate_bps — gives a
        // round duration like 1.536s for a 24576-byte/128kbps stream, which
        // is what the oracle reports.
        let general_duration_ms =
            (audio_bytes_consumed * 8 * 1000) / (bitrate_bps as u64);
        fa.Fill(
            StreamKind::General,
            0,
            "Duration",
            general_duration_ms.to_string(),
            true,
        );
        // OverallBitRate for CBR MPEG Audio is the audio bitrate itself —
        // the C++ side bypasses the FileSize/Duration computation in this
        // case. replace=true to override the harness fallback.
        fa.Fill(
            StreamKind::General,
            0,
            "OverallBitRate",
            bitrate_bps.to_string(),
            true,
        );
    }
    fa.Fill(StreamKind::General, 0, "AudioCount", "1", false);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_canonical_mpeg1_layer3_header() {
        // 0xFF 0xFB 0x94 0x44 — MPEG-1 Layer III, 128kbps, 48kHz, Joint
        // stereo, padding off, no protection.
        let h = parse_frame_header(&[0xFF, 0xFB, 0x94, 0x44]).expect("parse");
        assert_eq!(h.version, 3);
        assert_eq!(h.layer, 1);
        assert_eq!(h.bitrate_kbps, 128);
        assert_eq!(h.sample_rate, 48000);
        assert_eq!(h.channel_mode, 1);
        assert_eq!(h.samples_per_frame, 1152);
        // frame_size = 144 * 128000 / 48000 = 384
        assert_eq!(h.frame_size, 384);
    }

    #[test]
    fn rejects_non_sync_buffer() {
        assert!(parse_frame_header(&[0x00, 0x00, 0x00, 0x00]).is_none());
        assert!(parse_frame_header(&[0xFF, 0x1F, 0x94, 0x44]).is_none());
    }

    #[test]
    fn rejects_reserved_or_invalid_fields() {
        // version=1 (reserved): byte1 bits[4:3] = 01 ⇒ byte1 = 0b11101011 = 0xEB
        assert!(parse_frame_header(&[0xFF, 0xEB, 0x94, 0x44]).is_none());
        // layer=0 (reserved): byte1 bits[2:1] = 00 ⇒ byte1 = 0b11111001 = 0xF9
        assert!(parse_frame_header(&[0xFF, 0xF9, 0x94, 0x44]).is_none());
        // bitrate=15 (bad)
        assert!(parse_frame_header(&[0xFF, 0xFB, 0xF4, 0x44]).is_none());
        // bitrate=0 (free) — also reject; we don't compute frame size for free format
        assert!(parse_frame_header(&[0xFF, 0xFB, 0x04, 0x44]).is_none());
    }

    #[test]
    fn channel_count_derivation() {
        assert_eq!(channel_mode_to_count(0), 2);
        assert_eq!(channel_mode_to_count(1), 2);
        assert_eq!(channel_mode_to_count(2), 2);
        assert_eq!(channel_mode_to_count(3), 1);
    }
}
