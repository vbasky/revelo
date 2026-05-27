//! Sony DSF (DSD Stream File) parser — .dsf.
//!
//! Mirrors the subset of MediaInfoLib's `File_Dsf.cpp` needed to fill
//! the audio + general fields the oracle emits for plain DSF files.
//!
//! All multi-byte fields are little-endian.
//!
//! Magic: "DSD " (0x44 0x53 0x44 0x20) at offset 0.
//!
//! Layout:
//!   "DSD " chunk (28 bytes total):
//!     4 bytes:  "DSD " magic
//!     8 bytes LE: chunk_size (= 28)
//!     8 bytes LE: total_file_size
//!     8 bytes LE: pointer_to_metadata (0 if none)
//!   "fmt " chunk (52 bytes total):
//!     4 bytes:  "fmt " magic
//!     8 bytes LE: chunk_size (= 52)
//!     4 bytes LE: format_version
//!     4 bytes LE: format_id (0 = DSD raw)
//!     4 bytes LE: channel_type (1..=7 → known layouts)
//!     4 bytes LE: channel_num
//!     4 bytes LE: sampling_frequency
//!     4 bytes LE: bits_per_sample (1 = little-endian DSD, 8 = big)
//!     8 bytes LE: sample_count (per channel)
//!     4 bytes LE: block_size_per_channel
//!     4 bytes LE: reserved
//!   "data" chunk: 4 bytes magic + 8 bytes LE size + sample bytes

use mediainfo_core::{FileAnalyze, StreamKind};

const DSF_CHANNEL_POSITIONS: [&str; 8] = [
    "",
    "Front: C",
    "Front: L R",
    "Front: L C R",
    "Front: L C R, LFE",
    "Front: L R, Side: L R",
    "Front: L C R, Side: L R",
    "Front: L C R, Side: L R, LFE",
];

const DSF_CHANNEL_LAYOUT: [&str; 8] = [
    "",
    "M",
    "L R",
    "L R C",
    "L R C LFE",
    "L R Ls Rs",
    "L R C Ls Rs",
    "L R C Ls Rs LFE",
];

