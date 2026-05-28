//! FFV1 lossless video parser. Detects FFV1 magic bytes "FFV1".

use mediainfo_core::{FileAnalyze, StreamKind};

pub fn parse_ffv1(fa: &mut FileAnalyze) -> bool {
    if fa.Remain() < 4 {
        return false;
    }

    let bytes = fa.peek_raw(4);
    let Some(bytes) = bytes else { return false };
    let found = bytes == [0x46, 0x46, 0x56, 0x31];

    if !found {
        return false;
    }

    let mut _magic: u32 = 0;
    fa.Get_B4(&mut _magic, "magic");

    fa.Element_Begin("FFV1");
    let mut version: u8 = 0;
    let mut coder_type: u8 = 0;
    let mut colorspace_type: u8 = 0;
    if fa.Remain() >= 3 {
        fa.Get_B1(&mut version, "version");
        fa.Get_B1(&mut coder_type, "coder_type");
        fa.Get_B1(&mut colorspace_type, "colorspace_type");
    }
    fa.Element_End();

    fa.Stream_Prepare(StreamKind::Video);
    fa.Fill(StreamKind::Video, 0, "Format", "FFV1", false);
    if version > 0 { fa.Fill(StreamKind::Video, 0, "Format_Version", version.to_string(), false); }
    fa.Fill(StreamKind::Video, 0, "Compression_Mode", "Lossless", false);
    true
}
