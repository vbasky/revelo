//! HTML report — self-contained visual report with per-stream collapsible
//! sections and summary cards. No external assets (inline CSS, no
//! JavaScript — collapsibility uses native `<details>`/`<summary>`), so the
//! output is a single portable `.html` file.
//!
//! Field selection and value rendering are shared with the other
//! formatters (`crate::xml::{canonical_field_order, extra_field_order,
//! render_field_value}`) so displayed values stay consistent across
//! JSON/XML/CSV/HTML.

use revelo_core::{Stream, StreamCollection, StreamKind};

use crate::xml::{canonical_field_order, extra_field_order, render_field_value};

const KINDS: [StreamKind; 13] = [
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

const STYLE: &str = "\
:root{color-scheme:light dark}
*{box-sizing:border-box}
body{margin:0;padding:1.5rem;font-family:system-ui,-apple-system,'Segoe UI',Roboto,sans-serif;\
line-height:1.5;color:#1a1a1a;background:#f6f7f9}
h1{font-size:1.35rem;margin:0 0 .25rem;word-break:break-all}
h2{font-size:1.05rem;margin:1.5rem 0 .5rem}
.sub{color:#666;font-size:.85rem;margin:0 0 1.25rem}
.cards{display:flex;flex-wrap:wrap;gap:.75rem;margin:0 0 1.5rem}
.card{flex:1 1 180px;min-width:160px;background:#fff;border:1px solid #e2e5e9;border-radius:8px;\
padding:.75rem .9rem;box-shadow:0 1px 2px rgba(0,0,0,.04)}
.card h3{margin:0 0 .35rem;font-size:.95rem}
.card .count{color:#666;font-size:.8rem;font-weight:normal}
.card dl{margin:0;display:grid;grid-template-columns:auto 1fr;gap:.15rem .5rem;font-size:.82rem}
.card dt{color:#666}
.card dd{margin:0;text-align:right;word-break:break-word}
details{background:#fff;border:1px solid #e2e5e9;border-radius:8px;margin:0 0 .6rem;overflow:hidden}
summary{cursor:pointer;padding:.6rem .9rem;font-weight:600;user-select:none}
summary:hover{background:#f0f2f5}
.tblwrap{overflow-x:auto;padding:0 .9rem .6rem}
table{border-collapse:collapse;width:100%;font-size:.85rem}
th,td{text-align:left;padding:.3rem .5rem;border-bottom:1px solid #eceef1;vertical-align:top}
th{width:32%;color:#444;font-weight:600;word-break:break-word}
td{word-break:break-word;white-space:pre-wrap}
.extra-hd td{background:#f6f7f9;font-weight:600;color:#555}
@media (prefers-color-scheme:dark){
body{color:#e6e6e6;background:#16181c}
.card,details{background:#1f2228;border-color:#33373f;box-shadow:none}
.sub,.card .count,.card dt,th{color:#9aa0a8}
summary:hover{background:#272b32}
th,td{border-bottom-color:#2c3037}
.extra-hd td{background:#23272e;color:#c2c7cf}
}";

/// Render a self-contained HTML report for the parsed stream collection.
pub fn to_html(streams: &StreamCollection, file_path: &str) -> String {
    let escaped_path = html_escape(file_path);
    let mut out = String::new();

    out.push_str("<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n");
    out.push_str("<meta charset=\"utf-8\">\n");
    out.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    out.push_str("<title>revelo report — ");
    out.push_str(&escaped_path);
    out.push_str("</title>\n<style>\n");
    out.push_str(STYLE);
    out.push_str("\n</style>\n</head>\n<body>\n");

    out.push_str("<h1>");
    out.push_str(&escaped_path);
    out.push_str("</h1>\n");
    out.push_str("<p class=\"sub\">Media analysis report</p>\n");

    // Summary cards — one per present stream kind.
    out.push_str("<div class=\"cards\">\n");
    for kind in KINDS {
        let count = streams.stream_count(kind);
        if count == 0 {
            continue;
        }
        emit_card(&mut out, kind, count, streams);
    }
    out.push_str("</div>\n");

    // Per-stream collapsible detail sections.
    out.push_str("<h2>Streams</h2>\n");
    for kind in KINDS {
        let count = streams.stream_count(kind);
        for pos in 0..count {
            let Some(stream) = streams.stream(kind, pos) else { continue };
            emit_details(&mut out, kind, pos, count, stream);
        }
    }

    out.push_str("</body>\n</html>\n");
    out
}

/// Emit a summary card for a stream kind, showing 1-3 key facts pulled from
/// the first stream of that kind (only facts that exist).
fn emit_card(out: &mut String, kind: StreamKind, count: usize, streams: &StreamCollection) {
    out.push_str("<div class=\"card\">\n<h3>");
    out.push_str(&html_escape(kind.name()));
    out.push_str(" <span class=\"count\">×");
    out.push_str(&count.to_string());
    out.push_str("</span></h3>\n");

    let facts = key_facts(kind, streams.stream(kind, 0));
    if !facts.is_empty() {
        out.push_str("<dl>\n");
        for (label, value) in facts {
            out.push_str("<dt>");
            out.push_str(&html_escape(label));
            out.push_str("</dt><dd>");
            out.push_str(&html_escape(&value));
            out.push_str("</dd>\n");
        }
        out.push_str("</dl>\n");
    }
    out.push_str("</div>\n");
}

/// Pick 1-3 key facts to headline a kind's card. Only present fields are
/// returned; values are rendered through the shared value transform.
fn key_facts(kind: StreamKind, stream: Option<&Stream>) -> Vec<(&'static str, String)> {
    let Some(stream) = stream else { return Vec::new() };
    let mut facts: Vec<(&'static str, String)> = Vec::new();
    let push = |facts: &mut Vec<(&'static str, String)>, label: &'static str, field: &str| {
        if let Some(z) = stream.get(field) {
            facts.push((label, render_field_value(field, z.as_str())));
        }
    };

    match kind {
        StreamKind::General => {
            push(&mut facts, "Format", "Format");
            // Prefer a humanized size string when the parser provides one.
            if let Some(z) = stream.get("FileSize_String") {
                facts.push(("File size", z.as_str().to_string()));
            } else {
                push(&mut facts, "File size", "FileSize");
            }
            push(&mut facts, "Duration", "Duration");
        }
        StreamKind::Video => {
            push(&mut facts, "Format", "Format");
            if let (Some(w), Some(h)) = (stream.get("Width"), stream.get("Height")) {
                facts.push(("Resolution", format!("{} × {}", w.as_str(), h.as_str())));
            }
            push(&mut facts, "Frame rate", "FrameRate");
        }
        StreamKind::Audio => {
            push(&mut facts, "Format", "Format");
            push(&mut facts, "Channels", "Channels");
            push(&mut facts, "Sampling rate", "SamplingRate");
        }
        _ => {
            push(&mut facts, "Format", "Format");
        }
    }
    facts
}

/// Emit one collapsible `<details>` block for a single stream, containing a
/// two-column table of every field (canonical, then non-canonical
/// non-extra, then extras in a labeled sub-section).
fn emit_details(out: &mut String, kind: StreamKind, pos: usize, count: usize, stream: &Stream) {
    out.push_str("<details>\n<summary>");
    out.push_str(&html_escape(kind.name()));
    if count > 1 {
        out.push_str(" #");
        out.push_str(&(pos + 1).to_string());
    }
    out.push_str("</summary>\n<div class=\"tblwrap\">\n<table>\n");

    let canonical = canonical_field_order(kind);
    let extras = extra_field_order(kind);
    let extras_set: std::collections::HashSet<&'static str> = extras.iter().copied().collect();
    let mut emitted: std::collections::HashSet<&str> = std::collections::HashSet::new();

    // Canonical fields in canonical order.
    for field in canonical {
        if let Some(z) = stream.get(field) {
            emit_row(out, field, &render_field_value(field, z.as_str()));
            emitted.insert(*field);
        }
    }
    // Non-canonical, non-extra fields in insertion order.
    for (k, v) in stream.iter() {
        if !emitted.contains(k) && !extras_set.contains(k) {
            emit_row(out, k, &render_field_value(k, v.as_str()));
        }
    }

    // Extra fields, collected the same way the other formatters do.
    let mut extra_rows: Vec<(String, String)> = Vec::new();
    for field in extras {
        if let Some(z) = stream.get(field) {
            extra_rows.push((field.to_string(), render_field_value(field, z.as_str())));
        }
    }
    for (k, v) in stream.extras_iter() {
        extra_rows.push((k.to_string(), render_field_value(k, v.as_str())));
    }
    if !extra_rows.is_empty() {
        out.push_str("<tr class=\"extra-hd\"><td colspan=\"2\">extra</td></tr>\n");
        for (k, v) in &extra_rows {
            emit_row(out, k, v);
        }
    }

    out.push_str("</table>\n</div>\n</details>\n");
}

fn emit_row(out: &mut String, name: &str, value: &str) {
    out.push_str("<tr><th>");
    out.push_str(&html_escape(name));
    out.push_str("</th><td>");
    out.push_str(&html_escape(value));
    out.push_str("</td></tr>\n");
}

fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use revelo_util::Ztring;

    fn build_avc_streams() -> StreamCollection {
        let mut c = StreamCollection::new();
        c.set_field(StreamKind::General, 0, "Format", Ztring::from("MPEG-4"));
        c.set_field(StreamKind::General, 0, "FileSize", Ztring::from("1048576"));
        c.set_field(StreamKind::General, 0, "Duration", Ztring::from("60000"));
        c.set_field(StreamKind::Video, 0, "Format", Ztring::from("AVC"));
        c.set_field(StreamKind::Video, 0, "Width", Ztring::from("1920"));
        c.set_field(StreamKind::Video, 0, "Height", Ztring::from("1080"));
        c.set_field(StreamKind::Video, 0, "FrameRate", Ztring::from("25.000"));
        c.set_field(StreamKind::Audio, 0, "Format", Ztring::from("AAC"));
        c.set_field(StreamKind::Audio, 0, "Channels", Ztring::from("2"));
        c.set_field(StreamKind::Audio, 0, "SamplingRate", Ztring::from("48000"));
        c
    }

    #[test]
    fn html_is_a_full_document() {
        let c = StreamCollection::new();
        let h = to_html(&c, "/tmp/x.mp4");
        assert!(h.starts_with("<!DOCTYPE html>"), "{h}");
        assert!(h.contains("</html>"), "{h}");
        assert!(h.contains("<html lang=\"en\">"), "{h}");
        assert!(h.contains("<meta charset=\"utf-8\">"), "{h}");
    }

    #[test]
    fn html_contains_escaped_file_path() {
        let c = StreamCollection::new();
        let h = to_html(&c, "/tmp/a&b<c>.mp4");
        assert!(h.contains("/tmp/a&amp;b&lt;c&gt;.mp4"), "{h}");
        assert!(!h.contains("/tmp/a&b<c>.mp4"), "{h}");
    }

    #[test]
    fn html_emits_cards_and_details_and_values() {
        let c = build_avc_streams();
        let h = to_html(&c, "f.mp4");

        // A summary card per present kind.
        assert!(h.contains("<div class=\"card\">"), "{h}");
        assert!(h.contains(">General <span"), "{h}");
        assert!(h.contains(">Video <span"), "{h}");
        assert!(h.contains(">Audio <span"), "{h}");

        // Key facts on cards.
        assert!(h.contains("1920 × 1080"), "{h}");

        // One <details> per stream (3 streams here).
        assert_eq!(h.matches("<details>").count(), 3, "{h}");
        assert!(h.contains("<summary>Video</summary>"), "{h}");

        // Field values rendered in tables.
        assert!(h.contains("<td>1920</td>"), "{h}");
        assert!(h.contains("<td>1080</td>"), "{h}");
        // Duration goes through the shared seconds transform.
        assert!(h.contains("<td>60.000</td>"), "{h}");
    }

    #[test]
    fn html_escapes_field_values() {
        let mut c = StreamCollection::new();
        c.set_field(StreamKind::General, 0, "Title", Ztring::from("<script>x & y"));
        let h = to_html(&c, "f.mp4");
        assert!(h.contains("&lt;script&gt;x &amp; y"), "{h}");
        assert!(!h.contains("<script>x & y"), "{h}");
    }

    #[test]
    fn html_multiple_streams_get_numbered_summaries() {
        let mut c = StreamCollection::new();
        c.set_field(StreamKind::Audio, 0, "Format", Ztring::from("AAC"));
        c.set_field(StreamKind::Audio, 1, "Format", Ztring::from("AC-3"));
        let h = to_html(&c, "f.mp4");
        assert!(h.contains("<summary>Audio #1</summary>"), "{h}");
        assert!(h.contains("<summary>Audio #2</summary>"), "{h}");
    }
}
