use revelo_core::{FileAnalyze, StreamKind};

/// SMPTE ST 337 (non-PCM AES3 data transport, including Dolby E awareness).
pub fn parse_smpte_st0337(fa: &mut FileAnalyze) -> bool {
    let buf = fa.peek_raw(fa.remain()).map(|b| b.to_vec());
    let Some(buf) = buf else { return false };
    if buf.len() < 4 {
        return false;
    }
    // ST 337 uses 0xF872 AES3 sync with data_type bits indicating non-PCM
    if buf[0] != 0xF8 || buf[1] != 0x72 {
        return false;
    }
    let data_type = buf[3] & 0x1F;
    if data_type < 1 {
        return false;
    }

    let pos = fa.stream_prepare(StreamKind::Audio);
    fa.set_field(StreamKind::Audio, pos, "Format", "AES3");
    fa.set_field(StreamKind::Audio, pos, "Format_Info", "SMPTE ST 337");
    fa.set_field(StreamKind::Audio, pos, "Format_Settings", "Non-PCM");
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn st337_detects_aes3_non_pcm() {
        let buf: Vec<u8> = vec![0xF8, 0x72, 0x00, 0x01];
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_smpte_st0337(&mut fa));
    }
}
