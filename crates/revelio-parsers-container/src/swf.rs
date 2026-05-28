//! Adobe Flash SWF parser.
//!
//! Subset of MediaInfoLib's `File_Swf.cpp` covering only header extraction
//! for uncompressed ("FWS") streams. Compressed variants ("CWS" zlib,
//! "ZWS" LZMA) are detected so the General format is filled, but the
//! body (RECT + frame_rate + frame_count) is skipped because decoding
//! requires zlib/LZMA which is out of scope here.
//!
//! Magic bytes (3 ASCII):
//!   "FWS" — uncompressed
//!   "CWS" — zlib-compressed body (header bytes 0..7 still plain)
//!   "ZWS" — LZMA-compressed body (header bytes 0..7 still plain)
//!
//! Layout (uncompressed, after the 8-byte file header):
//!   [0..3]  magic (3 bytes)
//!   [3]     version (u8)
//!   [4..8]  FileLength (u32 LE, total bytes including header)
//!   [8..]   body:
//!             RECT { Nbits:5 bits, then 4 × Nbits twips: Xmin, Xmax, Ymin, Ymax }
//!             FrameRate (u16 LE, 8.8 fixed-point — pre-v8 the high byte is "ignored")
//!             FrameCount (u16 LE)

use revelio_core::{FileAnalyze, StreamKind};
use zenlib::{int16u, int32u, int8u};

const MAGIC_LEN: usize = 3;
const HEADER_LEN: usize = 8;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Signature {
    Fws, // uncompressed
    Cws, // zlib
    Zws, // LZMA
}

fn detect_signature(buf: &[u8]) -> Option<Signature> {
    if buf.len() < HEADER_LEN {
        return None;
    }
    match &buf[..MAGIC_LEN] {
        b"FWS" => Some(Signature::Fws),
        b"CWS" => Some(Signature::Cws),
        b"ZWS" => Some(Signature::Zws),
        _ => None,
    }
}

