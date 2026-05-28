//! OpenMG (Sony OMA / ATRAC3) parser — mirrors `File_OpenMG.cpp`.
//!
//! Layout:
//!   3 bytes  "EA3" magic
//!   1 byte   Flags
//!   2 bytes BE  Size (total header size)
//!  26 bytes  Unknown
//!   1 byte   CodecID  (0,1 = ATRAC3; 3 = MPEG Audio; 4 = PCM; 5 = WMA)
//!  ATRAC3 sub-header (only if CodecID<=1), bit-packed:
//!     7 bits  Unknown
//!     1 bit   JointStereo
//!     3 bits  SamplingRate code
//!     3 bits  Channels code
//!    10 bits  Frame size (in 8-byte blocks)
//!   ... padding to `Size`

use revelio_core::{FileAnalyze, StreamKind};
use zenlib::{int16u, int8u};

const MAGIC_EA3: [u8; 3] = *b"EA3";

fn codec_format(codec_id: u8) -> &'static str {
    match codec_id {
        0 | 1 => "ATRAC3",
        3 => "MPEG Audio",
        4 => "PCM",
        5 => "WMA",
        _ => "",
    }
}

fn codec_encryption(codec_id: u8) -> &'static str {
    match codec_id {
        1 => "SDMI",
        _ => "",
    }
}

fn sampling_rate(code: u8) -> u32 {
    match code {
        0 => 32000,
        1 => 44100,
        2 => 44800,
        3 => 88200,
        4 => 96000,
        _ => 0,
    }
}

fn channels(code: u8) -> u8 {
    // C++: codes 0..=4 map to their value; >=5 adds 1 to account for LFE.
    if code <= 4 { code } else { code + 1 }
}

fn channel_positions(code: u8) -> &'static str {
    match code {
        1 => "Front: C",
        2 => "Front: L R",
        3 => "Front: L R, Side: C",
        4 => "Front: L R, Back: L R",
        5 => "Front: L C R, Side: L R, LFE",
        6 => "Front: L C R, Side: L R, Back: C, LFE",
        7 => "Front: L C R, Side: L R, Back: L R, LFE",
        _ => "",
    }
}

fn channel_layout(code: u8) -> &'static str {
    match code {
        1 => "C",
        2 => "L R",
        3 => "L R S",
        4 => "L R BL BR",
        5 => "L R C SL SR LFE",
        6 => "L R C SL SR BC LFE",
        7 => "L R C SL SR BL BR LFE",
        _ => "",
    }
}

