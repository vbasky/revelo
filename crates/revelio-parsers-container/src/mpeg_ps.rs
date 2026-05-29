//! MPEG-PS (Program Stream, ISO/IEC 13818-1) parser.
//!
//! Identified by the pack start code 0x00 0x00 0x01 0xBA at the file
//! start (or in MPEG-1 PS variants, also by leading PES packets with
//! start codes 0x000001E0/C0/BD/FA/FD/FE).
//!
//! Stream IDs (the 4th byte of an 0x000001XX start code):
//!   0xBA           = pack header
//!   0xBB           = system header
//!   0xBC           = program stream map
//!   0xBD           = private stream 1 (AC-3, DTS, LPCM in DVD)
//!   0xBE / 0xBF    = padding / private stream 2
//!   0xC0..=0xDF    = MPEG audio (32 streams)
//!   0xE0..=0xEF    = MPEG video (16 streams)
//!
//! The minimal parser:
//!   1. Confirm magic start code.
//!   2. Walk packets, collecting the set of unique stream IDs.
//!   3. Emit one Audio/Video stream per ID.

use revelio_core::{FileAnalyze, StreamKind};
use std::collections::BTreeSet;

const PACK_SC: [u8; 4] = [0x00, 0x00, 0x01, 0xBA];

/// Detection: Pack start code 0x000001BA or PES start codes.
/// Fills: Video stream dimensions, aspect ratio, frame rate, MPEG-2 sequence headers.
pub fn parse_mpeg_ps(fa: &mut FileAnalyze) -> bool {
    let total = fa.remain();
    let buf = match fa.peek_raw(total) {
        Some(b) => b,
        None => return false,
    };
    if buf.len() < 14 || buf[..4] != PACK_SC {
        // Allow MPEG-1 PS files starting with a PES start code directly.
        if !starts_with_pes(buf) {
            return false;
        }
    }

    let mut audio_ids: BTreeSet<u8> = BTreeSet::new();
    let mut video_ids: BTreeSet<u8> = BTreeSet::new();
    let mut private1_seen = false;

    // Walk start codes. Bound the scan to first 1 MB for speed.
    let max_scan = total.min(1_048_576);
    let mut i = 0usize;
    while i + 4 <= max_scan {
        if buf[i] == 0x00 && buf[i + 1] == 0x00 && buf[i + 2] == 0x01 {
            let sid = buf[i + 3];
            match sid {
                0xBA => {
                    // Pack header. MPEG-2 layout: 10 bytes after the SC,
                    // then stuffing[bytes 13 & 0x7]. MPEG-1: 8 bytes.
                    if i + 14 <= buf.len() {
                        let is_mpeg2 = (buf[i + 4] & 0xC0) == 0x40;
                        if is_mpeg2 {
                            let stuff = buf[i + 13] & 0x07;
                            i += 14 + stuff as usize;
                        } else {
                            i += 12; // MPEG-1 pack header
                        }
                        continue;
                    } else {
                        break;
                    }
                }
                0xB9 => {
                    // MPEG_program_end_code — done.
                    break;
                }
                0xC0..=0xDF => {
                    audio_ids.insert(sid);
                }
                0xE0..=0xEF => {
                    video_ids.insert(sid);
                }
                0xBD => {
                    private1_seen = true;
                }
                _ => {}
            }
            // PES packet: skip past it using the declared length field.
            if i + 6 <= buf.len() && sid != 0xBA {
                let pes_len = ((buf[i + 4] as usize) << 8) | (buf[i + 5] as usize);
                if pes_len == 0 {
                    // Unbounded video PES (allowed) — search for next SC.
                    let mut j = i + 6;
                    while j + 4 <= max_scan {
                        if buf[j] == 0 && buf[j + 1] == 0 && buf[j + 2] == 1 {
                            break;
                        }
                        j += 1;
                    }
                    i = j;
                    continue;
                }
                i += 6 + pes_len;
            } else {
                i += 4;
            }
        } else {
            i += 1;
        }
    }

    if audio_ids.is_empty() && video_ids.is_empty() && !private1_seen && buf[..4] != PACK_SC {
        return false;
    }

    // Sniff audio + video payloads before mutating fa (buf borrow
    // lives until here).
    let audio_frames: Vec<Option<MpegAudioFrame>> =
        audio_ids.iter().map(|sid| sniff_mpeg_audio(buf, *sid)).collect();
    let video_seqs: Vec<Option<Mpeg2SeqHeader>> =
        video_ids.iter().map(|sid| sniff_mpeg2_sequence(buf, *sid)).collect();

    fa.stream_prepare(StreamKind::General);
    fa.set_field(StreamKind::General, 0, "Format", "MPEG-PS");
    if !video_ids.is_empty() {
        fa.set_field(StreamKind::General, 0, "VideoCount", video_ids.len().to_string());
    }
    let audio_count = audio_ids.len() + (if private1_seen { 1 } else { 0 });
    if audio_count > 0 {
        fa.set_field(StreamKind::General, 0, "AudioCount", audio_count.to_string());
    }

    let mut stream_order: u32 = 0;
    for (idx, sid) in video_ids.iter().enumerate() {
        let pos = fa.stream_prepare(StreamKind::Video);
        fa.set_field(StreamKind::Video, pos, "StreamOrder", stream_order.to_string());
        fa.set_field(StreamKind::Video, pos, "ID", sid.to_string());
        stream_order += 1;
        fa.set_field(StreamKind::Video, pos, "Format", "MPEG Video");
        fa.set_field(StreamKind::Video, pos, "Format_Version", "2");
        fa.set_field(StreamKind::Video, pos, "BitRate_Mode", "VBR");
        if let Some(seq) = &video_seqs[idx] {
            // MPEG-2 defaults — Sequence Header Extension parsing would
            // refine Format_Profile/Level. ffmpeg's mpeg2video encoder
            // defaults to Main/Main.
            fa.set_field(StreamKind::Video, pos, "Format_Profile", "Main");
            fa.set_field(StreamKind::Video, pos, "Format_Level", "Main");
            fa.set_field(StreamKind::Video, pos, "Format_Settings_BVOP", "No");
            fa.set_field(StreamKind::Video, pos, "Format_Settings_Matrix", "Default");
            fa.set_field(StreamKind::Video, pos, "Width", seq.width.to_string());
            fa.set_field(StreamKind::Video, pos, "Height", seq.height.to_string());
            fa.set_field(StreamKind::Video, pos, "Sampled_Width", seq.width.to_string());
            fa.set_field(StreamKind::Video, pos, "Sampled_Height", seq.height.to_string());
            fa.set_field(StreamKind::Video, pos, "PixelAspectRatio", "1.000");
            let dar = seq.width as f64 / seq.height as f64;
            fa.set_field(StreamKind::Video, pos, "DisplayAspectRatio", format!("{:.3}", dar));
            fa.set_field(StreamKind::Video, pos, "FrameRate", format!("{:.3}", seq.frame_rate));
            fa.set_field(StreamKind::Video, pos, "FrameRate_Num", seq.frame_rate_num.to_string());
            fa.set_field(StreamKind::Video, pos, "FrameRate_Den", seq.frame_rate_den.to_string());
            if seq.frame_count > 0 {
                fa.set_field(StreamKind::Video, pos, "FrameCount", seq.frame_count.to_string());
                if seq.frame_rate > 0.0 {
                    let dur_ms = (seq.frame_count as f64 * 1000.0 / seq.frame_rate).round() as u64;
                    fa.set_field(StreamKind::Video, pos, "Duration", dur_ms.to_string());
                }
            }
            if seq.stream_size > 0 {
                fa.set_field(StreamKind::Video, pos, "StreamSize", seq.stream_size.to_string());
            }
        }
        fa.set_field(StreamKind::Video, pos, "ColorSpace", "YUV");
        fa.set_field(StreamKind::Video, pos, "ChromaSubsampling", "4:2:0");
        fa.set_field(StreamKind::Video, pos, "BitDepth", "8");
        fa.set_field(StreamKind::Video, pos, "ScanType", "Progressive");
        fa.set_field(StreamKind::Video, pos, "Compression_Mode", "Lossy");
    }
    for (idx, sid) in audio_ids.iter().enumerate() {
        let pos = fa.stream_prepare(StreamKind::Audio);
        fa.set_field(StreamKind::Audio, pos, "StreamOrder", stream_order.to_string());
        fa.set_field(StreamKind::Audio, pos, "ID", sid.to_string());
        stream_order += 1;
        fa.set_field(StreamKind::Audio, pos, "Format", "MPEG Audio");
        if let Some(mp) = &audio_frames[idx] {
            fa.set_field(StreamKind::Audio, pos, "Format_Version", mp.version_name);
            fa.set_field(StreamKind::Audio, pos, "Format_Profile", mp.layer_name);
            fa.set_field(StreamKind::Audio, pos, "BitRate_Mode", "CBR");
            fa.set_field(
                StreamKind::Audio,
                pos,
                "BitRate",
                (mp.bitrate_kbps as u32 * 1000).to_string(),
            );
            fa.set_field(StreamKind::Audio, pos, "Channels", mp.channels.to_string());
            fa.set_field(StreamKind::Audio, pos, "SamplingRate", mp.sample_rate.to_string());
            fa.set_field(
                StreamKind::Audio,
                pos,
                "SamplesPerFrame",
                mp.samples_per_frame.to_string(),
            );
            if mp.frame_count > 0 && mp.sample_rate > 0 {
                let total_samples = mp.frame_count as u64 * mp.samples_per_frame as u64;
                let dur_ms =
                    (total_samples * 1000 + mp.sample_rate as u64 / 2) / mp.sample_rate as u64;
                fa.set_field(StreamKind::Audio, pos, "Duration", dur_ms.to_string());
                fa.set_field(StreamKind::Audio, pos, "FrameCount", mp.frame_count.to_string());
                fa.set_field(StreamKind::Audio, pos, "SamplingCount", total_samples.to_string());
                let frame_rate = mp.sample_rate as f64 / mp.samples_per_frame as f64;
                fa.set_field(StreamKind::Audio, pos, "FrameRate", format!("{:.3}", frame_rate));
            }
            if mp.stream_size > 0 {
                fa.set_field(StreamKind::Audio, pos, "StreamSize", mp.stream_size.to_string());
            }
        }
        fa.set_field(StreamKind::Audio, pos, "Compression_Mode", "Lossy");
    }
    if private1_seen {
        // Private stream 1 carries AC-3/DTS/LPCM in DVD VOBs. Without
        // sub-stream sniffing we just label it "Private".
        let pos = fa.stream_prepare(StreamKind::Audio);
        fa.set_field(StreamKind::Audio, pos, "StreamOrder", stream_order.to_string());
        fa.set_field(StreamKind::Audio, pos, "ID", "189"); // 0xBD
        fa.set_field(StreamKind::Audio, pos, "Format", "Private");
    }

    true
}

