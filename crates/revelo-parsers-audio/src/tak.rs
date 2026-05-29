//! TAK (Tom's lossless Audio Kompressor) parser.
//!
//! Mirrors MediaInfoLib's `File_Tak.cpp`. A TAK file is:
//!   "tBaK"                            (4 bytes magic)
//!   metadata_block*                   (until block_type == ENDOFMETADATA)
//!     1 byte LE: block_type
//!     3 bytes LE: block_length
//!     block_length bytes payload
//!   audio frames                      (rest of file)
//!
//! STREAMINFO (block_type=0x01) payload:
//!   1 byte:  unknown
//!   bit-packed (1 byte after BS_End padding):
//!      2 bits: num_samples (lo)
//!      3 bits: framesizecode
//!      2 bits: unknown
//!   4 bytes LE: num_samples (hi)
//!   3 bytes LE: samplerate_packed  (real Hz = packed/16 + 6000)
//!   bit-packed (1 byte):
//!      4 bits: unknown
//!      1 bit:  channels (0=mono, 1=stereo)
//!      2 bits: samplesize index → {8, 16, 24, 0}
//!      1 bit:  unknown
//!   3 bytes LE: crc

use revelo_core::{FileAnalyze, Reader, StreamKind};

const MAGIC_TBAK: [u8; 4] = *b"tBaK";

const BLOCK_ENDOFMETADATA: u8 = 0x00;
const BLOCK_STREAMINFO: u8 = 0x01;

const TAK_SAMPLESIZE: [u8; 4] = [8, 16, 24, 0];

#[derive(Debug, Default)]
struct StreamInfo {
    sample_rate: u32,
    channels: u8,
    bit_depth: u8,
    samples: u64,
}

/// Parse TAK (Tom's lossless Audio Kompressor).
///
/// Detection: `tBaK` magic.
/// Fills: Channels, sample rate, bit depth, encoder version.
pub fn parse_tak(fa: &mut FileAnalyze) -> bool {
    parse(fa).is_some()
}

fn parse(fa: &mut FileAnalyze) -> Option<()> {
    let r = &mut Reader::wrap(fa);
    if r.remain() < 4 {
        return None;
    }
    if r.peek_raw(4)? != MAGIC_TBAK {
        return None;
    }

    r.element_begin("TAK");
    r.fourcc("Signature")?;

    let mut streaminfo: Option<StreamInfo> = None;

    loop {
        if r.remain() < 4 {
            break;
        }
        let block_type = r.le_u8("Block Type")?;
        let block_length = r.le_u24("Block Length")?;
        let block_len = block_length as usize;

        if r.remain() < block_len {
            break;
        }

        match block_type {
            BLOCK_STREAMINFO => {
                r.element_begin("STREAMINFO");
                let before = r.element_offset();
                if let Some(info) = parse_streaminfo(r, block_len) {
                    streaminfo = Some(info);
                }
                let consumed = r.element_offset() - before;
                if consumed < block_len {
                    r.skip(block_len - consumed)?; // Trailer
                }
                r.element_end();
            }
            BLOCK_ENDOFMETADATA => break,
            _ => {
                r.skip(block_len)?; // MetadataBlock
            }
        }
    }

    let audio_stream_size = r.remain() as u64;
    r.element_end();

    let info = streaminfo?;
    fill_streams(r, &info, audio_stream_size);
    Some(())
}

fn parse_streaminfo(r: &mut Reader<'_, '_>, block_len: usize) -> Option<StreamInfo> {
    // STREAMINFO is 13 bytes; bail out cleanly on short blocks so a malformed
    // header doesn't poison the parser state.
    if block_len < 13 {
        return None;
    }
    r.le_u8("unknown")?;
    let num_samples_lo = r.bits(|b| {
        let num_samples_lo = b.read::<u8>(2, "num_samples (lo)")?;
        b.read::<u8>(3, "framesizecode")?;
        b.skip(2); // unknown
        Some(num_samples_lo)
    })?;
    let num_samples_hi = r.le_u32("num_samples (hi)")?;
    let samplerate_packed = r.le_u24("samplerate")?;
    let (channels_bit, samplesize_idx) = r.bits(|b| {
        b.skip(4); // unknown
        let channels_bit = b.read::<u8>(1, "channels")?;
        let samplesize_idx = b.read::<u8>(2, "samplesize")?;
        b.skip(1); // unknown
        Some((channels_bit, samplesize_idx))
    })?;
    r.le_u24("crc")?;

    if samplerate_packed == 0 {
        return None;
    }

    let sample_rate = (samplerate_packed / 16) + 6000;
    let samples = ((num_samples_hi as u64) << 2) | (num_samples_lo as u64);
    let channels = if channels_bit != 0 { 2 } else { 1 };
    let bit_depth = TAK_SAMPLESIZE[(samplesize_idx & 0x3) as usize];

    Some(StreamInfo { sample_rate, channels, bit_depth, samples })
}

