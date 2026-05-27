//! YUV4MPEG2 (Y4M) video parser.
//!
//! Mirrors MediaInfoLib's `File_Y4m.cpp`. Y4M is a trivial container:
//! a plain-text header line followed by raw YUV frames.
//!
//! Layout:
//!   "YUV4MPEG2 "  -- magic (10 bytes)
//!   parameter*    -- space-separated parameters ending with LF (0x0A)
//!   "FRAME\n" [raw frame data]*
//!
//! Parameters:
//!   W<width>  H<height>  C<colorspace>  F<fps_num:fps_den>
//!   I<p|t|b|m>  A<num:den>  X<comment>

use mediainfo_core::{FileAnalyze, StreamKind};

pub fn parse_y4m(fa: &mut FileAnalyze) -> bool {
    // Must be at least 10 bytes for magic
    let magic = match fa.peek_raw(10) {
        Some(m) => m,
        None => return false,
    };
    if magic != b"YUV4MPEG2 " {
        return false;
    }

    fa.Element_Begin("YUV4MPEG2");

    // Scan for LF to find end of header
    let hdr_end = {
        let remain = fa.Remain();
        let data = match fa.peek_raw(remain) {
            Some(d) => d,
            None => {
                fa.Element_End();
                return false;
            }
        };
        let mut pos = 0;
        for i in 10..data.len() {
            if data[i] == 0x0A {
                pos = i + 1;
                break;
            }
        }
        if pos == 0 {
            fa.Element_End();
            return false;
        }
        pos
    };

    // Read the header line and convert to owned string immediately
    let hdr_bytes = fa.read_raw(hdr_end).to_vec();
    // Strip trailing LF
    let hdr_bytes = if hdr_bytes.last() == Some(&0x0A) {
        &hdr_bytes[..hdr_bytes.len() - 1]
    } else {
        &hdr_bytes[..]
    };
    let header = String::from_utf8_lossy(hdr_bytes).into_owned();

    let mut width: u64 = 0;
    let mut height: u64 = 0;
    let mut multiplier: u64 = 0;
    let mut divisor: u64 = 1;
    let mut frame_rate: f64 = 0.0;

    // Split by spaces and parse parameters
    for param in header.split(' ') {
        if param.is_empty() {
            continue;
        }
        let bytes = param.as_bytes();
        if bytes.is_empty() {
            continue;
        }
        match bytes[0] {
            b'A' => {
                let val = &param[1..];
                if let Some(colon) = val.find(':') {
                    if let (Ok(x), Ok(y)) = (val[..colon].parse::<f64>(), val[colon + 1..].parse::<f64>()) {
                        if x > 0.0 && y > 0.0 {
                            fa.Fill(StreamKind::Video, 0, "PixelAspectRatio", format!("{:.3}", x / y), false);
                        }
                    }
                }
            }
            b'C' => {
                // Color space — the token is e.g. "C420" or "C420jpeg".
                // Compare the full token (param) rather than stripping 'C'.
                if param == "C420jpeg" || param == "C420paldv" || param == "C420" {
                    fa.Fill(StreamKind::Video, 0, "ChromaSubsampling", "4:2:0", false);
                    multiplier = 3;
                    divisor = 2;
                } else if param == "C422" {
                    fa.Fill(StreamKind::Video, 0, "ChromaSubsampling", "4:2:2", false);
                    multiplier = 2;
                } else if param == "C444" {
                    fa.Fill(StreamKind::Video, 0, "ChromaSubsampling", "4:4:4", false);
                    multiplier = 3;
                }
            }
            b'F' => {
                let val = &param[1..];
                if let Some(colon) = val.find(':') {
                    if let (Ok(n), Ok(d)) = (val[..colon].parse::<f64>(), val[colon + 1..].parse::<f64>()) {
                        if n > 0.0 && d > 0.0 {
                            frame_rate = n / d;
                            fa.Fill(StreamKind::Video, 0, "FrameRate", format!("{:.3}", frame_rate), false);
                        }
                    }
                }
            }
            b'H' => {
                if let Ok(h) = param[1..].parse::<u64>() {
                    height = h;
                    fa.Fill(StreamKind::Video, 0, "Height", h.to_string(), false);
                }
            }
            b'I' => {
                if param.len() == 2 {
                    match bytes[1] {
                        b'p' => fa.Fill(StreamKind::Video, 0, "ScanType", "Progressive", false),
                        b't' => {
                            fa.Fill(StreamKind::Video, 0, "ScanType", "Progressive", false);
                            fa.Fill(StreamKind::Video, 0, "ScanOrder", "TFF", false);
                        }
                        b'b' => {
                            fa.Fill(StreamKind::Video, 0, "ScanType", "Progressive", false);
                            fa.Fill(StreamKind::Video, 0, "ScanOrder", "BFF", false);
                        }
                        b'm' => fa.Fill(StreamKind::Video, 0, "ScanType", "Mixed", false),
                        _ => {}
                    }
                }
            }
            b'W' => {
                if let Ok(w) = param[1..].parse::<u64>() {
                    width = w;
                    fa.Fill(StreamKind::Video, 0, "Width", w.to_string(), false);
                }
            }
            _ => {}
        }
    }

    fa.Element_End();

    // Duration (no per-frame metadata in Y4M)
    if width > 0 && height > 0 && multiplier > 0 {
        let frame_byte_size = 6 + width * height * multiplier / divisor;
        let file_size = fa.Element_Size() as u64;
        if frame_byte_size > 0 {
            let frame_count = file_size / frame_byte_size;
            fa.Fill(StreamKind::Video, 0, "FrameCount", frame_count.to_string(), false);
            if frame_rate > 0.0 {
                let bitrate = (width * height * multiplier / divisor) as f64 * 8.0 * frame_rate;
                fa.Fill(StreamKind::Video, 0, "BitRate", format!("{:.0}", bitrate), false);
            }
        }
    }

    fa.Stream_Prepare(StreamKind::General);
    fa.Fill(StreamKind::General, 0, "Format", "YUV4MPEG2", false);
    fa.Stream_Prepare(StreamKind::Video);
    fa.Fill(StreamKind::Video, 0, "Format", "YUV", false);
    fa.Fill(StreamKind::Video, 0, "ColorSpace", "YUV", false);

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_y4m(params: &str) -> Vec<u8> {
        let mut buf = b"YUV4MPEG2 ".to_vec();
        buf.extend_from_slice(params.as_bytes());
        buf.push(b'\n');
        buf.extend_from_slice(b"FRAME");
        buf.push(0x00);
        buf.resize(buf.len() + 100, 0x80);
        buf
    }

    #[test]
    fn rejects_non_y4m_magic() {
        let buf = b"NotYUV4MPEG".to_vec();
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_y4m(&mut fa));
    }

    #[test]
    fn rejects_too_short() {
        let buf = b"YUV4M".to_vec();
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_y4m(&mut fa));
    }

    #[test]
    fn parses_basic_y4m_header() {
        let buf = make_y4m("W1920 H1080 F25:1 C420jpeg Ip A1:1");
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_y4m(&mut fa));
        assert_eq!(
            fa.Retrieve(StreamKind::Video, 0, "Format").map(|z| z.as_str()),
            Some("YUV")
        );
        assert_eq!(
            fa.Retrieve(StreamKind::Video, 0, "Width").map(|z| z.as_str()),
            Some("1920")
        );
        assert_eq!(
            fa.Retrieve(StreamKind::Video, 0, "Height").map(|z| z.as_str()),
            Some("1080")
        );
        assert_eq!(
            fa.Retrieve(StreamKind::Video, 0, "FrameRate").map(|z| z.as_str()),
            Some("25.000")
        );
    }

    #[test]
    fn parses_c420_chroma() {
        let buf = make_y4m("W10 H10 C420jpeg");
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_y4m(&mut fa));
        assert_eq!(
            fa.Retrieve(StreamKind::Video, 0, "ChromaSubsampling").map(|z| z.as_str()),
            Some("4:2:0")
        );
    }

    #[test]
    fn parses_c444_chroma() {
        let buf = make_y4m("W10 H10 C444");
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_y4m(&mut fa));
        assert_eq!(
            fa.Retrieve(StreamKind::Video, 0, "ChromaSubsampling").map(|z| z.as_str()),
            Some("4:4:4")
        );
    }
}
