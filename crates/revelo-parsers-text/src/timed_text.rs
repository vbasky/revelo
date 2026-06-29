use revelo_core::{FileAnalyze, StreamKind};

/// Parse 3GPP/MOV Timed Text track.
///
/// Detection: 16-bit BE length + UTF-8/UTF-16 text.
/// Fills: Format.
pub fn parse_timed_text(fa: &mut FileAnalyze) -> bool {
    let header = match fa.peek_raw(2) {
        Some(b) => b,
        None => return false,
    };

    // 3GPP Timed Text: 16-bit big-endian length prefix followed by UTF-8/UTF-16 text
    let size = u16::from_be_bytes([header[0], header[1]]) as usize;
    if size == 0 {
        return false;
    }

    let buf = match fa.peek_raw(2 + size) {
        Some(b) => b,
        None => return false,
    };
    let text_data = &buf[2..2 + size];

    // Check it's valid UTF-8 text
    if std::str::from_utf8(text_data).is_err() {
        return false;
    }

    let pos = fa.stream_prepare(StreamKind::Text);
    fa.set_field(StreamKind::Text, pos, "Format", "Timed Text");

    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use revelo_core::FileAnalyze;

    #[test]
    fn timed_text_binary_length_prefix() {
        let text = b"Hello World";
        let mut buf = vec![0u8; 2 + text.len()];
        buf[0] = 0;
        buf[1] = text.len() as u8;
        buf[2..].copy_from_slice(text);

        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_timed_text(&mut fa));
    }

    #[test]
    fn timed_text_rejects_binary() {
        let buf = vec![0xFF, 0xFE, 0x00, 0x00];
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_timed_text(&mut fa));
    }

    #[test]
    fn timed_text_does_not_request_trailing_payload() {
        let text = b"Hello";
        let mut buf = vec![0u8; 2 + text.len() + 1024 * 1024];
        buf[0] = 0;
        buf[1] = text.len() as u8;
        buf[2..2 + text.len()].copy_from_slice(text);

        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_timed_text(&mut fa));
        assert_eq!(fa.access_stats().max_request_len, 2 + text.len());
    }
}
