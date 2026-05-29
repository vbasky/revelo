//! Pro Tools session file (.ptx) parser.
//!
//! 17-byte magic at offset 0:
//!   03 30 30 31 30 31 31 31 31 30 30 31 30 31 30 31 31
//!
//! Identification only; the session structure (~590 lines C++)
//! is deferred.

use revelio_core::{FileAnalyze, StreamKind};

const PTX_MAGIC: [u8; 17] = [
    0x03, 0x30, 0x30, 0x31, 0x30, 0x31, 0x31, 0x31, 0x31, 0x30, 0x30, 0x31, 0x30, 0x31, 0x30, 0x31,
    0x31,
];

/// Parse PTX container.
/// Fills: Format.
pub fn parse_ptx(fa: &mut FileAnalyze) -> bool {
    let want = fa.remain().min(17);
    if want < 17 {
        return false;
    }
    let Some(buf) = fa.peek_raw(want) else { return false };
    if buf != PTX_MAGIC {
        return false;
    }
    fa.stream_prepare(StreamKind::General);
    fa.set_field(StreamKind::General, 0, "Format", "Pro Tools Session");
    fa.set_field(StreamKind::General, 0, "Format_Version", "Version 10");
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_ptx() {
        let mut fa = FileAnalyze::new(b"NOT A PTX FILE........................");
        assert!(!parse_ptx(&mut fa));
    }

    #[test]
    fn parses_ptx_magic() {
        let mut buf = PTX_MAGIC.to_vec();
        buf.extend_from_slice(&[0u8; 256]);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_ptx(&mut fa));
        assert_eq!(
            fa.retrieve(StreamKind::General, 0, "Format").map(|z| z.as_str().to_owned()),
            Some("Pro Tools Session".into())
        );
        assert_eq!(
            fa.retrieve(StreamKind::General, 0, "Format_Version").map(|z| z.as_str().to_owned()),
            Some("Version 10".into())
        );
    }
}
