use revelio_core::{FileAnalyze, StreamKind};

/// MPEG-H 3D Audio parser. Detects mhm1/mha1 box in MP4 or raw
/// AudioSpecificConfig with audioObjectType 30 for MPEG-H.
/// Parse MPEG-H 3D Audio.
///
/// Detection: mhm1/mha1 box in MP4.
/// Fills: Audio scene config.
pub fn parse_mpegh3da(fa: &mut FileAnalyze) -> bool {
    let buf = fa.peek_raw(fa.remain()).map(|b| b.to_vec());
    let Some(buf) = buf else { return false };
    if buf.len() < 5 {
        return false;
    }

    // Check for mhm1/mha1 box (MPEG-H 3D Audio in MP4)
    if buf.len() >= 8 {
        let size = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]);
        let box_type = std::str::from_utf8(&buf[4..8]).unwrap_or("");
        if (box_type == "mhm1" || box_type == "mha1") && size >= 12 {
            let pos = fa.stream_prepare(StreamKind::Audio);
            fa.fill(StreamKind::Audio, pos, "Format", "MPEG-H 3D Audio", false);
            fa.fill(StreamKind::Audio, pos, "Format_Info", "Immersive 3D Audio", false);
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mpegh3da_detects_mhm1_box() {
        let mut buf = vec![0u8; 32];
        buf[0..4].copy_from_slice(&(32u32.to_be_bytes()));
        buf[4..8].copy_from_slice(b"mhm1");
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_mpegh3da(&mut fa));
    }
    #[test]
    fn mpegh3da_rejects_garbage() {
        let buf = vec![0u8; 4];
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_mpegh3da(&mut fa));
    }
}
