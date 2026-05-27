//! SequenceInfo XML manifest parser.
//!
//! Detects an XML document with root element `<SEQUENCEINFO>`.
//! Identification only — image-sequence reference walking deferred.

use mediainfo_core::{FileAnalyze, StreamKind};

pub fn parse_sequence_info(fa: &mut FileAnalyze) -> bool {
    let want = fa.Remain().min(1024);
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
    if !root_matches(body, "SEQUENCEINFO") {
        return false;
    }
    fa.Stream_Prepare(StreamKind::General);
    fa.Fill(StreamKind::General, 0, "Format", "SequenceInfo", false);
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
    fn rejects_non_xml() {
        let mut fa = FileAnalyze::new(b"NOT XML.................");
        assert!(!parse_sequence_info(&mut fa));
    }

    #[test]
    fn parses_with_prolog() {
        let mut fa = FileAnalyze::new(b"<?xml version=\"1.0\"?><SEQUENCEINFO><Frame n=\"1\"/></SEQUENCEINFO>");
        assert!(parse_sequence_info(&mut fa));
        assert_eq!(
            fa.Retrieve(StreamKind::General, 0, "Format").map(|z| z.as_str().to_owned()),
            Some("SequenceInfo".into())
        );
    }

    #[test]
    fn rejects_wrong_root() {
        let mut fa = FileAnalyze::new(b"<?xml version=\"1.0\"?><MPD/>");
        assert!(!parse_sequence_info(&mut fa));
    }
}
