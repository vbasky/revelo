//! WAV / RIFF-WAVE parser.
//!
//! Subset of MediaInfoLib's `File_Riff.cpp` targeting plain PCM WAV. The
//! C++ parser handles AVI, RF64, BW64, DV-in-AVI, and many other RIFF
//! variants — this is just the PCM-WAV slice, enough to validate the
//! engine architecture end-to-end against the oracle.
//!
//! Layout walked:
//!   "RIFF" <u32 LE total-size-minus-8> "WAVE"
//!     chunks:
//!       "fmt " <u32 LE size> <format details>
//!       "data" <u32 LE size> <samples>
//!       (other chunks ignored)

use revelo_core::mime::mime_for_container;
use revelo_core::{FileAnalyze, StreamKind};

const FOURCC_RIFF: u32 = u32::from_be_bytes(*b"RIFF");
const FOURCC_RF64: u32 = u32::from_be_bytes(*b"RF64");
const FOURCC_WAVE: u32 = u32::from_be_bytes(*b"WAVE");
const FOURCC_FMT: u32 = u32::from_be_bytes(*b"fmt ");
const FOURCC_DATA: u32 = u32::from_be_bytes(*b"data");
const FOURCC_DS64: u32 = u32::from_be_bytes(*b"ds64");
const FOURCC_BEXT: u32 = u32::from_be_bytes(*b"bext");
const FOURCC_IXML: u32 = u32::from_be_bytes(*b"iXML");
const FOURCC_AXML: u32 = u32::from_be_bytes(*b"axml");
const FOURCC_UMID: u32 = u32::from_be_bytes(*b"umid");

const BEXT_FIXED_PREFIX_LEN: usize = 602;
const BEXT_CODING_HISTORY_LIMIT: usize = 16 * 1024;
const BEXT_PARSE_LIMIT: usize = BEXT_FIXED_PREFIX_LEN + BEXT_CODING_HISTORY_LIMIT;
const IXML_PARSE_LIMIT: usize = 64 * 1024;
const UMID_PARSE_LIMIT: usize = 64;

// Common WAVEFORMATEX format codes — only the ones we handle by name.
const WAVE_FORMAT_PCM: u16 = 0x0001;
const WAVE_FORMAT_IEEE_FLOAT: u16 = 0x0003;
const WAVE_FORMAT_ALAW: u16 = 0x0006;
const WAVE_FORMAT_MULAW: u16 = 0x0007;
const WAVE_FORMAT_EXTENSIBLE: u16 = 0xFFFE;

#[derive(Debug, Default)]
struct FmtChunk {
    audio_format: u16,
    num_channels: u16,
    sample_rate: u32,
    #[allow(dead_code)]
    byte_rate: u32,
    block_align: u16,
    bits_per_sample: u16,
}

#[derive(Debug, Default)]
struct BwfInfo {
    description: Option<String>,
    originator: Option<String>,
    originator_ref: Option<String>,
    origination_date: Option<String>,
    origination_time: Option<String>,
    time_reference: Option<u64>,
    bwf_version: Option<u16>,
    umid: Option<String>,
    coding_history: Option<String>,
    loudness_value: Option<i16>,
    loudness_range: Option<i16>,
    max_true_peak: Option<i16>,
}

fn read_padded_string(buf: &[u8], offset: usize, max_len: usize) -> Option<String> {
    let end = offset.saturating_add(max_len).min(buf.len());
    let slice = &buf[offset..end];
    let up_to_null = slice.split(|&b| b == 0).next().unwrap_or(slice);
    let s = std::str::from_utf8(up_to_null).ok()?;
    let trimmed = s.trim_end_matches(' ');
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.to_string())
}

