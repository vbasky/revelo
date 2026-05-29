//! CSV output — machine-readable format for pipeline integration.
//!
//! Each stream kind gets its own section with a header comment (`# General`,
//! `# Video`, etc.). The first data row is the field names, then one row per
//! stream position.

use revelo_core::{StreamCollection, StreamKind};

pub fn to_csv(streams: &StreamCollection, file_path: &str) -> String {
    let mut out = String::new();
    out.push_str("# ");
    out.push_str(file_path);
    out.push('\n');

    let kinds = [
        StreamKind::General,
        StreamKind::Video,
        StreamKind::Audio,
        StreamKind::Text,
        StreamKind::Other,
        StreamKind::Image,
        StreamKind::Menu,
    ];

    for kind in kinds {
        let count = streams.stream_count(kind);
        if count == 0 {
            continue;
        }

        // Collect all field names across all streams of this kind
        let mut fields: Vec<String> = Vec::new();
        for pos in 0..count {
            if let Some(stream) = streams.stream(kind, pos) {
                for (key, _) in stream.iter() {
                    if !fields.contains(&key.to_string()) {
                        fields.push(key.to_string());
                    }
                }
                for (key, _) in stream.extras_iter() {
                    let ek = format!("{key}");
                    if !fields.contains(&ek) {
                        fields.push(ek);
                    }
                }
            }
        }

        if fields.is_empty() {
            continue;
        }

        out.push_str(&format!("# {}\n", kind.name()));

        // Header row
        out.push_str("StreamIndex");
        for f in &fields {
            out.push(',');
            out.push_str(&csv_escape(f));
        }
        out.push('\n');

        // Data rows
        for pos in 0..count {
            out.push_str(&pos.to_string());
            if let Some(stream) = streams.stream(kind, pos) {
                for f in &fields {
                    out.push(',');
                    if let Some(v) = stream.get(f) {
                        out.push_str(&csv_escape(v.as_str()));
                    } else if stream.extras_iter().any(|(k, _)| k == f) {
                        let v = stream
                            .extras_iter()
                            .find(|(k, _)| k == f)
                            .map(|(_, v)| v.as_str())
                            .unwrap_or("");
                        out.push_str(&csv_escape(v));
                    }
                }
            }
            out.push('\n');
        }

        out.push('\n');
    }

    out
}

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        let mut escaped = String::with_capacity(s.len() + 2);
        escaped.push('"');
        for ch in s.chars() {
            if ch == '"' {
                escaped.push_str("\"\"");
            } else {
                escaped.push(ch);
            }
        }
        escaped.push('"');
        escaped
    } else {
        s.to_owned()
    }
}