struct Mpeg2SeqHeader {
    width: u32,
    height: u32,
    frame_rate: f64,
    frame_rate_num: u32,
    frame_rate_den: u32,
    /// Count of picture_start codes (0x00000100) seen in all video PES
    /// payloads for this stream.
    frame_count: u32,
    /// Total PES payload bytes for this video stream.
    stream_size: u64,
}

/// MPEG-2 Sequence Header start code = 0x000001B3, followed by:
///   12 bits horizontal_size_value
///   12 bits vertical_size_value
///    4 bits aspect_ratio_info
///    4 bits frame_rate_code
const FRAME_RATE_TABLE: [(u32, u32); 9] = [
    (0, 1),
    (24000, 1001), // 23.976
    (24, 1),
    (25, 1),
    (30000, 1001), // 29.97
    (30, 1),
    (50, 1),
    (60000, 1001), // 59.94
    (60, 1),
];

fn sniff_mpeg2_sequence(buf: &[u8], sid: u8) -> Option<Mpeg2SeqHeader> {
    // Concatenate all PES payloads for this stream first — picture_start
    // codes can straddle PES packet boundaries when frames are larger
    // than the PES MTU.
    let mut es: Vec<u8> = Vec::new();
    let mut i = 0;
    while i + 6 < buf.len() {
        if buf[i] != 0 || buf[i + 1] != 0 || buf[i + 2] != 1 || buf[i + 3] != sid {
            i += 1;
            continue;
        }
        let pes_len = ((buf[i + 4] as usize) << 8) | (buf[i + 5] as usize);
        if pes_len == 0 || i + 6 + pes_len > buf.len() {
            i += 1;
            continue;
        }
        let pes = &buf[i + 6..i + 6 + pes_len];
        let pes_payload_off =
            if pes.len() >= 3 && (pes[0] & 0xC0) == 0x80 { 3 + pes[2] as usize } else { 0 };
        if pes_payload_off < pes.len() {
            es.extend_from_slice(&pes[pes_payload_off..]);
        }
        i = i + 6 + pes_len;
    }
    if es.is_empty() {
        return None;
    }
    let es_size = es.len() as u64;
    let mut hdr: Option<Mpeg2SeqHeader> = None;
    let mut frame_count: u32 = 0;
    let mut j = 0;
    while j + 4 < es.len() {
        if es[j] == 0 && es[j + 1] == 0 && es[j + 2] == 1 {
            match es[j + 3] {
                0xB3 if hdr.is_none() && j + 8 < es.len() => {
                    let h = &es[j + 4..];
                    let width = ((h[0] as u32) << 4) | ((h[1] as u32) >> 4);
                    let height = (((h[1] & 0x0F) as u32) << 8) | (h[2] as u32);
                    let fr_code = (h[3] & 0x0F) as usize;
                    if fr_code != 0 && fr_code < FRAME_RATE_TABLE.len() {
                        let (num, den) = FRAME_RATE_TABLE[fr_code];
                        hdr = Some(Mpeg2SeqHeader {
                            width,
                            height,
                            frame_rate: num as f64 / den as f64,
                            frame_rate_num: num,
                            frame_rate_den: den,
                            frame_count: 0,
                            stream_size: 0,
                        });
                    }
                }
                0x00 => {
                    frame_count += 1;
                }
                _ => {}
            }
            j += 4;
        } else {
            j += 1;
        }
    }
    if let Some(mut h) = hdr {
        h.frame_count = frame_count;
        h.stream_size = es_size;
        Some(h)
    } else {
        None
    }
}