fn fill_streams(r: &mut Reader<'_, '_>, info: &StreamInfo, audio_stream_size: u64) {
    r.stream_prepare(StreamKind::General);
    r.set_field(StreamKind::General, 0, "Format", "TAK");
    // Per File_Tak::ENDOFMETADATA: General StreamSize is reported as 0; the
    // audio frames region is reported as Audio StreamSize.
    r.force_field(StreamKind::General, 0, "StreamSize", "0");
    r.set_field(StreamKind::General, 0, "AudioCount", "1");

    r.stream_prepare(StreamKind::Audio);
    r.set_field(StreamKind::Audio, 0, "Format", "TAK");
    r.set_field(StreamKind::Audio, 0, "Codec", "TAK");
    r.set_field(StreamKind::Audio, 0, "Compression_Mode", "Lossless");
    r.set_field(StreamKind::Audio, 0, "BitRate_Mode", "VBR");
    r.set_field(StreamKind::Audio, 0, "Channels", info.channels.to_string());
    r.set_field(StreamKind::Audio, 0, "SamplingRate", info.sample_rate.to_string());
    if info.bit_depth != 0 {
        r.set_field(StreamKind::Audio, 0, "BitDepth", info.bit_depth.to_string());
    }
    if info.samples > 0 && info.sample_rate > 0 {
        let duration_ms = info.samples * 1000 / (info.sample_rate as u64);
        r.set_field(StreamKind::Audio, 0, "Duration", duration_ms.to_string());
        r.set_field(StreamKind::Audio, 0, "SamplingCount", info.samples.to_string());
    }
    r.set_field(StreamKind::Audio, 0, "StreamSize", audio_stream_size.to_string());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_streaminfo_block(
        sample_rate_hz: u32,
        channels: u8,
        samplesize_idx: u8,
        samples: u64,
    ) -> Vec<u8> {
        // Inverse of the C++ encoding: stored = (Hz - 6000) * 16.
        let samplerate_packed = (sample_rate_hz - 6000) * 16;
        let num_samples_lo = (samples & 0x3) as u8;
        let num_samples_hi = (samples >> 2) as u32;
        let mut payload = Vec::new();
        payload.push(0x00); // unknown
        // Bit-packed byte 1: lo(2) | framesizecode(3)=0 | unknown(2)=0 | pad(1)=0
        let b1 = (num_samples_lo & 0x3) << 6;
        payload.push(b1);
        payload.extend_from_slice(&num_samples_hi.to_le_bytes());
        // 3-byte LE samplerate_packed
        let sr = samplerate_packed.to_le_bytes();
        payload.extend_from_slice(&sr[..3]);
        // Bit-packed byte 2: unknown(4)=0 | channels(1) | samplesize(2) | unknown(1)=0
        let channels_bit = if channels == 2 { 1u8 } else { 0u8 };
        let b2 = (channels_bit << 3) | ((samplesize_idx & 0x3) << 1);
        payload.push(b2);
        payload.extend_from_slice(&[0u8, 0u8, 0u8]); // crc

        let mut block = Vec::new();
        block.push(BLOCK_STREAMINFO);
        let len = payload.len() as u32;
        block.extend_from_slice(&len.to_le_bytes()[..3]);
        block.extend_from_slice(&payload);
        block
    }

    fn make_tak(
        sample_rate: u32,
        channels: u8,
        samplesize_idx: u8,
        samples: u64,
        audio_size: usize,
    ) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(b"tBaK");
        buf.extend_from_slice(&make_streaminfo_block(
            sample_rate,
            channels,
            samplesize_idx,
            samples,
        ));
        // ENDOFMETADATA (block_type=0, length=0).
        buf.push(BLOCK_ENDOFMETADATA);
        buf.extend_from_slice(&[0, 0, 0]);
        buf.resize(buf.len() + audio_size, 0);
        buf
    }

    #[test]
    fn rejects_non_tak_buffer() {
        let mut fa = FileAnalyze::new(b"NOT a TAK file at all..");
        assert!(!parse_tak(&mut fa));
    }

    #[test]
    fn parses_basic_tak_stream() {
        // 44100 Hz, stereo, 16-bit (idx=1), 44100 samples -> 1000 ms duration.
        let buf = make_tak(44100, 2, 1, 44100, 1234);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_tak(&mut fa));

        let g = |k: &str| fa.retrieve(StreamKind::General, 0, k).map(|z| z.as_str().to_owned());
        let a = |k: &str| fa.retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());

        assert_eq!(g("Format").as_deref(), Some("TAK"));
        assert_eq!(g("StreamSize").as_deref(), Some("0"));
        assert_eq!(g("AudioCount").as_deref(), Some("1"));
        assert_eq!(a("Format").as_deref(), Some("TAK"));
        assert_eq!(a("Codec").as_deref(), Some("TAK"));
        assert_eq!(a("Compression_Mode").as_deref(), Some("Lossless"));
        assert_eq!(a("BitRate_Mode").as_deref(), Some("VBR"));
        assert_eq!(a("Channels").as_deref(), Some("2"));
        assert_eq!(a("SamplingRate").as_deref(), Some("44100"));
        assert_eq!(a("BitDepth").as_deref(), Some("16"));
        assert_eq!(a("Duration").as_deref(), Some("1000"));
        assert_eq!(a("SamplingCount").as_deref(), Some("44100"));
        assert_eq!(a("StreamSize").as_deref(), Some("1234"));
    }

    #[test]
    fn parses_mono_24bit_tak() {
        // 48000 Hz, mono, 24-bit (idx=2), 96000 samples -> 2000 ms duration.
        let buf = make_tak(48000, 1, 2, 96000, 0);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_tak(&mut fa));

        let a = |k: &str| fa.retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(a("Channels").as_deref(), Some("1"));
        assert_eq!(a("BitDepth").as_deref(), Some("24"));
        assert_eq!(a("SamplingRate").as_deref(), Some("48000"));
        assert_eq!(a("Duration").as_deref(), Some("2000"));
        assert_eq!(a("SamplingCount").as_deref(), Some("96000"));
    }
}
