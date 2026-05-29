//! TwinVQ (VQF) parser — Nippon Telegraph and Telephone's transform-domain
//! weighted interleave vector quantization codec.
//!
//! Mirrors MediaInfoLib's `File_TwinVQ.cpp`. The file is a chunk container:
//! a "TWIN" magic, an 8-byte version tag, a 4-byte big-endian
//! `subchunks_size`, then a sequence of FourCC+size chunks. The COMM chunk
//! carries the stream parameters we surface; DATA marks the end of header.
//!
//! Layout:
//!   "TWIN"                          // 4-byte magic
//!   8 bytes:  version (ASCII, e.g. "NEW0VQF0")
//!   4 bytes BE: subchunks_size
//!   chunk*                          // FourCC + 4-byte BE size + payload
//!     COMM payload (16 bytes):
//!       4 bytes BE: channel_mode    (0=mono, 1=stereo)
//!       4 bytes BE: bitrate (kbps)
//!       4 bytes BE: samplerate code (11→11025, 22→22050, 44→44100)
//!       4 bytes BE: security_level
//!     DATA: terminates the header (audio payload follows, format unknown)

use revelio_core::{FileAnalyze, Reader, StreamKind};

const MAGIC_TWIN: u32 = 0x5457_494E; // "TWIN"
const CHUNK_COMM: u32 = 0x434F_4D4D; // "COMM"
const CHUNK_DATA: u32 = 0x4441_5441; // "DATA"

const HEADER_LEN: usize = 4 + 8 + 4; // magic + version + subchunks_size

fn samplerate_from_code(code: u32) -> Option<u32> {
    match code {
        11 => Some(11025),
        22 => Some(22050),
        44 => Some(44100),
        _ => None,
    }
}

pub fn parse_twin_vq(fa: &mut FileAnalyze) -> bool {
    parse(fa).is_some()
}

