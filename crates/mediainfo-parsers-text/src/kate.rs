//! Kate (Ogg-Kate) subtitle/karaoke identification-header parser.
//!
//! Kate is normally carried inside Ogg; this parser handles the
//! identification header payload (the first packet of a Kate logical
//! bitstream). All multi-byte integers after the magic are little-endian.
//!
//! Magic: 0x80 + "kate\0\0\0\0" (8 bytes — 0x80 packet-type byte followed
//! by the 7-byte signature, ending with NULs to pad to 8).
//!
//! Identification header layout (64 bytes):
//!    8 bytes : signature (0x80, 'k','a','t','e', 0, 0, 0)
//!    1 byte  : reserved (0)
//!    1 byte  : version major
//!    1 byte  : version minor
//!    1 byte  : num headers
//!    1 byte  : text encoding
//!    1 byte  : directionality
//!    1 byte  : reserved
//!    1 byte  : granule shift
//!    4 bytes : reserved (cw/ch shift + canvas size packing in older spec,
//!              treated as opaque here — File_Kate.cpp also skips it)
//!    2 bytes LE: canvas width (with cw_sh nibble)
//!    2 bytes LE: canvas height (with ch_sh nibble)
//!    4 bytes LE: granule rate numerator
//!    4 bytes LE: granule rate denominator
//!   16 bytes : language (UTF-8, NUL-padded)
//!   16 bytes : category (UTF-8, NUL-padded)

use mediainfo_core::{FileAnalyze, StreamKind};
use zenlib::{int8u, int16u, int32u};

const KATE_MAGIC: &[u8; 8] = b"\x80kate\x00\x00\x00";
const IDENTIFICATION_MIN_SIZE: usize = 64;

pub fn parse_kate(fa: &mut FileAnalyze) -> bool {
    let head = fa.peek_raw(fa.Remain().min(8));
    let Some(h) = head else { return false };
    if h.len() < 8 || h != KATE_MAGIC {
        return false;
    }
    if fa.Remain() < IDENTIFICATION_MIN_SIZE {
        return false;
    }

    fa.Element_Begin("Kate");
    fa.Skip_Hexa(8, "Signature");

    let mut _reserved0: int8u = 0;
    let mut _version_major: int8u = 0;
    let mut _version_minor: int8u = 0;
    let mut _num_headers: int8u = 0;
    let mut _text_encoding: int8u = 0;
    let mut _directionality: int8u = 0;
    let mut _reserved1: int8u = 0;
    let mut _granule_shift: int8u = 0;
    let mut _width: int16u = 0;
    let mut _height: int16u = 0;
    let mut _gr_num: int32u = 0;
    let mut _gr_den: int32u = 0;

    fa.Get_L1(&mut _reserved0, "Reserved");
    fa.Get_L1(&mut _version_major, "version major");
    fa.Get_L1(&mut _version_minor, "version minor");
    fa.Get_L1(&mut _num_headers, "num headers");
    fa.Get_L1(&mut _text_encoding, "text encoding");
    fa.Get_L1(&mut _directionality, "directionality");
    fa.Get_L1(&mut _reserved1, "Reserved");
    fa.Get_L1(&mut _granule_shift, "granule shift");
    fa.Skip_L4("Reserved");
    fa.Get_L2(&mut _width, "cw sh + canvas width");
    fa.Get_L2(&mut _height, "ch sh + canvas height");
    fa.Get_L4(&mut _gr_num, "granule rate numerator");
    fa.Get_L4(&mut _gr_den, "granule rate denominator");

    let lang_bytes = fa.read_raw(16).to_vec();
    let cat_bytes = fa.read_raw(16).to_vec();
    let language = parse_nul_terminated_utf8(&lang_bytes);
    let category = parse_nul_terminated_utf8(&cat_bytes);

    fa.Element_End();

    fill_streams(fa, &language, &category);
    true
}

