use revelo_core::{FileAnalyze, StreamKind};

/// Parse BBC Dirac / VC-2 video.
///
/// Detection: `BBCD` parse-info magic. A sequence header (parse code 0)
/// carries the source parameters: dimensions, chroma, scan, and frame rate.
/// The integers are the VC-2 interleaved unsigned exp-Golomb code.
pub fn parse_dirac(fa: &mut FileAnalyze) -> bool {
    let Some(buf) = fa.peek_raw(4) else { return false };
    if &buf[0..4] != b"BBCD" {
        return false;
    }
    let pos = fa.stream_prepare(StreamKind::Video);
    fa.set_field(StreamKind::Video, pos, "Format", "Dirac");
    fa.set_field(StreamKind::Video, pos, "Format_Info", "BBC Dirac wavelet");

    // A failed exact peek still records the requested length. The 6-byte
    // detection fixture must keep max_request_len at 4, so the 13-byte
    // parse-info header is read only when the file actually has it.
    if fa.remain() >= 13
        && let Some(hdr) = fa.peek_raw(13)
    {
        let parse_code = hdr[4];
        let next = u32::from_be_bytes([hdr[5], hdr[6], hdr[7], hdr[8]]) as usize;
        if parse_code == 0
            && (13..4096).contains(&next)
            && fa.remain() >= next
            && let Some(unit) = fa.peek_raw(next)
            && let Some(seq) = parse_sequence_header(&unit[13..])
        {
            if seq.width > 0 {
                fa.set_field(StreamKind::Video, pos, "Width", seq.width.to_string());
            }
            if seq.height > 0 {
                fa.set_field(StreamKind::Video, pos, "Height", seq.height.to_string());
            }
            if seq.rate_den > 0 && seq.rate_num > 0 {
                let rate = seq.rate_num as f64 / seq.rate_den as f64;
                fa.set_field(StreamKind::Video, pos, "FrameRate", format!("{rate:.3}"));
                fa.set_field(StreamKind::Video, pos, "FrameRate_Num", seq.rate_num.to_string());
                fa.set_field(StreamKind::Video, pos, "FrameRate_Den", seq.rate_den.to_string());
            }
            if let Some(chroma) = chroma_name(seq.chroma) {
                fa.set_field(StreamKind::Video, pos, "ChromaSubsampling", chroma);
            }
            fa.set_field(
                StreamKind::Video,
                pos,
                "ScanType",
                if seq.progressive { "Progressive" } else { "Interlaced" },
            );
        }
    }
    true
}

struct DiracSeq {
    width: u32,
    height: u32,
    chroma: u32,
    progressive: bool,
    rate_num: u32,
    rate_den: u32,
}

struct Preset {
    width: u32,
    height: u32,
    chroma: u32,
    interlaced: u32,
    rate_index: u32,
}

// dirac_source_parameters_defaults: width, height, chroma (0=444, 1=422,
// 2=420), interlaced, frame-rate index. Index 0 is custom dimensions in
// the generated sample; the preset itself is 640×480.
const PRESETS: [Preset; 21] = [
    Preset { width: 640, height: 480, chroma: 2, interlaced: 0, rate_index: 1 },
    Preset { width: 176, height: 120, chroma: 2, interlaced: 0, rate_index: 9 },
    Preset { width: 176, height: 144, chroma: 2, interlaced: 1, rate_index: 10 },
    Preset { width: 352, height: 240, chroma: 2, interlaced: 0, rate_index: 9 },
    Preset { width: 352, height: 288, chroma: 2, interlaced: 1, rate_index: 10 },
    Preset { width: 704, height: 480, chroma: 2, interlaced: 0, rate_index: 9 },
    Preset { width: 704, height: 576, chroma: 2, interlaced: 1, rate_index: 10 },
    Preset { width: 720, height: 480, chroma: 1, interlaced: 1, rate_index: 4 },
    Preset { width: 720, height: 576, chroma: 1, interlaced: 1, rate_index: 3 },
    Preset { width: 1280, height: 720, chroma: 1, interlaced: 0, rate_index: 7 },
    Preset { width: 1280, height: 720, chroma: 1, interlaced: 0, rate_index: 6 },
    Preset { width: 1920, height: 1080, chroma: 1, interlaced: 1, rate_index: 4 },
    Preset { width: 1920, height: 1080, chroma: 1, interlaced: 1, rate_index: 3 },
    Preset { width: 1920, height: 1080, chroma: 1, interlaced: 0, rate_index: 7 },
    Preset { width: 1920, height: 1080, chroma: 1, interlaced: 0, rate_index: 6 },
    Preset { width: 2048, height: 1080, chroma: 0, interlaced: 0, rate_index: 2 },
    Preset { width: 4096, height: 2160, chroma: 0, interlaced: 0, rate_index: 2 },
    Preset { width: 3840, height: 2160, chroma: 1, interlaced: 0, rate_index: 7 },
    Preset { width: 3840, height: 2160, chroma: 1, interlaced: 0, rate_index: 6 },
    Preset { width: 7680, height: 4320, chroma: 1, interlaced: 0, rate_index: 7 },
    Preset { width: 7680, height: 4320, chroma: 1, interlaced: 0, rate_index: 6 },
];

