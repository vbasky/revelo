use revelo_core::{FileAnalyze, StreamKind};

/// Parse VC-3/DNxHD (SMPTE ST 2019) intermediate codec.
///
/// Detection: 0x00000280 header prefix.
/// Fills: Compression ID (CID)→profile/bit_depth/chroma.
pub fn parse_vc3(fa: &mut FileAnalyze) -> bool {
    let buf = match fa.peek_raw(fa.remain()) {
        Some(b) => b,
        None => return false,
    };

    // VC-3 bitstream starts with a 0x00000280 header prefix
    if buf.len() < 0x2C {
        return false;
    }

    let header_prefix = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]);
    if header_prefix != 0x00000280 {
        return false;
    }

    let header_version = buf[4];
    if header_version > 3 {
        return false;
    }

    let active_lines = u16::from_be_bytes([buf[0x18], buf[0x19]]);
    let samples_per_line = u16::from_be_bytes([buf[0x1A], buf[0x1B]]);
    let flags_ss = u16::from_be_bytes([buf[0x22], buf[0x23]]);
    let sst = (flags_ss >> 2) & 1;
    let compression_id = u32::from_be_bytes([buf[0x28], buf[0x29], buf[0x2A], buf[0x2B]]);

    let height = if sst != 0 { active_lines * 2 } else { active_lines };
    let width = if compression_id >= 1270 {
        let width_block = (samples_per_line as u32).div_ceil(16);
        (width_block * 16) as u16
    } else {
        samples_per_line
    };

    // Derive profile/level/bit_depth from compression_id
    let (profile, level, bit_depth) = vc3_from_cid(compression_id);

    fill_vc3_streams(
        fa,
        Vc3Info {
            version: header_version,
            width,
            height,
            cid: compression_id,
            sst,
            profile,
            level,
            bit_depth,
        },
    );
    true
}

fn vc3_from_cid(cid: u32) -> (&'static str, &'static str, u8) {
    let profile = if (1235..=1260).contains(&cid) {
        "HD"
    } else if (1270..=1275).contains(&cid) {
        "RI"
    } else {
        ""
    };

    let level = match cid {
        1256 | 1270 => "444",
        1235 | 1241 | 1250 | 1271 => "HQX",
        1238 | 1243 | 1251 | 1272 => "HQ",
        1237 | 1242 | 1252 | 1273 => "SQ",
        1253 | 1274 => "LB",
        _ => "",
    };

    let bit_depth = match cid {
        1237 | 1238 | 1242 | 1243 | 1251 | 1252 | 1253 | 1258 | 1259 | 1260 => 8,
        1235 | 1241 | 1250 | 1256 => 10,
        _ => 8,
    };

    (profile, level, bit_depth)
}

struct Vc3Info {
    version: u8,
    width: u16,
    height: u16,
    cid: u32,
    sst: u16,
    profile: &'static str,
    level: &'static str,
    bit_depth: u8,
}

fn fill_vc3_streams(fa: &mut FileAnalyze, info: Vc3Info) {
    let Vc3Info { version, width, height, cid, sst, profile, level, bit_depth } = info;
    fa.stream_prepare(StreamKind::Video);
    fa.set_field(StreamKind::Video, 0, "Format", "VC-3");
    fa.set_field(StreamKind::Video, 0, "Format_Version", format!("Version {}", version));
    fa.set_field(StreamKind::Video, 0, "Format_Profile", format!("{}@{}", profile, level));
    fa.set_field(StreamKind::Video, 0, "Width", width.to_string());
    fa.set_field(StreamKind::Video, 0, "Height", height.to_string());
    fa.set_field(StreamKind::Video, 0, "BitDepth", bit_depth.to_string());

    let scan = if sst == 0 { "Progressive" } else { "Interlaced" };
    fa.set_field(StreamKind::Video, 0, "ScanType", scan);

    let chroma = if cid == 1256 || cid == 1270 { "4:4:4" } else { "4:2:2" };
    fa.set_field(StreamKind::Video, 0, "ChromaSubsampling", chroma);
    fa.set_field(StreamKind::Video, 0, "ColorSpace", "YUV");
    fa.set_field(StreamKind::Video, 0, "BitRate_Mode", "CBR");
}

#[cfg(test)]
mod tests {
    use super::*;
    use revelo_core::FileAnalyze;

    #[test]
    fn vc3_detects_header_prefix() {
        let mut buf = vec![0u8; 0x2C];
        buf[0] = 0x00;
        buf[1] = 0x00;
        buf[2] = 0x02;
        buf[3] = 0x80;
        buf[4] = 3; // version 3
        // active lines = 1080
        buf[0x18] = 0x04;
        buf[0x19] = 0x38;
        // samples per line = 1920
        buf[0x1A] = 0x07;
        buf[0x1B] = 0x80;
        // SST = progressive
        buf[0x22] = 0x00;
        buf[0x23] = 0x00;
        // CID = 1235 (DNxHD HQX 10-bit 1080p)
        buf[0x28] = 0x00;
        buf[0x29] = 0x00;
        buf[0x2A] = 0x04;
        buf[0x2B] = 0xD3;

        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_vc3(&mut fa));
        assert_eq!(
            fa.retrieve(StreamKind::Video, 0, "Format").map(|z| z.as_str().to_owned()),
            Some("VC-3".into())
        );
        assert_eq!(
            fa.retrieve(StreamKind::Video, 0, "Format_Profile").map(|z| z.as_str().to_owned()),
            Some("HD@HQX".into())
        );
    }

    #[test]
    fn vc3_rejects_garbage() {
        let buf = vec![0u8; 0x40];
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_vc3(&mut fa));
    }
}