struct MpegAudioFrame {
    version_name: &'static str,
    layer_name: &'static str,
    bitrate_kbps: u16,
    channels: u8,
    sample_rate: u32,
    samples_per_frame: u16,
    /// Count of frame syncs seen across all PES payloads of this stream.
    frame_count: u32,
    /// Total PES payload bytes for this stream (audio data).
    stream_size: u64,
}

/// Scan the buffer for the first PES packet matching `sid`, then look
/// inside its payload for an MPEG audio frame sync (0xFFF) and decode
/// the header.
fn sniff_mpeg_audio(buf: &[u8], sid: u8) -> Option<MpegAudioFrame> {
    // Concatenate all PES payloads for this stream first (frames can
    // span PES boundaries).
    let mut es: Vec<u8> = Vec::new();
    let mut p = 0;
    while p + 6 < buf.len() {
        if buf[p] != 0 || buf[p + 1] != 0 || buf[p + 2] != 1 || buf[p + 3] != sid {
            p += 1;
            continue;
        }
        let pes_len = ((buf[p + 4] as usize) << 8) | (buf[p + 5] as usize);
        if pes_len == 0 || p + 6 + pes_len > buf.len() {
            p += 1;
            continue;
        }
        let pes = &buf[p + 6..p + 6 + pes_len];
        let pes_payload_off =
            if pes.len() >= 3 && (pes[0] & 0xC0) == 0x80 { 3 + pes[2] as usize } else { 0 };
        if pes_payload_off < pes.len() {
            es.extend_from_slice(&pes[pes_payload_off..]);
        }
        p = p + 6 + pes_len;
    }
    if es.is_empty() {
        return None;
    }
    let stream_size = es.len() as u64;
    if let Some(mut info) = sniff_mpeg_audio_es(&es) {
        // Count remaining syncs after the first to fill FrameCount.
        let mut count: u32 = 0;
        let mut q = 0;
        while q + 4 <= es.len() {
            if es[q] == 0xFF && (es[q + 1] & 0xE0) == 0xE0 {
                count += 1;
                // Estimate frame size to skip ahead (avoid double-counting
                // false syncs within the same frame).
                let bytes_per_frame =
                    (info.bitrate_kbps as usize * 1000 * info.samples_per_frame as usize / 8)
                        .checked_div(info.sample_rate as usize)
                        .unwrap_or(0);
                if bytes_per_frame >= 4 {
                    q += bytes_per_frame;
                } else {
                    q += 4;
                }
            } else {
                q += 1;
            }
        }
        info.frame_count = count;
        info.stream_size = stream_size;
        return Some(info);
    }
    None
}

