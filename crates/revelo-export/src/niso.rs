use revelo_core::{StreamCollection, StreamKind};

/// NISO Z39.87 (MIX) technical-metadata export.
///
/// This is a **pragmatic, well-formed** rendering of the parsed stream fields,
/// not a schema-validated MIX document: revelo's field names are emitted as
/// namespaced elements verbatim rather than mapped onto the official MIX
/// vocabulary (that mapping would require the Z39.87 schema). All values and
/// the file path are XML-escaped. Previously this returned an empty shell that
/// ignored its inputs.
pub fn to_niso(streams: &StreamCollection, file_path: &str) -> String {
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str("<niso:metadata xmlns:niso=\"http://www.niso.org/schemas/z39.87/metadata\">\n");
    out.push_str(&format!("<niso:identifier>{}</niso:identifier>\n", crate::xml_escape(file_path)));
    for kind in [StreamKind::General, StreamKind::Video, StreamKind::Audio] {
        for p in 0..streams.stream_count(kind) {
            if let Some(s) = streams.stream(kind, p) {
                out.push_str(&format!("<niso:stream type=\"{}\">\n", kind.name()));
                for (k, v) in s.iter() {
                    out.push_str(&format!(
                        "<niso:{key}>{val}</niso:{key}>\n",
                        key = k,
                        val = crate::xml_escape(v.as_str())
                    ));
                }
                out.push_str("</niso:stream>\n");
            }
        }
    }
    out.push_str("</niso:metadata>\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use revelo_util::Ztring;

    #[test]
    fn emits_stream_fields_and_escapes() {
        let mut c = StreamCollection::new();
        c.set_field(StreamKind::General, 0, "Format", Ztring::from("MP4 & more"));
        let xml = to_niso(&c, "/x.mp4");
        assert!(xml.contains("niso:metadata"));
        assert!(xml.contains("<niso:Format>MP4 &amp; more</niso:Format>"));
        assert!(xml.contains("/x.mp4"));
    }
}
