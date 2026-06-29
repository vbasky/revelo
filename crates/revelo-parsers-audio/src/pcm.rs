use revelo_core::{FileAnalyze, StreamKind};

/// Generic raw PCM audio descriptor. Detects WAVEFORMATEX structure
/// (0xFFFE magic or reasonable PCM format tag) and fills basic PCM fields.
/// Parse raw PCM audio descriptor.
///
/// Detection: WAVEFORMATEX format_tag 0x0001/0xFFFE.
/// Fills: Channels, sample rate, bit depth, endianness.
pub fn parse_pcm(fa: &mut FileAnalyze) -> bool {
    let buf = match fa.peek_raw(16) {
        Some(b) => b,
        None => return false,
    };

    let format_tag = u16::from_le_bytes([buf[0], buf[1]]);
    if format_tag != 0x0001 && format_tag != 0xFFFE {
        return false;
    }

    let channels = u16::from_le_bytes([buf[2], buf[3]]);
    let sample_rate = u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]);
    let bits_per_sample = u16::from_le_bytes([buf[14], buf[15]]);

    let pos = fa.stream_prepare(StreamKind::Audio);
    fa.set_field(StreamKind::Audio, pos, "Format", "PCM");
    fa.set_field(StreamKind::Audio, pos, "Channels", channels.to_string());
    fa.set_field(StreamKind::Audio, pos, "SamplingRate", sample_rate.to_string());
    fa.set_field(StreamKind::Audio, pos, "BitDepth", bits_per_sample.to_string());
    fa.set_field(StreamKind::Audio, pos, "Format_Settings_Endianness", "Little");
    fa.set_field(StreamKind::Audio, pos, "Format_Settings_Sign", "Signed");
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pcm_detects_waveformatex() {
        let mut buf = vec![0u8; 16];
        buf[0] = 0x01;
        buf[1] = 0x00; // PCM format tag
        buf[2] = 0x02;
        buf[3] = 0x00; // 2 channels
        buf[4] = 0x80;
        buf[5] = 0xBB;
        buf[6] = 0x00;
        buf[7] = 0x00; // 48000
        buf[14] = 0x10;
        buf[15] = 0x00; // 16-bit
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_pcm(&mut fa));
    }

    #[test]
    fn pcm_does_not_request_full_payload() {
        let mut buf = vec![0u8; 1024 * 1024];
        buf[0] = 0x01;
        buf[2] = 0x02;
        buf[4] = 0x80;
        buf[5] = 0xBB;
        buf[14] = 0x10;
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_pcm(&mut fa));
        assert_eq!(fa.access_stats().max_request_len, 16);
    }
}
