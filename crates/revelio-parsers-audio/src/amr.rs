//! AMR (Adaptive Multi-Rate) speech codec — file header magic detect.
//!
//! Magic strings (all start with "#!AMR"):
//!   "#!AMR\n"          AMR-NB single channel
//!   "#!AMR_MC1.0\n"    AMR-NB multichannel
//!   "#!AMR-WB\n"       AMR-WB single channel
//!   "#!AMR-WB_MC1.0\n" AMR-WB multichannel

use revelio_core::{FileAnalyze, StreamKind};

const AMR_PREFIX: &[u8; 5] = b"#!AMR";
const MC_TAIL: &[u8; 7] = b"_MC1.0\n";

pub fn parse_amr(fa: &mut FileAnalyze) -> bool {
    let head = fa.peek_raw(fa.remain().min(16));
    let Some(h) = head else { return false };
    if h.len() < 6 || &h[0..5] != AMR_PREFIX {
        return false;
    }

    let (is_wb, header_size, channels) = if h.len() >= 8 && &h[5..8] == b"-WB" {
        // Wide-band variant: byte 8 is either '\n' or '_' (multichannel).
        if h.len() >= 9 && h[8] == b'\n' {
            (true, 9usize, 1u16)
        } else if h.len() >= 15 && &h[8..15] == MC_TAIL {
            (true, 15usize, 2u16)
        } else {
            return false;
        }
    } else if h[5] == b'\n' {
        (false, 6usize, 1u16)
    } else if h.len() >= 12 && &h[5..12] == MC_TAIL {
        (false, 12usize, 2u16)
    } else {
        return false;
    };

    let file_size = fa.remain();
    fa.skip_hexa(header_size, "AMR_Magic");

    fill_streams(fa, is_wb, channels, header_size, file_size);
    true
}

fn fill_streams(
    fa: &mut FileAnalyze,
    is_wb: bool,
    channels: u16,
    header_size: usize,
    file_size: usize,
) {
    fa.stream_prepare(StreamKind::General);
    fa.fill(StreamKind::General, 0, "Format", "AMR", false);
    fa.fill(StreamKind::General, 0, "AudioCount", "1", false);
    fa.fill(StreamKind::General, 0, "StreamSize", header_size.to_string(), true);

    fa.stream_prepare(StreamKind::Audio);
    fa.fill(StreamKind::Audio, 0, "Format", "AMR", false);
    fa.fill(StreamKind::Audio, 0, "Codec", "AMR", false);
    if is_wb {
        fa.fill(StreamKind::Audio, 0, "Format_Profile", "Wide band", false);
        fa.fill(StreamKind::Audio, 0, "SamplingRate", "16000", false);
        fa.fill(StreamKind::Audio, 0, "BitDepth", "14", false);
    } else {
        fa.fill(StreamKind::Audio, 0, "Format_Profile", "Narrow band", false);
        fa.fill(StreamKind::Audio, 0, "SamplingRate", "8000", false);
        fa.fill(StreamKind::Audio, 0, "BitDepth", "13", false);
    }
    fa.fill(StreamKind::Audio, 0, "Channels", channels.to_string(), false);
    fa.fill(StreamKind::Audio, 0, "Compression_Mode", "Lossy", false);
    // AMR uses variable per-frame mode codes; bitrate varies frame-to-frame
    // unless every frame happens to share the same mode (not detected here).
    fa.fill(StreamKind::Audio, 0, "BitRate_Mode", "VBR", false);
    let audio_bytes = file_size.saturating_sub(header_size);
    fa.fill(StreamKind::Audio, 0, "StreamSize", audio_bytes.to_string(), false);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_amr() {
        let mut fa = FileAnalyze::new(b"NOT AMR FILE");
        assert!(!parse_amr(&mut fa));
    }

    #[test]
    fn parses_amr_nb_minimal() {
        let mut buf = Vec::new();
        buf.extend_from_slice(b"#!AMR\n");
        buf.extend_from_slice(&[0x00; 32]);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_amr(&mut fa));
        let s = fa.streams();
        assert_eq!(
            s.retrieve(StreamKind::Audio, 0, "Format_Profile").map(|z| z.as_str()),
            Some("Narrow band")
        );
        assert_eq!(
            s.retrieve(StreamKind::Audio, 0, "SamplingRate").map(|z| z.as_str()),
            Some("8000")
        );
        assert_eq!(
            s.retrieve(StreamKind::Audio, 0, "Channels").map(|z| z.as_str()),
            Some("1")
        );
        assert_eq!(
            s.retrieve(StreamKind::Audio, 0, "BitRate_Mode").map(|z| z.as_str()),
            Some("VBR")
        );
    }

    #[test]
    fn parses_amr_wb_minimal() {
        let mut buf = Vec::new();
        buf.extend_from_slice(b"#!AMR-WB\n");
        buf.extend_from_slice(&[0x00; 32]);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_amr(&mut fa));
        let s = fa.streams();
        assert_eq!(
            s.retrieve(StreamKind::Audio, 0, "Format_Profile").map(|z| z.as_str()),
            Some("Wide band")
        );
        assert_eq!(
            s.retrieve(StreamKind::Audio, 0, "SamplingRate").map(|z| z.as_str()),
            Some("16000")
        );
    }
}
