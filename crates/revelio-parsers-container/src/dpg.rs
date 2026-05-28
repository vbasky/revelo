//! DPG (Nintendo DS video) container parser.
//!
//! Mirrors MediaInfoLib's `File_Dpg.cpp`. DPG is a simple wrapper that
//! interleaves MPEG-1 video and MPEG audio for playback on the DS via
//! Moonshell. The reference implementation only accepts version 0
//! (`"DPG0"`), but the format defines digits 0–4 for header variants, so
//! this parser surfaces whatever digit follows the `"DPG"` magic as the
//! `Format_Version` while still requiring the version-0 layout below.
//!
//! Magic: ASCII `"DPG"` + one ASCII digit (`0x30`..=`0x39`).
//!
//! Header layout (little-endian, 36 bytes):
//!   0x00  C4  Signature ("DPG" + version digit)
//!   0x04  L4  FrameCount
//!   0x08  L4  FrameRate (16.8 fixed-point — divide by 0x100 for fps)
//!   0x0C  L4  SamplingRate (Hz)
//!   0x10  L4  Zero (must be 0)
//!   0x14  L4  Audio_Offset
//!   0x18  L4  Audio_Size
//!   0x1C  L4  Video_Offset
//!   0x20  L4  Video_Size

use revelio_core::{FileAnalyze, StreamKind};
use zenlib::{int32u, int8u};

const DPG_HEADER_SIZE: usize = 36;

