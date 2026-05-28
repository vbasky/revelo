//! DASH MPD manifest parser.
//!
//! MPEG-DASH manifests are XML documents whose root element is `<MPD>`
//! declaring one of the DASH namespaces. Detection mirrors the C++
//! `File_DashMpd::FileHeader_Begin` logic in MediaInfoLib, which uses
//! tinyxml2 to confirm `Root->Attribute("xmlns")` equals one of three
//! accepted URIs before accepting.
//!
//! Only the General stream's Format is populated. The C++ parser then
//! walks Period/AdaptationSet/Representation children to enqueue
//! referenced segments via `ReferenceFiles`; the Rust engine does not
//! (yet) follow external references, so we stop after identification.

use revelio_core::{FileAnalyze, StreamKind};

const DASH_NAMESPACES: &[&str] = &[
    "urn:mpeg:DASH:schema:MPD:2011",
    "urn:mpeg:dash:schema:mpd:2011",
    "urn:3GPP:ns:PSS:AdaptiveHTTPStreamingMPD:2009",
];
const SCAN_WINDOW: usize = 1024;

/// Parse MPEG-DASH MPD manifest.
/// Fills: Format, adaptations.
pub fn parse_dash_mpd(fa: &mut FileAnalyze) -> bool {
    let window = SCAN_WINDOW.min(fa.remain());
    let Some(buf) = fa.peek_raw(window) else {
        return false;
    };

    // Only ASCII-compatible XML is supported here; UTF-16 prologs are rare
    // for manifests and the C++ FileHeader_Begin_XML path also expects
    // UTF-8 in practice for MPD.
    let text = match std::str::from_utf8(buf) {
        Ok(s) => s,
        Err(e) => std::str::from_utf8(&buf[..e.valid_up_to()]).unwrap_or(""),
    };

    let trimmed = text.trim_start();
    // Accept either a `<?xml ...?>` prolog or an immediate `<MPD` root —
    // both shapes appear in the wild and tinyxml2 tolerates either.
    let after_prolog = if let Some(rest) = trimmed.strip_prefix("<?xml") {
        match rest.find("?>") {
            Some(end) => rest[end + 2..].trim_start(),
            None => return false,
        }
    } else {
        trimmed
    };

    // Skip an optional XML comment or doctype before the root.
    let after_prolog = skip_xml_noise(after_prolog);

    let Some(rest) = after_prolog.strip_prefix("<MPD") else {
        return false;
    };
    // Next character must terminate the element name (whitespace or '>').
    let next = rest.chars().next().unwrap_or('\0');
    if !next.is_whitespace() && next != '>' {
        return false;
    }

    let Some(tag_end) = rest.find('>') else {
        return false;
    };
    let attrs = &rest[..tag_end];
    if !DASH_NAMESPACES.iter().any(|ns| attrs.contains(ns)) {
        return false;
    }

    fa.stream_prepare(StreamKind::General);
    fa.fill(StreamKind::General, 0, "Format", "DASH MPD", true);
    true
}

fn skip_xml_noise(mut s: &str) -> &str {
    loop {
        s = s.trim_start();
        if let Some(rest) = s.strip_prefix("<!--") {
            match rest.find("-->") {
                Some(end) => s = &rest[end + 3..],
                None => return s,
            }
        } else if let Some(rest) = s.strip_prefix("<!") {
            match rest.find('>') {
                Some(end) => s = &rest[end + 1..],
                None => return s,
            }
        } else {
            return s;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_mpd_with_prolog() {
        let xml = br#"<?xml version="1.0" encoding="UTF-8"?>
<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" type="static">
  <Period/>
</MPD>"#;
        let mut fa = FileAnalyze::new(xml);
        assert!(parse_dash_mpd(&mut fa));
        assert_eq!(
            fa.retrieve(StreamKind::General, 0, "Format")
                .map(|z| z.as_str().to_owned())
                .as_deref(),
            Some("DASH MPD")
        );
    }

    #[test]
    fn parses_uppercase_namespace_variant() {
        // C++ accepts the uppercase 2011 variant too.
        let xml = br#"<MPD xmlns="urn:mpeg:DASH:schema:MPD:2011"></MPD>"#;
        let mut fa = FileAnalyze::new(xml);
        assert!(parse_dash_mpd(&mut fa));
    }

    #[test]
    fn rejects_xml_with_wrong_root_element() {
        let xml = br#"<?xml version="1.0"?><manifest xmlns="http://ns.adobe.com/f4m/1.0"></manifest>"#;
        let mut fa = FileAnalyze::new(xml);
        assert!(!parse_dash_mpd(&mut fa));
    }

    #[test]
    fn rejects_mpd_with_wrong_namespace() {
        let xml = br#"<MPD xmlns="http://example.com/other"></MPD>"#;
        let mut fa = FileAnalyze::new(xml);
        assert!(!parse_dash_mpd(&mut fa));
    }

    #[test]
    fn rejects_non_xml_buffer() {
        let mut fa = FileAnalyze::new(b"RIFF\x00\x00\x00\x00WAVE");
        assert!(!parse_dash_mpd(&mut fa));
    }
}
