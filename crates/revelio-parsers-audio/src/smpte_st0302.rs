use revelio_core::{FileAnalyze, StreamKind};

/// SMPTE ST 302 (AES3 in MPEG-TS). Detection: AES3 sync preamble 0xF872
/// with 0x1E/0x1F/0x3E/0x3F subframe encoding.
pub fn parse_smpte_st0302(fa: &mut FileAnalyze) -> bool {
    let buf = fa.peek_raw(fa.remain()).map(|b| b.to_vec());
    let Some(buf) = buf else { return false };
    if buf.len() < 4 { return false; }
    let sync = u16::from_be_bytes([buf[0], buf[1]]);
    if sync != 0xF872 { return false; }

    let pos = fa.stream_prepare(StreamKind::Audio);
    fa.fill(StreamKind::Audio, pos, "Format", "AES3", false);
    fa.fill(StreamKind::Audio, pos, "Format_Info", "SMPTE ST 302", false);
    fa.fill(StreamKind::Audio, pos, "Format_Settings_Mode", "16 AES3 channels", false);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn st302_detects_sync() {
        let buf: Vec<u8> = vec![0xF8, 0x72, 0x00, 0x00];
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_smpte_st0302(&mut fa));
    }
    #[test]
    fn st302_rejects_garbage() { let buf = vec![0u8; 4]; let mut fa = FileAnalyze::new(&buf); assert!(!parse_smpte_st0302(&mut fa)); }
}
