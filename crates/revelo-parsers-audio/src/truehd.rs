use revelo_core::{FileAnalyze, StreamKind};

/// TrueHD (MLP) parser with Dolby Atmos (MAT) detection.
///
/// Detection: Sync 0xF8726FBA (TrueHD) / 0xF8726FBB (AC-3+TrueHD).
///
/// MAT (Metadata-Enhanced Audio Transport) for Atmos:
///   After the TrueHD sync, MAT frames carry a 6-byte MAT header:
///     MAT sync (2 bytes): 0x0003
///     MAT timestamp (4 bytes): big-endian PTS
///
///   MAT substream indicator: bytes following the TrueHD frame header
///   can indicate Atmos presence via specific substream header bits.
///
/// Fills: Channels, sample rate, bit depth, Lossless, VBR, Atmos.
pub fn parse_truehd(fa: &mut FileAnalyze) -> bool {
    let buf = match fa.peek_raw(fa.remain()).map(|b| b.to_vec()) {
        Some(b) => b,
        None => return false,
    };
    if buf.len() < 4 {
        return false;
    }

    let sync = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]);
    let has_ac3_core = sync == 0xF8726FBB;
    if sync != 0xF8726FBA && !has_ac3_core {
        return false;
    }

    let pos = fa.stream_prepare(StreamKind::Audio);
    fa.set_field(StreamKind::Audio, pos, "Format", "TrueHD");
    fa.set_field(StreamKind::Audio, pos, "Format_Profile", "MLP FBA");

    if has_ac3_core {
        fa.set_field(StreamKind::Audio, pos, "Format_Commercial", "Dolby TrueHD + AC-3");
    }

    // Parse sampling rate from bits[12-15] of the sync word + following nibble
    let mut has_atmos = false;
    let mut total_channels: u8 = 2;

    if buf.len() >= 5 {
        let sr_idx = (buf[4] >> 4) & 0x0F;
        let sr = match sr_idx {
            0 => 48000,
            1 => 96000,
            2 => 192000,
            3 => 44100,
            4 => 88200,
            5 => 176400,
            _ => 48000,
        };
        fa.set_field(StreamKind::Audio, pos, "SamplingRate", sr.to_string());

        // Channel assignment from bits[3-0] of byte 4
        let ch_code = buf[4] & 0x0F;
        total_channels = if ch_code <= 7 { ch_code + 1 } else { 2 };
        fa.set_field(StreamKind::Audio, pos, "Channels", total_channels.to_string());

        // Bit depth from byte 5
        if buf.len() >= 6 {
            let bit_depth = match (buf[5] >> 4) & 0x0F {
                1 => 16,
                2 => 20,
                3 => 24,
                _ => 24,
            };
            fa.set_field(StreamKind::Audio, pos, "BitDepth", bit_depth.to_string());
        }

        // ── MAT (Atmos) detection ───────────────────────────────
        // MAT frames follow the TrueHD frame header with specific
        // substream patterns. The TrueHD substream header at byte 5+
        // encodes number of substreams and their types.
        if buf.len() >= 8 {
            // Bits from byte 5-7: substream info
            // Byte 6[7..4] = num_substreams, byte 6[3..0] = substream type
            // Type 2 = TrueHD primary, type 3 = Atmos objects
            let ss_info = (buf[6] >> 4) & 0x0F;
            if ss_info >= 2 {
                has_atmos = true;
            }
            // Check for MAT sync (0x0003) following TrueHD frame
            for i in (8..buf.len().saturating_sub(1)).step_by(8) {
                if buf[i] == 0x00 && buf[i + 1] == 0x03 {
                    // MAT header detected
                    has_atmos = true;
                    break;
                }
            }
        }
    }

    if has_atmos {
        fa.set_field(StreamKind::Audio, pos, "Format_AdditionalFeatures", "Atmos");
        fa.set_field(StreamKind::Audio, pos, "Format_Commercial", "Dolby TrueHD with Atmos");
        // Atmos can carry up to 12 bed channels + 10 objects -> typically
        // set the maximum bed+object count if Atmos is detected.
        if total_channels >= 6 {
            fa.set_field(StreamKind::Audio, pos, "HDR_Format", "Dolby Atmos");
        }
    }

    fa.set_field(StreamKind::Audio, pos, "BitRate_Mode", "VBR");
    fa.set_field(StreamKind::Audio, pos, "Compression_Mode", "Lossless");
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_truehd(sync: u32, byte4: u8, byte5: u8, extra: &[u8]) -> Vec<u8> {
        let mut buf = sync.to_be_bytes().to_vec();
        buf.push(byte4);
        buf.push(byte5);
        buf.extend_from_slice(extra);
        buf
    }

    #[test]
    fn truehd_detects_sync() {
        let buf = make_truehd(0xF8726FBA, 0x12, 0x34, &[]);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_truehd(&mut fa));
        assert_eq!(
            fa.retrieve(StreamKind::Audio, 0, "Format").map(|z| z.as_str().to_owned()),
            Some("TrueHD".into())
        );
        assert_eq!(
            fa.retrieve(StreamKind::Audio, 0, "Compression_Mode").map(|z| z.as_str().to_owned()),
            Some("Lossless".into())
        );
    }

    #[test]
    fn truehd_rejects_garbage() {
        let buf = vec![0u8; 8];
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_truehd(&mut fa));
    }

    #[test]
    fn truehd_rejects_too_short() {
        let mut fa = FileAnalyze::new(&[0xF8, 0x72, 0x6F]);
        assert!(!parse_truehd(&mut fa));
    }

    #[test]
    fn ac3_plus_truehd_sync() {
        let buf = make_truehd(0xF8726FBB, 0x12, 0x34, &[]);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_truehd(&mut fa));
        let a = |k: &str| fa.retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(a("Format_Commercial").as_deref(), Some("Dolby TrueHD + AC-3"));
    }

    #[test]
    fn parses_sampling_rate_48000() {
        // sr_idx = 0 → byte4 top nibble = 0x0X
        let buf = make_truehd(0xF8726FBA, 0x03, 0x34, &[]);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_truehd(&mut fa));
        let a = |k: &str| fa.retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(a("SamplingRate").as_deref(), Some("48000"));
    }

    #[test]
    fn parses_sampling_rate_96000() {
        // sr_idx = 1
        let buf = make_truehd(0xF8726FBA, 0x13, 0x34, &[]);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_truehd(&mut fa));
        let a = |k: &str| fa.retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(a("SamplingRate").as_deref(), Some("96000"));
    }

    #[test]
    fn parses_sampling_rate_192000() {
        let buf = make_truehd(0xF8726FBA, 0x23, 0x34, &[]);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_truehd(&mut fa));
        let a = |k: &str| fa.retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(a("SamplingRate").as_deref(), Some("192000"));
    }

    #[test]
    fn parses_sampling_rate_44100() {
        let buf = make_truehd(0xF8726FBA, 0x33, 0x34, &[]);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_truehd(&mut fa));
        let a = |k: &str| fa.retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(a("SamplingRate").as_deref(), Some("44100"));
    }

    #[test]
    fn parses_sampling_rate_88200() {
        let buf = make_truehd(0xF8726FBA, 0x43, 0x34, &[]);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_truehd(&mut fa));
        let a = |k: &str| fa.retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(a("SamplingRate").as_deref(), Some("88200"));
    }

    #[test]
    fn parses_sampling_rate_176400() {
        let buf = make_truehd(0xF8726FBA, 0x53, 0x34, &[]);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_truehd(&mut fa));
        let a = |k: &str| fa.retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(a("SamplingRate").as_deref(), Some("176400"));
    }

    #[test]
    fn parses_channel_count() {
        for (ch_code, expected) in &[(0u8, 1u8), (1, 2), (2, 3), (3, 4), (5, 6), (7, 8)] {
            // ch_code = byte4 low nibble
            let byte4 = 0x10 | ch_code;
            let buf = make_truehd(0xF8726FBA, byte4, 0x34, &[]);
            let mut fa = FileAnalyze::new(&buf);
            assert!(parse_truehd(&mut fa), "ch_code={} should parse", ch_code);
            let a = |k: &str| fa.retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());
            assert_eq!(a("Channels").as_deref(), Some(expected.to_string()).as_deref());
        }
    }

    #[test]
    fn parses_bit_depth_16() {
        // bit_depth code = 1 → top nibble of byte5 = 0x1X
        let buf = make_truehd(0xF8726FBA, 0x13, 0x1E, &[]);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_truehd(&mut fa));
        let a = |k: &str| fa.retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(a("BitDepth").as_deref(), Some("16"));
    }

    #[test]
    fn parses_bit_depth_20() {
        let buf = make_truehd(0xF8726FBA, 0x13, 0x2E, &[]);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_truehd(&mut fa));
        let a = |k: &str| fa.retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(a("BitDepth").as_deref(), Some("20"));
    }

    #[test]
    fn parses_bit_depth_24() {
        let buf = make_truehd(0xF8726FBA, 0x13, 0x3E, &[]);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_truehd(&mut fa));
        let a = |k: &str| fa.retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(a("BitDepth").as_deref(), Some("24"));
    }

    #[test]
    fn detects_atmos_via_ss_info() {
        // ss_info (byte6 top nibble) >= 2 triggers Atmos
        // ch_code = 7 → 8 channels → >= 6 → set HDR_Format
        let extra = [0x20u8, 0x00, 0x00]; // byte6=0x20 → ss_info=2
        let buf = make_truehd(0xF8726FBA, 0x17, 0x3E, &extra); // ch_code=7 → 8ch
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_truehd(&mut fa));
        let a = |k: &str| fa.retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(a("Format_AdditionalFeatures").as_deref(), Some("Atmos"));
        assert_eq!(a("HDR_Format").as_deref(), Some("Dolby Atmos"));
    }

    #[test]
    fn detects_atmos_via_mat_sync() {
        // MAT sync 0x0003 right after frame header (at buf[8..9])
        // Needs buf.len() >= 8 and the MAT search starts at i=8
        let extra = vec![0x00, 0x00, 0x00, 0x03, 0x12, 0x34, 0x56, 0x78];
        let buf = make_truehd(0xF8726FBA, 0x13, 0x3E, &extra);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_truehd(&mut fa));
        let a = |k: &str| fa.retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(a("Format_AdditionalFeatures").as_deref(), Some("Atmos"));
    }

    #[test]
    fn atmos_no_hdr_format_when_channels_lt_6() {
        // Atmos detected but total_channels (from ch_code) < 6 → HDR_Format should NOT be set
        let extra = [0x20u8, 0x00, 0x00]; // ss_info=2
        let buf = make_truehd(0xF8726FBA, 0x12, 0x3E, &extra); // ch_code=2 → 3 channels
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_truehd(&mut fa));
        let a = |k: &str| fa.retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(a("Format_AdditionalFeatures").as_deref(), Some("Atmos"));
        assert_eq!(a("HDR_Format"), None);
    }

    #[test]
    fn no_atmos_when_ss_info_low() {
        let extra = [0x10u8, 0x00, 0x00]; // ss_info=1 → no Atmos
        let buf = make_truehd(0xF8726FBA, 0x13, 0x3E, &extra);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_truehd(&mut fa));
        let a = |k: &str| fa.retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(a("Format_AdditionalFeatures"), None);
    }

    #[test]
    fn compression_mode_lossless() {
        let buf = make_truehd(0xF8726FBA, 0x13, 0x3E, &[]);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_truehd(&mut fa));
        let a = |k: &str| fa.retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(a("Compression_Mode").as_deref(), Some("Lossless"));
    }

    #[test]
    fn bitrate_mode_vbr() {
        let buf = make_truehd(0xF8726FBA, 0x13, 0x3E, &[]);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_truehd(&mut fa));
        let a = |k: &str| fa.retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(a("BitRate_Mode").as_deref(), Some("VBR"));
    }
}
