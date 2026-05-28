//! Sony XDCAM clip manifest parser (.xml).
//!
//! Sidecar metadata file for XDCAM essence; root element is
//! `<NonRealTimeMeta>`. Identification only.

use revelio_core::{FileAnalyze, StreamKind};

pub fn parse_xdcam_clip(fa: &mut FileAnalyze) -> bool {
    let want = fa.remain().min(1024);
    if want < 8 {
        return false;
    }
    let Some(buf) = fa.peek_raw(want) else { return false };
    let s = match std::str::from_utf8(buf) {
        Ok(s) => s,
        Err(e) => match std::str::from_utf8(&buf[..e.valid_up_to()]) {
            Ok(s) => s,
            Err(_) => return false,
        },
    };
    let body = strip_xml_prolog(s.trim_start());
    if !root_matches(body, "NonRealTimeMeta") {
        return false;
    }
    fa.stream_prepare(StreamKind::General);
    fa.fill(StreamKind::General, 0, "Format", "XDCAM Clip", false);
    true
}

fn strip_xml_prolog(s: &str) -> &str {
    if let Some(rest) = s.strip_prefix("<?xml") {
        if let Some(end) = rest.find("?>") {
            return rest[end + 2..].trim_start();
        }
    }
    s
}

fn root_matches(body: &str, name: &str) -> bool {
    let Some(rest) = body.strip_prefix('<') else { return false };
    let Some(rest) = rest.strip_prefix(name) else { return false };
    rest.starts_with(|c: char| c.is_ascii_whitespace() || c == '>' || c == '/')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_xdcam() {
        let mut fa = FileAnalyze::new(b"<?xml version=\"1.0\"?><MPD></MPD>");
        assert!(!parse_xdcam_clip(&mut fa));
    }

    #[test]
    fn parses_xdcam_with_prolog() {
        let mut fa = FileAnalyze::new(b"<?xml version=\"1.0\"?><NonRealTimeMeta><CreationDate/></NonRealTimeMeta>");
        assert!(parse_xdcam_clip(&mut fa));
        assert_eq!(
            fa.retrieve(StreamKind::General, 0, "Format").map(|z| z.as_str().to_owned()),
            Some("XDCAM Clip".into())
        );
    }

    #[test]
    fn parses_xdcam_without_prolog() {
        let mut fa = FileAnalyze::new(b"<NonRealTimeMeta/>");
        assert!(parse_xdcam_clip(&mut fa));
    }
}
