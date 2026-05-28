//! JSON output — matches `mediainfo --Output=JSON` byte structure.
//!
//! Layout (mirrors MediaInfoLib):
//! ```text
//! {
//! "creatingLibrary":{"name":"MediaInfoLib","version":"X","url":"…"},
//! "media":{"@ref":"PATH","track":[{"@type":"General","F0":"v0",
//! "F1":"v1",
//! …
//! "extra":{"k":"v"}},{"@type":"Audio",…}]}
//! }
//! ```
//! Field order + value rendering are shared with the XML formatter
//! (`crate::xml::{canonical_field_order, extra_field_order,
//! render_field_value}`) so the two stay consistent.

use revelio_core::{Stream, StreamCollection, StreamKind};

use crate::xml::{canonical_field_order, extra_field_order, render_field_value};

pub fn to_json(streams: &StreamCollection, file_path: &str, library_version: &str) -> String {
    let mut out = String::new();
    out.push_str("{\n");
    out.push_str(&format!(
        "\"creatingLibrary\":{{\"name\":\"MediaInfoLib\",\"version\":\"{}\",\"url\":\"https://mediaarea.net/MediaInfo\"}},\n",
        json_escape(library_version)
    ));
    out.push_str("\"media\":{\"@ref\":\"");
    out.push_str(&json_escape(file_path));
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
    let mut first_track = true;
    for kind in kinds {
        let count = streams.count_get(kind);
        for pos in 0..count {
            let Some(stream) = streams.stream(kind, pos) else { continue };
            if !first_track {
                out.push(',');
            }
            first_track = false;
            emit_track(&mut out, kind, stream);
        }
    }

    out.push_str("]}\n}\n");
    out
}

fn emit_track(out: &mut String, kind: StreamKind, stream: &Stream) {
    out.push_str("{\"@type\":\"");
    out.push_str(kind.name());
    out.push('"');

    // Standard fields in canonical order, then any non-canonical,
    // non-extra fields in insertion order — same selection as XML.
    let canonical = canonical_field_order(kind);
    let extras = extra_field_order(kind);
    let extras_set: std::collections::HashSet<&'static str> = extras.iter().copied().collect();
    let mut emitted: std::collections::HashSet<&str> = std::collections::HashSet::new();

    // `field_idx` drives the separator: the first field after @type is
    // joined with just ",", every later field with ",\n" (oracle style).
    let mut field_idx = 0usize;
    for field in canonical {
        if let Some(z) = stream.get(field) {
            emit_field(out, &mut field_idx, field, &render_field_value(field, z.as_str()));
            emitted.insert(*field);
        }
    }
    for (k, v) in stream.iter() {
        if !emitted.contains(k) && !extras_set.contains(k) {
            emit_field(out, &mut field_idx, k, &render_field_value(k, v.as_str()));
        }
    }

    // Extra block — a nested object, compact (no inner newlines).
    let mut extra_pairs: Vec<(String, String)> = Vec::new();
    for field in extras {
        if let Some(z) = stream.get(field) {
            extra_pairs.push((field.to_string(), render_field_value(field, z.as_str())));
        }
    }
    for (k, v) in stream.extras_iter() {
        extra_pairs.push((k.to_string(), render_field_value(k, v.as_str())));
    }
    if !extra_pairs.is_empty() {
        // "extra" is positioned like another field (own ",\n" prefix).
        out.push(',');
        if field_idx > 0 {
            out.push('\n');
        }
        out.push_str("\"extra\":{");
        for (i, (k, v)) in extra_pairs.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push('"');
            out.push_str(&json_escape(k));
            out.push_str("\":\"");
            out.push_str(&json_escape(v));
            out.push('"');
        }
        out.push('}');
    }

    out.push('}');
}

fn emit_field(out: &mut String, field_idx: &mut usize, key: &str, value: &str) {
    out.push(',');
    if *field_idx > 0 {
        out.push('\n');
    }
    *field_idx += 1;
    out.push('"');
    out.push_str(&json_escape(key));
    out.push_str("\":\"");
    out.push_str(&json_escape(value));
    out.push('"');
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str(r"\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use zenlib::Ztring;

    #[test]
    fn json_has_creating_library_header() {
        let c = StreamCollection::new();
        let j = to_json(&c, "/x", "26.05");
        assert!(j.starts_with("{\n\"creatingLibrary\":{\"name\":\"MediaInfoLib\",\"version\":\"26.05\""), "{j}");
        assert!(j.contains("\"media\":{\"@ref\":\"/x\",\"track\":[]}\n}"), "{j}");
    }

    #[test]
    fn json_emits_canonical_order_and_pretty_fields() {
        let mut c = StreamCollection::new();
        c.fill(StreamKind::General, 0, "Format", Ztring::from("MPEG-4"), false);
        c.fill(StreamKind::General, 0, "FileSize", Ztring::from("100"), false);
        let j = to_json(&c, "/x", "1.0");
        // first field right after @type (comma, no newline); later fields on own lines
        assert!(j.contains("{\"@type\":\"General\",\"Format\":\"MPEG-4\",\n\"FileSize\":\"100\"}"), "{j}");
    }

    #[test]
    fn json_renders_duration_as_seconds() {
        let mut c = StreamCollection::new();
        c.fill(StreamKind::Audio, 0, "Duration", Ztring::from("209831"), false);
        let j = to_json(&c, "/x", "1.0");
        assert!(j.contains("\"Duration\":\"209.831\""), "{j}");
    }

    #[test]
    fn json_extra_block_is_compact() {
        let mut c = StreamCollection::new();
        c.fill(StreamKind::General, 0, "Format", Ztring::from("MPEG Audio"), false);
        c.fill_extra(StreamKind::General, 0, "comment", Ztring::from("hi"), false);
        let j = to_json(&c, "/x", "1.0");
        assert!(j.contains("\"extra\":{\"comment\":\"hi\"}"), "{j}");
    }

    #[test]
    fn json_escapes_quotes() {
        let mut c = StreamCollection::new();
        c.fill(StreamKind::General, 0, "Format", Ztring::from("A \"B\""), false);
        let j = to_json(&c, "/x", "1.0");
        assert!(j.contains("A \\\"B\\\""), "{j}");
    }
}
