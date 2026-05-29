use revelio_core::{FileAnalyze, StreamKind};

/// Parse Teletext subtitle.
///
/// Detection: Clock run-in 0x55 0x55 + framing code 0x27.
/// Fills: Format, page info.
pub fn parse_teletext(fa: &mut FileAnalyze) -> bool {
    let buf = match fa.peek_raw(fa.remain()) {
        Some(b) => b,
        None => return false,
    };

    if buf.len() < 3 {
        return false;
    }

    // Teletext sync: 0x55 0x55 0x27
    if buf[0] != 0x55 || buf[1] != 0x55 || buf[2] != 0x27 {
        return false;
    }

    // Full teletext packet is 45 bytes
    if buf.len() < 45 {
        return true; // partial packet, accept tentative
    }

    let pos = fa.stream_prepare(StreamKind::Text);
    fa.set_field(StreamKind::Text, pos, "Format", "Teletext");
    fa.set_field(StreamKind::Text, pos, "MuxingMode", "Teletext");

    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use revelio_core::FileAnalyze;

    #[test]
    fn teletext_detects_sync_bytes() {
        let mut buf = vec![0u8; 45];
        buf[0] = 0x55;
        buf[1] = 0x55;
        buf[2] = 0x27;
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_teletext(&mut fa));
        assert_eq!(
            fa.retrieve(StreamKind::Text, 0, "Format").map(|z| z.as_str().to_owned()),
            Some("Teletext".into())
        );
    }

    #[test]
    fn teletext_rejects_garbage() {
        let buf = vec![0u8; 45];
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_teletext(&mut fa));
    }
}