fn parse_bext_chunk(fa: &mut FileAnalyze, chunk_size: usize) -> BwfInfo {
    let raw = fa.peek_raw(chunk_size.min(BEXT_PARSE_LIMIT)).unwrap_or_default();
    let mut info = BwfInfo::default();

    if raw.len() < 256 {
        return info;
    }
    info.description = read_padded_string(raw, 0, 256);
    if raw.len() < 288 {
        return info;
    }
    info.originator = read_padded_string(raw, 256, 32);
    if raw.len() < 320 {
        return info;
    }
    info.originator_ref = read_padded_string(raw, 288, 32);
    if raw.len() < 340 {
        return info;
    }
    info.origination_date = read_padded_string(raw, 320, 20);
    info.origination_time = read_padded_string(raw, 340, 10);
    if raw.len() < 358 {
        return info;
    }
    let tr_low = u64::from(raw[350])
        | u64::from(raw[351]) << 8
        | u64::from(raw[352]) << 16
        | u64::from(raw[353]) << 24;
    let tr_high = u64::from(raw[354])
        | u64::from(raw[355]) << 8
        | u64::from(raw[356]) << 16
        | u64::from(raw[357]) << 24;
    info.time_reference = Some(tr_low | (tr_high << 32));
    if raw.len() < 360 {
        return info;
    }
    info.bwf_version = Some(u16::from(raw[358]) | u16::from(raw[359]) << 8);
    if raw.len() < 424 {
        return info;
    }
    let umid_bytes = &raw[360..424.min(raw.len())];
    let hex: String = umid_bytes.iter().map(|b| format!("{:02X}", b)).collect();
    if hex.chars().any(|c| c != '0') {
        info.umid = Some(hex);
    }
    if raw.len() >= 428 {
        info.loudness_value = Some(i16::from_le_bytes([raw[424], raw[425]]));
    }
    if raw.len() >= 430 {
        info.loudness_range = Some(i16::from_le_bytes([raw[426], raw[427]]));
    }
    if raw.len() >= 432 {
        info.max_true_peak = Some(i16::from_le_bytes([raw[428], raw[429]]));
    }
    if raw.len() > 602 {
        let coding_start = 602usize.min(raw.len());
        if coding_start < raw.len() {
            info.coding_history = read_padded_string(raw, coding_start, raw.len() - coding_start);
        }
    }
    info
}

