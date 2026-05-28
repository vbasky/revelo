//! HLS (HTTP Live Streaming) m3u8 manifest parser.
//!
//! Mirrors `File_Hls::FileHeader_Begin` in MediaInfoLib. HLS manifests are
//! UTF-8 text files (RFC 8216) whose first non-empty line must be the
//! literal ASCII magic tag `#EXTM3U`. The C++ parser rejects files larger
//! than 1 MiB or smaller than 10 bytes, requires the entire file to be
//! buffered, then splits on the first observed line terminator
//! (`\r\n`, `\r`, or `\n`).
//!
//! Structure walked:
//!   line 0:    `#EXTM3U`                       (required magic)
//!   line N:    `#EXT-X-KEY:METHOD=AES-128,...` (sets encryption metadata)
//!   line N:    `#EXT-X-STREAM-INF:...`         (master playlist marker;
//!                                               next non-tag line is a
//!                                               variant URI → master)
//!   line N:    `<segment-uri>`                 (media playlist entry →
//!                                               Format_Profile = Media)
//!   any line starting with `#` other than the above is a comment/tag.
//!
//! General fields filled:
//!   - Format = "HLS"
//!   - Format_Profile = "Master" | "Media"
//!   - Encryption_* family when an `#EXT-X-KEY` line declares AES-128.
//!
//! Reference file enumeration (the C++ `ReferenceFiles` machinery) is not
//! implemented — the Rust engine does not (yet) follow external segment
//! references — but the master-vs-media classification matches C++.

use revelio_core::{FileAnalyze, StreamKind};

const MAGIC: &str = "#EXTM3U";
// C++ bound: HLS files must be <=1 MiB and >=10 bytes.
const MAX_SIZE: usize = 1024 * 1024;
const MIN_SIZE: usize = 10;

pub fn parse_hls(fa: &mut FileAnalyze) -> bool {
    let size = fa.Remain();
    if size < MIN_SIZE || size > MAX_SIZE {
        return false;
    }
    let Some(buf) = fa.peek_raw(size) else {
        return false;
    };

    // HLS manifests are UTF-8. Tolerate a UTF-8 BOM on the first line —
    // `#EXTM3U` itself is pure ASCII so the prefix check below is enough.
    // Own the text so the immutable borrow on `fa` ends before we mutate.
    let owned = match std::str::from_utf8(buf) {
        Ok(s) => s.to_owned(),
        Err(_) => return false,
    };
    // `buf` (an &[u8] borrow of fa's buffer) is no longer needed — the
    // owned String now holds the data, freeing fa for mutation below.
    let _ = buf;
    let text: &str = owned.strip_prefix('\u{FEFF}').unwrap_or(owned.as_str());

    // First line must equal exactly "#EXTM3U" (no trailing chars before
    // the line terminator). C++ uses ZtringList line splitting on the
    // first terminator found in the document.
    let first_line_end = text
        .find(|c| c == '\r' || c == '\n')
        .unwrap_or(text.len());
    let first_line = &text[..first_line_end];
    if first_line != MAGIC {
        return false;
    }

    fa.Stream_Prepare(StreamKind::General);
    fa.Fill(StreamKind::General, 0, "Format", "HLS", true);

    let mut is_master = false;
    let mut saw_segment = false;
    let mut pending_stream_inf = false;

    for raw in text.lines() {
        let line = raw.trim_end_matches('\r').trim();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("#EXT-X-KEY:") {
            parse_ext_x_key(fa, rest);
            pending_stream_inf = false;
        } else if line.starts_with("#EXT-X-STREAM-INF:") {
            // The URI on the next non-tag line identifies a variant
            // playlist → this manifest is a master.
            pending_stream_inf = true;
        } else if line.starts_with('#') {
            // Other tags / comments — ignored for classification.
        } else if pending_stream_inf {
            is_master = true;
            pending_stream_inf = false;
        } else {
            saw_segment = true;
        }
    }

    let profile = if saw_segment && !is_master {
        "Media"
    } else if is_master {
        "Master"
    } else {
        // No variants and no segments — C++ falls through to "Master".
        "Master"
    };
    fa.Fill(StreamKind::General, 0, "Format_Profile", profile, false);

    true
}

