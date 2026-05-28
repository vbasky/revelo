use revelio_core::{FileAnalyze, StreamKind};

pub struct ProResInfo {
    pub version: u16,
    pub width: u16,
    pub height: u16,
    pub chrominance_factor: u8,
    pub frame_type: u8,
    pub primaries: u8,
    pub transfer: u8,
    pub matrix: u8,
    pub creator_id: u32,
}

pub fn parse_prores(fa: &mut FileAnalyze) -> bool {
    let head = fa.peek_raw(8);
    let Some(data) = head else { return false };

    // Look for icpf (ProRes standard) or apch/apcn/apcs/apco/ap4h (Apple ProRes variants)
    let magic = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
    if magic != 0x69637066 && magic != 0x6170636E && magic != 0x61706373
        && magic != 0x6170636F && magic != 0x61703468 && magic != 0x70727266
    {
        // Check for ProRes in MOV container: skip to frame data
        let raw = fa.peek_raw(fa.Remain() as usize);
        let buf = match raw { Some(b) => b, None => return false };
        if buf.len() < 20 { return false; }

        let frame_magic = u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]);
        if frame_magic != 0x69637066 && frame_magic != 0x6170636E && frame_magic != 0x61706373
            && frame_magic != 0x6170636F && frame_magic != 0x61703468 && frame_magic != 0x70727266
        {
            return false;
        }
    }

    let buf = match fa.peek_raw(fa.Remain() as usize) { Some(b) => b, None => return false };
    if buf.len() < 20 { return false; }

    let hdr_size = u16::from_be_bytes([buf[8], buf[9]]);
    let version = u16::from_be_bytes([buf[10], buf[11]]);
    let creator_id = u32::from_be_bytes([buf[12], buf[13], buf[14], buf[15]]);
    let frame_width = u16::from_be_bytes([buf[16], buf[17]]);
    let frame_height = u16::from_be_bytes([buf[18], buf[19]]);

    if buf.len() < 21 { return false; }

    let chrominance_factor = (buf[20] >> 6) & 3;
    let frame_type = (buf[20] >> 4) & 3;
    let primaries = *buf.get(22).unwrap_or(&2);
    let transfer = *buf.get(23).unwrap_or(&2);
    let matrix = *buf.get(24).unwrap_or(&2);

    let info = ProResInfo {
        version,
        width: frame_width,
        height: frame_height,
        chrominance_factor,
        frame_type,
        primaries,
        transfer,
        matrix,
        creator_id,
    };

    fill_prores_streams(fa, &info);
    true
}

fn fill_prores_streams(fa: &mut FileAnalyze, info: &ProResInfo) {
    fa.Stream_Prepare(StreamKind::Video);
    fa.Fill(StreamKind::Video, 0, "Format", "ProRes", false);
    fa.Fill(StreamKind::Video, 0, "Format_Version", format!("Version {}", info.version), false);

    let profile = match info.chrominance_factor {
        0 => "422 Proxy",
        _ => {
            if info.frame_type == 0 {
                match info.chrominance_factor {
                    2 => "422 HQ",
                    3 => "4444",
                    4 => "4444 XQ",
                    _ => "422",
                }
            } else {
                "4444"
            }
        }
    };
    fa.Fill(StreamKind::Video, 0, "Format_Profile", profile, false);

    fa.Fill(StreamKind::Video, 0, "Width", info.width.to_string(), false);
    fa.Fill(StreamKind::Video, 0, "Height", info.height.to_string(), false);

    let chroma = match info.chrominance_factor {
        2 => "4:2:2",
        3 | 4 => "4:4:4",
        _ => "4:2:2",
    };
    fa.Fill(StreamKind::Video, 0, "ChromaSubsampling", chroma, false);
    fa.Fill(StreamKind::Video, 0, "ColorSpace", "YUV", false);

    let scan = match info.frame_type {
        0 => "Progressive",
        1 | 2 => "Interlaced",
        _ => "",
    };
    if !scan.is_empty() {
        fa.Fill(StreamKind::Video, 0, "ScanType", scan, false);
    }

    let creator = match info.creator_id {
        0x61706C30 => "Apple",
        0x61727269 => "Arnold & Richter Cine Technik",
        0x616A6130 => "AJA Kona Hardware",
        _ => "",
    };
    if !creator.is_empty() {
        fa.Fill(StreamKind::Video, 0, "Encoded_Library", creator, false);
    }

    if info.primaries != 0 {
        let prim = match info.primaries {
            1 => "BT.709",
            9 => "BT.2020",
            _ => "BT.709",
        };
        fa.Fill(StreamKind::Video, 0, "colour_primaries", prim, false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use revelio_core::FileAnalyze;

    #[test]
    fn prores_icpf_header() {
        let mut buf = vec![0u8; 128];
        // size (BE)
        buf[0] = 0; buf[1] = 0; buf[2] = 0; buf[3] = 128;
        // icpf magic
        buf[4] = 0x69; buf[5] = 0x63; buf[6] = 0x70; buf[7] = 0x66;
        // hdrSize
        buf[8] = 0; buf[9] = 148;
        // version
        buf[10] = 0; buf[11] = 0;
        // creatorID
        buf[12] = 0x61; buf[13] = 0x70; buf[14] = 0x6C; buf[15] = 0x30;
        // width, height
        buf[16] = 0x07; buf[17] = 0x80; // 1920
        buf[18] = 0x04; buf[19] = 0x38; // 1080
        // chrominance + frame_type bits
        buf[20] = 0x82; // chrominance=2(422), frame_type=0

        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_prores(&mut fa));
        assert_eq!(fa.Retrieve(StreamKind::Video, 0, "Format").map(|z| z.as_str().to_owned()), Some("ProRes".into()));
        assert_eq!(fa.Retrieve(StreamKind::Video, 0, "ChromaSubsampling").map(|z| z.as_str().to_owned()), Some("4:2:2".into()));
    }

    #[test]
    fn prores_picks_up_apcn_variant() {
        let mut buf = vec![0u8; 128];
        buf[4] = 0x61; buf[5] = 0x70; buf[6] = 0x63; buf[7] = 0x6E; // apcn
        buf[8] = 0; buf[9] = 20;
        buf[10] = 0; buf[11] = 1;
        buf[16] = 0x07; buf[17] = 0x80;
        buf[18] = 0x04; buf[19] = 0x38;

        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_prores(&mut fa));
    }
}
