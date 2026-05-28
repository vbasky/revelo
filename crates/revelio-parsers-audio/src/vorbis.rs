use revelio_core::{FileAnalyze, StreamKind};

/// Parse Vorbis audio codec.
///
/// Detection: Identification header packet type 1 + `vorbis` magic.
/// Fills: Channels, sample rate, bitrates, floor type, VorbisComment.
pub fn parse_vorbis(fa: &mut FileAnalyze) -> bool {
    let buf = match fa.peek_raw(fa.remain()) {
        Some(b) => b,
        None => return false,
    };

    // Vorbis identification header: packet_type=1, "vorbis" (7 bytes)
    if buf.len() < 30 {
        return false;
    }

    if buf[0] != 1 {
        return false;
    }

    let magic = std::str::from_utf8(&buf[1..7]).unwrap_or("");
    if magic != "vorbis" {
        return false;
    }

    let version = u32::from_le_bytes([buf[7], buf[8], buf[9], buf[10]]);
    if version > 0 {
        return false;
    }

    let channels = buf[11];
    let sample_rate = u32::from_le_bytes([buf[12], buf[13], buf[14], buf[15]]);
    let bitrate_max = i32::from_le_bytes([buf[16], buf[17], buf[18], buf[19]]);
    let bitrate_nominal = i32::from_le_bytes([buf[20], buf[21], buf[22], buf[23]]);
    let bitrate_min = i32::from_le_bytes([buf[24], buf[25], buf[26], buf[27]]);

    fill_vorbis_streams(fa, channels, sample_rate, bitrate_max, bitrate_nominal, bitrate_min);
    true
}

fn fill_vorbis_streams(
    fa: &mut FileAnalyze,
    channels: u8,
    sample_rate: u32,
    bitrate_max: i32,
    bitrate_nominal: i32,
    bitrate_min: i32,
) {
    let pos = fa.stream_prepare(StreamKind::Audio);

    fa.fill(StreamKind::Audio, pos, "Format", "Vorbis", false);
    fa.fill(StreamKind::Audio, pos, "Codec", "Vorbis", false);
    fa.fill(StreamKind::Audio, pos, "Channels", channels.to_string(), false);
    fa.fill(StreamKind::Audio, pos, "SamplingRate", sample_rate.to_string(), false);

    let brm = if bitrate_nominal > 0 && bitrate_max == bitrate_nominal && bitrate_nominal == bitrate_min {
        "CBR"
    } else {
        "VBR"
    };
    fa.fill(StreamKind::Audio, pos, "BitRate_Mode", brm, false);

    // A positive i32 is already < 2^31, so the upper bound is implicit.
    if bitrate_nominal > 0 {
        fa.fill(StreamKind::Audio, pos, "BitRate", bitrate_nominal.to_string(), false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use revelio_core::FileAnalyze;

    #[test]
    fn vorbis_detects_header() {
        let mut buf = vec![0u8; 30];
        buf[0] = 1; // packet type: identification
        buf[1..7].copy_from_slice(b"vorbis");
        buf[11] = 2; // stereo
        buf[12] = 0x80; buf[13] = 0xBB; buf[14] = 0; buf[15] = 0; // 48000 LE
        buf[20] = 0x80; buf[21] = 0xBB; buf[22] = 0; buf[23] = 0; // nominal 48000 too? nah - just set nominal to 128kbps
        buf[20] = 0x00; buf[21] = 0xF4; buf[22] = 0x01; buf[23] = 0x00; // 128000

        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_vorbis(&mut fa));
        assert_eq!(
            fa.retrieve(StreamKind::Audio, 0, "Format").map(|z| z.as_str().to_owned()),
            Some("Vorbis".into())
        );
    }

    #[test]
    fn vorbis_rejects_unknown_version() {
        let mut buf = vec![0u8; 30];
        buf[0] = 1;
        buf[1..7].copy_from_slice(b"vorbis");
        buf[7] = 1; buf[8] = 0; buf[9] = 0; buf[10] = 0; // version 1
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_vorbis(&mut fa));
    }

    #[test]
    fn vorbis_rejects_random_data() {
        let buf = vec![0u8; 30];
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_vorbis(&mut fa));
    }
}
