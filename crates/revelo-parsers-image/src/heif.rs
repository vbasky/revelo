use revelo_core::{FileAnalyze, StreamKind};

/// HEIF/HEIC parser. Detects the ISO BMFF box structure starting with ftyp.
/// HEIF files have major_brand "mif1", "msf1", "heic", "heix", "hevc",
/// "heim", "heis", "hevm", "hevs".
pub fn parse_heif(fa: &mut FileAnalyze) -> bool {
    let file_size = fa.remain();
    let Some(buf) = fa.peek_raw(12) else { return false };

    // ISO BMFF box: 4-byte size + 4-byte type
    let box_size = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
    if box_size < 12 || box_size > file_size {
        return false;
    }
    let box_type = &buf[4..8];

    if box_type != b"ftyp" {
        return false;
    }

    let major_brand_bytes = [buf[8], buf[9], buf[10], buf[11]];
    let major_brand = std::str::from_utf8(&major_brand_bytes).unwrap_or("");
    let heif_brands = ["mif1", "msf1", "heic", "heix", "hevc", "heim", "heis", "hevm", "hevs"];

    if !heif_brands.contains(&major_brand) {
        return false;
    }

    let pos = fa.stream_prepare(StreamKind::Image);

    let format = match major_brand {
        "heic" | "heix" | "heim" | "heis" => "HEIC",
        "hevc" | "hevm" | "hevs" => "HEIF",
        "mif1" | "msf1" => "HEIF",
        _ => "HEIF",
    };
    fa.set_field(StreamKind::Image, pos, "Format", format);
    fa.set_field(StreamKind::Image, pos, "Format_Profile", major_brand);

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heif_detects_ftyp() {
        let mut buf = vec![0u8; 32];
        buf[0..4].copy_from_slice(&(32u32.to_be_bytes()));
        buf[4..8].copy_from_slice(b"ftyp");
        buf[8..12].copy_from_slice(b"heic");
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_heif(&mut fa));
        assert_eq!(
            fa.retrieve(StreamKind::Image, 0, "Format").map(|z| z.as_str().to_owned()),
            Some("HEIC".into())
        );
    }

    #[test]
    fn heif_probe_is_header_bounded() {
        let mut buf = vec![0u8; 1024 * 1024];
        buf[0..4].copy_from_slice(&(32u32.to_be_bytes()));
        buf[4..8].copy_from_slice(b"ftyp");
        buf[8..12].copy_from_slice(b"heic");
        let mut fa = FileAnalyze::new(&buf);

        assert!(parse_heif(&mut fa));
        assert_eq!(fa.access_stats().max_request_len, 12);
    }

    #[test]
    fn heif_rejects_non_ftyp() {
        let buf = vec![0u8; 32];
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_heif(&mut fa));
    }
}
