//! TTML (Timed Text Markup Language) parser.
//!
//! TTML documents are XML whose root element is `<tt>` declaring the
//! W3C namespace `http://www.w3.org/ns/ttml`. Detection scans the first
//! ~1 KiB for an optional XML prolog, the `<tt` root element name, and
//! the TTML namespace declaration — mirroring the C++
//! `File_Ttml::FileHeader_Begin` logic in MediaInfoLib, which uses
//! tinyxml2 to confirm the root tag and namespace before accepting.
//!
//! Both General and Text streams have Format=TTML populated, matching
//! the upstream behaviour for a recognised subtitle document.

use revelo_core::{FileAnalyze, StreamKind};

const TTML_NAMESPACE: &str = "http://www.w3.org/ns/ttml";
const SCAN_WINDOW: usize = 1024;

/// Parse TTML timed text subtitle.
///
/// Detection: `<tt>` XML root.
/// Fills: Format, namespace.
pub fn parse_ttml(fa: &mut FileAnalyze) -> bool {
    let window = SCAN_WINDOW.min(fa.remain());
    let Some(buf) = fa.peek_raw(window) else {
        return false;
    };

    // TTML in the wild is UTF-8; UTF-16 prologs exist but are out of
    // scope for the simple-detection path (matches C++ FileHeader_Begin_XML).
    let text = match std::str::from_utf8(buf) {
        Ok(s) => s,
        Err(e) => std::str::from_utf8(&buf[..e.valid_up_to()]).unwrap_or(""),
    };

    let trimmed = text.trim_start();
    let after_prolog = if let Some(rest) = trimmed.strip_prefix("<?xml") {
        match rest.find("?>") {
            Some(end) => rest[end + 2..].trim_start(),
            None => return false,
        }
    } else {
        trimmed
    };

    let Some(rest) = after_prolog.strip_prefix("<tt") else {
        return false;
    };
    // Next character must terminate the element name (whitespace or '>')
    // so we don't accept e.g. `<title>` or `<ttp:...>`.
    let next = rest.chars().next().unwrap_or('\0');
    if !next.is_whitespace() && next != '>' {
        return false;
    }

    let Some(tag_end) = rest.find('>') else {
        return false;
    };
    let attrs = &rest[..tag_end];
    if !attrs.contains(TTML_NAMESPACE) {
        return false;
    }

    fa.stream_prepare(StreamKind::General);
    fa.force_field(StreamKind::General, 0, "Format", "TTML");
    fa.stream_prepare(StreamKind::Text);
    fa.force_field(StreamKind::Text, 0, "Format", "TTML");
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_ttml_with_prolog() {
        let xml = br#"<?xml version="1.0" encoding="UTF-8"?>
<tt xmlns="http://www.w3.org/ns/ttml" xml:lang="en">
  <body><div><p begin="0s" end="2s">Hello</p></div></body>
</tt>"#;
        let mut fa = FileAnalyze::new(xml);
        assert!(parse_ttml(&mut fa));
        assert_eq!(
            fa.retrieve(StreamKind::General, 0, "Format").map(|z| z.as_str().to_owned()).as_deref(),
            Some("TTML")
        );
        assert_eq!(
            fa.retrieve(StreamKind::Text, 0, "Format").map(|z| z.as_str().to_owned()).as_deref(),
            Some("TTML")
        );
    }

    #[test]
    fn parses_ttml_without_xml_prolog() {
        let xml = br#"<tt xmlns="http://www.w3.org/ns/ttml"></tt>"#;
        let mut fa = FileAnalyze::new(xml);
        assert!(parse_ttml(&mut fa));
    }

    #[test]
    fn rejects_xml_with_wrong_namespace() {
        let xml = br#"<?xml version="1.0"?><tt xmlns="http://example.com/other"></tt>"#;
        let mut fa = FileAnalyze::new(xml);
        assert!(!parse_ttml(&mut fa));
    }

    #[test]
    fn rejects_non_xml_buffer() {
        let mut fa = FileAnalyze::new(b"RIFF\x00\x00\x00\x00WAVE");
        assert!(!parse_ttml(&mut fa));
    }
}
