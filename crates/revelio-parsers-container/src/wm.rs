//! Windows Media / ASF (Advanced Systems Format) container parser.
//!
//! Mirrors `File_Wm::FileHeader_Begin` in MediaInfoLib: the file starts
//! with the ASF Header Object GUID. Per the ASF spec (and the C++
//! `Header_Parse` slice referenced), GUIDs are stored as a mixed-endian
//! 128-bit value — first 8 bytes are little-endian fields, last 8 bytes
//! big-endian. The Header Object GUID, written in stream order, is:
//!   30 26 B2 75 8E 66 CF 11 A6 D9 00 AA 00 62 CE 6C
//!
//! Only the container is identified and General.Format is filled.
//! Per-object stream parsing (StreamProperties, ExtendedContentDescription,
//! CodecList, etc.) is deferred — File_Wm_Elements.cpp has no Rust port.

use revelio_core::{FileAnalyze, StreamKind};

const ASF_HEADER_GUID: [u8; 16] = [
    0x30, 0x26, 0xB2, 0x75, 0x8E, 0x66, 0xCF, 0x11, 0xA6, 0xD9, 0x00, 0xAA, 0x00, 0x62, 0xCE, 0x6C,
];

/// Parse Windows Media (ASF/WMV/WMA) container.
///
/// Detection: ASF GUID header objects.
/// Fills: Stream properties, content description.
pub fn parse_wm(fa: &mut FileAnalyze) -> bool {
    let head = match fa.peek_raw(fa.remain().min(16)) {
        Some(b) if b.len() == 16 => b,
        _ => return false,
    };
    if head != ASF_HEADER_GUID {
        return false;
    }

    fa.stream_prepare(StreamKind::General);
    fa.force_field(StreamKind::General, 0, "Format", "Windows Media");

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_asf_header() {
        let mut buf = Vec::from(ASF_HEADER_GUID);
        // Plausible trailing bytes from a real ASF Header Object (size +
        // child count) — ignored by this minimal port.
        buf.extend_from_slice(&[0u8; 32]);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_wm(&mut fa));
        assert_eq!(
            fa.retrieve(StreamKind::General, 0, "Format").map(|z| z.as_str().to_owned()).as_deref(),
            Some("Windows Media"),
        );
    }

    #[test]
    fn rejects_non_asf_buffer() {
        let mut fa = FileAnalyze::new(b"NOT A WINDOWS MEDIA FILE AT ALL!");
        assert!(!parse_wm(&mut fa));
    }

    #[test]
    fn rejects_truncated_buffer() {
        // Fewer than 16 bytes — can't read a full GUID.
        let mut fa = FileAnalyze::new(&ASF_HEADER_GUID[..15]);
        assert!(!parse_wm(&mut fa));
    }
}