/// Parse a WAV file buffer, filling the General and Audio streams on the
/// provided FileAnalyze. Returns `true` if a valid RIFF/WAVE container
/// was recognized.
pub fn parse_wav(fa: &mut FileAnalyze) -> bool {
    let magic = fa.peek_b4();
    if magic != FOURCC_RIFF && magic != FOURCC_RF64 {
        return false;
    }

    fa.element_begin("RIFF");
    let riff_id = fa.get_c4("ID");
    let _riff_size = fa.get_l4("Size");
    let form_type = fa.get_c4("Type");
    let is_rf64 = riff_id == FOURCC_RF64;

    if form_type != FOURCC_WAVE {
        fa.element_end();
        return false;
    }

    let mut fmt: Option<FmtChunk> = None;
    let mut data_size: u64 = 0;
    let mut rf64_data_size: Option<u64> = None;
    let mut bwf: Option<BwfInfo> = None;

    while fa.remain() >= 8 {
        let chunk_id = fa.get_c4("ChunkID");
        let chunk_size = fa.get_l4("ChunkSize");

        let chunk_size_usize = if is_rf64
            && chunk_id == FOURCC_DATA
            && chunk_size == u32::MAX
            && let Some(size64) = rf64_data_size
        {
            let Ok(size64) = usize::try_from(size64) else {
                break;
            };
            size64
        } else {
            chunk_size as usize
        };
        if fa.remain() < chunk_size_usize {
            break;
        }

        match chunk_id {
            FOURCC_DS64 if is_rf64 => {
                fa.element_begin("ds64");
                if chunk_size_usize >= 28 {
                    let _riff_size_64 = fa.get_l8("riffSize");
                    rf64_data_size = Some(fa.get_l8("dataSize"));
                    let _sample_count_64 = fa.get_l8("sampleCount");
                    let _table_length = fa.get_l4("tableLength");
                    fa.skip_hexa(chunk_size_usize - 28, "ds64_tail");
                } else {
                    fa.skip_hexa(chunk_size_usize, "ds64_short");
                }
                if chunk_size_usize % 2 == 1 {
                    let mut _pad: u8 = 0;
                    _pad = fa.get_b1("Padding");
                }
                fa.element_end();
            }
            FOURCC_FMT => {
                fa.element_begin("fmt");
                let audio_format = fa.get_l2("AudioFormat");
                let num_channels = fa.get_l2("NumChannels");
                let sample_rate = fa.get_l4("SampleRate");
                let byte_rate = fa.get_l4("ByteRate");
                let block_align = fa.get_l2("BlockAlign");
                let bits_per_sample = fa.get_l2("BitsPerSample");

                // Consume any trailing extension bytes within this chunk.
                let consumed_in_fmt: usize = 16;
                if chunk_size_usize > consumed_in_fmt {
                    fa.skip_hexa(chunk_size_usize - consumed_in_fmt, "Extension");
                }
                if chunk_size_usize % 2 == 1 {
                    let mut _pad: u8 = 0;
                    _pad = fa.get_b1("Padding");
                }

                fa.element_end();
                fmt = Some(FmtChunk {
                    audio_format,
                    num_channels,
                    sample_rate,
                    byte_rate,
                    block_align,
                    bits_per_sample,
                });
            }
            FOURCC_DATA => {
                fa.element_begin("data");
                data_size = if is_rf64 && chunk_size == u32::MAX {
                    rf64_data_size.unwrap_or(u64::from(chunk_size))
                } else {
                    u64::from(chunk_size)
                };
                fa.skip_hexa(chunk_size_usize, "Samples");
                if chunk_size_usize % 2 == 1 {
                    let mut _pad: u8 = 0;
                    _pad = fa.get_b1("Padding");
                }
                fa.element_end();
            }
            FOURCC_BEXT => {
                fa.element_begin("bext");
                bwf = Some(parse_bext_chunk(fa, chunk_size_usize));
                fa.skip_hexa(chunk_size_usize, "BroadcastAudioExtension");
                fa.element_end();
            }
            FOURCC_IXML => {
                fa.element_begin("iXML");
                if chunk_size_usize <= IXML_PARSE_LIMIT
                    && let Some(raw) = fa.peek_raw(chunk_size_usize)
                    && let Ok(xml) = std::str::from_utf8(raw)
                {
                    let b = bwf.get_or_insert_with(BwfInfo::default);
                    b.description = b.description.take().or_else(|| {
                        let s = xml.trim().to_string();
                        if s.is_empty() { None } else { Some(s) }
                    });
                }
                fa.skip_hexa(chunk_size_usize, "iXML");
                fa.element_end();
            }
            FOURCC_AXML => {
                fa.element_begin("axml");
                fa.skip_hexa(chunk_size_usize, "ADMXML");
                fa.element_end();
            }
            FOURCC_UMID => {
                fa.element_begin("umid");
                if let Some(raw) = fa.peek_raw(chunk_size_usize.min(UMID_PARSE_LIMIT)) {
                    let hex: String = raw.iter().map(|b| format!("{:02X}", b)).collect();
                    if hex.chars().any(|c| c != '0') {
                        let b = bwf.get_or_insert_with(BwfInfo::default);
                        b.umid = Some(hex);
                    }
                }
                fa.skip_hexa(chunk_size_usize, "UMID");
                fa.element_end();
            }
            _ => {
                // Unknown chunk — skip it, honoring word-alignment.
                fa.skip_hexa(chunk_size_usize, "Unknown");
                if chunk_size_usize % 2 == 1 {
                    let mut _pad: u8 = 0;
                    _pad = fa.get_b1("Padding");
                }
            }
        }
    }

    fa.element_end();

    if let Some(fmt) = fmt {
        fill_streams(fa, &fmt, data_size, &bwf);
        true
    } else {
        false
    }
}

