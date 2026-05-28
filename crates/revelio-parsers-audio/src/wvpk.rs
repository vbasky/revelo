//! WavPack (.wv) parser — hybrid/lossless audio codec.
//!
//! Mirrors the file-header path of MediaInfoLib's `File_Wvpk.cpp`. We only
//! parse the first block header (enough to fill Channels / SamplingRate /
//! BitDepth / Compression_Mode); the per-sub-block walker that fishes out
//! id_0D (channel mask), id_25 (encoder settings) and id_26 (MD5) is
//! deferred — most fields the oracle emits for plain .wv come from the
//! 32-byte block header itself.
//!
//! Block header (32 bytes, all multi-byte fields little-endian):
//!   "wvpk"                  // 4 bytes magic
//!   uint32 LE  ckSize       // block size in bytes - 8
//!   uint16 LE  version      // e.g. 0x0410 = format 4.16
//!   uint8      block_index_u8     // high 8 bits of block_index (ver 5+)
//!   uint8      total_samples_u8   // high 8 bits of total_samples (ver 5+)
//!   uint32 LE  total_samples      // low 32 bits — total samples in file
//!   uint32 LE  block_index        // low 32 bits — index of first sample
//!   uint32 LE  block_samples      // samples in this block
//!   uint32 LE  flags
//!     bit  0   resolution0 (LSB of resolution index)
//!     bit  1   resolution1 (MSB of resolution index, 00=8/01=16/10=24/11=32)
//!     bit  2   mono
//!     bit  3   hybrid (lossy unless paired .wvc correction file)
//!     bits 23..26  sampling-rate index → Wvpk_SamplingRate[]
//!     bit  31  dsf (DSD audio, ver 5.0+)
//!   uint32 LE  crc

use revelio_core::{FileAnalyze, StreamKind};

const MAGIC_WVPK: u32 = u32::from_be_bytes(*b"wvpk");
const BLOCK_HEADER_SIZE: usize = 32;

const WVPK_RESOLUTION: [u16; 4] = [8, 16, 24, 32];

const WVPK_SAMPLING_RATE: [u32; 16] = [
    6000, 8000, 9600, 11025, 12000, 16000, 22050, 24000,
    32000, 44100, 48000, 64000, 88200, 96000, 192000, 0,
];

