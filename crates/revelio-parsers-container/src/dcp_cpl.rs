//! DCP / IMF CompositionPlaylist (CPL) XML parser.
//!
//! Mirrors `File_DcpCpl::FileHeader_Begin` in MediaInfoLib: the root
//! element must be `<CompositionPlaylist>` and its namespace decides
//! whether the file is a DCP CPL (Inter-op `digicine` PROTO namespace or
//! SMPTE 429-7) or an IMF CPL (SMPTE ST 2067-3 with the `XXXX` muxer
//! variant tolerated). Anything else is rejected.
//!
//! The Rust port only identifies the container and fills
//! General.Format; it does not (yet) follow `ReelList`/`SegmentList`
//! references or merge data from a sibling `ASSETMAP.xml`.

use revelio_core::{FileAnalyze, StreamKind};

const SCAN_WINDOW: usize = 1024;

const NS_DCP_INTEROP: &str = "http://www.digicine.com/PROTO-ASDCP-CPL-20040511#";
const NS_DCP_SMPTE: &str = "http://www.smpte-ra.org/schemas/429-7/2006/CPL";
const NS_IMF_2013: &str = "http://www.smpte-ra.org/schemas/2067-3/2013";
const NS_IMF_2016: &str = "http://www.smpte-ra.org/schemas/2067-3/2016";
// Some muxers emit `XXXX` as the year placeholder; the C++ helper
// `IsSmpteSt2067_3` explicitly accepts it.
const NS_IMF_XXXX: &str = "http://www.smpte-ra.org/schemas/2067-3/XXXX";

pub fn parse_dcp_cpl(fa: &mut FileAnalyze) -> bool {
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

    let Some(rest) = after_prolog.strip_prefix("<CompositionPlaylist") else {
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

    let is_dcp = attrs.contains(NS_DCP_INTEROP) || attrs.contains(NS_DCP_SMPTE);
    let is_imf =
        attrs.contains(NS_IMF_2013) || attrs.contains(NS_IMF_2016) || attrs.contains(NS_IMF_XXXX);

    if !is_dcp && !is_imf {
        return false;
    }

    let format = if is_dcp { "DCP CPL" } else { "IMF CPL" };
    fa.Stream_Prepare(StreamKind::General);
    fa.Fill(StreamKind::General, 0, "Format", format, true);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_smpte_dcp_cpl() {
        let xml = br#"<?xml version="1.0" encoding="UTF-8"?>
<CompositionPlaylist xmlns="http://www.smpte-ra.org/schemas/429-7/2006/CPL">
  <Id>urn:uuid:00000000-0000-0000-0000-000000000000</Id>
</CompositionPlaylist>"#;
        let mut fa = FileAnalyze::new(xml);
        assert!(parse_dcp_cpl(&mut fa));
        assert_eq!(
            fa.Retrieve(StreamKind::General, 0, "Format")
                .map(|z| z.as_str().to_owned())
                .as_deref(),
            Some("DCP CPL")
        );
    }

    #[test]
    fn parses_imf_cpl() {
        let xml = br#"<CompositionPlaylist xmlns="http://www.smpte-ra.org/schemas/2067-3/2013"></CompositionPlaylist>"#;
        let mut fa = FileAnalyze::new(xml);
        assert!(parse_dcp_cpl(&mut fa));
        assert_eq!(
            fa.Retrieve(StreamKind::General, 0, "Format")
                .map(|z| z.as_str().to_owned())
                .as_deref(),
            Some("IMF CPL")
        );
    }

    #[test]
    fn rejects_compositionplaylist_with_unknown_namespace() {
        let xml = br#"<CompositionPlaylist xmlns="http://example.com/other"></CompositionPlaylist>"#;
        let mut fa = FileAnalyze::new(xml);
        assert!(!parse_dcp_cpl(&mut fa));
    }

    #[test]
    fn rejects_wrong_root_element() {
        let xml = br#"<?xml version="1.0"?><PackingList xmlns="http://www.smpte-ra.org/schemas/429-8/2007/PKL"></PackingList>"#;
        let mut fa = FileAnalyze::new(xml);
        assert!(!parse_dcp_cpl(&mut fa));
    }
}