fn fill_streams(fa: &mut FileAnalyze, fmt: &FmtChunk, data_size: u64, bwf: &Option<BwfInfo>) {
    fa.stream_prepare(StreamKind::General);
    fa.set_field(StreamKind::General, 0, "Format", "Wave");
    if let Some(m) = mime_for_container("WAVE") {
        fa.set_field(StreamKind::General, 0, "InternetMediaType", m);
    }

    if fmt.audio_format == WAVE_FORMAT_PCM {
        fa.set_field(StreamKind::General, 0, "Format_Settings", "PcmWaveformat");
    }

    fa.stream_prepare(StreamKind::Audio);
    let (fmt_name, endianness, sign) = wav_format_descriptors(fmt);
    fa.set_field(StreamKind::Audio, 0, "Format", fmt_name);
    if let Some(e) = endianness {
        fa.set_field(StreamKind::Audio, 0, "Format_Settings_Endianness", e);
    }
    if let Some(s) = sign {
        fa.set_field(StreamKind::Audio, 0, "Format_Settings_Sign", s);
    }
    fa.set_field(StreamKind::Audio, 0, "CodecID", fmt.audio_format.to_string());

    fa.set_field(StreamKind::Audio, 0, "BitRate_Mode", "CBR");

    let bitrate =
        (fmt.sample_rate as u64) * (fmt.num_channels as u64) * (fmt.bits_per_sample as u64);
    if bitrate > 0 {
        fa.set_field(StreamKind::Audio, 0, "BitRate", bitrate.to_string());
    }

    fa.set_field(StreamKind::Audio, 0, "Channels", fmt.num_channels.to_string());
    fa.set_field(StreamKind::Audio, 0, "SamplingRate", fmt.sample_rate.to_string());
    fa.set_field(StreamKind::Audio, 0, "BitDepth", fmt.bits_per_sample.to_string());
    fa.set_field(StreamKind::Audio, 0, "StreamSize", data_size.to_string());

    if fmt.block_align > 0 {
        let sample_count = data_size / fmt.block_align as u64;
        fa.set_field(StreamKind::Audio, 0, "SamplingCount", sample_count.to_string());
        if fmt.sample_rate > 0 {
            // Duration in milliseconds, matching C++ output convention.
            let duration_ms = revelo_core::duration_ms(sample_count, fmt.sample_rate as u64);
            fa.set_field(StreamKind::Audio, 0, "Duration", duration_ms.to_string());
        }
    }

    fa.set_field(StreamKind::General, 0, "AudioCount", "1");

    if let Some(bwf) = bwf {
        if bwf.description.is_some() || bwf.originator.is_some() {
            fa.set_field(StreamKind::General, 0, "Format_Commercial", "Broadcast Wave");
        }
        if let Some(ref d) = bwf.description {
            fa.set_field(StreamKind::General, 0, "Title", d.clone());
        }
        if let Some(ref o) = bwf.originator {
            fa.set_field(StreamKind::General, 0, "Encoded_Library", o.clone());
        }
        if let Some(ref r) = bwf.originator_ref {
            fa.set_field(StreamKind::General, 0, "Encoded_Library_Settings", r.clone());
        }
        if let Some(ref d) = bwf.origination_date {
            fa.set_field(StreamKind::General, 0, "Recorded_Date", d.clone());
        }
        if let Some(ref t) = bwf.origination_time {
            fa.set_field(StreamKind::General, 0, "Recorded_Time", t.clone());
        }
        if let Some(tr) = bwf.time_reference {
            fa.set_field(StreamKind::General, 0, "TimeReference", tr.to_string());
        }
        if let Some(v) = bwf.bwf_version {
            fa.set_field(StreamKind::General, 0, "Format_Version", v.to_string());
        }
        if let Some(ref u) = bwf.umid {
            fa.set_field(StreamKind::General, 0, "UMID", u.clone());
        }
        if let Some(lv) = bwf.loudness_value
            && lv != -32768
        {
            fa.set_field(
                StreamKind::Audio,
                0,
                "Loudness_Value",
                format!("{:.1} LUFS", lv as f64 * 0.1),
            );
        }
        if let Some(lr) = bwf.loudness_range
            && lr != -32768
        {
            fa.set_field(
                StreamKind::Audio,
                0,
                "Loudness_Range",
                format!("{:.1} LU", lr as f64 * 0.1),
            );
        }
        if let Some(pt) = bwf.max_true_peak
            && pt != -32768
        {
            fa.set_field(
                StreamKind::Audio,
                0,
                "Loudness_MaxTruePeakLevel",
                format!("{:.1} dBTP", pt as f64 * 0.1),
            );
        }
    }
}

