//! CMML (Continuous Media Markup Language) parser.
//!
//! CMML is an XML-based timed metadata language for continuous media,
//! originally developed for the Annodex/Ogg ecosystem. The on-disk shape
//! relevant here is the XML serialization whose root element is `<cmml`,
//! optionally preceded by an `<?xml ... ?>` prolog. MediaInfoLib's
//! `File_Cmml` (Source/MediaInfo/Text/File_Cmml.cpp) accepts the stream
//! after a CMML identification packet and then walks the textual head/clip
//! children; this Rust parser short-circuits at detection and only fills
//! the General and Text Format fields, since the broader engine does not
//! yet model CMML clip-level metadata.

use revelio_core::{FileAnalyze, StreamKind};

const SCAN_WINDOW: usize = 1024;

pub fn parse_cmml(fa: &mut FileAnalyze) -> bool {
    let window = SCAN_WINDOW.min(fa.Remain());
    let Some(buf) = fa.peek_raw(window) else {
        return false;
    };

    // CMML in the wild is UTF-8; tolerate a trailing partial UTF-8 sequence
    // at the scan window edge by using the valid prefix.
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

    let Some(rest) = after_prolog.strip_prefix("<cmml") else {
        return false;
    };
    // Next char must end the root element name so we don't match e.g.
    // `<cmmlfoo` — only whitespace, '>' or '/' are valid here.
    let next = rest.chars().next().unwrap_or('\0');
    if !next.is_whitespace() && next != '>' && next != '/' {
        return false;
    }

    fa.Stream_Prepare(StreamKind::General);
    fa.Fill(StreamKind::General, 0, "Format", "CMML", true);

    fa.Stream_Prepare(StreamKind::Text);
    fa.Fill(StreamKind::Text, 0, "Format", "CMML", true);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_cmml_with_prolog() {
        let xml = br#"<?xml version="1.0" encoding="UTF-8"?>
<cmml xmlns="http://www.annodex.net/cmml" granulerate="1/1000">
  <head><title>Sample</title></head>
  <clip id="c1" start="0"/>
</cmml>"#;
        let mut fa = FileAnalyze::new(xml);
        assert!(parse_cmml(&mut fa));
        assert_eq!(
            fa.Retrieve(StreamKind::General, 0, "Format")
                .map(|z| z.as_str().to_owned())
                .as_deref(),
            Some("CMML")
        );
        assert_eq!(
            fa.Retrieve(StreamKind::Text, 0, "Format")
                .map(|z| z.as_str().to_owned())
                .as_deref(),
            Some("CMML")
        );
        assert_eq!(fa.Count_Get(StreamKind::Text), 1);
    }

    #[test]
    fn parses_cmml_without_prolog() {
        let xml = br#"<cmml></cmml>"#;
        let mut fa = FileAnalyze::new(xml);
        assert!(parse_cmml(&mut fa));
    }

    #[test]
    fn rejects_other_xml_root() {
        // A different XML root with a name that shares a prefix must not
        // be misidentified as CMML.
        let xml = br#"<?xml version="1.0"?><cmmlfoo></cmmlfoo>"#;
        let mut fa = FileAnalyze::new(xml);
        assert!(!parse_cmml(&mut fa));
    }

    #[test]
    fn rejects_non_xml_buffer() {
        let mut fa = FileAnalyze::new(b"\x00\x00\x00\x00not xml");
        assert!(!parse_cmml(&mut fa));
    }
}
