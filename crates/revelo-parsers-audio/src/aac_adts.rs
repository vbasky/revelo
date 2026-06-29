//! AAC ADTS (Audio Data Transport Stream) parser — raw AAC frames
//! wrapped in self-synchronizing ADTS headers.
//!
//! ADTS frame header (7 bytes when protection_absent=1):
//!   syncword                  (12 bits, 0xFFF)
//!   ID                        (1 bit, 0=MPEG-4, 1=MPEG-2)
//!   layer                     (2 bits, 0)
//!   protection_absent         (1 bit)
//!   profile                   (2 bits, AOT-1)
//!   sample_rate_idx           (4 bits)
//!   private                   (1 bit)
//!   channel_config            (3 bits)
//!   ...
//!   aac_frame_length          (13 bits)
//!   adts_buffer_fullness      (11 bits)
//!   number_of_raw_data_blocks (2 bits)

use revelo_core::{FileAnalyze, StreamKind};

const SAMPLE_RATE_TABLE: [u32; 13] =
    [96000, 88200, 64000, 48000, 44100, 32000, 24000, 22050, 16000, 12000, 11025, 8000, 7350];

/// Parse AAC (Advanced Audio Coding) ADTS stream.
///
/// Detection: Sync word 0xFFF (12 bits).
/// Fills: Profile (LC/HE-AAC/HE-AACv2), sampling rate, channels.
pub fn parse_aac_adts(fa: &mut FileAnalyze) -> bool {
    let head = fa.peek_raw(7);
    let Some(h) = head else { return false };
    // ADTS sync: 12 bits of 1s. First byte = 0xFF, top 4 bits of second byte = 0xF.
    if h[0] != 0xFF || (h[1] & 0xF0) != 0xF0 {
        return false;
    }
    let id = (h[1] >> 3) & 0x1;
    let _layer = (h[1] >> 1) & 0x3;
    let profile = (h[2] >> 6) & 0x3;
    let sample_rate_idx = (h[2] >> 2) & 0xF;
    let channel_config = ((h[2] & 0x1) << 2) | ((h[3] >> 6) & 0x3);

    if sample_rate_idx as usize >= SAMPLE_RATE_TABLE.len() {
        return false;
    }
    let sample_rate = SAMPLE_RATE_TABLE[sample_rate_idx as usize];
    let channels = channel_config; // direct mapping for 1..6, channel_config=7 → 8 channels
    let channels_count: u16 = match channels {
        0 => 0,
        1 => 1,
        2 => 2,
        3 => 3,
        4 => 4,
        5 => 5,
        6 => 6,
        7 => 8,
        _ => 0,
    };

    // Scan frames to count and sum bytes.
    let file_size = fa.remain();
    let mut frame_count: u64 = 0;
    let mut pos = 0usize;
    while pos + 7 <= file_size {
        let Some(frame_header) = fa.peek_raw_at(pos, 7) else {
            break;
        };
        if frame_header[0] != 0xFF || (frame_header[1] & 0xF0) != 0xF0 {
            break;
        }
        let frame_length = (((frame_header[3] & 0x3) as usize) << 11)
            | ((frame_header[4] as usize) << 3)
            | (((frame_header[5] >> 5) & 0x7) as usize);
        if frame_length < 7 || pos + frame_length > file_size {
            break;
        }
        frame_count += 1;
        pos += frame_length;
    }

    fa.stream_prepare(StreamKind::General);
    fa.set_field(StreamKind::General, 0, "Format", "ADTS");
    fa.set_field(StreamKind::General, 0, "AudioCount", "1");
    fa.force_field(StreamKind::General, 0, "StreamSize", "0");
    fa.set_field(StreamKind::General, 0, "OverallBitRate_Mode", "VBR");

    fa.stream_prepare(StreamKind::Audio);
    fa.set_field(StreamKind::Audio, 0, "Format", "AAC");
    fa.set_field(StreamKind::Audio, 0, "Format_Version", if id == 0 { "4" } else { "2" });
    let profile_name = match profile {
        0 => Some("Main"),
        1 => Some("LC"),
        2 => Some("SSR"),
        3 => Some("LTP"),
        _ => None,
    };
    if let Some(p) = profile_name {
        fa.set_field(StreamKind::Audio, 0, "Format_AdditionalFeatures", p);
    }
    // CodecID = AOT (profile + 1)
    fa.set_field(StreamKind::Audio, 0, "CodecID", (profile + 1).to_string());
    fa.set_field(StreamKind::Audio, 0, "BitRate_Mode", "VBR");
    if channels_count > 0 {
        fa.set_field(StreamKind::Audio, 0, "Channels", channels_count.to_string());
        let (positions, layout) = channel_layout(channels_count);
        if let Some(p) = positions {
            fa.set_field(StreamKind::Audio, 0, "ChannelPositions", p);
        }
        if let Some(l) = layout {
            fa.set_field(StreamKind::Audio, 0, "ChannelLayout", l);
        }
    }
    fa.set_field(StreamKind::Audio, 0, "SamplesPerFrame", "1024");
    fa.set_field(StreamKind::Audio, 0, "SamplingRate", sample_rate.to_string());
    if frame_count > 0 {
        fa.set_field(StreamKind::Audio, 0, "FrameCount", frame_count.to_string());
        let frame_rate = (sample_rate as f64) / 1024.0;
        fa.set_field(StreamKind::Audio, 0, "FrameRate", format!("{:.3}", frame_rate));
    }
    fa.set_field(StreamKind::Audio, 0, "Compression_Mode", "Lossy");
    fa.set_field(StreamKind::Audio, 0, "StreamSize", file_size.to_string());
    true
}

fn channel_layout(channels: u16) -> (Option<&'static str>, Option<&'static str>) {
    match channels {
        1 => (Some("Front: C"), Some("C")),
        2 => (Some("Front: L R"), Some("L R")),
        _ => (None, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_adts_frame(frame_length: usize) -> Vec<u8> {
        let mut frame = vec![0u8; frame_length];
        frame[0] = 0xFF;
        frame[1] = 0xF1;
        frame[2] = 0x50;
        frame[3] = 0x80 | (((frame_length >> 11) & 0x3) as u8);
        frame[4] = ((frame_length >> 3) & 0xFF) as u8;
        frame[5] = (((frame_length & 0x7) as u8) << 5) | 0x1F;
        frame[6] = 0xFC;
        frame
    }

    #[test]
    fn rejects_non_adts() {
        let mut fa = FileAnalyze::new(b"NOT ADTS");
        assert!(!parse_aac_adts(&mut fa));
    }

    #[test]
    fn adts_frame_scan_reads_headers_by_offset() {
        let frame = make_adts_frame(7);
        let mut buf = frame.clone();
        buf.extend_from_slice(&frame);
        buf.resize(1024 * 1024, 0);
        let mut fa = FileAnalyze::new(&buf);

        assert!(parse_aac_adts(&mut fa));
        assert_eq!(
            fa.retrieve(StreamKind::Audio, 0, "FrameCount").map(|z| z.as_str().to_owned()),
            Some("2".to_owned())
        );
        assert_eq!(fa.access_stats().max_request_len, 7);
    }
}
