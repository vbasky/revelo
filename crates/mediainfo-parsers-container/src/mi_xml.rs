//! MediaInfo MiXml manifest parser.
//!
//! MiXml is MediaInfo's own XML serialization format whose root element
//! is `<MediaInfo>` carrying the namespace
//! `https://mediaarea.net/mediainfo`. Detection scans the first ~1 KiB
//! for an optional XML prolog, the `<MediaInfo` root element, and the
//! MediaArea namespace — mirroring the C++
//! `File_MiXml::FileHeader_Begin` logic in MediaInfoLib, which uses
//! tinyxml2 to confirm `Root->Attribute("xmlns")` equals
//! `https://mediaarea.net/mediainfo` before accepting.
//!
//! Only the General stream's Format is populated. The C++ parser then
//! walks `<media>`/`<track>` children to materialize per-stream tags;
//! the Rust engine does not (yet) round-trip a serialized analysis, so
//! we stop after identifying the container.

use mediainfo_core::{FileAnalyze, StreamKind};

const MIXML_NAMESPACE: &str = "https://mediaarea.net/mediainfo";
const SCAN_WINDOW: usize = 1024;

pub fn parse_mi_xml(fa: &mut FileAnalyze) -> bool {
    let window = SCAN_WINDOW.min(fa.Remain());
    let Some(buf) = fa.peek_raw(window) else {
        return false;
    };

    // Only ASCII-compatible XML is supported here; UTF-16 prologs are
    // rare for MiXml output and the C++ FileHeader_Begin_XML path also
    // expects UTF-8 in practice.
    let text = match std::str::from_utf8(buf) {
        Ok(s) => s,
        Err(e) => std::str::from_utf8(&buf[..e.valid_up_to()]).unwrap_or(""),
    };

    let trimmed = text.trim_start();
    // Accept either a `<?xml ...?>` prolog or an immediate `<MediaInfo`
    // root — tinyxml2 tolerates either shape.
    let after_prolog = if let Some(rest) = trimmed.strip_prefix("<?xml") {
        match rest.find("?>") {
            Some(end) => rest[end + 2..].trim_start(),
            None => return false,
        }
    } else {
        trimmed
    };

    let Some(rest) = after_prolog.strip_prefix("<MediaInfo") else {
        return false;
    };
    // Next character must terminate the element name (whitespace or '>').
    let next = rest.chars().next().unwrap_or('\0');
    if !next.is_whitespace() && next != '>' {
        return false;
    }

    // Confirm the MediaArea namespace appears inside the start tag — C++
    // explicitly Rejects when xmlns differs from this URI.
    let Some(tag_end) = rest.find('>') else {
        return false;
    };
    let attrs = &rest[..tag_end];
    if !attrs.contains(MIXML_NAMESPACE) {
        return false;
    }

    fa.Stream_Prepare(StreamKind::General);
    fa.Fill(StreamKind::General, 0, "Format", "MediaInfo XML", true);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_mixml_with_prolog() {
        let xml = br#"<?xml version="1.0" encoding="UTF-8"?>
<MediaInfo xmlns="https://mediaarea.net/mediainfo" version="2.0">
  <media ref="example.mkv">
    <track type="General"><Format>Matroska</Format></track>
  </media>
</MediaInfo>"#;
        let mut fa = FileAnalyze::new(xml);
        assert!(parse_mi_xml(&mut fa));
        assert_eq!(
            fa.Retrieve(StreamKind::General, 0, "Format")
                .map(|z| z.as_str().to_owned())
                .as_deref(),
            Some("MediaInfo XML")
        );
    }

    #[test]
    fn parses_mixml_without_xml_prolog() {
        let xml = br#"<MediaInfo xmlns="https://mediaarea.net/mediainfo"></MediaInfo>"#;
        let mut fa = FileAnalyze::new(xml);
        assert!(parse_mi_xml(&mut fa));
    }

    #[test]
    fn rejects_mediainfo_root_with_wrong_namespace() {
        // C++ explicitly Rejects when xmlns differs from the MediaArea URI.
        let xml = br#"<MediaInfo xmlns="http://example.com/other"></MediaInfo>"#;
        let mut fa = FileAnalyze::new(xml);
        assert!(!parse_mi_xml(&mut fa));
    }

    #[test]
    fn rejects_non_xml_buffer() {
        let mut fa = FileAnalyze::new(b"RIFF\x00\x00\x00\x00WAVE");
        assert!(!parse_mi_xml(&mut fa));
    }
}
