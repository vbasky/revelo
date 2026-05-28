//! Apple CoreAudio Format (.caf) container parser.
//!
//! Mirrors the subset of MediaInfoLib's `File_Caf.cpp` needed to populate
//! General.Format=CAF plus the basic audio-description fields. CAF is a
//! big-endian chunk container.
//!
//! Magic: "caff" (0x6361_6666).
//!
//! File header (8 bytes):
//!   4 bytes:  "caff" FileType
//!   2 bytes BE: FileVersion (only Version 1 is supported)
//!   2 bytes BE: FileFlags
//!
//! Each chunk:
//!   4 bytes BE: ChunkType (FourCC)
//!   8 bytes BE: ChunkSize (signed; -1 means "to end of file" for `data`)
//!   ChunkSize bytes payload
//!
//! Audio Description chunk ("desc"), payload 32 bytes:
//!   8 bytes BE Float64: SampleRate
//!   4 bytes FourCC:     FormatID
//!   4 bytes BE u32:     FormatFlags
//!   4 bytes BE u32:     BytesPerPacket
//!   4 bytes BE u32:     FramesPerPacket
//!   4 bytes BE u32:     ChannelsPerFrame
//!   4 bytes BE u32:     BitsPerChannel

use revelio_core::{FileAnalyze, StreamKind};

const MAGIC_CAFF: u32 = u32::from_be_bytes(*b"caff");
const CHUNK_DESC: u32 = u32::from_be_bytes(*b"desc");

#[derive(Debug, Default)]
struct AudioDesc {
    sample_rate: f64,
    format_id: u32,
    bytes_per_packet: u32,
    frames_per_packet: u32,
    channels_per_frame: u32,
    bits_per_channel: u32,
}

pub fn parse_caf(fa: &mut FileAnalyze) -> bool {
    let head = fa.peek_raw(fa.remain().min(8));
    let Some(h) = head else { return false };
    if h.len() < 8 {
        return false;
    }
    let magic = u32::from_be_bytes([h[0], h[1], h[2], h[3]]);
    if magic != MAGIC_CAFF {
        return false;
    }
    let file_version = u16::from_be_bytes([h[4], h[5]]);

    fa.element_begin("CAF");
    let mut magic_consume: u32 = 0;
    fa.get_c4(&mut magic_consume, "FileType");
    let mut ver: u16 = 0;
    fa.get_b2(&mut ver, "FileVersion");
    let mut flags: u16 = 0;
    fa.get_b2(&mut flags, "FileFlags");

    let mut desc: Option<AudioDesc> = None;

    // Only Version 1 is supported by MediaInfoLib; for other versions we
    // still emit General.Format=CAF and stop parsing chunks.
    if file_version == 1 {
        while fa.remain() >= 12 {
            let mut chunk_type: u32 = 0;
            fa.get_c4(&mut chunk_type, "ChunkType");
            let mut chunk_size: u64 = 0;
            fa.get_b8(&mut chunk_size, "ChunkSize");
            let csize = chunk_size as usize;
            // The `data` chunk may declare size=-1 (to EOF) — clamp to remaining bytes.
            let effective = if chunk_size as i64 == -1 || csize > fa.remain() {
                fa.remain()
            } else {
                csize
            };

            if chunk_type == CHUNK_DESC && effective >= 32 {
                fa.element_begin("desc");
                desc = Some(parse_desc(fa));
                fa.element_end();
                if effective > 32 {
                    fa.skip_hexa(effective - 32, "Extension");
                }
            } else {
                if effective > 0 {
                    fa.skip_hexa(effective, "Chunk");
                }
            }
        }
    }

    fa.element_end();

    fa.stream_prepare(StreamKind::General);
    fa.fill(StreamKind::General, 0, "Format", "CAF", false);
    fa.fill(
        StreamKind::General,
        0,
        "Format_Version",
        format!("Version {}", file_version),
        false,
    );

    fa.stream_prepare(StreamKind::Audio);
    if let Some(d) = desc {
        fill_audio(fa, &d);
    }
    fa.fill(StreamKind::General, 0, "AudioCount", "1", false);

    true
}

fn parse_desc(fa: &mut FileAnalyze) -> AudioDesc {
    let mut sample_rate: f64 = 0.0;
    fa.get_bf8(&mut sample_rate, "SampleRate");
    let mut format_id: u32 = 0;
    fa.get_c4(&mut format_id, "FormatID");
    let mut format_flags: u32 = 0;
    fa.get_b4(&mut format_flags, "FormatFlags");
    let mut bytes_per_packet: u32 = 0;
    fa.get_b4(&mut bytes_per_packet, "BytesPerPacket");
    let mut frames_per_packet: u32 = 0;
    fa.get_b4(&mut frames_per_packet, "FramesPerPacket");
    let mut channels_per_frame: u32 = 0;
    fa.get_b4(&mut channels_per_frame, "ChannelsPerFrame");
    let mut bits_per_channel: u32 = 0;
    fa.get_b4(&mut bits_per_channel, "BitsPerChannel");

    AudioDesc {
        sample_rate,
        format_id,
        bytes_per_packet,
        frames_per_packet,
        channels_per_frame,
        bits_per_channel,
    }
}

