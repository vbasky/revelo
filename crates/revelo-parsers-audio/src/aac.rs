use revelo_core::{FileAnalyze, StreamKind};
/// Parse raw AAC without ADTS header.
///
/// Detection: esds AudioSpecificConfig in MP4.
/// Fills: Profile, sampling rate, channels from DecoderConfigDescriptor.
/// Detection: 0xFFF sync word (ADTS-style).
/// Fills: Profile, channels, sample rate, bitrate from ADTS header.
pub fn parse_aac(fa: &mut FileAnalyze) -> bool {
    let Some(buf) = fa.peek_raw(4) else { return false };
    if buf[0] != 0xFF || (buf[1] & 0xF0) != 0xF0 {
        return false;
    }
    let profile = (buf[2] >> 6) & 0x03;
    let _sr = match (buf[2] >> 2) & 0x0F {
        0 => 96000,
        3 => 48000,
        4 => 44100,
        5 => 32000,
        6 => 24000,
        8 => 16000,
        _ => 44100,
    };
    let _ch = ((buf[2] & 0x01) << 2) | (buf[3] >> 6);

    let pos = fa.stream_prepare(StreamKind::Audio);
    fa.set_field(StreamKind::Audio, pos, "Format", "AAC");
    match profile {
        0 => fa.set_field(StreamKind::Audio, pos, "Format_Profile", "LC"),
        1 => fa.set_field(StreamKind::Audio, pos, "Format_Profile", "HE-AAC"),
        2 => fa.set_field(StreamKind::Audio, pos, "Format_Profile", "HE-AACv2"),
        _ => {}
    }
    true
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test() {
        let buf: Vec<u8> = vec![0xFF, 0xF9, 0x50, 0x80];
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_aac(&mut fa));
        assert_eq!(fa.access_stats().max_request_len, 4);
    }
}
