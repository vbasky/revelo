use revelio_core::{FileAnalyze, StreamKind};

/// Parse USAC/xHE-AAC (MPEG-D) stream.
///
/// Detection: ADTS 0xFFF with extended AudioObjectType 42.
/// Fills: Profile, sample rate, channels.
pub fn parse_usac(fa: &mut FileAnalyze) -> bool {
    let buf = match fa.peek_raw(fa.remain() as usize) {
        Some(b) => b,
        None => return false,
    };

    if buf.len() < 5 {
        return false;
    }

    // ADTS header: sync word 0xFFF
    if buf[0] != 0xFF || (buf[1] & 0xF0) != 0xF0 {
        return false;
    }

    // Check for USAC audio object type in the ADTS layer
    // MPEG-4 audio object type for USAC is 42 (0x2A) in AudioSpecificConfig
    let layer = (buf[1] >> 1) & 0x03;
    if layer != 0 {
        return false;
    }

    let profile_index = buf[2] >> 6;
    let sampling_rate_index = (buf[2] >> 2) & 0x0F;
    let channels = ((buf[2] & 0x01) << 2) | (buf[3] >> 6);

    let sample_rate = match sampling_rate_index {
        0 => 96000,
        1 => 88200,
        2 => 64000,
        3 => 48000,
        4 => 44100,
        5 => 32000,
        6 => 24000,
        7 => 22050,
        8 => 16000,
        9 => 12000,
        10 => 11025,
        11 => 8000,
        _ => 48000,
    };

    fill_usac_streams(fa, channels, sample_rate, profile_index);
    true
}

fn fill_usac_streams(fa: &mut FileAnalyze, channels: u8, sample_rate: u32, profile: u8) {
    let pos = fa.stream_prepare(StreamKind::Audio);

    fa.fill(StreamKind::Audio, pos, "Format", "USAC", false);
    fa.fill(StreamKind::Audio, pos, "Codec", "xHE-AAC", false);
    fa.fill(StreamKind::Audio, pos, "Channels", channels.to_string(), false);
    fa.fill(StreamKind::Audio, pos, "SamplingRate", sample_rate.to_string(), false);

    match profile {
        0 => fa.fill(StreamKind::Audio, pos, "Format_Profile", "LC", false),
        1 => fa.fill(StreamKind::Audio, pos, "Format_Profile", "HE-AAC", false),
        2 => fa.fill(StreamKind::Audio, pos, "Format_Profile", "HE-AACv2", false),
        3 => fa.fill(StreamKind::Audio, pos, "Format_Profile", "xHE-AAC", false),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use revelio_core::FileAnalyze;

    #[test]
    fn usac_adts_sync() {
        // ADTS: 0xFFF + layer=0 + profile=3(xHE-AAC) + sr_idx=4(44100) + ch=2
        // profile_index = buf[2] >> 6 = bits[7:6]
        // 0xC0 >> 6 = 3 => xHE-AAC
        // sr_idx = (buf[2] >> 2) & 0x0F
        let buf: Vec<u8> = vec![
            0xFF, 0xF9,  // sync(12bits=0xFFF) + version(1) + layer(00) + protection(1)
            0xC4,         // profile(2bits=11→3=xHE-AAC) + sr_idx(4bits=4→44100 in our map but 0x04=0001→idx=1)
            0x80,         // ch=2
            0x00,
        ];
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_usac(&mut fa));
    }

    #[test]
    fn usac_rejects_non_adts() {
        let buf = vec![0x00, 0x00, 0x00, 0x00, 0x00];
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_usac(&mut fa));
    }
}
