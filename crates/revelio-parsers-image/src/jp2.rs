//! JPEG 2000 (JP2 / J2K) parser.
//!
//! Supports both the JP2 file format (ISO/IEC 15444-1, Annex I) and raw
//! J2K codestream (Annex A).
//!
//! JP2 detection: 12-byte signature `\x00\x00\x00\x0C\x6A\x50\x20\x20\x0D\x0A\x87\x0A`
//! followed by a File Type box (ftyp) and a JP2 Header box (jp2h) containing
//! an Image Header box (ihdr).
//!
//! J2K detection: SOC marker `\xFF\x4F`.

use revelio_core::{FileAnalyze, StreamKind};

const JP2_SIG: [u8; 12] = [0x00, 0x00, 0x00, 0x0C, 0x6A, 0x50, 0x20, 0x20, 0x0D, 0x0A, 0x87, 0x0A];
const SOC_MARKER: u16 = 0xFF4F;
const SIZ_MARKER: u16 = 0xFF51;

/// Parse a JPEG 2000 image (JP2 file or raw J2K codestream).
pub fn parse_jp2(fa: &mut FileAnalyze) -> bool {
    // Try raw J2K codestream first (peek_b2 doesn't borrow fa long-term)
    let mut magic: u16 = 0;
    magic = fa.peek_b2();
    if magic == SOC_MARKER {
        return parse_j2k_codestream(fa);
    }

    // Try JP2 file format — use owned copy to avoid borrow conflict
    // when calling parse_jp2_file which also peeks fa
    {
        let buf = match fa.peek_raw(fa.remain().min(4096)) {
            Some(b) => b.to_vec(),
            None => return false,
        };
        if buf.len() < 12 || buf[..12] != JP2_SIG {
            return false;
        }
    }
    parse_jp2_file(fa)
}

fn parse_jp2_file(fa: &mut FileAnalyze) -> bool {
    let buf = match fa.peek_raw(fa.remain().min(4096)) {
        Some(b) => b,
        None => return false,
    };

    if buf.len() < 12 || buf[..12] != JP2_SIG {
        return false;
    }

    // Skip JPEG 2000 signature box (12 bytes)
    let mut pos = 12;
    let mut width: u32 = 0;
    let mut height: u32 = 0;
    let mut bpc: u8 = 0;
    let mut num_components: u16 = 0;
    let mut compression: u8 = 7; // JPEG 2000

    while pos + 8 <= buf.len() {
        let box_len = u32::from_be_bytes([buf[pos], buf[pos + 1], buf[pos + 2], buf[pos + 3]]);
        let box_type = u32::from_be_bytes([buf[pos + 4], buf[pos + 5], buf[pos + 6], buf[pos + 7]]);

        if box_len == 0 {
            break;
        }
        let box_end = pos + (box_len as usize);
        if box_end > buf.len() {
            break;
        }

        match box_type {
            0x66747970 => { /* ftyp — skip */ }
            0x6A703268 => {
                // jp2h — JP2 Header
                let inner = &buf[pos + 8..box_end];
                if inner.len() >= 4 {
                    let ihdr_len = u32::from_be_bytes([inner[0], inner[1], inner[2], inner[3]]);
                    if ihdr_len as usize >= 14 {
                        // ihdr: height(4), width(4), num_components(2), bpc(1), compress(1), ...
                        let hi = 8;
                        if hi + 12 <= inner.len() {
                            height = u32::from_be_bytes([
                                inner[hi],
                                inner[hi + 1],
                                inner[hi + 2],
                                inner[hi + 3],
                            ]);
                            width = u32::from_be_bytes([
                                inner[hi + 4],
                                inner[hi + 5],
                                inner[hi + 6],
                                inner[hi + 7],
                            ]);
                            num_components = u16::from_be_bytes([inner[hi + 8], inner[hi + 9]]);
                            bpc = inner[hi + 10];
                            compression = inner[hi + 11];
                        }
                    }
                }
            }
            0x72657320 => { /* res — resolution, skip */ }
            _ => {}
        }

        pos = box_end;
    }

    fa.stream_prepare(StreamKind::Image);
    fa.set_field(StreamKind::Image, 0, "Format", "JPEG 2000");

    if compression == 7 {
        fa.set_field(StreamKind::Image, 0, "Format_Info", "JPEG 2000 file format (JP2)");
    }

    if width > 0 {
        fa.set_field(StreamKind::Image, 0, "Width", width.to_string());
    }
    if height > 0 {
        fa.set_field(StreamKind::Image, 0, "Height", height.to_string());
    }
    if num_components > 0 {
        fa.set_field(StreamKind::Image, 0, "BitDepth_Planes", num_components.to_string());
    }
    if bpc > 0 {
        let bit_depth = (bpc & 0x7F) + 1;
        fa.set_field(StreamKind::Image, 0, "BitDepth", bit_depth.to_string());
        if (bpc & 0x80) != 0 {
            fa.set_field(StreamKind::Image, 0, "Format_Settings_Sign", "Signed");
        }
    }

    true
}

