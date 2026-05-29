//! Ibi (MediaInfo Index) parser.
//!
//! MediaInfoLib's `File_Ibi.cpp` reuses the EBML container framing (same
//! root id as Matroska — 0x1A45DFA3) but is distinguished by a DocType
//! string of "MediaInfo Index". We only need enough to identify the
//! format and fill General.Format=Ibi; the rich index payload that the
//! C++ parser walks is irrelevant to MediaInfo output for now.

use revelo_core::{FileAnalyze, StreamKind};

const EBML_HEADER_BYTES: [u8; 4] = [0x1A, 0x45, 0xDF, 0xA3];
const DOC_TYPE: u64 = 0x4282;
const IBI_DOC_TYPE: &str = "MediaInfo Index";

/// Parse Index of Binary Information file.
/// Fills: Seek table metadata.
pub fn parse_ibi(fa: &mut FileAnalyze) -> bool {
    let head = fa.peek_raw(4);
    let Some(h) = head else { return false };
    if h != EBML_HEADER_BYTES {
        return false;
    }

    let file_size = fa.remain();
    let mut doc_type: Option<String> = None;

    walk_elements(fa, file_size, &mut |fa, id, size, _| {
        // EBML header id == 0x1A45DFA3 — walk its children to grab DocType.
        if id == 0x1A45DFA3 {
            walk_elements(fa, size, &mut |fa, id, sz, _| {
                if id == DOC_TYPE {
                    let bytes = fa.read_raw(sz).to_vec();
                    doc_type = Some(strip_nuls(&bytes));
                } else {
                    fa.skip_hexa(sz, "ebml_child");
                }
            });
        } else {
            fa.skip_hexa(size, "top_level");
        }
    });

    if doc_type.as_deref() != Some(IBI_DOC_TYPE) {
        return false;
    }

    fa.stream_prepare(StreamKind::General);
    fa.set_field(StreamKind::General, 0, "Format", "Ibi");
    true
}

fn strip_nuls(bytes: &[u8]) -> String {
    let s = String::from_utf8_lossy(bytes).into_owned();
    s.trim_end_matches('\0').to_string()
}

fn read_vint_id(fa: &mut FileAnalyze) -> Option<u64> {
    let first_bytes = fa.peek_raw(1)?;
    let first = first_bytes[0];
    if first == 0 {
        return None;
    }
    let len = first.leading_zeros() as usize + 1;
    if len > 8 {
        return None;
    }
    let bytes = fa.read_raw(len);
    if bytes.len() < len {
        return None;
    }
    let mut v: u64 = 0;
    for b in bytes {
        v = (v << 8) | (*b as u64);
    }
    Some(v)
}

fn read_vint_size(fa: &mut FileAnalyze) -> Option<u64> {
    let first_bytes = fa.peek_raw(1)?;
    let first = first_bytes[0];
    if first == 0 {
        return None;
    }
    let len = first.leading_zeros() as usize + 1;
    if len > 8 {
        return None;
    }
    let bytes = fa.read_raw(len);
    if bytes.len() < len {
        return None;
    }
    // Strip the leading 1-bit marker on the first byte to recover the value.
    let marker_mask: u8 = if len == 8 { 0 } else { !(0xFF << (8 - len)) };
    let mut v: u64 = (bytes[0] & marker_mask) as u64;
    for b in &bytes[1..] {
        v = (v << 8) | (*b as u64);
    }
    Some(v)
}

fn walk_elements(
    fa: &mut FileAnalyze,
    region_size: usize,
    visit: &mut dyn FnMut(&mut FileAnalyze, u64, usize, usize),
) {
    let region_end = fa.element_offset() + region_size;
    while fa.element_offset() < region_end && fa.remain() > 0 {
        let elem_start = fa.element_offset();
        let Some(id) = read_vint_id(fa) else { break };
        let Some(size) = read_vint_size(fa) else { break };
        let body_size = size as usize;
        if fa.element_offset() + body_size > region_end {
            break;
        }
        let body_end = fa.element_offset() + body_size;
        visit(fa, id, body_size, elem_start);
        if fa.element_offset() < body_end {
            fa.skip_hexa(body_end - fa.element_offset(), "element_tail");
        } else if fa.element_offset() > body_end {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_ibi_header(doc_type: &str) -> Vec<u8> {
        // Build EBML header child: DocType element + a DocTypeVersion stub.
        // DocType id = 0x4282 (2-byte VINT), size = VINT-encoded string length.
        let mut header_children = Vec::new();
        header_children.extend_from_slice(&[0x42, 0x82]);
        // VINT size for ASCII <128 bytes: 1-byte form 0x80 | len.
        header_children.push(0x80 | doc_type.len() as u8);
        header_children.extend_from_slice(doc_type.as_bytes());

        let mut buf = Vec::new();
        buf.extend_from_slice(&EBML_HEADER_BYTES);
        // EBML header size as 1-byte VINT (works while children < 128 bytes).
        buf.push(0x80 | header_children.len() as u8);
        buf.extend_from_slice(&header_children);
        buf
    }

    #[test]
    fn parse_minimal_ibi_header() {
        let buf = make_ibi_header("MediaInfo Index");
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_ibi(&mut fa));
        assert_eq!(
            fa.retrieve(StreamKind::General, 0, "Format").map(|z| z.as_str().to_owned()).as_deref(),
            Some("Ibi")
        );
    }

    #[test]
    fn rejects_matroska_doctype() {
        // Same EBML framing, different DocType -> not Ibi (avoids stealing MKV).
        let buf = make_ibi_header("matroska");
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_ibi(&mut fa));
    }

    #[test]
    fn rejects_non_ebml_buffer() {
        let mut fa = FileAnalyze::new(b"NOT an EBML file at all");
        assert!(!parse_ibi(&mut fa));
    }
}
