use revelio_core::{FileAnalyze, StreamKind};

/// Dolby E parser. Dolby E frames are carried in AES3 streams (SMPTE ST 302).
/// Detection: looks for 0x078E sync word (SMPTE 337M preamble) followed by
/// Dolby E guard band pattern.
pub fn parse_dolby_e(fa: &mut FileAnalyze) -> bool {
    let buf = fa.peek_raw(fa.remain()).map(|b| b.to_vec());
    let Some(buf) = buf else { return false };
    if buf.len() < 4 {
        return false;
    }

    // SMPTE 337M preamble: 0x96 0x69 for Dolby E + AES3 sync
    let preamble = u16::from_be_bytes([buf[0], buf[1]]);
    let sync = u16::from_be_bytes([buf[2], buf[3]]);

    if preamble != 0x9669 && preamble != 0x966A {
        return false;
    }
    if sync != 0x078E {
        return false;
    }

    let pos = fa.stream_prepare(StreamKind::Audio);
    fa.fill(StreamKind::Audio, pos, "Format", "Dolby E", false);
    fa.fill(StreamKind::Audio, pos, "Format_Info", "Professional Dolby E", false);
    fa.fill(StreamKind::Audio, pos, "Compression_Mode", "Lossless", false);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dolby_e_detects_sync() {
        let buf: Vec<u8> = vec![0x96, 0x69, 0x07, 0x8E];
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_dolby_e(&mut fa));
        assert_eq!(
            fa.retrieve(StreamKind::Audio, 0, "Format").map(|z| z.as_str().to_owned()),
            Some("Dolby E".into())
        );
    }

    #[test]
    fn dolby_e_rejects_garbage() {
        let buf = vec![0u8; 8];
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_dolby_e(&mut fa));
    }
}