fn wav_format_descriptors(
    fmt: &FmtChunk,
) -> (&'static str, Option<&'static str>, Option<&'static str>) {
    match fmt.audio_format {
        WAVE_FORMAT_PCM => {
            let sign = if fmt.bits_per_sample <= 8 { Some("Unsigned") } else { Some("Signed") };
            ("PCM", Some("Little"), sign)
        }
        WAVE_FORMAT_IEEE_FLOAT => ("PCM", Some("Little"), Some("Signed")),
        WAVE_FORMAT_ALAW => ("A-law", None, None),
        WAVE_FORMAT_MULAW => ("µ-law", None, None),
        WAVE_FORMAT_EXTENSIBLE => ("PCM", Some("Little"), None),
        _ => ("Unknown", None, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AUDIO_METADATA_ONLY_BUDGET: u64 = 8 * 1024 * 1024;
    const TEST_LARGE_CHUNK_SIZE: usize = 9 * 1024 * 1024;

    fn append_pcm_fmt(buf: &mut Vec<u8>) {
        buf.extend_from_slice(b"fmt ");
        buf.extend_from_slice(&16u32.to_le_bytes());
        buf.extend_from_slice(&1u16.to_le_bytes());
        buf.extend_from_slice(&1u16.to_le_bytes());
        buf.extend_from_slice(&48000u32.to_le_bytes());
        buf.extend_from_slice(&96000u32.to_le_bytes());
        buf.extend_from_slice(&2u16.to_le_bytes());
        buf.extend_from_slice(&16u16.to_le_bytes());
    }

    fn append_large_chunk(buf: &mut Vec<u8>, id: &[u8; 4], size_field: u32, payload_len: usize) {
        buf.extend_from_slice(id);
        buf.extend_from_slice(&size_field.to_le_bytes());
        buf.resize(buf.len() + payload_len, 0);
    }

    fn wav_with_large_metadata_and_data() -> Vec<u8> {
        let mut body = Vec::new();
        append_pcm_fmt(&mut body);
        append_large_chunk(&mut body, b"LIST", TEST_LARGE_CHUNK_SIZE as u32, TEST_LARGE_CHUNK_SIZE);
        append_large_chunk(&mut body, b"ID3 ", TEST_LARGE_CHUNK_SIZE as u32, TEST_LARGE_CHUNK_SIZE);
        append_large_chunk(&mut body, b"data", TEST_LARGE_CHUNK_SIZE as u32, TEST_LARGE_CHUNK_SIZE);

        let mut buf = Vec::new();
        buf.extend_from_slice(b"RIFF");
        buf.extend_from_slice(&(4 + body.len() as u32).to_le_bytes());
        buf.extend_from_slice(b"WAVE");
        buf.extend(body);
        buf
    }

    fn rf64_with_large_data() -> Vec<u8> {
        let mut body = Vec::new();
        body.extend_from_slice(b"ds64");
        body.extend_from_slice(&28u32.to_le_bytes());
        let riff_size_64 = 4 + (8 + 28) + (8 + 16) + (8 + TEST_LARGE_CHUNK_SIZE as u64) * 2;
        body.extend_from_slice(&riff_size_64.to_le_bytes());
        body.extend_from_slice(&(TEST_LARGE_CHUNK_SIZE as u64).to_le_bytes());
        body.extend_from_slice(&((TEST_LARGE_CHUNK_SIZE / 2) as u64).to_le_bytes());
        body.extend_from_slice(&0u32.to_le_bytes());
        append_pcm_fmt(&mut body);
        append_large_chunk(&mut body, b"ID3 ", TEST_LARGE_CHUNK_SIZE as u32, TEST_LARGE_CHUNK_SIZE);
        append_large_chunk(&mut body, b"data", u32::MAX, TEST_LARGE_CHUNK_SIZE);

        let mut buf = Vec::new();
        buf.extend_from_slice(b"RF64");
        buf.extend_from_slice(&u32::MAX.to_le_bytes());
        buf.extend_from_slice(b"WAVE");
        buf.extend(body);
        buf
    }

    /// Build a minimal valid PCM WAV: 1 channel, 8000 Hz, 16-bit, with
    /// `frame_count` audio frames (each 2 bytes). Returns the buffer.
    fn make_pcm_wav(channels: u16, sample_rate: u32, bits: u16, frame_count: u32) -> Vec<u8> {
        let block_align = channels * (bits / 8);
        let data_size = frame_count * block_align as u32;
        let byte_rate = sample_rate * (block_align as u32);
        let mut buf = Vec::with_capacity(44 + data_size as usize);

        buf.extend_from_slice(b"RIFF");
        buf.extend_from_slice(&(36 + data_size).to_le_bytes());
        buf.extend_from_slice(b"WAVE");

        buf.extend_from_slice(b"fmt ");
        buf.extend_from_slice(&16u32.to_le_bytes());
        buf.extend_from_slice(&1u16.to_le_bytes()); // PCM
        buf.extend_from_slice(&channels.to_le_bytes());
        buf.extend_from_slice(&sample_rate.to_le_bytes());
        buf.extend_from_slice(&byte_rate.to_le_bytes());
        buf.extend_from_slice(&block_align.to_le_bytes());
        buf.extend_from_slice(&bits.to_le_bytes());

        buf.extend_from_slice(b"data");
        buf.extend_from_slice(&data_size.to_le_bytes());
        buf.resize(buf.len() + data_size as usize, 0);
        buf
    }

    #[test]
    fn parse_minimal_pcm_wav() {
        let buf = make_pcm_wav(1, 8000, 16, 1600); // 1ch, 8kHz, 16-bit, 0.2s of silence
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_wav(&mut fa));

        let g = |key: &str| fa.retrieve(StreamKind::General, 0, key).map(|z| z.as_str().to_owned());
        let a = |key: &str| fa.retrieve(StreamKind::Audio, 0, key).map(|z| z.as_str().to_owned());

        assert_eq!(g("Format").as_deref(), Some("Wave"));
        assert_eq!(g("Format_Settings").as_deref(), Some("PcmWaveformat"));
        assert_eq!(g("AudioCount").as_deref(), Some("1"));

        assert_eq!(a("Format").as_deref(), Some("PCM"));
        assert_eq!(a("Format_Settings_Endianness").as_deref(), Some("Little"));
        assert_eq!(a("Format_Settings_Sign").as_deref(), Some("Signed"));
        assert_eq!(a("CodecID").as_deref(), Some("1"));
        assert_eq!(a("BitRate_Mode").as_deref(), Some("CBR"));
        assert_eq!(a("BitRate").as_deref(), Some("128000")); // 8000*1*16
        assert_eq!(a("Channels").as_deref(), Some("1"));
        assert_eq!(a("SamplingRate").as_deref(), Some("8000"));
        assert_eq!(a("BitDepth").as_deref(), Some("16"));
        assert_eq!(a("SamplingCount").as_deref(), Some("1600"));
        assert_eq!(a("Duration").as_deref(), Some("200")); // 1600/8000 = 0.2s = 200ms
        assert_eq!(a("StreamSize").as_deref(), Some("3200"));
    }

    #[test]
    fn parse_stereo_48khz_24bit() {
        let buf = make_pcm_wav(2, 48000, 24, 48000); // 2ch, 48kHz, 24-bit, 1s
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_wav(&mut fa));
        let a = |key: &str| fa.retrieve(StreamKind::Audio, 0, key).map(|z| z.as_str().to_owned());
        assert_eq!(a("Channels").as_deref(), Some("2"));
        assert_eq!(a("SamplingRate").as_deref(), Some("48000"));
        assert_eq!(a("BitDepth").as_deref(), Some("24"));
        assert_eq!(a("BitRate").as_deref(), Some("2304000"));
        assert_eq!(a("Duration").as_deref(), Some("1000"));
    }

    #[test]
    fn eight_bit_pcm_is_unsigned() {
        let buf = make_pcm_wav(1, 11025, 8, 100);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_wav(&mut fa));
        assert_eq!(
            fa.retrieve(StreamKind::Audio, 0, "Format_Settings_Sign")
                .map(|z| z.as_str().to_owned())
                .as_deref(),
            Some("Unsigned")
        );
    }

    #[test]
    fn non_riff_buffer_returns_false() {
        let buf = b"This is not a WAV file at all";
        let mut fa = FileAnalyze::new(buf);
        assert!(!parse_wav(&mut fa));
    }

    #[test]
    fn riff_without_wave_form_type_returns_false() {
        let mut buf = Vec::new();
        buf.extend_from_slice(b"RIFF");
        buf.extend_from_slice(&8u32.to_le_bytes());
        buf.extend_from_slice(b"AVI ");
        buf.extend_from_slice(&[0; 4]);
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_wav(&mut fa));
    }

    #[test]
    fn skips_unknown_chunks_between_fmt_and_data() {
        // Build a WAV with a LIST chunk between fmt and data.
        let mut buf = Vec::new();
        let frame_count = 100u32;
        let block_align = 2u16;
        let data_size = frame_count * block_align as u32;
        let list_size = 12u32;

        buf.extend_from_slice(b"RIFF");
        let riff_size = 4 + (8 + 16) + (8 + list_size) + (8 + data_size);
        buf.extend_from_slice(&riff_size.to_le_bytes());
        buf.extend_from_slice(b"WAVE");

        // fmt
        buf.extend_from_slice(b"fmt ");
        buf.extend_from_slice(&16u32.to_le_bytes());
        buf.extend_from_slice(&1u16.to_le_bytes());
        buf.extend_from_slice(&1u16.to_le_bytes());
        buf.extend_from_slice(&8000u32.to_le_bytes());
        buf.extend_from_slice(&16000u32.to_le_bytes());
        buf.extend_from_slice(&2u16.to_le_bytes());
        buf.extend_from_slice(&16u16.to_le_bytes());

        // LIST chunk (unknown, should be skipped)
        buf.extend_from_slice(b"LIST");
        buf.extend_from_slice(&list_size.to_le_bytes());
        buf.resize(buf.len() + list_size as usize, 0xAA);

        // data
        buf.extend_from_slice(b"data");
        buf.extend_from_slice(&data_size.to_le_bytes());
        buf.resize(buf.len() + data_size as usize, 0);

        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_wav(&mut fa));
        assert_eq!(
            fa.retrieve(StreamKind::Audio, 0, "SamplingCount")
                .map(|z| z.as_str().to_owned())
                .as_deref(),
            Some("100")
        );
    }

    #[test]
    fn large_wav_metadata_and_data_access_stays_bounded() {
        for (label, buf) in
            [("riff_wave", wav_with_large_metadata_and_data()), ("rf64", rf64_with_large_data())]
        {
            assert!(buf.len() as u64 > AUDIO_METADATA_ONLY_BUDGET, "{label}");

            let mut fa = FileAnalyze::new(&buf);
            assert!(parse_wav(&mut fa), "{label}");
            let stream_size =
                fa.retrieve(StreamKind::Audio, 0, "StreamSize").map(|z| z.as_str().to_owned());
            assert_eq!(stream_size.as_deref(), Some("9437184"), "{label}");

            let stats = fa.access_stats();
            assert!(stats.bytes_requested < AUDIO_METADATA_ONLY_BUDGET, "{label}: {stats:?}");
            assert!(stats.bytes_returned < AUDIO_METADATA_ONLY_BUDGET, "{label}: {stats:?}");
            assert!(stats.max_request_len <= BEXT_PARSE_LIMIT, "{label}: {stats:?}");
        }
    }

    #[test]
    fn parse_bwf_bext_chunk() {
        fn make_bwf_wav() -> Vec<u8> {
            let bext = {
                let mut b = Vec::new();
                let desc = b"My Test Recording\0";
                b.extend_from_slice(desc);
                b.resize(256, 0);
                let orig = b"TestCorp\0";
                b.extend_from_slice(orig);
                b.resize(288, 0);
                let refstr = b"TC-2024-001\0";
                b.extend_from_slice(refstr);
                b.resize(320, 0);
                b.extend_from_slice(b"2024-03-15");
                b.resize(340, 0);
                b.extend_from_slice(b"10:30:00");
                b.resize(350, 0);
                b.extend_from_slice(&48000u64.to_le_bytes());
                b.extend_from_slice(&1u16.to_le_bytes());
                b.resize(424, 0);
                b.extend_from_slice(&(-240i16).to_le_bytes());
                b.extend_from_slice(&100i16.to_le_bytes());
                b.extend_from_slice(&(-10i16).to_le_bytes());
                b.resize(604, 0);
                b.extend_from_slice(b"A=PCM,F=48000,W=16,M=stereo,T=TestCorp\0");
                b
            };
            let data = vec![0u8; 48000 * 2];
            let data_size = data.len() as u32;
            let bext_size = bext.len() as u32;
            let riff_size = 4
                + 8
                + 16
                + 8
                + bext_size
                + 8
                + data_size
                + (if bext_size % 2 == 1 { 1 } else { 0 });
            let mut buf = Vec::new();
            buf.extend_from_slice(b"RIFF");
            buf.extend_from_slice(&riff_size.to_le_bytes());
            buf.extend_from_slice(b"WAVE");
            buf.extend_from_slice(b"fmt ");
            buf.extend_from_slice(&16u32.to_le_bytes());
            buf.extend_from_slice(&1u16.to_le_bytes()); // PCM
            buf.extend_from_slice(&1u16.to_le_bytes()); // mono
            buf.extend_from_slice(&48000u32.to_le_bytes());
            buf.extend_from_slice(&96000u32.to_le_bytes());
            buf.extend_from_slice(&2u16.to_le_bytes()); // block align
            buf.extend_from_slice(&16u16.to_le_bytes());
            buf.extend_from_slice(b"bext");
            buf.extend_from_slice(&bext_size.to_le_bytes());
            buf.extend_from_slice(&bext);
            if bext_size % 2 == 1 {
                buf.push(0);
            }
            buf.extend_from_slice(b"data");
            buf.extend_from_slice(&data_size.to_le_bytes());
            buf.extend_from_slice(&data);
            buf
        }
        let buf = make_bwf_wav();
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_wav(&mut fa));
        let g = |key: &str| fa.retrieve(StreamKind::General, 0, key).map(|z| z.as_str().to_owned());
        let a = |key: &str| fa.retrieve(StreamKind::Audio, 0, key).map(|z| z.as_str().to_owned());
        assert_eq!(g("Format_Commercial").as_deref(), Some("Broadcast Wave"));
        assert_eq!(g("Title").as_deref(), Some("My Test Recording"));
        assert_eq!(g("Encoded_Library").as_deref(), Some("TestCorp"));
        assert_eq!(g("Recorded_Date").as_deref(), Some("2024-03-15"));
        assert_eq!(g("Recorded_Time").as_deref(), Some("10:30:00"));
        assert_eq!(a("Loudness_Value").as_deref(), Some("-24.0 LUFS"));
        assert_eq!(a("Loudness_Range").as_deref(), Some("10.0 LU"));
        assert_eq!(a("Loudness_MaxTruePeakLevel").as_deref(), Some("-1.0 dBTP"));
        assert_eq!(g("TimeReference").as_deref(), Some("48000"));
        assert_eq!(g("Format_Version").as_deref(), Some("1"));
    }

    #[test]
    fn parse_bwf_ixml_chunk() {
        let ixml =
            br#"<?xml version="1.0"?><BWFXML><Description>Scene 5 Take 2</Description></BWFXML>"#;
        let data = vec![0u8; 48000 * 2];
        let data_size = data.len() as u32;
        let ixml_size = ixml.len() as u32;
        let riff_size = 4 + 8 + 16 + 8 + ixml_size + 8 + data_size;
        let mut buf = Vec::new();
        buf.extend_from_slice(b"RIFF");
        buf.extend_from_slice(&riff_size.to_le_bytes());
        buf.extend_from_slice(b"WAVE");
        buf.extend_from_slice(b"fmt ");
        buf.extend_from_slice(&16u32.to_le_bytes());
        buf.extend_from_slice(&1u16.to_le_bytes());
        buf.extend_from_slice(&1u16.to_le_bytes());
        buf.extend_from_slice(&48000u32.to_le_bytes());
        buf.extend_from_slice(&96000u32.to_le_bytes());
        buf.extend_from_slice(&2u16.to_le_bytes());
        buf.extend_from_slice(&16u16.to_le_bytes());
        buf.extend_from_slice(b"iXML");
        buf.extend_from_slice(&ixml_size.to_le_bytes());
        buf.extend_from_slice(ixml);
        buf.extend_from_slice(b"data");
        buf.extend_from_slice(&data_size.to_le_bytes());
        buf.extend_from_slice(&data);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_wav(&mut fa));
        let g = |key: &str| fa.retrieve(StreamKind::General, 0, key).map(|z| z.as_str().to_owned());
        assert_eq!(g("Format_Commercial").as_deref(), Some("Broadcast Wave"));
    }

    #[test]
    fn large_bext_chunk_reads_only_metadata_prefix() {
        let mut bext = Vec::new();
        bext.extend_from_slice(b"Bounded BEXT\0");
        bext.resize(BEXT_PARSE_LIMIT + 4096, b'H');

        let bext_size = bext.len() as u32;
        let riff_size = 4 + 8 + 16 + 8 + bext_size + 8;
        let mut buf = Vec::new();
        buf.extend_from_slice(b"RIFF");
        buf.extend_from_slice(&riff_size.to_le_bytes());
        buf.extend_from_slice(b"WAVE");
        buf.extend_from_slice(b"fmt ");
        buf.extend_from_slice(&16u32.to_le_bytes());
        buf.extend_from_slice(&1u16.to_le_bytes());
        buf.extend_from_slice(&1u16.to_le_bytes());
        buf.extend_from_slice(&48000u32.to_le_bytes());
        buf.extend_from_slice(&96000u32.to_le_bytes());
        buf.extend_from_slice(&2u16.to_le_bytes());
        buf.extend_from_slice(&16u16.to_le_bytes());
        buf.extend_from_slice(b"bext");
        buf.extend_from_slice(&bext_size.to_le_bytes());
        buf.extend_from_slice(&bext);
        buf.extend_from_slice(b"data");
        buf.extend_from_slice(&0u32.to_le_bytes());

        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_wav(&mut fa));
        assert_eq!(fa.access_stats().max_request_len, BEXT_PARSE_LIMIT);
    }
}
