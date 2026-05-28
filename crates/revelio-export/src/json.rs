use revelio_core::{StreamCollection, StreamKind};

pub fn to_json(streams: &StreamCollection, file_path: &str) -> String {
    let mut out = String::new();
    out.push_str("{\"media\":{\"@ref\":\"");
    out.push_str(&json_escape_attr(file_path));
    out.push_str("\",\"track\":[");
    let kinds = [
        StreamKind::General,
        StreamKind::Video,
        StreamKind::Audio,
        StreamKind::Text,
        StreamKind::Other,
        StreamKind::Image,
        StreamKind::Menu,
    ];
    let mut first = true;
    for kind in kinds {
        let count = streams.Count_Get(kind);
        for pos in 0..count {
            if let Some(stream) = streams.stream(kind, pos) {
                if !first {
                    out.push(',');
                } else {
                    first = false;
                }
                out.push_str("{\"@type\":\"");
                out.push_str(kind.name());
                out.push('"');

                for (k, v) in stream.iter() {
                    out.push_str(",\"");
                    out.push_str(k);
                    out.push_str("\":\"");
                    out.push_str(&json_escape_value(v.as_str()));
                    out.push('"');
                }

                let extra_count = stream.extras_iter().count();
                if extra_count > 0 {
                    out.push_str(",\"extra\":{");
                    let mut first_extra = true;
                    for (k, v) in stream.extras_iter() {
                        if !first_extra {
                            out.push(',');
                        }
                        first_extra = false;
                        out.push_str("\"");
                        out.push_str(k);
                        out.push_str("\":\"");
                        out.push_str(&json_escape_value(v.as_str()));
                        out.push('"');
                    }
                    out.push('}');
                }

                out.push('}');
            }
        }
    }
    out.push_str("]}}");
    out
}

fn json_escape_attr(s: &str) -> String {
    s.replace('\\', r"\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\t', "\\t")
}

fn json_escape_value(s: &str) -> String {
    json_escape_attr(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use zenlib::Ztring;

    #[test]
    fn json_produces_valid_structure() {
        let mut c = StreamCollection::new();
        c.Fill(StreamKind::General, 0, "Format", Ztring::from("MPEG-4"), false);
        c.Fill(StreamKind::Video, 0, "Width", Ztring::from("1920"), false);
        let json = to_json(&c, "/tmp/test.mp4");
        assert!(json.starts_with("{\"media\":{\"@ref\":\"/tmp/test.mp4\""));
        assert!(json.contains("\"@type\":\"General\""));
        assert!(json.contains("\"@type\":\"Video\""));
        assert!(json.contains("\"Format\":\"MPEG-4\""));
        assert!(json.contains("\"Width\":\"1920\""));
    }

    #[test]
    fn json_escapes_special_chars() {
        let mut c = StreamCollection::new();
        c.Fill(StreamKind::General, 0, "Format", Ztring::from("A \"B\" C"), false);
        let json = to_json(&c, "/x");
        assert!(json.contains("A \\\"B\\\" C"));
    }

    #[test]
    fn json_empty_collection_produces_empty_tracks() {
        let c = StreamCollection::new();
        let json = to_json(&c, "/x");
        assert_eq!(json, "{\"media\":{\"@ref\":\"/x\",\"track\":[]}}");
    }
}
