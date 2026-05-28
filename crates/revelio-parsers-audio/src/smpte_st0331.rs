use revelio_core::{FileAnalyze, StreamKind};

/// SMPTE ST 331/337 (AES3 non-PCM data over SDI). Uses 0x96 0x69 preamble pattern.
pub fn parse_smpte_st0331(fa: &mut FileAnalyze) -> bool {
    let buf = fa.peek_raw(fa.remain()).map(|b| b.to_vec());
    let Some(buf) = buf else { return false };
    if buf.len() < 2 { return false; }
    let preamble = u16::from_be_bytes([buf[0], buf[1]]);
    if preamble != 0x9669 { return false; }

    let pos = fa.stream_prepare(StreamKind::Audio);
    fa.fill(StreamKind::Audio, pos, "Format", "AES3", false);
    fa.fill(StreamKind::Audio, pos, "Format_Info", "SMPTE ST 331", false);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn st331_detects_preamble() {
        let buf: Vec<u8> = vec![0x96, 0x69];
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_smpte_st0331(&mut fa));
    }
}