pub fn parse_open_mg(fa: &mut FileAnalyze) -> bool {
    if fa.remain() < 3 {
        return false;
    }
    let head = match fa.peek_raw(fa.remain().min(3)) {
        Some(h) if h.len() == 3 => h,
        _ => return false,
    };
    // Accept both "EA3" and lowercase "ea3" — the task spec mentions both, even
    // though the C++ only checks the uppercase form.
    if head != MAGIC_EA3 && head != *b"ea3" {
        return false;
    }
    // Minimum header is 3 + 1 + 2 + 26 + 1 = 33 bytes before the optional
    // ATRAC3 sub-header.
    if fa.remain() < 33 {
        return false;
    }

    let file_size = fa.remain() as u64;

    fa.element_begin("OpenMG");
    fa.skip_hexa(3, "Code");
    fa.skip_b1("Flags");
    let mut size: int16u = 0;
    fa.get_b2(&mut size, "Size");
    fa.skip_hexa(26, "Unknown");
    let mut codec_id: int8u = 0;
    fa.get_b1(&mut codec_id, "CodecID");

    let mut joint_stereo: u8 = 0;
    let mut sr_code: u8 = 0;
    let mut ch_code: u8 = 0;
    let mut frame_size_raw: int16u = 0;
    if codec_id <= 1 {
        fa.bs_begin();
        fa.skip_s1(7, "Unknown");
        fa.get_s1(1, &mut joint_stereo, "Joint Stereo");
        fa.get_s1(3, &mut sr_code, "Sampling Rate");
        fa.get_s1(3, &mut ch_code, "Channels");
        fa.get_s2(10, &mut frame_size_raw, "Frame size");
        fa.bs_end();
    }
    let consumed = fa.element_offset();
    let header_size = size as usize;
    if header_size > consumed && fa.remain() >= header_size - consumed {
        fa.skip_hexa(header_size - consumed, "Unknown");
    }
    fa.element_end();

    fa.stream_prepare(StreamKind::General);
    fa.fill(StreamKind::General, 0, "Format", "OpenMG", false);

    fa.stream_prepare(StreamKind::Audio);
    let fmt = codec_format(codec_id);
    if !fmt.is_empty() {
        fa.fill(StreamKind::Audio, 0, "Format", fmt, false);
    }
    let enc = codec_encryption(codec_id);
    if !enc.is_empty() {
        fa.fill(StreamKind::Audio, 0, "Encryption", enc, false);
    }

    let header_consumed = (size as u64).max(consumed as u64);
    let stream_size = file_size.saturating_sub(header_consumed);
    fa.fill(StreamKind::Audio, 0, "StreamSize", stream_size.to_string(), false);

    if codec_id <= 1 {
        let ch = channels(ch_code);
        fa.fill(StreamKind::Audio, 0, "Channels", ch.to_string(), false);
        let positions = channel_positions(ch_code);
        if !positions.is_empty() {
            fa.fill(StreamKind::Audio, 0, "ChannelPositions", positions, false);
        }
        let layout = channel_layout(ch_code);
        if !layout.is_empty() {
            fa.fill(StreamKind::Audio, 0, "ChannelLayout", layout, false);
        }
        if ch_code == 1 && joint_stereo != 0 {
            fa.fill(StreamKind::Audio, 0, "Format_Settings_Mode", "Joint Stereo", false);
        }
        let sr = sampling_rate(sr_code);
        if sr != 0 {
            fa.fill(StreamKind::Audio, 0, "SamplingRate", sr.to_string(), false);
        }
        // C++: codec_id==1 (SDMI) adds 1 to FrameSize before the <<3.
        let mut fs = frame_size_raw as u32;
        if codec_id == 1 {
            fs += 1;
        }
        let frame_size_bytes = fs << 3;
        if sr != 0 && frame_size_bytes != 0 {
            let bitrate = (sr as u64) * (frame_size_bytes as u64) / 256;
            fa.fill(StreamKind::Audio, 0, "BitRate", bitrate.to_string(), false);
            if bitrate != 0 {
                let duration_ms = stream_size * 8 * 1000 / bitrate;
                fa.fill(StreamKind::Audio, 0, "Duration", duration_ms.to_string(), false);
            }
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_open_mg(
        magic: &[u8; 3],
        codec_id: u8,
        joint_stereo: bool,
        sr_code: u8,
        ch_code: u8,
        frame_size_raw: u16,
        trailing_audio: usize,
    ) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(magic);
        buf.push(0); // Flags
        // Size = full header length; 33 bytes for non-ATRAC3, 36 with sub-header.
        let header_size: u16 = if codec_id <= 1 { 36 } else { 33 };
        buf.extend_from_slice(&header_size.to_be_bytes());
        buf.extend_from_slice(&[0u8; 26]);
        buf.push(codec_id);
        if codec_id <= 1 {
            // 24-bit big-endian packed field:
            //  7 bits Unknown | 1 bit JointStereo | 3 bits SR | 3 bits CH | 10 bits FrameSize
            let mut packed: u32 = 0;
            packed |= (joint_stereo as u32) << 16;
            packed |= ((sr_code as u32) & 0x7) << 13;
            packed |= ((ch_code as u32) & 0x7) << 10;
            packed |= (frame_size_raw as u32) & 0x3FF;
            buf.push(((packed >> 16) & 0xFF) as u8);
            buf.push(((packed >> 8) & 0xFF) as u8);
            buf.push((packed & 0xFF) as u8);
        }
        buf.resize(buf.len() + trailing_audio, 0);
        buf
    }

    #[test]
    fn rejects_non_openmg_buffer() {
        let mut fa = FileAnalyze::new(b"NOT an OpenMG file at all..............");
        assert!(!parse_open_mg(&mut fa));
    }

    #[test]
    fn parses_atrac3_stereo_44100() {
        // ATRAC3 CBR @ 44100 Hz stereo: frame_size_raw=24 → 192 bytes/frame
        // → bitrate = 44100 * 192 / 256 = 33075 bps.
        let buf = make_open_mg(b"EA3", 0, false, 1, 2, 24, 1000);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_open_mg(&mut fa));

        let g = |k: &str| fa.retrieve(StreamKind::General, 0, k).map(|z| z.as_str().to_owned());
        let a = |k: &str| fa.retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());

        assert_eq!(g("Format").as_deref(), Some("OpenMG"));
        assert_eq!(a("Format").as_deref(), Some("ATRAC3"));
        assert_eq!(a("Channels").as_deref(), Some("2"));
        assert_eq!(a("ChannelPositions").as_deref(), Some("Front: L R"));
        assert_eq!(a("ChannelLayout").as_deref(), Some("L R"));
        assert_eq!(a("SamplingRate").as_deref(), Some("44100"));
        assert_eq!(a("BitRate").as_deref(), Some("33075"));
        assert_eq!(a("StreamSize").as_deref(), Some("1000"));
    }

    #[test]
    fn detects_joint_stereo_mono_and_sdmi_encryption() {
        // CodecID=1 → SDMI encryption; ch_code=1 (mono) + JointStereo → Format_Settings_Mode.
        let buf = make_open_mg(b"EA3", 1, true, 1, 1, 10, 100);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_open_mg(&mut fa));

        let a = |k: &str| fa.retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(a("Format").as_deref(), Some("ATRAC3"));
        assert_eq!(a("Encryption").as_deref(), Some("SDMI"));
        assert_eq!(a("Format_Settings_Mode").as_deref(), Some("Joint Stereo"));
        assert_eq!(a("Channels").as_deref(), Some("1"));
        assert_eq!(a("SamplingRate").as_deref(), Some("44100"));
    }

    #[test]
    fn accepts_lowercase_ea3_magic() {
        let buf = make_open_mg(b"ea3", 0, false, 1, 2, 24, 500);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_open_mg(&mut fa));
        assert_eq!(
            fa.retrieve(StreamKind::Audio, 0, "Format").map(|z| z.as_str().to_owned()).as_deref(),
            Some("ATRAC3")
        );
    }
}
