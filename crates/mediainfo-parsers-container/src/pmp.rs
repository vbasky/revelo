//! PMP (PSP Movie Player) container parser.
//!
//! Mirrors MediaInfoLib's `File_Pmp.cpp`. PMP is a flat container used by
//! the PSP Movie Player homebrew app, with a fixed-size little-endian
//! header carrying one video and one audio stream description.
//!
//! Magic: ASCII `"pmpm"` (0x70 0x6D 0x70 0x6D).
//!
//! Header layout (version 1, little-endian, 48 bytes):
//!   0x00  C4  Signature ("pmpm")
//!   0x04  L4  Version
//!   0x08  L4  video_format (0 = MPEG-4 Visual, 1 = AVC)
//!   0x0C  L4  number of frames
//!   0x10  L4  video_width
//!   0x14  L4  video_height
//!   0x18  L4  time_base_num
//!   0x1C  L4  time_base_den
//!   0x20  L4  number of audio streams
//!   0x24  L4  audio_format (0 = MPEG Audio, 1 = AAC)
//!   0x28  L4  channels
//!   0x2C  L4  unknown
//!   0x30  L4  sample_rate

use mediainfo_core::{FileAnalyze, StreamKind};
use zenlib::int32u;

const PMP_MAGIC_SIZE: usize = 4;
const PMP_V1_HEADER_SIZE: usize = 52;

fn pmp_video_format(video_format: int32u) -> &'static str {
    match video_format {
        0 => "MPEG-4 Visual",
        1 => "AVC",
        _ => "",
    }
}

fn pmp_audio_format(audio_format: int32u) -> &'static str {
    match audio_format {
        0 => "MPEG Audio",
        1 => "AAC",
        _ => "",
    }
}