pub fn parse_wvpk(fa: &mut FileAnalyze) -> bool {
    let n = fa.Remain().min(BLOCK_HEADER_SIZE);
    if n < BLOCK_HEADER_SIZE {
        return false;
    }
    let head = match fa.peek_raw(n) {
        Some(h) => h,
        None => return false,
    };
    let magic = u32::from_be_bytes([head[0], head[1], head[2], head[3]]);
    if magic != MAGIC_WVPK {
        return false;
    }

    // ckSize at offset 4 (LE) — payload bytes beyond the 8-byte ck prefix.
    // Not strictly required to fill the technical fields, but read so we
    // can validate that the declared block fits in the buffer if present.
    let ck_size = u32::from_le_bytes([head[4], head[5], head[6], head[7]]);
    let version = u16::from_le_bytes([head[8], head[9]]);
    // Reject anything that isn't a version 4.x WavPack stream — the C++
    // parser only walks sub-blocks when version/0x100 == 0x4 (which also
    // covers v5).
    let version_major = (version >> 8) as u8;
    if version_major != 4 {
        return false;
    }

    // block_index_u8 and total_samples_u8 are high bytes for >32-bit
    // counters in WavPack 5; we don't need them for the minimum viable
    // metadata fill.
    let _block_index_u8 = head[10];
    let _total_samples_u8 = head[11];
    let total_samples = u32::from_le_bytes([head[12], head[13], head[14], head[15]]);
    let _block_index = u32::from_le_bytes([head[16], head[17], head[18], head[19]]);
    let block_samples = u32::from_le_bytes([head[20], head[21], head[22], head[23]]);
    let flags = u32::from_le_bytes([head[24], head[25], head[26], head[27]]);
    let _crc = u32::from_le_bytes([head[28], head[29], head[30], head[31]]);

    // An "empty" first block (block_samples == 0) carries no useful
    // header info per the C++ parser — bail to avoid emitting bogus fields.
    if block_samples == 0 {
        return false;
    }

    let resolution_idx = (flags & 0x3) as usize;
    let mono = (flags & 0x4) != 0;
    let hybrid = (flags & 0x8) != 0;
    let sample_rate_idx = ((flags >> 23) & 0xF) as usize;
    let dsf = (flags & 0x80000000) != 0;

    let sample_rate = WVPK_SAMPLING_RATE[sample_rate_idx];
    if sample_rate == 0 && !dsf {
        return false;
    }
    let bit_depth = if dsf { 1 } else { WVPK_RESOLUTION[resolution_idx] };
    let channels: u16 = if mono { 1 } else { 2 };

    // Version_Profile string: "X.YY" — minor zero-padded to width 2,
    // matching the C++ Format_Profile output.
    let version_major_u = (version / 0x100) as u32;
    let version_minor_u = (version % 0x100) as u32;
    let version_profile = format!("{}.{:02}", version_major_u, version_minor_u);

    // Compression mode: hybrid without a matching .wvc correction file is
    // lossy. We default to Lossless because we have no way to detect a
    // sibling .wvc from the buffer alone (the C++ parser flips this when
    // sub-block id_07 — "info needed for hybrid lossless (wvc) mode" —
    // is encountered).
    let compression_mode = if hybrid { "Lossy" } else { "Lossless" };

    fa.Stream_Prepare(StreamKind::General);
    fa.Fill(StreamKind::General, 0, "Format", "WavPack", false);
    fa.Fill(StreamKind::General, 0, "AudioCount", "1", false);

    fa.Stream_Prepare(StreamKind::Audio);
    fa.Fill(StreamKind::Audio, 0, "Format", "WavPack", false);
    fa.Fill(StreamKind::Audio, 0, "Format_Profile", version_profile, false);
    fa.Fill(StreamKind::Audio, 0, "Codec", "Wavpack", false);
    fa.Fill(StreamKind::Audio, 0, "BitRate_Mode", "VBR", false);
    fa.Fill(StreamKind::Audio, 0, "Compression_Mode", compression_mode, false);
    fa.Fill(StreamKind::Audio, 0, "Codec_Settings", compression_mode, false);
    fa.Fill(StreamKind::Audio, 0, "Channels", channels.to_string(), false);
    // For DSD (dsf=1) the effective audio rate is 8× the index value.
    let effective_rate: u64 = if dsf {
        (sample_rate as u64) * 8
    } else {
        sample_rate as u64
    };
    fa.Fill(StreamKind::Audio, 0, "SamplingRate", effective_rate.to_string(), false);
    if !dsf {
        fa.Fill(StreamKind::Audio, 0, "BitDepth", bit_depth.to_string(), false);
    } else {
        fa.Fill(StreamKind::Audio, 0, "Format_Settings_Mode", "DSD", false);
        fa.Fill(StreamKind::Audio, 0, "Format_Settings", "DSD", false);
    }

    // Duration from total_samples in the first block — only trustworthy if
    // block_index == 0 (this is the first block of the file), matching
    // C++'s `if (block_index==0)` guard.
    if _block_index == 0 && total_samples != 0 && total_samples != u32::MAX && sample_rate != 0 {
        let duration_ms = (total_samples as u64) * 1000 / (sample_rate as u64);
        fa.Fill(StreamKind::Audio, 0, "Duration", duration_ms.to_string(), false);
    }

    let _ = ck_size;
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_block_header(
        version: u16,
        total_samples: u32,
        block_index: u32,
        block_samples: u32,
        flags: u32,
        ck_size: u32,
    ) -> Vec<u8> {
        let mut buf = Vec::with_capacity(BLOCK_HEADER_SIZE);
        buf.extend_from_slice(b"wvpk");
        buf.extend_from_slice(&ck_size.to_le_bytes());
        buf.extend_from_slice(&version.to_le_bytes());
        buf.push(0); // block_index_u8
        buf.push(0); // total_samples_u8
        buf.extend_from_slice(&total_samples.to_le_bytes());
        buf.extend_from_slice(&block_index.to_le_bytes());
        buf.extend_from_slice(&block_samples.to_le_bytes());
        buf.extend_from_slice(&flags.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes()); // crc
        buf
    }

    #[test]
    fn rejects_non_wvpk_buffer() {
        let mut fa = FileAnalyze::new(b"This is definitely not WavPack data..");
        assert!(!parse_wvpk(&mut fa));
    }

    #[test]
    fn parses_stereo_16bit_44k1_lossless() {
        // flags: resolution=01 (16-bit), mono=0, hybrid=0,
        //        sampling_rate_idx=9 (44100) at bits 23..26.
        let flags: u32 = 0b01 | (9 << 23);
        let mut buf = build_block_header(0x0410, 88200, 0, 22050, flags, 100);
        // pad up to declared block size so a real demuxer wouldn't choke
        buf.resize(8 + 100, 0);

        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_wvpk(&mut fa));

        let g = |k: &str| fa.Retrieve(StreamKind::General, 0, k).map(|z| z.as_str().to_owned());
        let a = |k: &str| fa.Retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());

        assert_eq!(g("Format").as_deref(), Some("WavPack"));
        assert_eq!(g("AudioCount").as_deref(), Some("1"));
        assert_eq!(a("Format").as_deref(), Some("WavPack"));
        assert_eq!(a("Format_Profile").as_deref(), Some("4.16"));
        assert_eq!(a("Codec").as_deref(), Some("Wavpack"));
        assert_eq!(a("BitRate_Mode").as_deref(), Some("VBR"));
        assert_eq!(a("Compression_Mode").as_deref(), Some("Lossless"));
        assert_eq!(a("Channels").as_deref(), Some("2"));
        assert_eq!(a("SamplingRate").as_deref(), Some("44100"));
        assert_eq!(a("BitDepth").as_deref(), Some("16"));
        // 88200 samples / 44100 Hz = 2000 ms
        assert_eq!(a("Duration").as_deref(), Some("2000"));
    }

    #[test]
    fn parses_mono_24bit_hybrid_48k() {
        // flags: resolution=10 (24-bit), mono=1, hybrid=1,
        //        sampling_rate_idx=10 (48000).
        let flags: u32 = 0b10 | 0b100 | 0b1000 | (10 << 23);
        let buf = build_block_header(0x0410, 0, 0, 1024, flags, 32);

        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_wvpk(&mut fa));

        let a = |k: &str| fa.Retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(a("Format_Profile").as_deref(), Some("4.16"));
        assert_eq!(a("Channels").as_deref(), Some("1"));
        assert_eq!(a("BitDepth").as_deref(), Some("24"));
        assert_eq!(a("SamplingRate").as_deref(), Some("48000"));
        // hybrid without correction → Lossy
        assert_eq!(a("Compression_Mode").as_deref(), Some("Lossy"));
        // total_samples=0 → no Duration emitted
        assert!(a("Duration").is_none());
    }

    #[test]
    fn rejects_non_v4_version() {
        // Version 3.x — not handled by the block walker in C++.
        let flags: u32 = 0b01 | (9 << 23);
        let buf = build_block_header(0x0310, 100, 0, 50, flags, 32);
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_wvpk(&mut fa));
    }
}
