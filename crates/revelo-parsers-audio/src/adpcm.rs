//! ADPCM (Adaptive Differential PCM) parser.
//!
//! ADPCM has no file-level magic — in the C++ engine (`File_Adpcm.cpp`)
//! `Read_Buffer_Continue` explicitly notes "It is impossible to detect"
//! and is only ever instantiated as a sub-parser from container formats
//! (RIFF/WAV, AVI, MOV) once the codec ID is already known. The parser
//! simply declares Format=ADPCM on General and Audio streams plus
//! Compression_Mode=Lossy and BitRate_Mode=CBR.
//!
//! This Rust port mirrors that behavior: any non-empty buffer is
//! accepted and the same fields are filled. Because there is no magic
//! to validate against, callers must place it at the very end of the
//! dispatch chain so a real format isn't overridden. Codec-specific
//! Profile/Firm tags (A-Law/U-Law/IMA/Unisys) come from the container
//! supplying the codec ID and are populated by the container parser.

use revelo_core::{FileAnalyze, StreamKind};

pub fn parse_adpcm(fa: &mut FileAnalyze) -> bool {
    if fa.remain() == 0 {
        return false;
    }

    fa.stream_prepare(StreamKind::General);
    fa.set_field(StreamKind::General, 0, "Format", "ADPCM");
    fa.set_field(StreamKind::General, 0, "AudioCount", "1");

    fa.stream_prepare(StreamKind::Audio);
    fa.set_field(StreamKind::Audio, 0, "Format", "ADPCM");
    fa.set_field(StreamKind::Audio, 0, "Codec", "ADPCM");
    fa.set_field(StreamKind::Audio, 0, "Compression_Mode", "Lossy");
    fa.set_field(StreamKind::Audio, 0, "BitRate_Mode", "CBR");
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_empty_buffer() {
        let mut fa = FileAnalyze::new(b"");
        assert!(!parse_adpcm(&mut fa));
    }

    #[test]
    fn accepts_any_non_empty_buffer() {
        let mut fa = FileAnalyze::new(&[0x00, 0x01, 0x02, 0x03]);
        assert!(parse_adpcm(&mut fa));
        let g = |k: &str| fa.retrieve(StreamKind::General, 0, k).map(|z| z.as_str().to_owned());
        let a = |k: &str| fa.retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(g("Format").as_deref(), Some("ADPCM"));
        assert_eq!(a("Format").as_deref(), Some("ADPCM"));
        assert_eq!(a("Codec").as_deref(), Some("ADPCM"));
        assert_eq!(a("Compression_Mode").as_deref(), Some("Lossy"));
        assert_eq!(a("BitRate_Mode").as_deref(), Some("CBR"));
    }

    #[test]
    fn fills_one_audio_stream() {
        let mut fa = FileAnalyze::new(&[0xAAu8; 64]);
        assert!(parse_adpcm(&mut fa));
        assert_eq!(fa.stream_count(StreamKind::Audio), 1);
        assert_eq!(fa.stream_count(StreamKind::General), 1);
        assert_eq!(fa.stream_count(StreamKind::Video), 0);
    }
}