pub fn parse_dsf(fa: &mut FileAnalyze) -> bool {
    // Need at least DSD chunk (28) + fmt chunk header (12) + fmt payload (40)
    // = 80 bytes to read fmt fields meaningfully.
    let head = fa.peek_raw(fa.Remain().min(80));
    let Some(h) = head else { return false };
    if h.len() < 80 || &h[0..4] != b"DSD " {
        return false;
    }

    // DSD chunk fields at offsets 4, 12, 20.
    let dsd_chunk_size = u64::from_le_bytes(h[4..12].try_into().unwrap());
    let total_file_size = u64::from_le_bytes(h[12..20].try_into().unwrap());
    // DSD chunk is fixed at 28 bytes.
    if dsd_chunk_size != 28 {
        return false;
    }

    // fmt chunk starts at offset 28.
    if &h[28..32] != b"fmt " {
        return false;
    }
    let fmt_chunk_size = u64::from_le_bytes(h[32..40].try_into().unwrap());
    if fmt_chunk_size != 52 {
        return false;
    }
    let format_version = u32::from_le_bytes(h[40..44].try_into().unwrap());
    let format_id = u32::from_le_bytes(h[44..48].try_into().unwrap());
    let channel_type = u32::from_le_bytes(h[48..52].try_into().unwrap());
    let channel_num = u32::from_le_bytes(h[52..56].try_into().unwrap());
    let sampling_frequency = u32::from_le_bytes(h[56..60].try_into().unwrap());
    let bits_per_sample = u32::from_le_bytes(h[60..64].try_into().unwrap());
    let sample_count = u64::from_le_bytes(h[64..72].try_into().unwrap());
    // h[72..76] = block_size_per_channel, h[76..80] = reserved (unused).

    if sampling_frequency == 0 || channel_num == 0 {
        return false;
    }

    // Try to read the data chunk header (at offset 80) for StreamSize.
    let mut audio_stream_size: u64 = 0;
    if let Some(full) = fa.peek_raw(fa.Remain().min(92)) {
        if full.len() >= 92 && &full[80..84] == b"data" {
            let data_chunk_size = u64::from_le_bytes(full[84..92].try_into().unwrap());
            // data chunk_size includes the 12-byte chunk header per spec.
            audio_stream_size = data_chunk_size.saturating_sub(12);
        }
    }

    fa.Stream_Prepare(StreamKind::General);
    fa.Fill(StreamKind::General, 0, "Format", "DSF", false);
    fa.Fill(
        StreamKind::General,
        0,
        "Format_Version",
        format!("Version {}", format_version),
        false,
    );
    fa.Fill(StreamKind::General, 0, "AudioCount", "1", false);
    let file_size_now = fa.Remain() as u64;
    if total_file_size != 0 && total_file_size != file_size_now {
        fa.Fill(StreamKind::General, 0, "Truncated", "Yes", false);
    }

    fa.Stream_Prepare(StreamKind::Audio);
    // FormatID 0 = DSD raw. C++ falls back to numeric for unknown IDs; we
    // do the same so non-standard files still get a Format value.
    if format_id == 0 {
        fa.Fill(StreamKind::Audio, 0, "Format", "DSD", false);
    } else {
        fa.Fill(StreamKind::Audio, 0, "Format", format_id.to_string(), false);
    }

    let ct_idx = channel_type as usize;
    if ct_idx > 0 && ct_idx < DSF_CHANNEL_POSITIONS.len() {
        fa.Fill(
            StreamKind::Audio,
            0,
            "ChannelPositions",
            DSF_CHANNEL_POSITIONS[ct_idx],
            false,
        );
        fa.Fill(
            StreamKind::Audio,
            0,
            "ChannelLayout",
            DSF_CHANNEL_LAYOUT[ct_idx],
            false,
        );
    }
    fa.Fill(StreamKind::Audio, 0, "Channels", channel_num.to_string(), false);
    fa.Fill(StreamKind::Audio, 0, "SamplingRate", sampling_frequency.to_string(), false);
    // bits_per_sample here is the byte-bit-order of the DSD stream (1=LE, 8=BE),
    // not the audio bit depth. Audio bit depth for DSD is always 1.
    match bits_per_sample {
        1 => {
            fa.Fill(StreamKind::Audio, 0, "Format_Settings", "Little", false);
            fa.Fill(StreamKind::Audio, 0, "Format_Settings_Endianness", "Little", false);
        }
        8 => {
            fa.Fill(StreamKind::Audio, 0, "Format_Settings", "Big", false);
            fa.Fill(StreamKind::Audio, 0, "Format_Settings_Endianness", "Big", false);
        }
        _ => {}
    }
    fa.Fill(StreamKind::Audio, 0, "BitDepth", "1", false);
    fa.Fill(StreamKind::Audio, 0, "SamplingCount", sample_count.to_string(), false);
    fa.Fill(StreamKind::Audio, 0, "Compression_Mode", "Lossless", false);
    fa.Fill(StreamKind::Audio, 0, "BitRate_Mode", "CBR", false);
    if audio_stream_size > 0 {
        fa.Fill(
            StreamKind::Audio,
            0,
            "StreamSize",
            audio_stream_size.to_string(),
            false,
        );
    }

    // Format_Commercial_IfAny: DSDxxx where xxx is the multiplier over the
    // CD/DAT base rate (DSD64 = 64×44100, DSD128 = 128×44100, etc).
    let sr = sampling_frequency as u64;
    let mut mult = 64u64;
    while mult <= 512 {
        let base = sr / mult;
        if base == 48000 || base == 44100 {
            fa.Fill(
                StreamKind::Audio,
                0,
                "Format_Commercial_IfAny",
                format!("DSD{}", mult),
                false,
            );
            break;
        }
        mult *= 2;
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal valid DSF buffer with the given fmt fields.
    fn make_dsf(
        channel_type: u32,
        channel_num: u32,
        sampling_freq: u32,
        bits_per_sample: u32,
        sample_count: u64,
        data_payload_size: usize,
    ) -> Vec<u8> {
        let data_chunk_size: u64 = 12 + data_payload_size as u64;
        let total_file_size: u64 = 28 + 52 + data_chunk_size;
        let mut buf = Vec::new();
        // DSD chunk
        buf.extend_from_slice(b"DSD ");
        buf.extend_from_slice(&28u64.to_le_bytes());
        buf.extend_from_slice(&total_file_size.to_le_bytes());
        buf.extend_from_slice(&0u64.to_le_bytes()); // metadata ptr = 0
        // fmt chunk
        buf.extend_from_slice(b"fmt ");
        buf.extend_from_slice(&52u64.to_le_bytes());
        buf.extend_from_slice(&1u32.to_le_bytes()); // version
        buf.extend_from_slice(&0u32.to_le_bytes()); // format_id = DSD
        buf.extend_from_slice(&channel_type.to_le_bytes());
        buf.extend_from_slice(&channel_num.to_le_bytes());
        buf.extend_from_slice(&sampling_freq.to_le_bytes());
        buf.extend_from_slice(&bits_per_sample.to_le_bytes());
        buf.extend_from_slice(&sample_count.to_le_bytes());
        buf.extend_from_slice(&4096u32.to_le_bytes()); // block size
        buf.extend_from_slice(&0u32.to_le_bytes()); // reserved
        // data chunk
        buf.extend_from_slice(b"data");
        buf.extend_from_slice(&data_chunk_size.to_le_bytes());
        buf.resize(buf.len() + data_payload_size, 0);
        buf
    }

    #[test]
    fn parses_minimal_dsf_stereo_dsd64() {
        // 2.8224 MHz = 64 × 44100, stereo, 1-bit LE.
        let buf = make_dsf(2, 2, 2_822_400, 1, 100_000, 25_000);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_dsf(&mut fa));

        let g = |k: &str| fa.Retrieve(StreamKind::General, 0, k).map(|z| z.as_str().to_owned());
        let a = |k: &str| fa.Retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());

        assert_eq!(g("Format").as_deref(), Some("DSF"));
        assert_eq!(g("Format_Version").as_deref(), Some("Version 1"));
        assert_eq!(g("AudioCount").as_deref(), Some("1"));

        assert_eq!(a("Format").as_deref(), Some("DSD"));
        assert_eq!(a("Channels").as_deref(), Some("2"));
        assert_eq!(a("ChannelPositions").as_deref(), Some("Front: L R"));
        assert_eq!(a("ChannelLayout").as_deref(), Some("L R"));
        assert_eq!(a("SamplingRate").as_deref(), Some("2822400"));
        assert_eq!(a("BitDepth").as_deref(), Some("1"));
        assert_eq!(a("SamplingCount").as_deref(), Some("100000"));
        assert_eq!(a("Compression_Mode").as_deref(), Some("Lossless"));
        assert_eq!(a("BitRate_Mode").as_deref(), Some("CBR"));
        assert_eq!(a("Format_Settings_Endianness").as_deref(), Some("Little"));
        assert_eq!(a("StreamSize").as_deref(), Some("25000"));
        assert_eq!(a("Format_Commercial_IfAny").as_deref(), Some("DSD64"));
    }

    #[test]
    fn rejects_non_dsf() {
        let mut fa = FileAnalyze::new(b"RIFF....WAVEfmt this is not DSF padding padding padding padding");
        assert!(!parse_dsf(&mut fa));
    }

    #[test]
    fn parses_dsd128_5_1() {
        // 5.6448 MHz = 128 × 44100, channel_type=7 (5.1 with LFE), 6 channels.
        let buf = make_dsf(7, 6, 5_644_800, 8, 200_000, 60_000);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_dsf(&mut fa));
        let a = |k: &str| fa.Retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(a("Channels").as_deref(), Some("6"));
        assert_eq!(a("ChannelLayout").as_deref(), Some("L R C Ls Rs LFE"));
        assert_eq!(a("Format_Settings_Endianness").as_deref(), Some("Big"));
        assert_eq!(a("Format_Commercial_IfAny").as_deref(), Some("DSD128"));
    }
}
