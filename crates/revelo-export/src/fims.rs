use revelo_core::{StreamCollection, StreamKind};

/// FIMS (Framework for Interoperable Media Services) 1.0 export.
///
/// This is a **pragmatic, well-formed** rendering of the parsed stream fields,
/// not a schema-validated FIMS document: revelo's field names are emitted as
/// namespaced elements verbatim rather than mapped onto the FIMS `bmContent`
/// vocabulary (that mapping would require the FIMS schema). All values and the
/// file path are XML-escaped. Previously this returned an empty shell that
/// ignored its inputs.
pub fn to_fims(streams: &StreamCollection, file_path: &str) -> String {
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str("<fims:media xmlns:fims=\"http://www.fims.tv/schemas/2013\" version=\"1.0\">\n");
    out.push_str(&format!("<fims:location>{}</fims:location>\n", crate::xml_escape(file_path)));
    for kind in [StreamKind::General, StreamKind::Video, StreamKind::Audio] {
        for p in 0..streams.stream_count(kind) {
            if let Some(s) = streams.stream(kind, p) {
                out.push_str(&format!("<fims:format type=\"{}\">\n", kind.name()));
                for (k, v) in s.iter() {
                    out.push_str(&format!(
                        "<fims:{key}>{val}</fims:{key}>\n",
                        key = k,
                        val = crate::xml_escape(v.as_str())
                    ));
                }
                out.push_str("</fims:format>\n");
            }
        }
    }
    out.push_str("</fims:media>\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use revelo_util::Ztring;

    #[test]
    fn emits_stream_fields_and_escapes() {
        let mut c = StreamCollection::new();
        c.set_field(StreamKind::Video, 0, "Format", Ztring::from("VVC <test>"));
        let xml = to_fims(&c, "/x.mp4");
        assert!(xml.contains("fims:media"));
        assert!(xml.contains("<fims:Format>VVC &lt;test&gt;</fims:Format>"));
    }
}