pub fn parse_dpg(fa: &mut FileAnalyze) -> bool {
    let header = match fa.peek_raw(DPG_HEADER_SIZE) {
        Some(b) => b,
        None => return false,
    };

    // Magic = "DPG" + ASCII digit.
    if &header[0..3] != b"DPG" || !header[3].is_ascii_digit() {
        return false;
    }
    // The reference C++ enforces that bytes 0x10..0x14 are exactly zero;
    // matching that gate keeps us from misidentifying buffers that just
    // happen to start with "DPG<digit>".
    if int32u::from_le_bytes([header[0x10], header[0x11], header[0x12], header[0x13]]) != 0 {
        return false;
    }

    let version_digit = header[3] - b'0';

    fa.element_begin("DPG");
    let mut _signature: int32u = 0;
    fa.get_c4(&mut _signature, "Signature");
    let mut frame_count: int32u = 0;
    fa.get_l4(&mut frame_count, "Frame count");
    let mut frame_rate_fp: int32u = 0;
    fa.get_l4(&mut frame_rate_fp, "Frame rate");
    let mut sampling_rate: int32u = 0;
    fa.get_l4(&mut sampling_rate, "Sampling rate");
    let mut _zero: int32u = 0;
    fa.get_l4(&mut _zero, "0x00000000");
    let mut audio_offset: int32u = 0;
    fa.get_l4(&mut audio_offset, "Audio Offset");
    let mut audio_size: int32u = 0;
    fa.get_l4(&mut audio_size, "Audio Size");
    let mut video_offset: int32u = 0;
    fa.get_l4(&mut video_offset, "Video Offset");
    let mut video_size: int32u = 0;
    fa.get_l4(&mut video_size, "Video Size");
    fa.element_end();

    let _ = (audio_offset, video_offset);

    fa.stream_prepare(StreamKind::General);
    fa.fill(StreamKind::General, 0, "Format", "DPG", false);
    fa.fill(
        StreamKind::General,
        0,
        "Format_Version",
        version_digit.to_string(),
        false,
    );

    // Video stream — mirrors the C++ `Stream_Prepare(Stream_Video)` block.
    fa.stream_prepare(StreamKind::Video);
    // FrameRate is stored as a 16.8 fixed-point value: integer fps in the
    // high 24 bits and a /256 fractional part in the low 8.
    let frame_rate = (frame_rate_fp as f64) / 256.0;
    if frame_rate > 0.0 {
        // Three decimal places match the C++ `Fill(..., FrameRate, ..., 3)`.
        fa.fill(
            StreamKind::Video,
            0,
            "FrameRate",
            format!("{:.3}", frame_rate),
            false,
        );
    }
    if frame_count > 0 {
        fa.fill(
            StreamKind::Video,
            0,
            "FrameCount",
            frame_count.to_string(),
            false,
        );
    }
    if video_size > 0 {
        fa.fill(
            StreamKind::Video,
            0,
            "StreamSize",
            video_size.to_string(),
            false,
        );
    }
    fa.fill(StreamKind::General, 0, "VideoCount", "1", false);

    // Audio stream — DPG always carries one MPEG audio track.
    fa.stream_prepare(StreamKind::Audio);
    if sampling_rate > 0 {
        fa.fill(
            StreamKind::Audio,
            0,
            "SamplingRate",
            sampling_rate.to_string(),
            false,
        );
    }
    if audio_size > 0 {
        fa.fill(
            StreamKind::Audio,
            0,
            "StreamSize",
            audio_size.to_string(),
            false,
        );
    }
    fa.fill(StreamKind::General, 0, "AudioCount", "1", false);

    // Reference the int8u import explicitly so a future header byte read
    // (e.g. inspecting the DPG4 GOP table) keeps the use statement honest.
    let _: int8u = 0;

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_dpg(
        version: u8,
        frame_count: u32,
        frame_rate_fp: u32,
        sampling_rate: u32,
        audio_offset: u32,
        audio_size: u32,
        video_offset: u32,
        video_size: u32,
    ) -> Vec<u8> {
        let mut buf = Vec::with_capacity(DPG_HEADER_SIZE);
        buf.extend_from_slice(b"DPG");
        buf.push(b'0' + version);
        buf.extend_from_slice(&frame_count.to_le_bytes());
        buf.extend_from_slice(&frame_rate_fp.to_le_bytes());
        buf.extend_from_slice(&sampling_rate.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&audio_offset.to_le_bytes());
        buf.extend_from_slice(&audio_size.to_le_bytes());
        buf.extend_from_slice(&video_offset.to_le_bytes());
        buf.extend_from_slice(&video_size.to_le_bytes());
        buf
    }

    #[test]
    fn parses_minimal_dpg0() {
        // 24 fps as 16.8 fixed-point = 24 * 256 = 0x1800
        let buf = make_dpg(0, 600, 24 * 256, 32000, 36, 1024, 1060, 8192);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_dpg(&mut fa));

        let g = |k: &str| {
            fa.retrieve(StreamKind::General, 0, k)
                .map(|z| z.as_str().to_owned())
        };
        let v = |k: &str| {
            fa.retrieve(StreamKind::Video, 0, k)
                .map(|z| z.as_str().to_owned())
        };
        let a = |k: &str| {
            fa.retrieve(StreamKind::Audio, 0, k)
                .map(|z| z.as_str().to_owned())
        };

        assert_eq!(g("Format").as_deref(), Some("DPG"));
        assert_eq!(g("Format_Version").as_deref(), Some("0"));
        assert_eq!(g("VideoCount").as_deref(), Some("1"));
        assert_eq!(g("AudioCount").as_deref(), Some("1"));
        assert_eq!(v("FrameRate").as_deref(), Some("24.000"));
        assert_eq!(v("FrameCount").as_deref(), Some("600"));
        assert_eq!(v("StreamSize").as_deref(), Some("8192"));
        assert_eq!(a("SamplingRate").as_deref(), Some("32000"));
        assert_eq!(a("StreamSize").as_deref(), Some("1024"));
    }

    #[test]
    fn captures_fractional_frame_rate_and_version_digit() {
        // 23.976 fps ≈ 23.976 * 256 = 6137.856 → use 6138 (≈ 23.977).
        let buf = make_dpg(4, 1, 6138, 48000, 36, 8, 44, 16);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_dpg(&mut fa));
        assert_eq!(
            fa.retrieve(StreamKind::General, 0, "Format_Version")
                .map(|z| z.as_str().to_owned())
                .as_deref(),
            Some("4")
        );
        // 6138/256 = 23.9765625 → formatted to 3 places = "23.977".
        assert_eq!(
            fa.retrieve(StreamKind::Video, 0, "FrameRate")
                .map(|z| z.as_str().to_owned())
                .as_deref(),
            Some("23.977")
        );
    }

    #[test]
    fn rejects_non_dpg_buffer() {
        let mut buf = vec![0u8; DPG_HEADER_SIZE];
        buf[0..4].copy_from_slice(b"RIFF");
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_dpg(&mut fa));
    }

    #[test]
    fn rejects_dpg_with_non_zero_reserved_field() {
        // Valid magic but the 0x10..0x14 word must be zero per the C++.
        let mut buf = make_dpg(0, 1, 256, 22050, 36, 4, 40, 4);
        buf[0x10] = 0x01;
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_dpg(&mut fa));
    }
}