fn parse_nul_terminated_utf8(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

// http://wiki.xiph.org/index.php/OggText#Categories_of_Text_Codecs — mirrors
// the Kate_Category mapping in File_Kate.cpp so MediaInfo output matches.
fn map_category(category: &str) -> &str {
    match category {
        "CC" => "Closed caption",
        "SUB" => "Subtitles",
        "TAD" => "Textual audio descriptions",
        "KTV" => "Karaoke",
        "TIK" => "Ticker text",
        "AR" => "Active regions",
        "NB" => "Semantic annotations",
        "META" => "Metadata, mostly machine-readable",
        "TRX" => "Transcript",
        "LRC" => "Lyrics",
        "LIN" => "Linguistic markup",
        "CUE" => "Cue points",
        "K-SLD-I" => "Slides, as images",
        "K-SLD-T" => "Slides, as text",
        other => other,
    }
}

fn fill_streams(fa: &mut FileAnalyze, language: &str, category: &str) {
    fa.Stream_Prepare(StreamKind::General);
    fa.Fill(StreamKind::General, 0, "Format", "Kate", false);
    fa.Fill(StreamKind::General, 0, "TextCount", "1", false);

    fa.Stream_Prepare(StreamKind::Text);
    fa.Fill(StreamKind::Text, 0, "Format", "Kate", false);
    fa.Fill(StreamKind::Text, 0, "Codec", "Kate", false);
    if !language.is_empty() {
        fa.Fill(StreamKind::Text, 0, "Language", language, false);
    }
    if !category.is_empty() {
        fa.Fill(StreamKind::Text, 0, "Language_More", map_category(category), false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_kate_header(language: &str, category: &str) -> Vec<u8> {
        let mut buf = Vec::with_capacity(IDENTIFICATION_MIN_SIZE);
        buf.extend_from_slice(KATE_MAGIC);
        buf.push(0);    // reserved
        buf.push(0);    // version major
        buf.push(6);    // version minor
        buf.push(3);    // num headers
        buf.push(0);    // text encoding (UTF-8)
        buf.push(0);    // directionality
        buf.push(0);    // reserved
        buf.push(32);   // granule shift
        buf.extend_from_slice(&0u32.to_le_bytes());     // reserved
        buf.extend_from_slice(&0u16.to_le_bytes());     // width
        buf.extend_from_slice(&0u16.to_le_bytes());     // height
        buf.extend_from_slice(&1000u32.to_le_bytes());  // granule num
        buf.extend_from_slice(&1u32.to_le_bytes());     // granule den

        let mut lang = [0u8; 16];
        let lb = language.as_bytes();
        let n = lb.len().min(16);
        lang[..n].copy_from_slice(&lb[..n]);
        buf.extend_from_slice(&lang);

        let mut cat = [0u8; 16];
        let cb = category.as_bytes();
        let n = cb.len().min(16);
        cat[..n].copy_from_slice(&cb[..n]);
        buf.extend_from_slice(&cat);

        buf
    }

    #[test]
    fn rejects_non_kate() {
        let mut fa = FileAnalyze::new(b"NOT a Kate header at all............................................");
        assert!(!parse_kate(&mut fa));
    }

    #[test]
    fn parses_subtitle_stream_with_language() {
        let buf = build_kate_header("en_US", "SUB");
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_kate(&mut fa));

        let g = |k: &str| fa.Retrieve(StreamKind::General, 0, k).map(|z| z.as_str().to_owned());
        let t = |k: &str| fa.Retrieve(StreamKind::Text, 0, k).map(|z| z.as_str().to_owned());

        assert_eq!(g("Format").as_deref(), Some("Kate"));
        assert_eq!(g("TextCount").as_deref(), Some("1"));
        assert_eq!(t("Format").as_deref(), Some("Kate"));
        assert_eq!(t("Codec").as_deref(), Some("Kate"));
        assert_eq!(t("Language").as_deref(), Some("en_US"));
        assert_eq!(t("Language_More").as_deref(), Some("Subtitles"));
    }

    #[test]
    fn maps_karaoke_category_and_passes_through_unknown() {
        let buf = build_kate_header("ja", "KTV");
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_kate(&mut fa));
        let t = |k: &str| fa.Retrieve(StreamKind::Text, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(t("Language").as_deref(), Some("ja"));
        assert_eq!(t("Language_More").as_deref(), Some("Karaoke"));

        let buf2 = build_kate_header("", "ZZZ-CUSTOM");
        let mut fa2 = FileAnalyze::new(&buf2);
        assert!(parse_kate(&mut fa2));
        let t2 = |k: &str| fa2.Retrieve(StreamKind::Text, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(t2("Language"), None);
        assert_eq!(t2("Language_More").as_deref(), Some("ZZZ-CUSTOM"));
    }
}
