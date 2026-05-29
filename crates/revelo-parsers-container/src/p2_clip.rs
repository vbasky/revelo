//! Panasonic P2 Clip manifest parser (.xml).
//!
//! P2 clips wrap MXF essence with a sidecar XML clip-metadata file
//! whose root element is `<P2Main>`. Identification only — the
//! manifest's full ClipContent/EssenceList tree is deferred until
//! a reference-files walker exists.

use revelo_core::{FileAnalyze, StreamKind};

pub fn parse_p2_clip(fa: &mut FileAnalyze) -> bool {
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
    if !root_matches(body, "P2Main") {
        return false;
    }
    fa.stream_prepare(StreamKind::General);
    fa.set_field(StreamKind::General, 0, "Format", "P2 Clip");
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
    fn rejects_non_p2() {
        let mut fa = FileAnalyze::new(b"<?xml version=\"1.0\"?><MPD></MPD>");
        assert!(!parse_p2_clip(&mut fa));
    }

    #[test]
    fn parses_p2main_with_prolog() {
        let mut fa = FileAnalyze::new(b"<?xml version=\"1.0\"?><P2Main><ClipContent/></P2Main>");
        assert!(parse_p2_clip(&mut fa));
        assert_eq!(
            fa.retrieve(StreamKind::General, 0, "Format").map(|z| z.as_str().to_owned()),
            Some("P2 Clip".into())
        );
    }

    #[test]
    fn parses_p2main_without_prolog() {
        let mut fa = FileAnalyze::new(b"<P2Main/>");
        assert!(parse_p2_clip(&mut fa));
    }
}
