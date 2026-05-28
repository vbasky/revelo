//! Digital Cinema Package AssetMap parser.
//!
//! A DCP AssetMap is an XML document whose root element is `<AssetMap>`
//! declared in either the Interop namespace
//! `http://www.digicine.com/PROTO-ASDCP-AM-20040311#` or the SMPTE
//! namespace `http://www.smpte-ra.org/schemas/429-9/2007/AM`. Detection
//! scans the first ~1 KiB for an XML prolog or whitespace, then the
//! `<AssetMap` element name carrying one of those namespaces — mirroring
//! the C++ `File_DcpAm::FileHeader_Begin` logic in MediaInfoLib which
//! uses tinyxml2 to confirm the root local name is `AssetMap` and the
//! namespace URI matches Interop or SMPTE before accepting.
//!
//! Only the General stream's Format and Format/Version are populated.
//! The C++ parser then walks `<AssetList>/<Asset>/<ChunkList>/<Chunk>`
//! children to enqueue referenced PKL/CPL files via `ReferenceFiles`,
//! and may re-label the container as `IMF AM` once a CPL identifies as
//! IMF; the Rust engine does not (yet) follow external references, so
//! we stop after identifying the container.

use revelio_core::{FileAnalyze, StreamKind};

const INTEROP_NAMESPACE: &str = "http://www.digicine.com/PROTO-ASDCP-AM-20040311#";
const SMPTE_NAMESPACE: &str = "http://www.smpte-ra.org/schemas/429-9/2007/AM";
const SCAN_WINDOW: usize = 1024;

/// Parse DCP Asset Map.
/// Fills: Format.
pub fn parse_dcp_am(fa: &mut FileAnalyze) -> bool {
    let window = SCAN_WINDOW.min(fa.remain());
    let Some(buf) = fa.peek_raw(window) else {
        return false;
    };

    // Only ASCII-compatible XML is supported here; UTF-16 prologs are
    // rare for AssetMaps and the C++ FileHeader_Begin_XML path also
    // expects UTF-8 in practice for DCP.
    let text = match std::str::from_utf8(buf) {
        Ok(s) => s,
        Err(e) => std::str::from_utf8(&buf[..e.valid_up_to()]).unwrap_or(""),
    };

    let trimmed = text.trim_start();
    // Accept either a `<?xml ...?>` prolog or an immediate `<AssetMap`
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

    let Some(rest) = after_prolog.strip_prefix("<AssetMap") else {
        return false;
    };
    let next = rest.chars().next().unwrap_or('\0');
    if !next.is_whitespace() && next != '>' {
        return false;
    }

    let Some(tag_end) = rest.find('>') else {
        return false;
    };
    let attrs = &rest[..tag_end];
    // C++ rejects unless the root namespace URI is exactly the Interop
    // or SMPTE AssetMap schema; mirror that by requiring one of the two
    // strings inside the start tag.
    let version = if attrs.contains(INTEROP_NAMESPACE) {
        "Interop"
    } else if attrs.contains(SMPTE_NAMESPACE) {
        "SMPTE"
    } else {
        return false;
    };

    fa.stream_prepare(StreamKind::General);
    fa.fill(StreamKind::General, 0, "Format", "DCP AM", true);
    fa.fill(StreamKind::General, 0, "Format_Version", version, true);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_smpte_assetmap_with_prolog() {
        let xml = br#"<?xml version="1.0" encoding="UTF-8"?>
<AssetMap xmlns="http://www.smpte-ra.org/schemas/429-9/2007/AM">
  <Id>urn:uuid:00000000-0000-0000-0000-000000000000</Id>
</AssetMap>"#;
        let mut fa = FileAnalyze::new(xml);
        assert!(parse_dcp_am(&mut fa));
        assert_eq!(
            fa.retrieve(StreamKind::General, 0, "Format")
                .map(|z| z.as_str().to_owned())
                .as_deref(),
            Some("DCP AM")
        );
        assert_eq!(
            fa.retrieve(StreamKind::General, 0, "Format_Version")
                .map(|z| z.as_str().to_owned())
                .as_deref(),
            Some("SMPTE")
        );
    }

    #[test]
    fn parses_interop_assetmap_without_prolog() {
        let xml = br#"<AssetMap xmlns="http://www.digicine.com/PROTO-ASDCP-AM-20040311#"></AssetMap>"#;
        let mut fa = FileAnalyze::new(xml);
        assert!(parse_dcp_am(&mut fa));
        assert_eq!(
            fa.retrieve(StreamKind::General, 0, "Format_Version")
                .map(|z| z.as_str().to_owned())
                .as_deref(),
            Some("Interop")
        );
    }

    #[test]
    fn rejects_assetmap_with_wrong_namespace() {
        let xml = br#"<AssetMap xmlns="http://example.com/other"></AssetMap>"#;
        let mut fa = FileAnalyze::new(xml);
        assert!(!parse_dcp_am(&mut fa));
    }

    #[test]
    fn rejects_xml_with_wrong_root_element() {
        let xml = br#"<?xml version="1.0"?><PackingList xmlns="http://www.smpte-ra.org/schemas/429-8/2007/PKL"></PackingList>"#;
        let mut fa = FileAnalyze::new(xml);
        assert!(!parse_dcp_am(&mut fa));
    }

    #[test]
    fn rejects_non_xml_buffer() {
        let mut fa = FileAnalyze::new(b"RIFF\x00\x00\x00\x00WAVE");
        assert!(!parse_dcp_am(&mut fa));
    }
}