fn fill_audio(fa: &mut FileAnalyze, d: &AudioDesc) {
    // FormatID is a 4-character codec tag (e.g. "lpcm", "aac ", "alac").
    // MediaInfoLib runs it through CodecID_Fill; we emit the raw FourCC
    // here and let downstream codec-id mapping (when added) refine it.
    let fourcc = format_id_to_string(d.format_id);
    if !fourcc.is_empty() {
        fa.fill(StreamKind::Audio, 0, "Format", fourcc.as_str(), false);
    }

    if d.sample_rate > 0.0 {
        // SampleRate is Float64 but values are typically integral (44100, 48000…).
        let sr = d.sample_rate;
        let sr_str = if (sr - sr.round()).abs() < 1e-6 {
            (sr.round() as u64).to_string()
        } else {
            format!("{}", sr)
        };
        fa.fill(StreamKind::Audio, 0, "SamplingRate", sr_str, false);
    }
    if d.channels_per_frame > 0 {
        fa.fill(StreamKind::Audio, 0, "Channels", d.channels_per_frame.to_string(), false);
    }
    if d.bits_per_channel > 0 {
        fa.fill(StreamKind::Audio, 0, "BitDepth", d.bits_per_channel.to_string(), false);
    }
    if d.bytes_per_packet > 0 && d.frames_per_packet > 0 && d.sample_rate > 0.0 {
        let bitrate = d.sample_rate * (d.bytes_per_packet as f64) * 8.0 / (d.frames_per_packet as f64);
        fa.fill(StreamKind::Audio, 0, "BitRate", (bitrate.round() as u64).to_string(), false);
    }
}

fn format_id_to_string(id: u32) -> String {
    let bytes = id.to_be_bytes();
    if bytes.iter().all(|b| b.is_ascii_graphic() || *b == b' ') {
        // Trim trailing spaces — "aac " becomes "aac".
        String::from_utf8_lossy(&bytes).trim_end().to_string()
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_caf(format_id: &[u8; 4], sample_rate: f64, channels: u32, bits: u32) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(b"caff");
        buf.extend_from_slice(&1u16.to_be_bytes()); // FileVersion
        buf.extend_from_slice(&0u16.to_be_bytes()); // FileFlags

        // "desc" chunk, size = 32
        buf.extend_from_slice(b"desc");
        buf.extend_from_slice(&32u64.to_be_bytes());
        buf.extend_from_slice(&sample_rate.to_be_bytes());
        buf.extend_from_slice(format_id);
        buf.extend_from_slice(&0u32.to_be_bytes()); // FormatFlags
        buf.extend_from_slice(&2u32.to_be_bytes()); // BytesPerPacket
        buf.extend_from_slice(&1u32.to_be_bytes()); // FramesPerPacket
        buf.extend_from_slice(&channels.to_be_bytes());
        buf.extend_from_slice(&bits.to_be_bytes());

        // Trailing "data" chunk with size=-1 (to EOF), some payload.
        buf.extend_from_slice(b"data");
        buf.extend_from_slice(&(-1i64).to_be_bytes());
        buf.extend_from_slice(&[0u8; 64]);
        buf
    }

    #[test]
    fn parses_lpcm_caf() {
        let buf = make_caf(b"lpcm", 48000.0, 2, 16);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_caf(&mut fa));

        let g = |k: &str| fa.retrieve(StreamKind::General, 0, k).map(|z| z.as_str().to_owned());
        let a = |k: &str| fa.retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());

        assert_eq!(g("Format").as_deref(), Some("CAF"));
        assert_eq!(g("Format_Version").as_deref(), Some("Version 1"));
        assert_eq!(g("AudioCount").as_deref(), Some("1"));
        assert_eq!(a("Format").as_deref(), Some("lpcm"));
        assert_eq!(a("SamplingRate").as_deref(), Some("48000"));
        assert_eq!(a("Channels").as_deref(), Some("2"));
        assert_eq!(a("BitDepth").as_deref(), Some("16"));
        // BitRate = 48000 * 2 * 8 / 1 = 768000.
        assert_eq!(a("BitRate").as_deref(), Some("768000"));
    }

    #[test]
    fn rejects_non_caf() {
        let mut fa = FileAnalyze::new(b"RIFFnope_not_a_caf_file_at_all");
        assert!(!parse_caf(&mut fa));
    }

    #[test]
    fn handles_aac_fourcc_with_trailing_space() {
        // "aac " is the canonical FourCC for AAC in CAF — trailing space.
        let buf = make_caf(b"aac ", 44100.0, 2, 0);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_caf(&mut fa));
        let a = |k: &str| fa.retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(a("Format").as_deref(), Some("aac"));
        assert_eq!(a("SamplingRate").as_deref(), Some("44100"));
        assert_eq!(a("Channels").as_deref(), Some("2"));
        // bits_per_channel=0 (compressed format) → BitDepth not filled.
        assert!(a("BitDepth").is_none());
    }
}
