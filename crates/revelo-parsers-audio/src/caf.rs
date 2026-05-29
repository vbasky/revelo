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
//!   8 bytes BE f64: SampleRate
//!   4 bytes FourCC:     FormatID
//!   4 bytes BE u32:     FormatFlags
//!   4 bytes BE u32:     BytesPerPacket
//!   4 bytes BE u32:     FramesPerPacket
//!   4 bytes BE u32:     ChannelsPerFrame
//!   4 bytes BE u32:     BitsPerChannel

use revelo_core::{FileAnalyze, Reader, StreamKind};

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

/// Parse Core Audio Format (Apple).
///
/// Detection: `caff` magic.
/// Fills: Format, channels, sample rate, codec descriptor.
pub fn parse_caf(fa: &mut FileAnalyze) -> bool {
    parse(fa).is_some()
}

fn parse(fa: &mut FileAnalyze) -> Option<()> {
    let r = &mut Reader::wrap(fa);
    let h = r.peek_raw(8)?;
    if h.len() < 8 {
        return None;
    }
    let magic = u32::from_be_bytes([h[0], h[1], h[2], h[3]]);
    if magic != MAGIC_CAFF {
        return None;
    }
    let file_version = u16::from_be_bytes([h[4], h[5]]);

    r.element_begin("CAF");
    r.fourcc("FileType")?;
    r.be_u16("FileVersion")?;
    r.be_u16("FileFlags")?;

    let mut desc: Option<AudioDesc> = None;

    // Only Version 1 is supported by MediaInfoLib; for other versions we
    // still emit General.Format=CAF and stop parsing chunks.
    if file_version == 1 {
        while r.remain() >= 12 {
            let chunk_type = r.fourcc("ChunkType")?;
            let chunk_size = r.be_u64("ChunkSize")?;
            let csize = chunk_size as usize;
            // The `data` chunk may declare size=-1 (to EOF) — clamp to remaining bytes.
            let effective =
                if chunk_size as i64 == -1 || csize > r.remain() { r.remain() } else { csize };

            if chunk_type == CHUNK_DESC && effective >= 32 {
                r.element_begin("desc");
                desc = Some(parse_desc(r)?);
                r.element_end();
                if effective > 32 {
                    r.skip(effective - 32)?; // Extension
                }
            } else if effective > 0 {
                r.skip(effective)?; // Chunk
            }
        }
    }

    r.element_end();

    r.stream_prepare(StreamKind::General);
    r.set_field(StreamKind::General, 0, "Format", "CAF");
    r.set_field(StreamKind::General, 0, "Format_Version", format!("Version {}", file_version));

    r.stream_prepare(StreamKind::Audio);
    if let Some(d) = desc {
        fill_audio(r, &d);
    }
    r.set_field(StreamKind::General, 0, "AudioCount", "1");
    Some(())
}

fn parse_desc(r: &mut Reader<'_, '_>) -> Option<AudioDesc> {
    let sample_rate = r.be_f64("SampleRate")?;
    let format_id = r.fourcc("FormatID")?;
    r.be_u32("FormatFlags")?;
    let bytes_per_packet = r.be_u32("BytesPerPacket")?;
    let frames_per_packet = r.be_u32("FramesPerPacket")?;
    let channels_per_frame = r.be_u32("ChannelsPerFrame")?;
    let bits_per_channel = r.be_u32("BitsPerChannel")?;

    Some(AudioDesc {
        sample_rate,
        format_id,
        bytes_per_packet,
        frames_per_packet,
        channels_per_frame,
        bits_per_channel,
    })
}

fn fill_audio(r: &mut Reader<'_, '_>, d: &AudioDesc) {
    // FormatID is a 4-character codec tag (e.g. "lpcm", "aac ", "alac").
    // MediaInfoLib runs it through CodecID_Fill; we emit the raw FourCC
    // here and let downstream codec-id mapping (when added) refine it.
    let fourcc = format_id_to_string(d.format_id);
    if !fourcc.is_empty() {
        r.set_field(StreamKind::Audio, 0, "Format", fourcc.as_str());
    }

    if d.sample_rate > 0.0 {
        // SampleRate is f64 but values are typically integral (44100, 48000…).
        let sr = d.sample_rate;
        let sr_str = if (sr - sr.round()).abs() < 1e-6 {
            (sr.round() as u64).to_string()
        } else {
            format!("{}", sr)
        };
        r.set_field(StreamKind::Audio, 0, "SamplingRate", sr_str);
    }
    if d.channels_per_frame > 0 {
        r.set_field(StreamKind::Audio, 0, "Channels", d.channels_per_frame.to_string());
    }
    if d.bits_per_channel > 0 {
        r.set_field(StreamKind::Audio, 0, "BitDepth", d.bits_per_channel.to_string());
    }
    if d.bytes_per_packet > 0 && d.frames_per_packet > 0 && d.sample_rate > 0.0 {
        let bitrate =
            d.sample_rate * (d.bytes_per_packet as f64) * 8.0 / (d.frames_per_packet as f64);
        r.set_field(StreamKind::Audio, 0, "BitRate", (bitrate.round() as u64).to_string());
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