fn sniff_mpeg_audio_es(buf: &[u8]) -> Option<MpegAudioFrame> {
    const BITRATES: [[[u16; 16]; 4]; 4] = [
        // [version][layer][bitrate_idx]
        [[0; 16]; 4], // reserved version
        [
            // MPEG 2.5 (treated same as MPEG 2)
            [0; 16],
            [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160, 0], // Layer 3
            [0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160, 0], // Layer 2
            [0, 32, 48, 56, 64, 80, 96, 112, 128, 144, 160, 176, 192, 224, 256, 0], // Layer 1
        ],
        [[0; 16]; 4], // reserved layer
        [
            // MPEG 1
            [0; 16],
            [0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 0], // Layer 3
            [0, 32, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 384, 0], // Layer 2
            [0, 32, 64, 96, 128, 160, 192, 224, 256, 288, 320, 352, 384, 416, 448, 0], // Layer 1
        ],
    ];
    const SAMPLE_RATES: [[u32; 4]; 4] =
        [[11025, 12000, 8000, 0], [0, 0, 0, 0], [22050, 24000, 16000, 0], [44100, 48000, 32000, 0]];
    // Scan for the first MPEG audio frame sync in the raw ES bytes.
    for j in 0..buf.len().saturating_sub(4) {
        if buf[j] != 0xFF || (buf[j + 1] & 0xE0) != 0xE0 {
            continue;
        }
        let version = ((buf[j + 1] >> 3) & 0x3) as usize;
        let layer = ((buf[j + 1] >> 1) & 0x3) as usize;
        let bitrate_idx = ((buf[j + 2] >> 4) & 0xF) as usize;
        let sr_idx = ((buf[j + 2] >> 2) & 0x3) as usize;
        let channel_mode = (buf[j + 3] >> 6) & 0x3;
        if version == 1 || layer == 0 || bitrate_idx == 0 || bitrate_idx == 15 {
            continue;
        }
        let bitrate_kbps = BITRATES[version][layer][bitrate_idx];
        let sample_rate = SAMPLE_RATES[version][sr_idx];
        if bitrate_kbps == 0 || sample_rate == 0 {
            continue;
        }
        let channels: u8 = if channel_mode == 3 { 1 } else { 2 };
        let version_name = match version {
            3 => "1",
            2 => "2",
            0 => "2.5",
            _ => "",
        };
        let layer_name = match layer {
            3 => "Layer 1",
            2 => "Layer 2",
            1 => "Layer 3",
            _ => "",
        };
        let samples_per_frame: u16 = match (version, layer) {
            (3, 3) => 384,
            (3, _) => 1152,
            (_, 3) => 384,
            (_, 2) => 1152,
            _ => 576,
        };
        return Some(MpegAudioFrame {
            version_name,
            layer_name,
            bitrate_kbps,
            channels,
            sample_rate,
            samples_per_frame,
            frame_count: 0,
            stream_size: 0,
        });
    }
    None
}