pub fn parse_pmp(fa: &mut FileAnalyze) -> bool {
    let peek_len = fa.Remain().min(PMP_V1_HEADER_SIZE);
    let header = match fa.peek_raw(peek_len) {
        Some(b) => b,
        None => return false,
    };
    if header.len() < PMP_MAGIC_SIZE {
        return false;
    }
    if &header[0..4] != b"pmpm" {
        return false;
    }

    fa.Element_Begin("PMP");
    let mut _signature: int32u = 0;
    fa.Get_C4(&mut _signature, "Signature");
    let mut version: int32u = 0;
    fa.Get_L4(&mut version, "Version");

    let mut video_format: int32u = 0;
    let mut nb_frames: int32u = 0;
    let mut video_width: int32u = 0;
    let mut video_height: int32u = 0;
    let mut time_base_num: int32u = 0;
    let mut time_base_den: int32u = 0;
    let mut audio_format: int32u = 0;
    let mut channels: int32u = 0;
    let mut sample_rate: int32u = 0;

    if version == 1 && fa.Remain() >= PMP_V1_HEADER_SIZE - 8 {
        fa.Get_L4(&mut video_format, "video_format");
        fa.Get_L4(&mut nb_frames, "number of frames");
        fa.Get_L4(&mut video_width, "video_width");
        fa.Get_L4(&mut video_height, "video_height");
        fa.Get_L4(&mut time_base_num, "time_base_num");
        fa.Get_L4(&mut time_base_den, "time_base_den");
        let mut _nb_audio: int32u = 0;
        fa.Get_L4(&mut _nb_audio, "number of audio streams");
        fa.Get_L4(&mut audio_format, "audio_format");
        fa.Get_L4(&mut channels, "channels");
        let mut _unknown: int32u = 0;
        fa.Get_L4(&mut _unknown, "unknown");
        fa.Get_L4(&mut sample_rate, "sample_rate");
    }
    fa.Element_End();

    fa.Stream_Prepare(StreamKind::General);
    fa.Fill(StreamKind::General, 0, "Format", "PMP", false);

    if version == 1 {
        fa.Stream_Prepare(StreamKind::Video);
        let vfmt = pmp_video_format(video_format);
        if !vfmt.is_empty() {
            fa.Fill(StreamKind::Video, 0, "Format", vfmt, false);
        }
        if nb_frames > 0 {
            fa.Fill(StreamKind::Video, 0, "FrameCount", nb_frames.to_string(), false);
        }
        if video_width > 0 {
            fa.Fill(StreamKind::Video, 0, "Width", video_width.to_string(), false);
        }
        if video_height > 0 {
            fa.Fill(StreamKind::Video, 0, "Height", video_height.to_string(), false);
        }
        // The reference C++ uses `(float)time_base_den / 100` directly; we
        // mirror it verbatim — the `time_base_num` field is parsed but not
        // consumed in the upstream FrameRate calculation.
        let _ = time_base_num;
        if time_base_den > 0 {
            let frame_rate = (time_base_den as f64) / 100.0;
            fa.Fill(
                StreamKind::Video,
                0,
                "FrameRate",
                format!("{:.3}", frame_rate),
                false,
            );
        }
        fa.Fill(StreamKind::General, 0, "VideoCount", "1", false);

        fa.Stream_Prepare(StreamKind::Audio);
        let afmt = pmp_audio_format(audio_format);
        if !afmt.is_empty() {
            fa.Fill(StreamKind::Audio, 0, "Format", afmt, false);
        }
        if channels > 0 {
            fa.Fill(StreamKind::Audio, 0, "Channels", channels.to_string(), false);
        }
        if sample_rate > 0 {
            fa.Fill(
                StreamKind::Audio,
                0,
                "SamplingRate",
                sample_rate.to_string(),
                false,
            );
        }
        fa.Fill(StreamKind::General, 0, "AudioCount", "1", false);
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_pmp_v1(
        video_format: u32,
        nb_frames: u32,
        width: u32,
        height: u32,
        time_base_num: u32,
        time_base_den: u32,
        nb_audio: u32,
        audio_format: u32,
        channels: u32,
        sample_rate: u32,
    ) -> Vec<u8> {
        let mut buf = Vec::with_capacity(PMP_V1_HEADER_SIZE);
        buf.extend_from_slice(b"pmpm");
        buf.extend_from_slice(&1u32.to_le_bytes());
        buf.extend_from_slice(&video_format.to_le_bytes());
        buf.extend_from_slice(&nb_frames.to_le_bytes());
        buf.extend_from_slice(&width.to_le_bytes());
        buf.extend_from_slice(&height.to_le_bytes());
        buf.extend_from_slice(&time_base_num.to_le_bytes());
        buf.extend_from_slice(&time_base_den.to_le_bytes());
        buf.extend_from_slice(&nb_audio.to_le_bytes());
        buf.extend_from_slice(&audio_format.to_le_bytes());
        buf.extend_from_slice(&channels.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&sample_rate.to_le_bytes());
        buf
    }

    #[test]
    fn parses_minimal_pmp_v1_avc_aac() {
        // 30 fps → time_base_den = 3000 (per C++ `(float)den/100`).
        let buf = make_pmp_v1(1, 1500, 480, 272, 1, 3000, 1, 1, 2, 44100);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_pmp(&mut fa));

        let g = |k: &str| {
            fa.Retrieve(StreamKind::General, 0, k)
                .map(|z| z.as_str().to_owned())
        };
        let v = |k: &str| {
            fa.Retrieve(StreamKind::Video, 0, k)
                .map(|z| z.as_str().to_owned())
        };
        let a = |k: &str| {
            fa.Retrieve(StreamKind::Audio, 0, k)
                .map(|z| z.as_str().to_owned())
        };

        assert_eq!(g("Format").as_deref(), Some("PMP"));
        assert_eq!(g("VideoCount").as_deref(), Some("1"));
        assert_eq!(g("AudioCount").as_deref(), Some("1"));

        assert_eq!(v("Format").as_deref(), Some("AVC"));
        assert_eq!(v("FrameCount").as_deref(), Some("1500"));
        assert_eq!(v("Width").as_deref(), Some("480"));
        assert_eq!(v("Height").as_deref(), Some("272"));
        assert_eq!(v("FrameRate").as_deref(), Some("30.000"));

        assert_eq!(a("Format").as_deref(), Some("AAC"));
        assert_eq!(a("Channels").as_deref(), Some("2"));
        assert_eq!(a("SamplingRate").as_deref(), Some("44100"));
    }

    #[test]
    fn parses_mpeg4_visual_with_mpeg_audio() {
        let buf = make_pmp_v1(0, 600, 320, 240, 1, 2997, 1, 0, 1, 22050);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_pmp(&mut fa));
        assert_eq!(
            fa.Retrieve(StreamKind::Video, 0, "Format")
                .map(|z| z.as_str().to_owned())
                .as_deref(),
            Some("MPEG-4 Visual")
        );
        assert_eq!(
            fa.Retrieve(StreamKind::Audio, 0, "Format")
                .map(|z| z.as_str().to_owned())
                .as_deref(),
            Some("MPEG Audio")
        );
        // 2997 / 100 = 29.97
        assert_eq!(
            fa.Retrieve(StreamKind::Video, 0, "FrameRate")
                .map(|z| z.as_str().to_owned())
                .as_deref(),
            Some("29.970")
        );
    }

    #[test]
    fn rejects_non_pmp_buffer() {
        let buf = b"RIFFxxxxWAVE........";
        let mut fa = FileAnalyze::new(buf);
        assert!(!parse_pmp(&mut fa));
    }
}
