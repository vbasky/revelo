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

use revelo_core::{FileAnalyze, Reader, StreamKind};

const KATE_MAGIC: &[u8; 8] = b"\x80kate\x00\x00\x00";
const IDENTIFICATION_MIN_SIZE: usize = 64;

/// Parse OggKate overlay codec.
///
/// Detection: `kate\0\0\0\x80` magic.
/// Fills: Format.
pub fn parse_kate(fa: &mut FileAnalyze) -> bool {
    parse(fa).is_some()
}

fn parse(fa: &mut FileAnalyze) -> Option<()> {
    let language;
    let category;
    {
        let r = &mut Reader::wrap(fa);
        let h = r.peek_raw(8)?;
        if h.len() < 8 || h != KATE_MAGIC {
            return None;
        }
        if r.remain() < IDENTIFICATION_MIN_SIZE {
            return None;
        }

        r.element_begin("Kate");
        r.skip(8)?; // Signature

        r.le_u8("Reserved")?;
        r.le_u8("version major")?;
        r.le_u8("version minor")?;
        r.le_u8("num headers")?;
        r.le_u8("text encoding")?;
        r.le_u8("directionality")?;
        r.le_u8("Reserved")?;
        r.le_u8("granule shift")?;
        r.le_u32("Reserved")?;
        r.le_u16("cw sh + canvas width")?;
        r.le_u16("ch sh + canvas height")?;
        r.le_u32("granule rate numerator")?;
        r.le_u32("granule rate denominator")?;

        language = parse_nul_terminated_utf8(r.read_raw(16)?);
        category = parse_nul_terminated_utf8(r.read_raw(16)?);

        r.element_end();
    }

    fill_streams(fa, &language, &category);
    Some(())
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
    fa.stream_prepare(StreamKind::General);
    fa.set_field(StreamKind::General, 0, "Format", "Kate");
    fa.set_field(StreamKind::General, 0, "TextCount", "1");

    fa.stream_prepare(StreamKind::Text);
    fa.set_field(StreamKind::Text, 0, "Format", "Kate");
    fa.set_field(StreamKind::Text, 0, "Codec", "Kate");
    if !language.is_empty() {
        fa.set_field(StreamKind::Text, 0, "Language", language);
    }
    if !category.is_empty() {
        fa.set_field(StreamKind::Text, 0, "Language_More", map_category(category));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_kate_header(language: &str, category: &str) -> Vec<u8> {
        let mut buf = Vec::with_capacity(IDENTIFICATION_MIN_SIZE);
        buf.extend_from_slice(KATE_MAGIC);
        buf.push(0); // reserved
        buf.push(0); // version major
        buf.push(6); // version minor
        buf.push(3); // num headers
        buf.push(0); // text encoding (UTF-8)
        buf.push(0); // directionality
        buf.push(0); // reserved
        buf.push(32); // granule shift
        buf.extend_from_slice(&0u32.to_le_bytes()); // reserved
        buf.extend_from_slice(&0u16.to_le_bytes()); // width
        buf.extend_from_slice(&0u16.to_le_bytes()); // height
        buf.extend_from_slice(&1000u32.to_le_bytes()); // granule num
        buf.extend_from_slice(&1u32.to_le_bytes()); // granule den

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
        let mut fa = FileAnalyze::new(
            b"NOT a Kate header at all............................................",
        );
        assert!(!parse_kate(&mut fa));
    }

    #[test]
    fn parses_subtitle_stream_with_language() {
        let buf = build_kate_header("en_US", "SUB");
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_kate(&mut fa));

        let g = |k: &str| fa.retrieve(StreamKind::General, 0, k).map(|z| z.as_str().to_owned());
        let t = |k: &str| fa.retrieve(StreamKind::Text, 0, k).map(|z| z.as_str().to_owned());

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
        let t = |k: &str| fa.retrieve(StreamKind::Text, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(t("Language").as_deref(), Some("ja"));
        assert_eq!(t("Language_More").as_deref(), Some("Karaoke"));

        let buf2 = build_kate_header("", "ZZZ-CUSTOM");
        let mut fa2 = FileAnalyze::new(&buf2);
        assert!(parse_kate(&mut fa2));
        let t2 = |k: &str| fa2.retrieve(StreamKind::Text, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(t2("Language"), None);
        assert_eq!(t2("Language_More").as_deref(), Some("ZZZ-CUSTOM"));
    }
}
