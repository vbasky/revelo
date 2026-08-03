//! YAML output — pipeline-friendly serialization of the parsed streams.
//!
//! Mirrors the [`crate::json`] structure (a `media` mapping with a `track`
//! sequence) and reuses the shared field ordering/value rendering helpers
//! from [`crate::xml`] so all formatters stay consistent.
//!
//! Layout (block style, 2-space indent):
//! ```text
//! media:
//!   "@ref": "PATH"
//!   track:
//!     - "@type": "General"
//!       "Format": "MPEG-4"
//!       …
//!       extra:
//!         "key": "value"
//!     - "@type": "Video"
//!       …
//! ```
//! Every key and every scalar value is emitted as a YAML double-quoted
//! scalar. Quoting unconditionally sidesteps all of YAML's plain-scalar
//! ambiguities: keys begin with `@`, and values may contain `:`, `#`,
//! leading spaces, etc.

use revelo_core::{Stream, StreamCollection, StreamKind};

use crate::xml::{canonical_field_order, extra_field_order, render_field_value};

/// Serialize the parsed stream collection as YAML.
pub fn to_yaml(streams: &StreamCollection, file_path: &str) -> String {
    let mut out = String::new();
    out.push_str("media:\n");
    out.push_str("  \"@ref\": ");
    out.push_str(&yaml_escape(file_path));
    out.push('\n');

    let kinds = [
        StreamKind::General,
        StreamKind::Video,
        StreamKind::Audio,
        StreamKind::Text,
        StreamKind::Other,
        StreamKind::Image,
        StreamKind::Menu,
        StreamKind::Exif,
        StreamKind::Iptc,
        StreamKind::Xmp,
        StreamKind::Icc,
        StreamKind::C2pa,
        StreamKind::MakerNotes,
    ];

    // Buffer the track items first so we can decide between `track:` with
    // nested items and the empty-sequence `track: []` shell.
    let mut items = String::new();
    for kind in kinds {
        let count = streams.stream_count(kind);
        for pos in 0..count {
            let Some(stream) = streams.stream(kind, pos) else { continue };
            emit_track(&mut items, kind, stream);
        }
    }

    if items.is_empty() {
        out.push_str("  track: []\n");
    } else {
        out.push_str("  track:\n");
        out.push_str(&items);
    }
    out
}

/// Append one YAML sequence item for a single stream. The item sits at
/// indent level 2 (4 spaces): the leading `- ` introduces the mapping and
/// every subsequent mapping key aligns under the first key (6 spaces).
fn emit_track(out: &mut String, kind: StreamKind, stream: &Stream) {
    // First mapping entry rides on the sequence dash.
    out.push_str("    - \"@type\": ");
    out.push_str(&yaml_escape(kind.name()));
    out.push('\n');

    // Standard fields in canonical order, then any non-canonical,
    // non-extra fields in insertion order — same selection as json.rs.
    let canonical = canonical_field_order(kind);
    let extras = extra_field_order(kind);
    let extras_set: std::collections::HashSet<&'static str> = extras.iter().copied().collect();
    let mut emitted: std::collections::HashSet<&str> = std::collections::HashSet::new();

    for field in canonical {
        if let Some(z) = stream.get(field) {
            emit_field(out, field, &render_field_value(field, z.as_str()));
            emitted.insert(*field);
        }
    }
    for (k, v) in stream.iter() {
        if !emitted.contains(k) && !extras_set.contains(k) {
            emit_field(out, k, &render_field_value(k, v.as_str()));
        }
    }

    // Extra block — a nested mapping under `extra:`, emitted only when at
    // least one extra field is present. Collected exactly like json.rs.
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
        out.push_str("      extra:\n");
        for (k, v) in &extra_pairs {
            out.push_str("        ");
            out.push_str(&yaml_escape(k));
            out.push_str(": ");
            out.push_str(&yaml_escape(v));
            out.push('\n');
        }
    }
}

/// Emit a `"key": "value"` mapping entry at the track item's key indent
/// (6 spaces).
fn emit_field(out: &mut String, key: &str, value: &str) {
    out.push_str("      ");
    out.push_str(&yaml_escape(key));
    out.push_str(": ");
    out.push_str(&yaml_escape(value));
    out.push('\n');
}

/// Render `s` as a YAML double-quoted scalar. Double-quoted style accepts
/// the widest set of characters and lets us escape everything that would
/// otherwise be significant, so the emitted key/value is always
/// unambiguous regardless of its content.
fn yaml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\x{:02x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use revelo_util::Ztring;

    #[test]
    fn yaml_includes_ref_path() {
        let c = StreamCollection::new();
        let y = to_yaml(&c, "/tmp/test.mp4");
        assert!(y.starts_with("media:\n"), "{y}");
        assert!(y.contains("  \"@ref\": \"/tmp/test.mp4\"\n"), "{y}");
    }

    #[test]
    fn yaml_empty_collection_emits_empty_track_sequence() {
        let c = StreamCollection::new();
        let y = to_yaml(&c, "/x");
        assert!(y.contains("  track: []\n"), "{y}");
        assert!(!y.contains("\"@type\""), "{y}");
    }

    #[test]
    fn yaml_emits_general_and_video_tracks_with_values() {
        let mut c = StreamCollection::new();
        c.set_field(StreamKind::General, 0, "Format", Ztring::from("MPEG-4"));
        c.set_field(StreamKind::Video, 0, "Format", Ztring::from("AVC"));
        c.set_field(StreamKind::Video, 0, "Width", Ztring::from("1920"));
        let y = to_yaml(&c, "/x");
        assert!(y.contains("  track:\n"), "{y}");
        assert!(y.contains("    - \"@type\": \"General\"\n"), "{y}");
        assert!(y.contains("    - \"@type\": \"Video\"\n"), "{y}");
        assert!(y.contains("      \"Format\": \"MPEG-4\"\n"), "{y}");
        assert!(y.contains("      \"Width\": \"1920\"\n"), "{y}");
    }

    #[test]
    fn yaml_escapes_quotes_and_colons() {
        let mut c = StreamCollection::new();
        c.set_field(StreamKind::General, 0, "Title", Ztring::from("A \"B\": C"));
        let y = to_yaml(&c, "/x");
        // The embedded quote is backslash-escaped; the colon stays inside
        // the double-quoted scalar, so it cannot break the mapping.
        assert!(y.contains("\"Title\": \"A \\\"B\\\": C\"\n"), "{y}");
    }

    #[test]
    fn yaml_escapes_control_chars() {
        let mut c = StreamCollection::new();
        c.set_field(StreamKind::General, 0, "Format", Ztring::from("a\nb\tc"));
        let y = to_yaml(&c, "/x");
        assert!(y.contains("\"Format\": \"a\\nb\\tc\"\n"), "{y}");
    }

    #[test]
    fn yaml_extra_block_nests_under_extra_mapping() {
        let mut c = StreamCollection::new();
        c.set_field(StreamKind::General, 0, "Format", Ztring::from("MPEG Audio"));
        c.set_extra_field(StreamKind::General, 0, "comment", Ztring::from("hi"));
        let y = to_yaml(&c, "/x");
        assert!(y.contains("      extra:\n"), "{y}");
        assert!(y.contains("        \"comment\": \"hi\"\n"), "{y}");
    }

    #[test]
    fn yaml_renders_duration_as_seconds() {
        let mut c = StreamCollection::new();
        c.set_field(StreamKind::Audio, 0, "Duration", Ztring::from("209831"));
        let y = to_yaml(&c, "/x");
        assert!(y.contains("\"Duration\": \"209.831\"\n"), "{y}");
    }
}
