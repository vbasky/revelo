//! Catch-all subtitle/text detector mirroring `File_OtherText` in
//! MediaInfoLib (`Source/MediaInfo/Text/File_OtherText.cpp`).
//!
//! The C++ implementation decodes the first 64 KiB as UTF-8 (falling back
//! to local/ISO-8859-1, then UTF-16), normalises line endings, and runs a
//! cascade of pattern checks against the first ~32 lines to identify a
//! number of obscure subtitle and captioning formats (SSA, ASS, SAMI,
//! Adobe encore DVD, AQTitle, Captions 32, Captions Inc, Cheeta, CPC
//! Captioning, plus the common MicroDVD/JACOsub/MPL2 line-based shapes
//! that other generic detectors also handle). This Rust port follows the
//! same ordering and only declares the General/Text Format when a match
//! is found — like its C++ counterpart, the parser is expected to sit at
//! the tail of the dispatch chain so real formats are never overridden.

use revelio_core::{FileAnalyze, StreamKind};

const SCAN_WINDOW: usize = 65536;
const MAX_LINES: usize = 32;

struct Match {
    format: &'static str,
    format_info: Option<&'static str>,
    codec: &'static str,
}

pub fn parse_other_text(fa: &mut FileAnalyze) -> bool {
    // C++ requires Buffer_Size>=0x200 before attempting detection; mirror
    // that to avoid false positives on tiny fragments.
    if fa.Remain() < 0x200 {
        return false;
    }

    let window = SCAN_WINDOW.min(fa.Remain());
    let Some(buf) = fa.peek_raw(window) else {
        return false;
    };

    let text = match std::str::from_utf8(buf) {
        Ok(s) => s,
        Err(e) => {
            // Keep the valid prefix; the C++ path tries non-UTF8 decoders
            // afterwards but the line-based heuristics only ever look at
            // ASCII tokens, so the valid prefix is sufficient in practice.
            std::str::from_utf8(&buf[..e.valid_up_to()]).unwrap_or("")
        }
    };

    if text.len() < 0x100 {
        return false;
    }

    let normalised = text.replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<&str> = normalised.split('\n').take(MAX_LINES).collect();
    let line0 = lines.first().copied().unwrap_or("");
    let line1 = lines.get(1).copied().unwrap_or("");
    let line2 = lines.get(2).copied().unwrap_or("");

    let m = detect(&lines, line0, line1, line2);
    let Some(m) = m else {
        return false;
    };

    fa.Stream_Prepare(StreamKind::General);
    fa.Fill(StreamKind::General, 0, "Format", m.format, true);
    if let Some(info) = m.format_info {
        fa.Fill(StreamKind::General, 0, "Format_Info", info, true);
    }

    fa.Stream_Prepare(StreamKind::Text);
    fa.Fill(StreamKind::Text, 0, "Format", m.format, true);
    fa.Fill(StreamKind::Text, 0, "Codec", m.codec, true);
    true
}

