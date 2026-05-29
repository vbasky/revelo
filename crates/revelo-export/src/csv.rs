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

#[cfg(test)]
mod tests {
    use super::*;
    use revelo_util::Ztring;

    fn stream_with(kind: StreamKind, fields: &[(&str, &str)]) -> StreamCollection {
        let mut c = StreamCollection::new();
        for (k, v) in fields {
            c.set_field(kind, 0, k, Ztring::from(*v));
        }
        c
    }

    #[test]
    fn csv_includes_file_path_comment() {
        let c = stream_with(StreamKind::General, &[("Format", "MPEG-4")]);
        let out = to_csv(&c, "/tmp/test.mp4");
        assert!(out.starts_with("# /tmp/test.mp4\n"), "{out}");
    }

    #[test]
    fn csv_emits_header_and_data_rows() {
        let mut c = StreamCollection::new();
        c.set_field(StreamKind::General, 0, "Format", "MPEG-4");
        c.set_field(StreamKind::Video, 0, "Format", "AVC");
        c.set_field(StreamKind::Video, 0, "Width", "1920");
        c.set_field(StreamKind::Video, 0, "Height", "1080");
        let out = to_csv(&c, "f.mp4");
        assert!(out.contains("# General\n"));
        assert!(out.contains("# Video\n"));
        assert!(out.contains("StreamIndex"));
        assert!(out.contains("1920"));
        assert!(out.contains("1080"));
    }

    #[test]
    fn csv_escapes_commas() {
        let mut c = StreamCollection::new();
        c.set_field(StreamKind::General, 0, "Format", "MPEG-4");
        c.set_field(StreamKind::General, 0, "Title", "Video, Part 1");
        let out = to_csv(&c, "f.mp4");
        assert!(out.contains("\"Video, Part 1\""));
    }

    #[test]
    fn csv_empty_streams_produce_empty_output() {
        let c = stream_with(StreamKind::Video, &[]);
        let out = to_csv(&c, "f.mp4");
        // No fields to emit — only the file path header
        assert!(out.starts_with("# f.mp4\n"));
        assert_eq!(out.lines().count(), 1);
    }

    #[test]
    fn csv_handles_multiple_streams_per_kind() {
        let mut c = StreamCollection::new();
        c.set_field(StreamKind::Audio, 0, "Format", "AAC");
        c.set_field(StreamKind::Audio, 1, "Format", "AC3");
        c.set_field(StreamKind::Audio, 1, "Channels", "6");
        let out = to_csv(&c, "f.mp4");
        assert!(out.contains("0,AAC"));
        assert!(out.contains("1,AC3"));
    }
}