fn parse_sequence_header(payload: &[u8]) -> Option<DiracSeq> {
    let mut r = Ue::new(payload);
    let _major = r.ue()?;
    let _minor = r.ue()?;
    let _profile = r.ue()?;
    let _level = r.ue()?;
    let base = r.ue()?;
    let preset = PRESETS.get(base as usize)?;

    let mut width = preset.width;
    let mut height = preset.height;
    let mut chroma = preset.chroma;
    let mut scan = preset.interlaced;
    let mut rate_index = preset.rate_index;
    let mut rate_num = 0u32;
    let mut rate_den = 1u32;

    if r.flag()? {
        width = r.ue()?;
        height = r.ue()?;
    }
    if r.flag()? {
        chroma = r.ue()?;
        if chroma > 2 {
            return None;
        }
    }
    if r.flag()? {
        scan = r.ue()?;
        if scan > 1 {
            return None;
        }
    }
    if r.flag()? {
        rate_index = r.ue()?;
        if rate_index > 10 {
            return None;
        }
        if rate_index == 0 {
            rate_num = r.ue()?;
            rate_den = r.ue()?;
            if rate_den == 0 {
                return None;
            }
        }
    }
    let (rate_num, rate_den) = frame_rate(rate_index, rate_num, rate_den)?;
    Some(DiracSeq { width, height, chroma, progressive: scan == 0, rate_num, rate_den })
}

fn frame_rate(index: u32, num: u32, den: u32) -> Option<(u32, u32)> {
    Some(match index {
        0 => (num, den),
        1 => (24000, 1001),
        2 => (24, 1),
        3 => (25, 1),
        4 => (30000, 1001),
        5 => (30, 1),
        6 => (50, 1),
        7 => (60000, 1001),
        8 => (60, 1),
        9 => (15000, 1001),
        10 => (25, 2),
        _ => return None,
    })
}

fn chroma_name(chroma: u32) -> Option<&'static str> {
    match chroma {
        0 => Some("4:4:4"),
        1 => Some("4:2:2"),
        2 => Some("4:2:0"),
        _ => None,
    }
}

/// VC-2 interleaved unsigned exp-Golomb (FFmpeg `get_interleaved_ue_golomb`).
struct Ue<'a> {
    data: &'a [u8],
    bit: usize,
}

impl<'a> Ue<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, bit: 0 }
    }

    fn bit(&mut self) -> Option<u32> {
        if self.bit / 8 >= self.data.len() {
            return None;
        }
        let b = u32::from((self.data[self.bit / 8] >> (7 - (self.bit % 8))) & 1);
        self.bit += 1;
        Some(b)
    }

    fn flag(&mut self) -> Option<bool> {
        self.bit().map(|b| b == 1)
    }

    fn ue(&mut self) -> Option<u32> {
        let mut value = 1u32;
        loop {
            if self.bit()? == 1 {
                break;
            }
            let next = self.bit()?;
            value = value.checked_shl(1)?.checked_add(next)?;
            if value > 1_000_000 {
                return None;
            }
        }
        Some(value - 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test() {
        let buf = b"BBCD\x00\x00".to_vec();
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_dirac(&mut fa));
        assert_eq!(fa.access_stats().max_request_len, 4);
    }

    #[test]
    fn sequence_header_reads_custom_dimensions() {
        // Parse code 0, next offset 29, then the ffmpeg vc2 sequence header
        // for 320×240, 24000/1001, 4:2:0, progressive.
        let mut buf =
            vec![b'B', b'B', b'C', b'D', 0x00, 0x00, 0x00, 0x00, 0x1d, 0x00, 0x00, 0x00, 0x00];
        buf.extend_from_slice(&[
            0x70, 0x87, 0x10, 0x01, 0xaa, 0x03, 0xbf, 0x15, 0x15, 0x00, 0x1a, 0xa8, 0x89, 0xc9,
            0x5f, 0xfc,
        ]);
        assert_eq!(buf.len(), 29);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_dirac(&mut fa));
        let v = |k: &str| fa.retrieve(StreamKind::Video, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(v("Format").as_deref(), Some("Dirac"));
        assert_eq!(v("Width").as_deref(), Some("320"));
        assert_eq!(v("Height").as_deref(), Some("240"));
        assert_eq!(v("FrameRate").as_deref(), Some("23.976"));
        assert_eq!(v("FrameRate_Num").as_deref(), Some("24000"));
        assert_eq!(v("FrameRate_Den").as_deref(), Some("1001"));
        assert_eq!(v("ChromaSubsampling").as_deref(), Some("4:2:0"));
        assert_eq!(v("ScanType").as_deref(), Some("Progressive"));
    }
}
