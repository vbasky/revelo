use revelo_core::{StreamCollection, StreamKind};
pub fn to_mpeg7(streams: &StreamCollection, file_path: &str) -> String {
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<Mpeg7 xmlns=\"urn:mpeg:mpeg7:schema:2004\">\n");
    out.push_str(&format!("<MediaInformation><MediaLocator>{file_path}</MediaLocator>\n"));
    for kind in [StreamKind::General, StreamKind::Video, StreamKind::Audio] {
        for p in 0..streams.stream_count(kind) {
            if let Some(s) = streams.stream(kind, p) {
                out.push_str(&format!("<{kind:?}>\n", kind = kind));
                for (k, v) in s.iter() {
                    out.push_str(&format!("<{k}>{v}</{k}>\n", k = k, v = v.as_str()));
                }
                out.push_str(&format!("</{kind:?}>\n", kind = kind));
            }
        }
    }
    out.push_str("</MediaInformation></Mpeg7>\n");
    out
}
#[cfg(test)]
mod tests {
    use super::*;
    use zenlib::Ztring;
    #[test]
    fn test() {
        let mut c = StreamCollection::new();
        c.set_field(StreamKind::General, 0, "Format", Ztring::from("MP4"));
        let xml = to_mpeg7(&c, "/x.mp4");
        assert!(xml.contains("Mpeg7"));
    }
}
