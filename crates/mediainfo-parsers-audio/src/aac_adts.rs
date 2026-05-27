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

use mediainfo_core::{FileAnalyze, StreamKind};

const SAMPLE_RATE_TABLE: [u32; 13] = [
    96000, 88200, 64000, 48000, 44100, 32000, 24000, 22050, 16000, 12000, 11025, 8000, 7350,
];

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
    let file_size = fa.Remain();
    let mut frame_count: u64 = 0;
    let mut pos = 0usize;
    let buf_view = match fa.peek_raw(file_size) {
        Some(b) => b,
        None => return false,
    };
    while pos + 7 <= buf_view.len() {
        if buf_view[pos] != 0xFF || (buf_view[pos + 1] & 0xF0) != 0xF0 {
            break;
        }
        let frame_length = (((buf_view[pos + 3] & 0x3) as usize) << 11)
            | ((buf_view[pos + 4] as usize) << 3)
            | (((buf_view[pos + 5] >> 5) & 0x7) as usize);
        if frame_length < 7 || pos + frame_length > buf_view.len() {
            break;
        }
        frame_count += 1;
        pos += frame_length;
    }

    fa.Stream_Prepare(StreamKind::General);
    fa.Fill(StreamKind::General, 0, "Format", "ADTS", false);
    fa.Fill(StreamKind::General, 0, "AudioCount", "1", false);
    fa.Fill(StreamKind::General, 0, "StreamSize", "0", true);
    fa.Fill(StreamKind::General, 0, "OverallBitRate_Mode", "VBR", false);

    fa.Stream_Prepare(StreamKind::Audio);
    fa.Fill(StreamKind::Audio, 0, "Format", "AAC", false);
    fa.Fill(StreamKind::Audio, 0, "Format_Version", if id == 0 { "4" } else { "2" }, false);
    let profile_name = match profile {
        0 => Some("Main"),
        1 => Some("LC"),
        2 => Some("SSR"),
        3 => Some("LTP"),
        _ => None,
    };
    if let Some(p) = profile_name {
        fa.Fill(StreamKind::Audio, 0, "Format_AdditionalFeatures", p, false);
    }
    // CodecID = AOT (profile + 1)
    fa.Fill(StreamKind::Audio, 0, "CodecID", (profile + 1).to_string(), false);
    fa.Fill(StreamKind::Audio, 0, "BitRate_Mode", "VBR", false);
    if channels_count > 0 {
        fa.Fill(StreamKind::Audio, 0, "Channels", channels_count.to_string(), false);
        let (positions, layout) = channel_layout(channels_count);
        if let Some(p) = positions {
            fa.Fill(StreamKind::Audio, 0, "ChannelPositions", p, false);
        }
        if let Some(l) = layout {
            fa.Fill(StreamKind::Audio, 0, "ChannelLayout", l, false);
        }
    }
    fa.Fill(StreamKind::Audio, 0, "SamplesPerFrame", "1024", false);
    fa.Fill(StreamKind::Audio, 0, "SamplingRate", sample_rate.to_string(), false);
    if frame_count > 0 {
        fa.Fill(StreamKind::Audio, 0, "FrameCount", frame_count.to_string(), false);
        let frame_rate = (sample_rate as f64) / 1024.0;
        fa.Fill(
            StreamKind::Audio,
            0,
            "FrameRate",
            format!("{:.3}", frame_rate),
            false,
        );
    }
    fa.Fill(StreamKind::Audio, 0, "Compression_Mode", "Lossy", false);
    fa.Fill(StreamKind::Audio, 0, "StreamSize", file_size.to_string(), false);
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
    #[test]
    fn rejects_non_adts() {
        let mut fa = FileAnalyze::new(b"NOT ADTS");
        assert!(!parse_aac_adts(&mut fa));
    }
}