/// Parse an `#EXT-X-KEY:` tag's attribute list and fill encryption fields
/// when AES-128 is declared. Attribute list is a comma-separated set of
/// `KEY=VALUE` pairs per RFC 8216 §4.3.2.4 — quoted values are tolerated.
fn parse_ext_x_key(fa: &mut FileAnalyze, attrs: &str) {
    for attr in attrs.split(',') {
        let Some((key, value)) = attr.split_once('=') else { continue };
        let key = key.trim();
        let value = value.trim().trim_matches('"');
        if key == "METHOD" {
            if value.starts_with("AES-128") {
                // Match the exact set of fields C++ fills for AES-128.
                fa.Fill(StreamKind::General, 0, "Encryption_Format", "AES", false);
                fa.Fill(StreamKind::General, 0, "Encryption_Length", "128", false);
                fa.Fill(StreamKind::General, 0, "Encryption_Method", "Segment", false);
                fa.Fill(StreamKind::General, 0, "Encryption_Mode", "CBC", false);
                fa.Fill(StreamKind::General, 0, "Encryption_Padding", "PKCS7", false);
                fa.Fill(
                    StreamKind::General,
                    0,
                    "Encryption_InitializationVector",
                    "Sequence number",
                    false,
                );
            }
            fa.Fill(StreamKind::General, 0, "Encryption", value, false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_master_playlist() {
        let m3u8 = b"#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=1280000\nlow/index.m3u8\n";
        let mut fa = FileAnalyze::new(m3u8);
        assert!(parse_hls(&mut fa));
        let g = |k: &str| fa.Retrieve(StreamKind::General, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(g("Format").as_deref(), Some("HLS"));
        assert_eq!(g("Format_Profile").as_deref(), Some("Master"));
    }

    #[test]
    fn parses_media_playlist_with_segments() {
        let m3u8 = b"#EXTM3U\n#EXT-X-TARGETDURATION:10\n#EXTINF:9.009,\nseg1.ts\n#EXTINF:9.009,\nseg2.ts\n#EXT-X-ENDLIST\n";
        let mut fa = FileAnalyze::new(m3u8);
        assert!(parse_hls(&mut fa));
        let g = |k: &str| fa.Retrieve(StreamKind::General, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(g("Format").as_deref(), Some("HLS"));
        assert_eq!(g("Format_Profile").as_deref(), Some("Media"));
    }

    #[test]
    fn extracts_aes_128_encryption_fields() {
        let m3u8 = b"#EXTM3U\n#EXT-X-KEY:METHOD=AES-128,URI=\"key.bin\"\n#EXTINF:9,\na.ts\n";
        let mut fa = FileAnalyze::new(m3u8);
        assert!(parse_hls(&mut fa));
        let g = |k: &str| fa.Retrieve(StreamKind::General, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(g("Encryption_Format").as_deref(), Some("AES"));
        assert_eq!(g("Encryption_Length").as_deref(), Some("128"));
        assert_eq!(g("Encryption_Method").as_deref(), Some("Segment"));
        assert_eq!(g("Encryption_Mode").as_deref(), Some("CBC"));
        assert_eq!(g("Encryption_Padding").as_deref(), Some("PKCS7"));
        assert_eq!(g("Encryption_InitializationVector").as_deref(), Some("Sequence number"));
        assert_eq!(g("Format_Profile").as_deref(), Some("Media"));
    }

    #[test]
    fn rejects_non_hls_text() {
        // Magic must be on the very first line; comments before it are not allowed.
        let m3u8 = b"# this is not HLS\n#EXTM3U\n";
        let mut fa = FileAnalyze::new(m3u8);
        assert!(!parse_hls(&mut fa));
    }

    #[test]
    fn rejects_empty_buffer() {
        let mut fa = FileAnalyze::new(b"");
        assert!(!parse_hls(&mut fa));
    }
}
