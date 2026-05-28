//! DXW (Digimetrics XML Wrapper) manifest parser.
//!
//! Mirrors `File_Dxw::FileHeader_Begin` in MediaInfoLib: the root element
//! must be `<indexFile>` and its `xmlns` attribute must equal
//! `urn:digimetrics-xml-wrapper`. Anything else is rejected.
//!
//! The Rust port only identifies the container and fills General.Format;
//! it does not (yet) follow the `<clip>`/`<frame>` reference children.

use revelio_core::{FileAnalyze, StreamKind};

const SCAN_WINDOW: usize = 1024;

const NS_DXW: &str = "urn:digimetrics-xml-wrapper";

pub fn parse_dxw(fa: &mut FileAnalyze) -> bool {
    let window = SCAN_WINDOW.min(fa.Remain());
    let Some(buf) = fa.peek_raw(window) else {
        return false;
    };

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

    let Some(rest) = after_prolog.strip_prefix("<indexFile") else {
        return false;
    };
    let next = rest.chars().next().unwrap_or('\0');
    // Ensure we matched the whole element name, not a prefix like <indexFileFoo>.
    if !next.is_whitespace() && next != '>' {
        return false;
    }

    let Some(tag_end) = rest.find('>') else {
        return false;
    };
    let attrs = &rest[..tag_end];

    if !attrs.contains(NS_DXW) {
        return false;
    }

    fa.Stream_Prepare(StreamKind::General);
    fa.Fill(StreamKind::General, 0, "Format", "DXW", true);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_dxw() {
        let xml = br#"<?xml version="1.0" encoding="UTF-8"?>
<indexFile xmlns="urn:digimetrics-xml-wrapper">
  <clip file="track1.mxf" type="video" source="main"/>
</indexFile>"#;
        let mut fa = FileAnalyze::new(xml);
        assert!(parse_dxw(&mut fa));
        assert_eq!(
            fa.Retrieve(StreamKind::General, 0, "Format")
                .map(|z| z.as_str().to_owned())
                .as_deref(),
            Some("DXW")
        );
    }

    #[test]
    fn rejects_indexfile_with_wrong_namespace() {
        let xml = br#"<indexFile xmlns="http://example.com/other"></indexFile>"#;
        let mut fa = FileAnalyze::new(xml);
        assert!(!parse_dxw(&mut fa));
    }

    #[test]
    fn rejects_wrong_root_element() {
        let xml = br#"<?xml version="1.0"?><other xmlns="urn:digimetrics-xml-wrapper"></other>"#;
        let mut fa = FileAnalyze::new(xml);
        assert!(!parse_dxw(&mut fa));
    }
}