fn parse(fa: &mut FileAnalyze) -> Option<()> {
    let r = &mut Reader::wrap(fa);
    if r.remain() < HEADER_LEN {
        return None;
    }
    let head = r.peek_raw(4)?;
    let magic = u32::from_be_bytes([head[0], head[1], head[2], head[3]]);
    if magic != MAGIC_TWIN {
        return None;
    }

    let file_size = r.remain() as u64;

    r.element_begin("TwinVQ");
    r.fourcc("magic")?;
    r.skip(8)?; // version
    r.be_u32("subchunks_size")?;

    let mut channel_mode: Option<u32> = None;
    let mut bitrate_kbps: Option<u32> = None;
    let mut samplerate: Option<u32> = None;

    // Walk chunks until DATA (or buffer exhausted). DATA terminates the
    // header per the C++ reference — its payload format is not parsed.
    loop {
        if r.remain() < 8 {
            break;
        }
        let id = r.fourcc("id")?;
        let size = r.be_u32("size")?;

        if id == CHUNK_DATA {
            break;
        }

        let size_usize = size as usize;
        if r.remain() < size_usize {
            break;
        }

        if id == CHUNK_COMM && size_usize >= 16 {
            let cm = r.be_u32("channel_mode")?;
            let br = r.be_u32("bitrate")?;
            let sr = r.be_u32("samplerate")?;
            r.be_u32("security_level")?;
            channel_mode = Some(cm);
            bitrate_kbps = Some(br);
            samplerate = samplerate_from_code(sr);
            if size_usize > 16 {
                r.skip(size_usize - 16)?; // Extension
            }
        } else {
            r.skip(size_usize)?; // ChunkData
        }
    }
    r.element_end();

    // COMM is the only chunk we need to surface stream params; bail if missing.
    let (cm, br, sr) = match (channel_mode, bitrate_kbps, samplerate) {
        (Some(c), Some(b), Some(s)) => (c, b, s),
        _ => return None,
    };

    r.stream_prepare(StreamKind::General);
    r.set_field(StreamKind::General, 0, "Format", "TwinVQ");
    r.set_field(StreamKind::General, 0, "AudioCount", "1");

    r.stream_prepare(StreamKind::Audio);
    r.set_field(StreamKind::Audio, 0, "Format", "TwinVQ");
    r.set_field(StreamKind::Audio, 0, "Codec", "TwinVQ");
    // C++ stores channel_mode+1: 0→mono, 1→stereo.
    r.set_field(StreamKind::Audio, 0, "Channels", (cm + 1).to_string());
    r.set_field(StreamKind::Audio, 0, "BitRate", (br as u64 * 1000).to_string());
    r.set_field(StreamKind::Audio, 0, "SamplingRate", sr.to_string());
    r.set_field(StreamKind::Audio, 0, "Compression_Mode", "Lossy");
    r.set_field(StreamKind::Audio, 0, "StreamSize", file_size.to_string());
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_twinvq(
        channel_mode: u32,
        bitrate_kbps: u32,
        sr_code: u32,
        audio_bytes: usize,
    ) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(b"TWIN");
        buf.extend_from_slice(b"NEW0VQF0"); // 8-byte version
        // subchunks_size: not validated by parser, so any value works
        buf.extend_from_slice(&0u32.to_be_bytes());
        // COMM chunk
        buf.extend_from_slice(b"COMM");
        buf.extend_from_slice(&16u32.to_be_bytes()); // size
        buf.extend_from_slice(&channel_mode.to_be_bytes());
        buf.extend_from_slice(&bitrate_kbps.to_be_bytes());
        buf.extend_from_slice(&sr_code.to_be_bytes());
        buf.extend_from_slice(&0u32.to_be_bytes()); // security_level
        // DATA chunk terminates header
        buf.extend_from_slice(b"DATA");
        buf.extend_from_slice(&0u32.to_be_bytes()); // size (ignored)
        buf.resize(buf.len() + audio_bytes, 0);
        buf
    }

    #[test]
    fn rejects_non_twinvq_buffer() {
        let mut fa = FileAnalyze::new(b"NOT a TwinVQ file at all............");
        assert!(!parse_twin_vq(&mut fa));
    }

    #[test]
    fn parses_stereo_44100_stream() {
        // channel_mode=1 → stereo, bitrate=48 kbps, samplerate code 44 → 44100.
        let buf = make_twinvq(1, 48, 44, 200);
        let expected_stream_size = buf.len() as u64;
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_twin_vq(&mut fa));

        let g = |k: &str| fa.retrieve(StreamKind::General, 0, k).map(|z| z.as_str().to_owned());
        let a = |k: &str| fa.retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());

        assert_eq!(g("Format").as_deref(), Some("TwinVQ"));
        assert_eq!(g("AudioCount").as_deref(), Some("1"));
        assert_eq!(a("Format").as_deref(), Some("TwinVQ"));
        assert_eq!(a("Codec").as_deref(), Some("TwinVQ"));
        assert_eq!(a("Channels").as_deref(), Some("2"));
        assert_eq!(a("BitRate").as_deref(), Some("48000"));
        assert_eq!(a("SamplingRate").as_deref(), Some("44100"));
        assert_eq!(a("Compression_Mode").as_deref(), Some("Lossy"));
        assert_eq!(a("StreamSize").as_deref(), Some(&*expected_stream_size.to_string()));
    }

    #[test]
    fn parses_mono_22050_stream() {
        // channel_mode=0 → mono, bitrate=32 kbps, samplerate code 22 → 22050.
        let buf = make_twinvq(0, 32, 22, 50);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_twin_vq(&mut fa));
        let a = |k: &str| fa.retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(a("Channels").as_deref(), Some("1"));
        assert_eq!(a("BitRate").as_deref(), Some("32000"));
        assert_eq!(a("SamplingRate").as_deref(), Some("22050"));
    }

    #[test]
    fn rejects_unknown_samplerate_code() {
        let buf = make_twinvq(1, 48, 99, 0);
        let mut fa = FileAnalyze::new(&buf);
        // Unknown samplerate code → no Audio fill, parser returns false.
        assert!(!parse_twin_vq(&mut fa));
    }
}