fn parse_j2k_codestream(fa: &mut FileAnalyze) -> bool {
    let buf = match fa.peek_raw(fa.remain().min(128)) {
        Some(b) => b,
        None => return false,
    };

    if buf.len() < 4 {
        return false;
    }
    let mut magic: u16 = 0;
    magic = fa.peek_b2();
    if magic != SOC_MARKER {
        return false;
    }

    // Find SIZ marker (0xFF51)
    let mut width: u32 = 0;
    let mut height: u32 = 0;
    let mut bit_depth: u8 = 0;
    let mut num_components: u16 = 0;

    let mut i = 2;
    while i + 3 < buf.len() {
        let marker = u16::from_be_bytes([buf[i], buf[i + 1]]);
        if marker == SIZ_MARKER {
            if i + 45 > buf.len() {
                break;
            }
            let seg_len = u16::from_be_bytes([buf[i + 2], buf[i + 3]]);
            let siz_end = i + 2 + seg_len as usize;
            if siz_end > buf.len() {
                break;
            }

            // SIZ marker segment (from i+2):
            // Lsiz(2), Rsiz(2), Xsiz(4), Ysiz(4), XOsiz(4), YOsiz(4),
            // XTsiz(4), YTsiz(4), XTOsiz(4), YTOsiz(4), Csiz(2)
            // Then per-component: Ssiz(1), XRsiz(1), YRsiz(1)
            let si = i + 4; // skip marker + Lsiz
            if si + 34 > buf.len() {
                break;
            }
            let ref_width =
                u32::from_be_bytes([buf[si + 2], buf[si + 3], buf[si + 4], buf[si + 5]]);
            let ref_height =
                u32::from_be_bytes([buf[si + 6], buf[si + 7], buf[si + 8], buf[si + 9]]);
            width = ref_width;
            height = ref_height;
            num_components = u16::from_be_bytes([buf[si + 34], buf[si + 35]]);

            if num_components > 0 && si + 36 < buf.len() {
                let ssiz = buf[si + 36];
                bit_depth = (ssiz & 0x7F) + 1;
            }
            break;
        }
        // Skip to next marker
        if marker == 0xFF00 || marker == 0xFFFF {
            i += 1;
            continue;
        }
        if marker == 0xFF90 || marker == 0xFF93 || marker == 0xFFD9 {
            i += 2;
            continue;
        }
        if i + 4 <= buf.len() {
            let seg_len = u16::from_be_bytes([buf[i + 2], buf[i + 3]]) as usize;
            i += 2 + seg_len;
        } else {
            i += 2;
        }
    }

    fa.stream_prepare(StreamKind::Image);
    fa.set_field(StreamKind::Image, 0, "Format", "JPEG 2000");
    fa.set_field(StreamKind::Image, 0, "Format_Info", "JPEG 2000 codestream (J2K)");

    if width > 0 {
        fa.set_field(StreamKind::Image, 0, "Width", width.to_string());
    }
    if height > 0 {
        fa.set_field(StreamKind::Image, 0, "Height", height.to_string());
    }
    if num_components > 0 {
        fa.set_field(StreamKind::Image, 0, "BitDepth_Planes", num_components.to_string());
    }
    if bit_depth > 0 {
        fa.set_field(StreamKind::Image, 0, "BitDepth", bit_depth.to_string());
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use revelio_core::FileAnalyze;

    fn make_jp2_file() -> Vec<u8> {
        let mut buf = Vec::new();
        // JPEG 2000 signature box
        buf.extend_from_slice(&JP2_SIG);
        // File Type box (ftyp) — ftyp includes TBox, so content = ftyp[4..]
        let ftyp = b"ftypjp2 \x00\x00\x00\x00";
        let ftyp_len = 8 + (ftyp.len() - 4) as u32;
        buf.extend_from_slice(&ftyp_len.to_be_bytes());
        buf.extend_from_slice(ftyp);
        // JP2 Header box (jp2h) containing Image Header box (ihdr)
        let mut jp2h = Vec::new();
        // ihdr: LBox(4), TBox('ihdr'=0x69686472), height(4), width(4), num_comps(2), bpc(1), compress(1), ...
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&0u32.to_be_bytes()); // placeholder LBox
        ihdr.extend_from_slice(b"ihdr");
        ihdr.extend_from_slice(&1920u32.to_be_bytes()); // height
        ihdr.extend_from_slice(&1080u32.to_be_bytes()); // width
        ihdr.extend_from_slice(&3u16.to_be_bytes()); // num_components
        ihdr.push(7); // bpc (7 = 8-bit unsigned)
        ihdr.push(7); // compression type (7 = JPEG 2000)
        ihdr.push(1); // unknown_colorspace
        ihdr.push(0); // intellectual_property
        // Fix LBox
        let ihdr_len = ihdr.len() as u32;
        ihdr[0..4].copy_from_slice(&ihdr_len.to_be_bytes());

        jp2h.extend_from_slice(&ihdr);

        let jp2h_len = 8 + jp2h.len() as u32;
        buf.extend_from_slice(&jp2h_len.to_be_bytes());
        buf.extend_from_slice(b"jp2h");
        buf.extend_from_slice(&jp2h);
        buf
    }

    #[test]
    fn parse_jp2_file() {
        let buf = make_jp2_file();
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_jp2(&mut fa));
        let g = |k: &str| fa.retrieve(StreamKind::Image, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(g("Format").as_deref(), Some("JPEG 2000"));
        assert_eq!(g("Width").as_deref(), Some("1080"));
        assert_eq!(g("Height").as_deref(), Some("1920"));
        assert_eq!(g("BitDepth").as_deref(), Some("8"));
        assert_eq!(g("BitDepth_Planes").as_deref(), Some("3"));
    }

    #[test]
    fn parse_j2k_codestream() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&SOC_MARKER.to_be_bytes()); // SOC
        // SIZ marker
        buf.extend_from_slice(&SIZ_MARKER.to_be_bytes()); // SIZ
        let siz_data: Vec<u8> = {
            let mut s = Vec::new();
            s.extend_from_slice(&45u16.to_be_bytes()); // Lsiz = 45 (includes Lsiz field)
            s.extend_from_slice(&0u16.to_be_bytes()); // Rsiz
            s.extend_from_slice(&1920u32.to_be_bytes()); // Xsiz
            s.extend_from_slice(&1080u32.to_be_bytes()); // Ysiz
            s.extend_from_slice(&0u32.to_be_bytes()); // XOsiz
            s.extend_from_slice(&0u32.to_be_bytes()); // YOsiz
            s.extend_from_slice(&1920u32.to_be_bytes()); // XTsiz
            s.extend_from_slice(&1080u32.to_be_bytes()); // YTsiz
            s.extend_from_slice(&0u32.to_be_bytes()); // XTOsiz
            s.extend_from_slice(&0u32.to_be_bytes()); // YTOsiz
            s.extend_from_slice(&3u16.to_be_bytes()); // Csiz (3 components)
            // Per-component:
            s.push(7); // Ssiz for Cb
            s.push(1); // XRsiz
            s.push(1); // YRsiz
            s.push(7); // Cr
            s.push(1);
            s.push(1);
            s.push(7); // Y
            s.push(1);
            s.push(1);
            s
        };
        buf.extend_from_slice(&siz_data);
        // EOC marker
        buf.extend_from_slice(&0xFFD9u16.to_be_bytes());

        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_jp2(&mut fa));
        let g = |k: &str| fa.retrieve(StreamKind::Image, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(g("Format").as_deref(), Some("JPEG 2000"));
        assert_eq!(g("Width").as_deref(), Some("1920"));
        assert_eq!(g("Height").as_deref(), Some("1080"));
        assert_eq!(g("BitDepth").as_deref(), Some("8"));
        assert_eq!(g("BitDepth_Planes").as_deref(), Some("3"));
    }

    #[test]
    fn jp2_rejects_non_jpeg2000() {
        let buf = b"This is not JPEG 2000";
        let mut fa = FileAnalyze::new(buf);
        assert!(!parse_jp2(&mut fa));
    }
}
