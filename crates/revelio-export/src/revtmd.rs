use revelio_core::{StreamCollection, StreamKind};
pub fn to_revtmd(streams: &StreamCollection, file_path: &str) -> String {
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<REVTMD>\n");
    out.push_str(&format!("<Source>{file_path}</Source>\n"));
    for kind in [StreamKind::General, StreamKind::Video, StreamKind::Audio] {
        for p in 0..streams.count_get(kind) {
            if let Some(s) = streams.stream(kind, p) {
                for (k, v) in s.iter() {
                    out.push_str(&format!("<{k}>{v}</{k}>\n", k = k, v = v.as_str()));
                }
            }
        }
    }
    out.push_str("</REVTMD>\n");
    out
}
#[cfg(test)]
mod tests {
    use super::*;
    use zenlib::Ztring;
    #[test]
    fn test() {
        let mut c = StreamCollection::new();
        c.fill(StreamKind::General, 0, "Format", Ztring::from("MP4"), false);
        assert!(to_revtmd(&c, "/x.mp4").contains("REVTMD"));
    }
}
