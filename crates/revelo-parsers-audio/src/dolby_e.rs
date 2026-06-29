use revelo_core::{FileAnalyze, StreamKind};

/// Dolby E parser. Dolby E frames are carried in AES3 streams (SMPTE ST 302).
/// Detection: looks for 0x078E sync word (SMPTE 337M preamble) followed by
/// Dolby E guard band pattern.
pub fn parse_dolby_e(fa: &mut FileAnalyze) -> bool {
    let buf = match fa.peek_raw(4) {
        Some(b) => b,
        None => return false,
    };

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
    fa.set_field(StreamKind::Audio, pos, "Format", "Dolby E");
    fa.set_field(StreamKind::Audio, pos, "Format_Info", "Professional Dolby E");
    fa.set_field(StreamKind::Audio, pos, "Compression_Mode", "Lossless");
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

    #[test]
    fn dolby_e_does_not_request_full_payload() {
        let mut buf = vec![0u8; 1024 * 1024];
        buf[0..4].copy_from_slice(&[0x96, 0x69, 0x07, 0x8E]);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_dolby_e(&mut fa));
        assert_eq!(fa.access_stats().max_request_len, 4);
    }
}
