use revelio_core::{StreamCollection, StreamKind};
pub fn to_graph(streams: &StreamCollection) -> String {
    let mut out = String::from("digraph revelio {\n");
    for kind in [
        StreamKind::General,
        StreamKind::Video,
        StreamKind::Audio,
        StreamKind::Text,
        StreamKind::Image,
    ] {
        for p in 0..streams.stream_count(kind) {
            if let Some(s) = streams.stream(kind, p) {
                out.push_str(&format!("  \"{}\" [shape=box];\n", kind.name()));
                for (k, _) in s.iter() {
                    out.push_str(&format!("  \"{k}\";\n"));
                }
            }
        }
    }
    out.push_str("}\n");
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
        assert!(to_graph(&c).contains("digraph"));
    }
}
