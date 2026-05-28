//! DCP / IMF PackingList (PKL) XML parser.
//!
//! Mirrors `File_DcpPkl::FileHeader_Begin` in MediaInfoLib: the root
//! element must be `<PackingList>` carrying one of the Inter-op
//! (`digicine` PROTO) or SMPTE PKL namespaces. The C++ side then resolves
//! the referenced assets (via a sibling `ASSETMAP.xml`) and only upgrades
//! the format to `IMF PKL` in `Streams_Finish` when a referenced stream is
//! an IMF CPL; a standalone PKL therefore always reports `DCP PKL`.
//!
//! This Rust port identifies the container and fills General.Format +
//! StreamSize (the whole file is the "stream", as the oracle reports); it
//! does not follow the AssetList references.

use revelio_core::{FileAnalyze, StreamKind};

const SCAN_WINDOW: usize = 1024;

const NS_DCP_INTEROP: &str = "http://www.digicine.com/PROTO-ASDCP-PKL-20040311#";
const NS_DCP_SMPTE: &str = "http://www.smpte-ra.org/schemas/429-8/2007/PKL";
const NS_IMF_2016: &str = "http://www.smpte-ra.org/schemas/2067-2/2016/PKL";

/// Parse DCP Packing List.
/// Fills: Format.
pub fn parse_dcp_pkl(fa: &mut FileAnalyze) -> bool {
    let file_size = fa.remain();
    let window = SCAN_WINDOW.min(file_size);
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

    let Some(rest) = after_prolog.strip_prefix("<PackingList") else {
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

    if !attrs.contains(NS_DCP_INTEROP)
        && !attrs.contains(NS_DCP_SMPTE)
        && !attrs.contains(NS_IMF_2016)
    {
        return false;
    }

    fa.stream_prepare(StreamKind::General);
    fa.fill(StreamKind::General, 0, "Format", "DCP PKL", true);
    // The oracle reports General.StreamSize == FileSize for these
    // reference XML files (the whole file is the elementary stream).
    fa.fill(StreamKind::General, 0, "StreamSize", file_size.to_string(), true);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_smpte_dcp_pkl() {
        let xml = br#"<?xml version="1.0" encoding="UTF-8"?>
<PackingList xmlns="http://www.smpte-ra.org/schemas/429-8/2007/PKL">
  <Id>urn:uuid:00000000-0000-0000-0000-000000000000</Id>
</PackingList>"#;
        let mut fa = FileAnalyze::new(xml);
        assert!(parse_dcp_pkl(&mut fa));
        assert_eq!(
            fa.retrieve(StreamKind::General, 0, "Format").map(|z| z.as_str().to_owned()).as_deref(),
            Some("DCP PKL")
        );
    }

    #[test]
    fn parses_interop_dcp_pkl() {
        let xml = br#"<PackingList xmlns="http://www.digicine.com/PROTO-ASDCP-PKL-20040311#"></PackingList>"#;
        let mut fa = FileAnalyze::new(xml);
        assert!(parse_dcp_pkl(&mut fa));
    }

    #[test]
    fn rejects_cpl_root() {
        let xml = br#"<CompositionPlaylist xmlns="http://www.smpte-ra.org/schemas/429-7/2006/CPL"></CompositionPlaylist>"#;
        let mut fa = FileAnalyze::new(xml);
        assert!(!parse_dcp_pkl(&mut fa));
    }

    #[test]
    fn rejects_packinglist_unknown_namespace() {
        let xml = br#"<PackingList xmlns="http://example.com/other"></PackingList>"#;
        let mut fa = FileAnalyze::new(xml);
        assert!(!parse_dcp_pkl(&mut fa));
    }
}
