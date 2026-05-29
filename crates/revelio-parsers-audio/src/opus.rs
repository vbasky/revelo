use revelio_core::{FileAnalyze, StreamKind};

const OPUS_CHANNEL_POSITIONS: [&str; 8] = [
    "Front: C",
    "Front: L R",
    "Front: L C R",
    "Front: L R,   Rear: L R",
    "Front: L C R, Back: L R",
    "Front: L C R, Back: L R, LFE",
    "Front: L C R, Side: L R, Back: C, LFE",
    "Front: L C R, Side: L R, Back: L R, LFE",
];

const OPUS_CHANNEL_LAYOUT: [&str; 8] = [
    "M",
    "L R",
    "L R C",
    "L R BL BR",
    "L R BL BR LFE",
    "L R C BL BR LFE",
    "L R C SL SR BC LFE",
    "L R C SL SR BL BR LFE",
];

/// Parse Opus audio codec (RFC 6716).
///
/// Detection: OpusHead packet in Ogg/WebM, TOC byte.
/// Fills: Channels, channel mapping family, preskip, sample rate.
pub fn parse_opus(fa: &mut FileAnalyze) -> bool {
    let buf = match fa.peek_raw(fa.remain()) {
        Some(b) => b,
        None => return false,
    };

    // Opus identification header: "OpusHead" (8 bytes)
    if buf.len() < 19 {
        return false;
    }

    let magic = match std::str::from_utf8(&buf[0..8]) {
        Ok("OpusHead") => true,
        _ => false,
    };

    if !magic {
        return false;
    }

    let version_id = buf[8];
    let channel_count = buf[9];
    let preskip = u16::from_le_bytes([buf[10], buf[11]]);
    let sample_rate = u32::from_le_bytes([buf[12], buf[13], buf[14], buf[15]]);
    let channel_map = buf[18];

    if channel_map > 1 {
        return false; // Vorbis or unsupported mapping
    }

    fill_opus_streams(fa, version_id, channel_count, preskip, sample_rate, channel_map);
    true
}

fn fill_opus_streams(
    fa: &mut FileAnalyze,
    _version_id: u8,
    channel_count: u8,
    _preskip: u16,
    sample_rate: u32,
    channel_map: u8,
) {
    let pos = fa.stream_prepare(StreamKind::Audio);

    fa.set_field(StreamKind::Audio, pos, "Format", "Opus");
    fa.set_field(StreamKind::Audio, pos, "Codec", "Opus");
    fa.set_field(StreamKind::Audio, pos, "Channels", channel_count.to_string());

    let sr = if sample_rate > 0 { sample_rate } else { 48000 };
    fa.set_field(StreamKind::Audio, pos, "SamplingRate", sr.to_string());

    if channel_map == 0 || channel_map == 1 {
        let ch = channel_count as usize;
        if ch > 0 && ch <= 8 {
            fa.set_field(
                StreamKind::Audio,
                pos,
                "ChannelPositions",
                OPUS_CHANNEL_POSITIONS[ch - 1],
            );
            fa.set_field(StreamKind::Audio, pos, "ChannelLayout", OPUS_CHANNEL_LAYOUT[ch - 1]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use revelio_core::FileAnalyze;

    #[test]
    fn opus_detects_header() {
        let mut buf = vec![0u8; 19];
        buf[0..8].copy_from_slice(b"OpusHead");
        buf[8] = 1; // version
        buf[9] = 2; // 2 channels (stereo)
        buf[10] = 120;
        buf[11] = 0; // preskip LE
        buf[12] = 0x80;
        buf[13] = 0xBB;
        buf[14] = 0;
        buf[15] = 0; // 48000 LE

        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_opus(&mut fa));
        assert_eq!(
            fa.retrieve(StreamKind::Audio, 0, "Format").map(|z| z.as_str().to_owned()),
            Some("Opus".into())
        );
        assert_eq!(
            fa.retrieve(StreamKind::Audio, 0, "Channels").map(|z| z.as_str().to_owned()),
            Some("2".into())
        );
    }

    #[test]
    fn opus_rejects_random_data() {
        let buf = vec![0u8; 19];
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_opus(&mut fa));
    }
}
