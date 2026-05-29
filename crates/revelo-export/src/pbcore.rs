use revelo_core::{StreamCollection, StreamKind};
pub fn to_pbcore(streams: &StreamCollection, file_path: &str) -> String {
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<pbcoreDescriptionDocument xmlns=\"http://www.pbcore.org/PBCore/PBCoreNamespace.html\">\n");
    out.push_str(&format!("<pbcoreIdentifier source=\"revelo\">{file_path}</pbcoreIdentifier>\n"));
    for kind in [StreamKind::General, StreamKind::Video, StreamKind::Audio] {
        for p in 0..streams.stream_count(kind) {
            if let Some(s) = streams.stream(kind, p) {
                out.push_str(&format!(
                    "<pbcoreInstantiation>\n<pbcoreFormatID source=\"{}\"/>\n",
                    kind.name()
                ));
                for (k, v) in s.iter() {
                    out.push_str(&format!("<pbcore{}>{}</pbcore{}>\n", k, v.as_str(), k));
                }
                out.push_str("</pbcoreInstantiation>\n");
            }
        }
    }
    out.push_str("</pbcoreDescriptionDocument>\n");
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
        let xml = to_pbcore(&c, "/x.mp4");
        assert!(xml.contains("pbcoreDescriptionDocument"));
    }
}
