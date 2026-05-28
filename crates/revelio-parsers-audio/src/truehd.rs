use revelio_core::{FileAnalyze, StreamKind};

/// TrueHD (MLP) parser. Detects the sync code 0xF8726FBA for
/// TrueHD or 0xF8726FBB for AC-3 core + TrueHD substream.
pub fn parse_truehd(fa: &mut FileAnalyze) -> bool {
    let buf = fa.peek_raw(fa.Remain() as usize).map(|b| b.to_vec());
    let Some(buf) = buf else { return false };
    if buf.len() < 4 { return false; }

    let sync = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]);
    if sync != 0xF8726FBA && sync != 0xF8726FBB {
        return false;
    }

    let pos = fa.Stream_Prepare(StreamKind::Audio);
    fa.Fill(StreamKind::Audio, pos, "Format", "TrueHD", false);
    fa.Fill(StreamKind::Audio, pos, "Format_Profile", "MLP FBA", false);

    // Parse sampling rate from bits[12-15] of the sync word + following nibble
    if buf.len() >= 5 {
        let sr_idx = ((buf[4] >> 4) & 0x0F) as u8;
        let sr = match sr_idx {
            0 => 48000,
            1 => 96000,
            2 => 192000,
            3 => 44100,
            4 => 88200,
            5 => 176400,
            _ => 48000,
        };
        fa.Fill(StreamKind::Audio, pos, "SamplingRate", sr.to_string(), false);

        // Channel assignment from bits[3-0] of byte 4
        let ch_code = (buf[4] & 0x0F) as u8;
        let channels = if ch_code <= 7 { ch_code + 1 } else { 2 };
        fa.Fill(StreamKind::Audio, pos, "Channels", channels.to_string(), false);

        // Bit depth from byte 5
        if buf.len() >= 6 {
            let bd = match (buf[5] >> 4) & 0x0F {
                1 => 16,
                2 => 20,
                3 => 24,
                _ => 24,
            };
            fa.Fill(StreamKind::Audio, pos, "BitDepth", bd.to_string(), false);
        }
    }

    fa.Fill(StreamKind::Audio, pos, "BitRate_Mode", "VBR", false);
    fa.Fill(StreamKind::Audio, pos, "Compression_Mode", "Lossless", false);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truehd_detects_sync() {
        let buf: Vec<u8> = vec![0xF8, 0x72, 0x6F, 0xBA, 0x12, 0x34];
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_truehd(&mut fa));
        assert_eq!(fa.Retrieve(StreamKind::Audio, 0, "Format").map(|z| z.as_str().to_owned()), Some("TrueHD".into()));
        assert_eq!(fa.Retrieve(StreamKind::Audio, 0, "Compression_Mode").map(|z| z.as_str().to_owned()), Some("Lossless".into()));
    }

    #[test]
    fn truehd_rejects_garbage() {
        let buf = vec![0u8; 8];
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_truehd(&mut fa));
    }
}
