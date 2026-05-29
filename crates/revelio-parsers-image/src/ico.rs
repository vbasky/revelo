//! ICO (Windows icon) / CUR (Windows cursor) parser.
//!
//! Header (6 bytes):
//!   2 bytes  reserved (0)
//!   2 bytes  type: 1=ICO, 2=CUR
//!   2 bytes  count of directory entries
//!
//! Each directory entry (16 bytes):
//!   1 byte   width  (0 means 256)
//!   1 byte   height (0 means 256)
//!   1 byte   colour count (0 if ≥256)
//!   1 byte   reserved
//!   2 bytes  colour planes (ICO) / X hotspot (CUR)
//!   2 bytes  bits per pixel (ICO) / Y hotspot (CUR)
//!   4 bytes  size of the bitmap data
//!   4 bytes  offset to the bitmap data

use revelio_core::{FileAnalyze, StreamKind};

pub fn parse_ico(fa: &mut FileAnalyze) -> bool {
    let head = fa.peek_raw(6);
    let Some(h) = head else { return false };
    if h[0] != 0 || h[1] != 0 {
        return false;
    }
    let kind = u16::from_le_bytes([h[2], h[3]]);
    if kind != 1 && kind != 2 {
        return false;
    }
    let count = u16::from_le_bytes([h[4], h[5]]);
    if count == 0 {
        return false;
    }
    let header_size = 6 + (count as usize) * 16;
    let file_size = fa.remain();
    let entries: Vec<(u32, u32, u16, u32)>;
    let mut total_data: u64 = 0;
    {
        let buf = match fa.peek_raw(header_size) {
            Some(b) => b,
            None => return false,
        };
        let mut tmp: Vec<(u32, u32, u16, u32)> = Vec::with_capacity(count as usize);
        for i in 0..count as usize {
            let e = &buf[6 + i * 16..6 + i * 16 + 16];
            let w = if e[0] == 0 { 256u32 } else { e[0] as u32 };
            let h_ = if e[1] == 0 { 256u32 } else { e[1] as u32 };
            let bpp = u16::from_le_bytes([e[6], e[7]]);
            let size = u32::from_le_bytes([e[8], e[9], e[10], e[11]]);
            let off = u32::from_le_bytes([e[12], e[13], e[14], e[15]]) as u64;
            if off > file_size as u64 || off + size as u64 > file_size as u64 {
                return false;
            }
            total_data += size as u64;
            tmp.push((w, h_, bpp, size));
        }
        entries = tmp;
    }
    if header_size as u64 + total_data > file_size as u64 {
        return false;
    }

    fa.stream_prepare(StreamKind::General);
    fa.set_field(StreamKind::General, 0, "Format", if kind == 1 { "ICO" } else { "CUR" });
    fa.set_field(StreamKind::General, 0, "ImageCount", count.to_string());
    // General.StreamSize = file overhead = file_size − total bitmap data.
    let overhead = file_size as u64 - total_data;
    fa.force_field(StreamKind::General, 0, "StreamSize", overhead.to_string());

    for (w, h_, bpp, size) in entries {
        let pos = fa.stream_prepare(StreamKind::Image);
        fa.set_field(StreamKind::Image, pos, "Width", w.to_string());
        fa.set_field(StreamKind::Image, pos, "Height", h_.to_string());
        if kind == 1 {
            fa.set_field(StreamKind::Image, pos, "BitDepth", bpp.to_string());
        }
        fa.set_field(StreamKind::Image, pos, "StreamSize", size.to_string());
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_minimal_ico(entries: &[(u8, u8, u16, u32)]) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(&[0u8, 0]);
        buf.extend_from_slice(&1u16.to_le_bytes()); // ICO
        buf.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        let header_end = 6 + entries.len() * 16;
        let mut current_off = header_end as u32;
        for (w, h, bpp, size) in entries {
            buf.push(*w);
            buf.push(*h);
            buf.push(0); // colour count
            buf.push(0); // reserved
            buf.extend_from_slice(&1u16.to_le_bytes()); // planes
            buf.extend_from_slice(&bpp.to_le_bytes());
            buf.extend_from_slice(&size.to_le_bytes());
            buf.extend_from_slice(&current_off.to_le_bytes());
            current_off += *size;
        }
        // Pad bitmap data so the offset/size validation passes.
        let total_size: u32 = entries.iter().map(|e| e.3).sum();
        buf.resize(buf.len() + total_size as usize, 0);
        buf
    }

    #[test]
    fn rejects_non_ico() {
        let mut fa = FileAnalyze::new(b"NOT AN ICO FILE AT ALL");
        assert!(!parse_ico(&mut fa));
    }

    #[test]
    fn parses_single_entry_ico() {
        let buf = build_minimal_ico(&[(32, 32, 32, 100)]);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_ico(&mut fa));
        let g = |k: &str| fa.retrieve(StreamKind::General, 0, k).map(|z| z.as_str().to_owned());
        let i = |k: &str| fa.retrieve(StreamKind::Image, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(g("Format").as_deref(), Some("ICO"));
        assert_eq!(g("ImageCount").as_deref(), Some("1"));
        assert_eq!(i("Width").as_deref(), Some("32"));
        assert_eq!(i("Height").as_deref(), Some("32"));
        assert_eq!(i("BitDepth").as_deref(), Some("32"));
        assert_eq!(i("StreamSize").as_deref(), Some("100"));
    }

    #[test]
    fn zero_dimension_means_256() {
        let buf = build_minimal_ico(&[(0, 0, 32, 100)]);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_ico(&mut fa));
        let i = |k: &str| fa.retrieve(StreamKind::Image, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(i("Width").as_deref(), Some("256"));
        assert_eq!(i("Height").as_deref(), Some("256"));
    }

    #[test]
    fn multi_entry_ico_creates_multiple_image_streams() {
        let buf = build_minimal_ico(&[(16, 16, 8, 50), (32, 32, 32, 100), (48, 48, 32, 200)]);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_ico(&mut fa));
        assert_eq!(fa.stream_count(StreamKind::Image), 3);
    }
}
