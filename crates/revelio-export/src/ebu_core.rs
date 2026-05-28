use revelio_core::{StreamCollection, StreamKind};
pub fn to_ebu_core(streams: &StreamCollection, file_path: &str) -> String {
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str("<ebucore:ebuCoreMain xmlns:ebucore=\"urn:ebu:metadata-schema:ebuCore_2015\">\n");
    out.push_str("<ebucore:coreMetadata>\n");
    out.push_str(&format!("<ebucore:identifier>{file_path}</ebucore:identifier>\n"));
    for kind in [StreamKind::General, StreamKind::Video, StreamKind::Audio] {
        let c = streams.Count_Get(kind);
        for p in 0..c { if let Some(s) = streams.stream(kind, p) {
            out.push_str(&format!("<ebucore:format type=\"{}\">\n", kind.name()));
            for (k, v) in s.iter() { out.push_str(&format!("<ebucore:{}>{}</ebucore:{}>\n", k.to_lowercase().replace('_', ""), v.as_str(), k.to_lowercase().replace('_', ""))); }
            out.push_str("</ebucore:format>\n");
        }}
    }
    out.push_str("</ebucore:coreMetadata>\n</ebucore:ebuCoreMain>\n");
    out
}
#[cfg(test)] mod tests { use super::*; use zenlib::Ztring;
    #[test] fn test() { let mut c = StreamCollection::new(); c.Fill(StreamKind::General, 0, "Format", Ztring::from("MP4"), false); let xml = to_ebu_core(&c, "/x.mp4"); assert!(xml.contains("ebucore:ebuCoreMain")); assert!(xml.contains("/x.mp4")); }
}
