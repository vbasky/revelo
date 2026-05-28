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

use revelio_core::{FileAnalyze, StreamKind};
use zenlib::Int32u;

const MAGIC_TWIN: u32 = 0x5457_494E; // "TWIN"
const CHUNK_COMM: u32 = 0x434F_4D4D; // "COMM"
const CHUNK_DATA: u32 = 0x4441_5441; // "DATA"

const HEADER_LEN: usize = 4 + 8 + 4; // magic + version + subchunks_size

fn samplerate_from_code(code: Int32u) -> Option<u32> {
    match code {
        11 => Some(11025),
        22 => Some(22050),
        44 => Some(44100),
        _ => None,
    }
}

pub fn parse_twin_vq(fa: &mut FileAnalyze) -> bool {
    if fa.remain() < HEADER_LEN {
        return false;
    }
    let head = match fa.peek_raw(fa.remain().min(4)) {
        Some(h) if h.len() == 4 => h,
        _ => return false,
    };
    let magic = u32::from_be_bytes([head[0], head[1], head[2], head[3]]);
    if magic != MAGIC_TWIN {
        return false;
    }

    let file_size = fa.remain() as u64;

    fa.element_begin("TwinVQ");
    let mut magic_consume: Int32u = 0;
    fa.get_c4(&mut magic_consume, "magic");
    fa.skip_hexa(8, "version");
    let mut subchunks_size: Int32u = 0;
    fa.get_b4(&mut subchunks_size, "subchunks_size");

    let mut channel_mode: Option<Int32u> = None;
    let mut bitrate_kbps: Option<Int32u> = None;
    let mut samplerate: Option<u32> = None;

    // Walk chunks until DATA (or buffer exhausted). DATA terminates the
    // header per the C++ reference — its payload format is not parsed.
    loop {
        if fa.remain() < 8 {
            break;
        }
        let mut id: Int32u = 0;
        let mut size: Int32u = 0;
        fa.get_c4(&mut id, "id");
        fa.get_b4(&mut size, "size");

        if id == CHUNK_DATA {
            break;
        }

        let size_usize = size as usize;
        if fa.remain() < size_usize {
            break;
        }

        if id == CHUNK_COMM && size_usize >= 16 {
            let mut cm: Int32u = 0;
            let mut br: Int32u = 0;
            let mut sr: Int32u = 0;
            fa.get_b4(&mut cm, "channel_mode");
            fa.get_b4(&mut br, "bitrate");
            fa.get_b4(&mut sr, "samplerate");
            fa.skip_b4("security_level");
            channel_mode = Some(cm);
            bitrate_kbps = Some(br);
            samplerate = samplerate_from_code(sr);
            if size_usize > 16 {
                fa.skip_hexa(size_usize - 16, "Extension");
            }
        } else {
            fa.skip_hexa(size_usize, "ChunkData");
        }
    }
    fa.element_end();

    // COMM is the only chunk we need to surface stream params; bail if missing.
    let (cm, br, sr) = match (channel_mode, bitrate_kbps, samplerate) {
        (Some(c), Some(b), Some(s)) => (c, b, s),
        _ => return false,
    };

    fa.stream_prepare(StreamKind::General);
    fa.fill(StreamKind::General, 0, "Format", "TwinVQ", false);
    fa.fill(StreamKind::General, 0, "AudioCount", "1", false);

    fa.stream_prepare(StreamKind::Audio);
    fa.fill(StreamKind::Audio, 0, "Format", "TwinVQ", false);
    fa.fill(StreamKind::Audio, 0, "Codec", "TwinVQ", false);
    // C++ stores channel_mode+1: 0→mono, 1→stereo.
    fa.fill(StreamKind::Audio, 0, "Channels", (cm + 1).to_string(), false);
    fa.fill(StreamKind::Audio, 0, "BitRate", (br as u64 * 1000).to_string(), false);
    fa.fill(StreamKind::Audio, 0, "SamplingRate", sr.to_string(), false);
    fa.fill(StreamKind::Audio, 0, "Compression_Mode", "Lossy", false);
    fa.fill(StreamKind::Audio, 0, "StreamSize", file_size.to_string(), false);

    true
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