fn detect(lines: &[&str], line0: &str, line1: &str, line2: &str) -> Option<Match> {
    let is_script_info = line0 == "[Script Info]";

    if is_script_info
        && (lines.iter().any(|l| *l == "ScriptType: v4.00")
            || lines.iter().any(|l| *l == "Script Type: V4.00"))
        && lines.iter().any(|l| *l == "[V4 Styles]")
    {
        return Some(Match {
            format: "SSA",
            format_info: Some("SubStation Alpha"),
            codec: "SSA",
        });
    }
    if is_script_info
        && (lines.iter().any(|l| *l == "ScriptType: v4.00+")
            || lines.iter().any(|l| *l == "Script Type: V4.00+"))
        && lines.iter().any(|l| *l == "[V4+ Styles]")
    {
        return Some(Match {
            format: "ASS",
            format_info: Some("Advanced SubStation Alpha"),
            codec: "ASS",
        });
    }

    if matches_timecode_pair(line0, b' ') {
        return Some(Match {
            format: "Adobe encore DVD",
            format_info: None,
            codec: "Adobe",
        });
    }

    let l0b = line0.as_bytes();
    if l0b.len() == 11
        && l0b[0] == b'-'
        && l0b[1] == b'-'
        && l0b[2] == b'>'
        && l0b[3] == b'>'
        && l0b[4] == b' '
        && l0b[5] == b'0'
        && !line1.is_empty()
    {
        return Some(Match { format: "AQTitle", format_info: None, codec: "AQTitle" });
    }

    if matches_captions32(line0) {
        return Some(Match {
            format: "Captions 32",
            format_info: None,
            codec: "Caption 32",
        });
    }

    if line0 == "*Timecode type: PAL/EBU"
        && line1.is_empty()
        && matches_timecode_pair(line2, b' ')
    {
        return Some(Match {
            format: "Captions Inc",
            format_info: None,
            codec: "Captions Inc",
        });
    }

    if line0.starts_with('*')
        && line0.len() > 1
        && lines.iter().any(|l| *l == "** Caption Number 1")
    {
        return Some(Match { format: "Cheeta", format_info: None, codec: "Cheeta" });
    }

    if l0b.len() > 10
        && l0b[0] == b'~'
        && l0b[1] == b'C'
        && l0b[2] == b'P'
        && l0b[3] == b'C'
        && l0b[9] == b'~'
        && line1.len() > 8
    {
        let l1b = line1.as_bytes();
        if l1b[0] == b'0' && l1b[1] == b'0' && l1b[2] == b':' && l1b[5] == b':' && l1b[8] == b':' {
            return Some(Match {
                format: "CPC Captioning",
                format_info: None,
                codec: "CPC Captioning",
            });
        }
    }

    if line0.starts_with("<SAMI>") {
        return Some(Match { format: "SAMI", format_info: None, codec: "SAMI" });
    }

    None
}

fn matches_timecode_pair(line: &str, separator: u8) -> bool {
    let b = line.as_bytes();
    b.len() > 23
        && b[0] == b'0'
        && b[1] == b'0'
        && b[2] == b':'
        && b[5] == b':'
        && b[8] == b':'
        && b[11] == separator
        && b[12] == b'0'
        && b[13] == b'0'
        && b[14] == b':'
        && b[17] == b':'
        && b[20] == b':'
}

fn matches_captions32(line: &str) -> bool {
    let b = line.as_bytes();
    b.len() > 28
        && b[0] == b'0'
        && b[1] == b'0'
        && b[2] == b':'
        && b[5] == b':'
        && b[8] == b':'
        && b[11] == b' '
        && b[12] == b','
        && b[13] == b' '
        && b[14] == b'0'
        && b[15] == b'0'
        && b[16] == b':'
        && b[19] == b':'
        && b[22] == b':'
        && b[25] == b' '
        && b[27] == b' '
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pad(s: &str) -> Vec<u8> {
        // Tests need >=0x200 bytes of buffer AND >=0x100 chars of decoded
        // text to clear the C++ size guards.
        let mut v = s.as_bytes().to_vec();
        while v.len() < 0x300 {
            v.push(b' ');
        }
        v
    }

    #[test]
    fn detects_sami() {
        let buf = pad("<SAMI>\n<HEAD></HEAD>\n<BODY></BODY>\n</SAMI>\n");
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_other_text(&mut fa));
        let g = |k: &str| fa.Retrieve(StreamKind::General, 0, k).map(|z| z.as_str().to_owned());
        let t = |k: &str| fa.Retrieve(StreamKind::Text, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(g("Format").as_deref(), Some("SAMI"));
        assert_eq!(t("Format").as_deref(), Some("SAMI"));
        assert_eq!(t("Codec").as_deref(), Some("SAMI"));
    }

    #[test]
    fn detects_ssa() {
        let buf = pad("[Script Info]\nScriptType: v4.00\n[V4 Styles]\nFormat: Name\n");
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_other_text(&mut fa));
        let t = |k: &str| fa.Retrieve(StreamKind::Text, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(t("Format").as_deref(), Some("SSA"));
        assert_eq!(t("Codec").as_deref(), Some("SSA"));
    }

    #[test]
    fn rejects_unknown_text() {
        let buf = pad("hello world\nthis is not subtitles\n");
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_other_text(&mut fa));
    }

    #[test]
    fn rejects_short_buffer() {
        let mut fa = FileAnalyze::new(b"<SAMI>");
        assert!(!parse_other_text(&mut fa));
    }
}