pub fn parse_swf(fa: &mut FileAnalyze) -> bool {
    let sig = {
        let peek = match fa.peek_raw(fa.Remain().min(HEADER_LEN)) {
            Some(p) if p.len() >= HEADER_LEN => p,
            _ => return false,
        };
        match detect_signature(peek) {
            Some(s) => s,
            None => return false,
        }
    };

    fa.Element_Begin("SWF header");
    // Consume the 8-byte fixed header in trace-friendly steps.
    fa.Skip_Hexa(MAGIC_LEN, "Signature");

    let mut version: int8u = 0;
    fa.Get_L1(&mut version, "Version");
    let mut file_length: int32u = 0;
    fa.Get_L4(&mut file_length, "FileLength");
    fa.Element_End();

    // General fields are filled for every recognized signature.
    fa.Stream_Prepare(StreamKind::General);
    fa.Fill(StreamKind::General, 0, "Format", "Flash", false);
    fa.Fill(StreamKind::General, 0, "Format_Version", version.to_string(), false);
    if file_length > 0 {
        fa.Fill(StreamKind::General, 0, "FileSize", file_length.to_string(), false);
    }
    match sig {
        Signature::Cws => {
            fa.Fill(StreamKind::General, 0, "Format_Profile", "Compressed (zlib)", false);
            return true;
        }
        Signature::Zws => {
            fa.Fill(StreamKind::General, 0, "Format_Profile", "Compressed (LZMA)", false);
            return true;
        }
        Signature::Fws => {
            fa.Fill(StreamKind::General, 0, "Format_Profile", "Uncompressed", false);
        }
    }

    // Body parse — RECT (bit-packed) + frame_rate + frame_count.
    fa.Element_Begin("Movie header");
    fa.BS_Begin();
    let mut nbits: int8u = 0;
    fa.Get_S1(5, &mut nbits, "Nbits");
    let nbits_usize = nbits as usize;
    // Nbits >32 would overflow our u32 accumulators; treat as malformed.
    if nbits_usize == 0 || nbits_usize > 32 {
        fa.BS_End();
        fa.Element_End();
        return true;
    }
    let mut xmin: int32u = 0;
    let mut xmax: int32u = 0;
    let mut ymin: int32u = 0;
    let mut ymax: int32u = 0;
    fa.Get_S4(nbits_usize, &mut xmin, "Xmin");
    fa.Get_S4(nbits_usize, &mut xmax, "Xmax");
    fa.Get_S4(nbits_usize, &mut ymin, "Ymin");
    fa.Get_S4(nbits_usize, &mut ymax, "Ymax");
    fa.BS_End();

    let mut frame_rate_raw: int16u = 0;
    fa.Get_L2(&mut frame_rate_raw, "FrameRate");
    let mut frame_count: int16u = 0;
    fa.Get_L2(&mut frame_count, "FrameCount");
    fa.Element_End();

    // Twips → pixels (20 twips per pixel). RECT values are unsigned in the
    // SWF spec for Xmax/Ymax >= Xmin/Ymin so wrapping is unlikely, but use
    // saturating subtraction to be robust against malformed inputs.
    let width = xmax.saturating_sub(xmin) / 20;
    let height = ymax.saturating_sub(ymin) / 20;

    // Pre-v8 stores frame rate as an int8 in the high byte with the low
    // byte "ignored"; from v8 on the field is true 8.8 fixed-point. Either
    // way the encoded value matches frame_rate_raw / 256.0.
    let frame_rate = (frame_rate_raw as f64) / 256.0;

    fa.Stream_Prepare(StreamKind::Video);
    if width > 0 {
        fa.Fill(StreamKind::Video, 0, "Width", width.to_string(), false);
    }
    if height > 0 {
        fa.Fill(StreamKind::Video, 0, "Height", height.to_string(), false);
    }
    if frame_rate > 0.0 {
        fa.Fill(StreamKind::Video, 0, "FrameRate", format!("{:.3}", frame_rate), false);
    }
    if frame_count > 0 {
        fa.Fill(StreamKind::Video, 0, "FrameCount", frame_count.to_string(), false);
    }
    fa.Fill(StreamKind::General, 0, "VideoCount", "1", false);

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal uncompressed SWF: 8-byte header + RECT + frame_rate
    /// + frame_count. RECT encodes Xmin=0, Xmax = width*20, Ymin=0,
    /// Ymax = height*20 with Nbits chosen as the smallest fit.
    fn make_fws(version: u8, width: u32, height: u32, fr_8_8: u16, frames: u16) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(b"FWS");
        buf.push(version);

        // Body first so we can put the real FileLength into the header.
        let mut body = Vec::new();
        // Pick Nbits as the smallest N where max(xmax, ymax) fits in N bits
        // (xmin/ymin are zero so they trivially fit).
        let max_twip = (width * 20).max(height * 20).max(1);
        let nbits = (32 - max_twip.leading_zeros()).max(1) as u8;

        // Pack: 5 bits Nbits | nbits Xmin | nbits Xmax | nbits Ymin | nbits Ymax
        let mut bits: Vec<u8> = Vec::new();
        push_bits(&mut bits, nbits as u32, 5);
        push_bits(&mut bits, 0, nbits as usize); // Xmin
        push_bits(&mut bits, width * 20, nbits as usize); // Xmax
        push_bits(&mut bits, 0, nbits as usize); // Ymin
        push_bits(&mut bits, height * 20, nbits as usize); // Ymax
        // Pad to byte boundary.
        while bits.len() % 8 != 0 {
            bits.push(0);
        }
        body.extend(pack_bits_msb(&bits));
        body.extend_from_slice(&fr_8_8.to_le_bytes());
        body.extend_from_slice(&frames.to_le_bytes());

        let file_length = (HEADER_LEN + body.len()) as u32;
        buf.extend_from_slice(&file_length.to_le_bytes());
        buf.extend(body);
        buf
    }

    fn push_bits(out: &mut Vec<u8>, value: u32, n: usize) {
        for i in (0..n).rev() {
            out.push(((value >> i) & 1) as u8);
        }
    }

    fn pack_bits_msb(bits: &[u8]) -> Vec<u8> {
        let mut out = vec![0u8; bits.len() / 8];
        for (i, &b) in bits.iter().enumerate() {
            if b != 0 {
                out[i / 8] |= 1 << (7 - (i % 8));
            }
        }
        out
    }

    #[test]
    fn rejects_non_swf() {
        let buf = b"This is definitely not a SWF stream at all";
        let mut fa = FileAnalyze::new(buf);
        assert!(!parse_swf(&mut fa));
    }

    #[test]
    fn parses_minimal_fws() {
        // version 10, 320x240, 30 fps (encoded as 30<<8 = 7680 in 8.8), 120 frames
        let buf = make_fws(10, 320, 240, 30 << 8, 120);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_swf(&mut fa));

        let g = |k: &str| fa.Retrieve(StreamKind::General, 0, k).map(|z| z.as_str().to_owned());
        let v = |k: &str| fa.Retrieve(StreamKind::Video, 0, k).map(|z| z.as_str().to_owned());

        assert_eq!(g("Format").as_deref(), Some("Flash"));
        assert_eq!(g("Format_Version").as_deref(), Some("10"));
        assert_eq!(g("Format_Profile").as_deref(), Some("Uncompressed"));
        assert_eq!(g("VideoCount").as_deref(), Some("1"));
        assert_eq!(v("Width").as_deref(), Some("320"));
        assert_eq!(v("Height").as_deref(), Some("240"));
        assert_eq!(v("FrameRate").as_deref(), Some("30.000"));
        assert_eq!(v("FrameCount").as_deref(), Some("120"));
    }

    #[test]
    fn detects_cws_without_body_parse() {
        // CWS header with a bogus body — parser must accept the signature
        // and populate General without touching the compressed payload.
        let mut buf = Vec::new();
        buf.extend_from_slice(b"CWS");
        buf.push(8); // version
        buf.extend_from_slice(&1234u32.to_le_bytes()); // FileLength
        buf.extend_from_slice(&[0xFF; 16]); // garbage compressed body

        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_swf(&mut fa));
        let g = |k: &str| fa.Retrieve(StreamKind::General, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(g("Format").as_deref(), Some("Flash"));
        assert_eq!(g("Format_Version").as_deref(), Some("8"));
        assert_eq!(g("Format_Profile").as_deref(), Some("Compressed (zlib)"));
        // No Video stream — body wasn't parsed.
        assert_eq!(fa.Count_Get(StreamKind::Video), 0);
    }
}