fn starts_with_pes(buf: &[u8]) -> bool {
    if buf.len() < 4 || buf[0] != 0 || buf[1] != 0 || buf[2] != 1 {
        return false;
    }
    let sid = buf[3];
    // Allow MPEG-1 PS files starting with a PES start code per the C++
    // detection logic (File_MpegPs.cpp:929).
    matches!(sid, 0xE0..=0xEF | 0xC0..=0xDF | 0xBD | 0xFA | 0xFD | 0xFE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_ps() {
        let mut fa = FileAnalyze::new(b"NOT AN MPEG-PS FILE...........");
        assert!(!parse_mpeg_ps(&mut fa));
    }

    fn build_minimal_ps_with_video_audio() -> Vec<u8> {
        // Pack header (MPEG-2)
        let mut buf = vec![0x00, 0x00, 0x01, 0xBA];
        // 10 bytes MPEG-2 header: first byte must have bits 01 in top 2 → 0x44
        buf.extend_from_slice(&[0x44, 0x00, 0x04, 0x00, 0x04, 0x00, 0x01, 0x00, 0x00, 0x00]);
        // Video PES (stream id E0, length 4, payload 4 bytes)
        buf.extend_from_slice(&[0x00, 0x00, 0x01, 0xE0, 0x00, 0x04, 0xAA, 0xAA, 0xAA, 0xAA]);
        // Audio PES (stream id C0, length 4)
        buf.extend_from_slice(&[0x00, 0x00, 0x01, 0xC0, 0x00, 0x04, 0xBB, 0xBB, 0xBB, 0xBB]);
        // Program end code
        buf.extend_from_slice(&[0x00, 0x00, 0x01, 0xB9]);
        buf
    }

    #[test]
    fn parses_synthetic_program_stream() {
        let buf = build_minimal_ps_with_video_audio();
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_mpeg_ps(&mut fa));
        assert_eq!(
            fa.retrieve(StreamKind::General, 0, "Format").map(|z| z.as_str().to_owned()),
            Some("MPEG-PS".into())
        );
        assert_eq!(fa.stream_count(StreamKind::Video), 1);
        assert_eq!(fa.stream_count(StreamKind::Audio), 1);
        assert_eq!(
            fa.retrieve(StreamKind::Video, 0, "Format").map(|z| z.as_str().to_owned()),
            Some("MPEG Video".into())
        );
        assert_eq!(
            fa.retrieve(StreamKind::Audio, 0, "Format").map(|z| z.as_str().to_owned()),
            Some("MPEG Audio".into())
        );
    }
}
