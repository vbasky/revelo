//! Adobe HDS F4M manifest parser.
//!
//! HDS (HTTP Dynamic Streaming) manifests are XML documents whose root
//! element is `<manifest>` carrying the Adobe namespace
//! `http://ns.adobe.com/f4m/1.0`. Detection therefore scans the first
//! ~1 KiB for an XML prolog or whitespace, the `<manifest` element name,
//! and the F4M namespace declaration — mirroring the C++
//! `File_HdsF4m::FileHeader_Begin` logic in MediaInfoLib, which uses
//! tinyxml2 to confirm `Root->Attribute("xmlns")` equals
//! `http://ns.adobe.com/f4m/1.0` before accepting.
//!
//! Only the General stream's Format is populated. The C++ parser then
//! walks `<media url=...>` children to enqueue referenced segment files
//! via `ReferenceFiles`; the Rust engine does not (yet) follow external
//! references, so we stop after identifying the container.

use revelio_core::{FileAnalyze, StreamKind};

const F4M_NAMESPACE: &str = "http://ns.adobe.com/f4m/1.0";
const SCAN_WINDOW: usize = 1024;

pub fn parse_hds_f4m(fa: &mut FileAnalyze) -> bool {
    let window = SCAN_WINDOW.min(fa.Remain());
    let Some(buf) = fa.peek_raw(window) else {
        return false;
    };

    // Only ASCII-compatible XML is supported here; UTF-16 prologs are rare
    // for manifests and the C++ FileHeader_Begin_XML path also expects
    // UTF-8 in practice for HDS.
    let text = match std::str::from_utf8(buf) {
        Ok(s) => s,
        Err(e) => {
            // Use the valid prefix — XML errors past this point are not
            // ours to diagnose, we just need the manifest root.
            std::str::from_utf8(&buf[..e.valid_up_to()]).unwrap_or("")
        }
    };

    let trimmed = text.trim_start();
    // Accept either a `<?xml ...?>` prolog or an immediate `<manifest`
    // root — both shapes appear in the wild and the C++ tinyxml2 path
    // tolerates either.
    let after_prolog = if let Some(rest) = trimmed.strip_prefix("<?xml") {
        match rest.find("?>") {
            Some(end) => rest[end + 2..].trim_start(),
            None => return false,
        }
    } else {
        trimmed
    };

    let Some(rest) = after_prolog.strip_prefix("<manifest") else {
        return false;
    };
    // Next character must terminate the element name (whitespace or '>').
    let next = rest.chars().next().unwrap_or('\0');
    if !next.is_whitespace() && next != '>' {
        return false;
    }

    // Confirm the F4M namespace appears inside the manifest start tag.
    let Some(tag_end) = rest.find('>') else {
        return false;
    };
    let attrs = &rest[..tag_end];
    if !attrs.contains(F4M_NAMESPACE) {
        return false;
    }

    fa.Stream_Prepare(StreamKind::General);
    fa.Fill(StreamKind::General, 0, "Format", "HDS F4M", true);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_manifest_with_prolog() {
        let xml = br#"<?xml version="1.0" encoding="UTF-8"?>
<manifest xmlns="http://ns.adobe.com/f4m/1.0">
  <id>stream</id>
</manifest>"#;
        let mut fa = FileAnalyze::new(xml);
        assert!(parse_hds_f4m(&mut fa));
        assert_eq!(
            fa.Retrieve(StreamKind::General, 0, "Format")
                .map(|z| z.as_str().to_owned())
                .as_deref(),
            Some("HDS F4M")
        );
    }

    #[test]
    fn parses_manifest_without_xml_prolog() {
        let xml = br#"<manifest xmlns="http://ns.adobe.com/f4m/1.0"></manifest>"#;
        let mut fa = FileAnalyze::new(xml);
        assert!(parse_hds_f4m(&mut fa));
    }

    #[test]
    fn rejects_xml_with_wrong_root_element() {
        let xml = br#"<?xml version="1.0"?><MPD xmlns="urn:mpeg:dash:schema:mpd:2011"></MPD>"#;
        let mut fa = FileAnalyze::new(xml);
        assert!(!parse_hds_f4m(&mut fa));
    }

    #[test]
    fn rejects_manifest_with_wrong_namespace() {
        // C++ explicitly Rejects when xmlns differs from the F4M URI.
        let xml = br#"<manifest xmlns="http://example.com/other"></manifest>"#;
        let mut fa = FileAnalyze::new(xml);
        assert!(!parse_hds_f4m(&mut fa));
    }

    #[test]
    fn rejects_non_xml_buffer() {
        let mut fa = FileAnalyze::new(b"RIFF\x00\x00\x00\x00WAVE");
        assert!(!parse_hds_f4m(&mut fa));
    }
}
