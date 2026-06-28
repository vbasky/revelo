//! AIFF (Audio Interchange File Format) parser.
//!
//! Same `FORM`-style chunked container as WAV but big-endian throughout
//! and with a different chunk vocabulary (`COMM` instead of `fmt `,
//! `SSND` instead of `data`). Sample rate is encoded as an 80-bit IEEE
//! 754 extended-precision float.
//!
//! Layout:
//!   "FORM" <u32 BE total-size-minus-8> ("AIFF" | "AIFC")
//!     chunks:
//!       "COMM" <u32 BE size> numChannels<2> numSampleFrames<4> sampleSize<2> sampleRate<10 80-bit BE>
//!                            [AIFC only:] compressionType<4 FourCC> compressionName<Pascal string>
//!       "SSND" <u32 BE size> offset<4> blockSize<4> samples<size-8>
//!       (other chunks ignored)
//!
//! BitRate formula matches the C++ side (see File_Riff_Elements.cpp's
//! WAVE_data_Continue equivalent): the stored Audio_Duration is the
//! integer-millisecond truncation of `numSampleFrames/sampleRate*1000`,
//! then BitRate = StreamSize * 8 * 1000 / Duration_ms, formatted with
//! 10 decimal digits.

use revelo_core::{FileAnalyze, Reader, StreamKind};

const FOURCC_FORM: u32 = u32::from_be_bytes(*b"FORM");
const FOURCC_AIFF: u32 = u32::from_be_bytes(*b"AIFF");
const FOURCC_AIFC: u32 = u32::from_be_bytes(*b"AIFC");
const FOURCC_COMM: u32 = u32::from_be_bytes(*b"COMM");
const FOURCC_SSND: u32 = u32::from_be_bytes(*b"SSND");

#[derive(Debug, Default)]
struct CommChunk {
    num_channels: u16,
    num_sample_frames: u32,
    sample_size: u16,
    sample_rate: f64,
    // AIFC-only: present when the FORM type is "AIFC".
    compression_type: Option<u32>,
}

/// Decoded codec mapping for an AIFC compressionType FourCC. `format`
/// is empty when the compressionType is unknown (caller should skip
/// filling Format in that case).
#[derive(Debug, Default)]
struct AifcCodec {
    format: &'static str,
    endianness: Option<&'static str>,
    sign: Option<&'static str>,
    is_float: bool,
}

fn map_aifc_compression(fourcc: u32) -> AifcCodec {
    // FourCCs matched as ASCII bytes; AIFC compressionType is case-sensitive
    // per the spec but real files use both cases for the same codec, so we
    // accept both. Endianness/Sign assignments follow File_Pcm.cpp's table.
    match &fourcc.to_be_bytes() {
        b"NONE" | b"twos" => AifcCodec {
            format: "PCM",
            endianness: Some("Big"),
            sign: Some("Signed"),
            ..Default::default()
        },
        // "sowt" = "twos" reversed; QuickTime's marker for little-endian PCM.
        b"sowt" => AifcCodec {
            format: "PCM",
            endianness: Some("Little"),
            sign: Some("Signed"),
            ..Default::default()
        },
        b"raw " => AifcCodec {
            format: "PCM",
            endianness: Some("Little"),
            sign: Some("Unsigned"),
            ..Default::default()
        },
        b"fl32" | b"FL32" | b"fl64" | b"FL64" => {
            AifcCodec { format: "PCM", is_float: true, ..Default::default() }
        }
        b"alaw" | b"ALAW" => AifcCodec { format: "A-law", ..Default::default() },
        b"ulaw" | b"ULAW" => AifcCodec { format: "\u{00B5}-law", ..Default::default() },
        b"ima4" => AifcCodec { format: "ADPCM", ..Default::default() },
        _ => AifcCodec::default(),
    }
}

/// Parse AIFF/AIFC audio container.
///
/// Detection: `FORM` + `AIFF`/`AIFC` form type.
/// Fills: COMM chunk (channels, sample rate, bit depth), SSND data size.
pub fn parse_aiff(fa: &mut FileAnalyze) -> bool {
    parse(fa).is_some()
}

fn parse(fa: &mut FileAnalyze) -> Option<()> {
    let (comm, audio_stream_size) = {
        let r = &mut Reader::wrap(fa);
        if r.peek_be_u32()? != FOURCC_FORM {
            return None;
        }

        r.element_begin("FORM");
        r.fourcc("ID")?;
        r.be_u32("Size")?;
        let form_type = r.fourcc("Type")?;

        if form_type != FOURCC_AIFF && form_type != FOURCC_AIFC {
            r.element_end();
            return None;
        }
        let is_aifc = form_type == FOURCC_AIFC;

        let mut comm: Option<CommChunk> = None;
        let mut audio_stream_size: u64 = 0;

        while r.remain() >= 8 {
            let chunk_id = r.fourcc("ChunkID")?;
            let chunk_size = r.be_u32("ChunkSize")?;

            let chunk_size_usize = chunk_size as usize;
            if r.remain() < chunk_size_usize {
                break;
            }

            match chunk_id {
                FOURCC_COMM => {
                    r.element_begin("Common");
                    let num_channels = r.be_u16("numChannels")?;
                    let num_sample_frames = r.be_u32("numSampleFrames")?;
                    let sample_size = r.be_u16("sampleSize")?;
                    let sample_rate = r.be_f80("sampleRate")?;

                    let mut consumed: usize = 18;
                    let mut compression_type: Option<u32> = None;
                    if is_aifc && chunk_size_usize >= consumed + 4 {
                        compression_type = Some(r.fourcc("compressionType")?);
                        consumed += 4;
                        // Pascal string: 1-byte length + payload, then padded to
                        // even total length within the chunk body.
                        if chunk_size_usize > consumed {
                            let pa_len = r.be_u8("compressionName_length")?;
                            consumed += 1;
                            let pa_total = pa_len as usize;
                            let pa_take = pa_total.min(chunk_size_usize - consumed);
                            if pa_take > 0 {
                                r.skip(pa_take)?; // compressionName
                                consumed += pa_take;
                            }
                            // Pascal string occupies (1 + len) bytes, padded so
                            // the whole pair is even.
                            if (1 + pa_total) % 2 == 1 && chunk_size_usize > consumed {
                                r.skip(1)?; // compressionName_pad
                                consumed += 1;
                            }
                        }
                    }
                    if chunk_size_usize > consumed {
                        r.skip(chunk_size_usize - consumed)?; // Extension
                    }
                    if chunk_size_usize % 2 == 1 {
                        let _ = r.be_u8("Padding");
                    }
                    r.element_end();

                    comm = Some(CommChunk {
                        num_channels,
                        num_sample_frames,
                        sample_size,
                        sample_rate,
                        compression_type,
                    });
                }
                FOURCC_SSND => {
                    r.element_begin("SoundData");
                    r.be_u32("offset")?;
                    r.be_u32("blockSize")?;
                    // Actual audio data is the chunk body minus the 8-byte
                    // offset+blockSize prefix.
                    let samples_size = chunk_size_usize.saturating_sub(8);
                    audio_stream_size = samples_size as u64;
                    r.skip(samples_size)?; // Samples
                    if chunk_size_usize % 2 == 1 {
                        let _ = r.be_u8("Padding");
                    }
                    r.element_end();
                }
                _ => {
                    r.skip(chunk_size_usize)?; // Unknown
                    if chunk_size_usize % 2 == 1 {
                        let _ = r.be_u8("Padding");
                    }
                }
            }
        }

        r.element_end();
        (comm, audio_stream_size)
    };

    let comm = comm?;
    fill_streams(fa, &comm, audio_stream_size);
    Some(())
}

fn fill_streams(fa: &mut FileAnalyze, comm: &CommChunk, audio_stream_size: u64) {
    fa.stream_prepare(StreamKind::General);
    fa.set_field(StreamKind::General, 0, "Format", "AIFF");

    fa.stream_prepare(StreamKind::Audio);
    let codec = match comm.compression_type {
        Some(ct) => map_aifc_compression(ct),
        None => AifcCodec {
            format: "PCM",
            endianness: Some("Big"),
            sign: Some("Signed"),
            ..Default::default()
        },
    };
    if !codec.format.is_empty() {
        fa.set_field(StreamKind::Audio, 0, "Format", codec.format);
    }
    if let Some(end) = codec.endianness {
        fa.set_field(StreamKind::Audio, 0, "Format_Settings_Endianness", end);
    }
    if let Some(sign) = codec.sign {
        fa.set_field(StreamKind::Audio, 0, "Format_Settings_Sign", sign);
    }
    if codec.is_float {
        fa.set_field(StreamKind::Audio, 0, "Format_Settings_Floating", "Yes");
    }

    // Duration as integer milliseconds, matching the C++ AfterComma=0 fill
    // of `numSampleFrames/sampleRate*1000`.
    let duration_ms_int: i64 = if comm.sample_rate > 0.0 {
        ((comm.num_sample_frames as f64) / comm.sample_rate * 1000.0).round() as i64
    } else {
        0
    };

    fa.set_field(StreamKind::Audio, 0, "Duration", duration_ms_int.to_string());
    fa.set_field(StreamKind::Audio, 0, "BitRate_Mode", "CBR");

    // BitRate = StreamSize * 8 * 1000 / Duration_ms_int, with the integer
    // millisecond truncation that the C++ retrieves via To_float64() of
    // the stored Audio_Duration. 10-decimal display precision to match
    // oracle's %.10f formatting.
    if duration_ms_int > 0 {
        let bitrate_f64 = (audio_stream_size as f64) * 8.0 * 1000.0 / (duration_ms_int as f64);
        fa.set_field(StreamKind::Audio, 0, "BitRate", format!("{:.10}", bitrate_f64));
    }

    fa.set_field(StreamKind::Audio, 0, "Channels", comm.num_channels.to_string());
    // Sample rate stored as integer if it's whole; matches oracle's "48000"
    // not "48000.000" for typical AIFF.
    let sr_int: i64 = comm.sample_rate.round() as i64;
    fa.set_field(StreamKind::Audio, 0, "SamplingRate", sr_int.to_string());
    fa.set_field(StreamKind::Audio, 0, "SamplingCount", comm.num_sample_frames.to_string());
    fa.set_field(StreamKind::Audio, 0, "BitDepth", comm.sample_size.to_string());
    fa.set_field(StreamKind::Audio, 0, "StreamSize", audio_stream_size.to_string());

    fa.set_field(StreamKind::General, 0, "AudioCount", "1");
}

#[cfg(test)]
mod tests {
    use super::*;

    const AIFF_METADATA_ONLY_BUDGET: u64 = 8 * 1024 * 1024;
    const TEST_LARGE_CHUNK_SIZE: usize = 9 * 1024 * 1024;

    /// Build a minimal valid AIFF: stereo 24-bit, with the given sample
    /// rate and frame count. SSND prefix offset/block_size are zero.
    fn make_aiff(channels: u16, sample_rate_hz: u32, bits: u16, frame_count: u32) -> Vec<u8> {
        let block_align = channels * (bits / 8);
        let data_size = frame_count * block_align as u32;
        let ssnd_chunk_size = 8 + data_size;
        let comm_chunk_size = 18u32;

        let mut buf =
            Vec::with_capacity(12 + 8 + comm_chunk_size as usize + 8 + ssnd_chunk_size as usize);
        buf.extend_from_slice(b"FORM");
        let form_size = 4 + (8 + comm_chunk_size) + (8 + ssnd_chunk_size);
        buf.extend_from_slice(&form_size.to_be_bytes());
        buf.extend_from_slice(b"AIFF");

        buf.extend_from_slice(b"COMM");
        buf.extend_from_slice(&comm_chunk_size.to_be_bytes());
        buf.extend_from_slice(&channels.to_be_bytes());
        buf.extend_from_slice(&frame_count.to_be_bytes());
        buf.extend_from_slice(&bits.to_be_bytes());
        buf.extend_from_slice(&encode_f80_be(sample_rate_hz as f64));

        buf.extend_from_slice(b"SSND");
        buf.extend_from_slice(&ssnd_chunk_size.to_be_bytes());
        buf.extend_from_slice(&0u32.to_be_bytes()); // offset
        buf.extend_from_slice(&0u32.to_be_bytes()); // blockSize
        buf.resize(buf.len() + data_size as usize, 0);
        buf
    }

    /// Encode an integer sample rate as a 10-byte 80-bit big-endian
    /// extended precision float. Only valid for positive whole numbers
    /// (the only case the AIFF spec uses in practice).
    fn encode_f80_be(value: f64) -> [u8; 10] {
        debug_assert!(value > 0.0 && value < 2f64.powi(63));
        // value = mantissa_as_u64 / 2^63 * 2^(exp - 16383)
        // Equivalently, find E such that 2^E <= value < 2^(E+1).
        let int_part = value.trunc() as u64;
        let e = 63 - int_part.leading_zeros() as i32; // floor(log2)
        // mantissa with explicit integer bit (= 1) and 63 fraction bits
        let scaled = value * 2f64.powi(63 - e);
        let mantissa = scaled.round() as u64;
        let biased_exp = (16383 + e) as u16;
        let mut out = [0u8; 10];
        out[0] = ((biased_exp >> 8) & 0x7F) as u8; // sign=0
        out[1] = (biased_exp & 0xFF) as u8;
        out[2..10].copy_from_slice(&mantissa.to_be_bytes());
        out
    }

    #[test]
    fn parse_minimal_aiff_24bit_stereo() {
        let buf = make_aiff(2, 48000, 24, 71638);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_aiff(&mut fa));

        let g = |key: &str| fa.retrieve(StreamKind::General, 0, key).map(|z| z.as_str().to_owned());
        let a = |key: &str| fa.retrieve(StreamKind::Audio, 0, key).map(|z| z.as_str().to_owned());

        assert_eq!(g("Format").as_deref(), Some("AIFF"));
        assert_eq!(g("AudioCount").as_deref(), Some("1"));
        assert_eq!(a("Format").as_deref(), Some("PCM"));
        assert_eq!(a("Format_Settings_Endianness").as_deref(), Some("Big"));
        assert_eq!(a("BitRate_Mode").as_deref(), Some("CBR"));
        assert_eq!(a("Channels").as_deref(), Some("2"));
        assert_eq!(a("SamplingRate").as_deref(), Some("48000"));
        assert_eq!(a("SamplingCount").as_deref(), Some("71638"));
        assert_eq!(a("BitDepth").as_deref(), Some("24"));
        assert_eq!(a("StreamSize").as_deref(), Some("429828"));
        assert_eq!(a("Duration").as_deref(), Some("1492"));
        // BitRate = 429828 * 8 * 1000 / 1492 = 2304707.7747989...
        // Formatted with %.10f
        let br = a("BitRate").unwrap();
        assert!(br.starts_with("2304707.77479"), "got {br}");
    }

    #[test]
    fn rejects_non_form_buffer() {
        let mut fa = FileAnalyze::new(b"NOTAFORMfile");
        assert!(!parse_aiff(&mut fa));
    }

    #[test]
    fn rejects_form_with_non_aiff_type() {
        let mut buf = Vec::new();
        buf.extend_from_slice(b"FORM");
        buf.extend_from_slice(&8u32.to_be_bytes());
        buf.extend_from_slice(b"WAVE");
        buf.extend_from_slice(&[0; 4]);
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_aiff(&mut fa));
    }

    /// Build a minimal AIFC with the given compressionType FourCC and an
    /// empty Pascal compressionName (length=0, then 1 pad byte → 2 bytes
    /// total). COMM body is 18 (base) + 4 (compressionType) + 2 (Pascal) = 24.
    fn make_aifc(
        channels: u16,
        sample_rate_hz: u32,
        bits: u16,
        frame_count: u32,
        compression_type: &[u8; 4],
    ) -> Vec<u8> {
        let block_align = channels * (bits / 8);
        let data_size = frame_count * block_align as u32;
        let ssnd_chunk_size = 8 + data_size;
        let comm_chunk_size: u32 = 18 + 4 + 2;

        let mut buf = Vec::new();
        buf.extend_from_slice(b"FORM");
        let form_size = 4 + (8 + comm_chunk_size) + (8 + ssnd_chunk_size);
        buf.extend_from_slice(&form_size.to_be_bytes());
        buf.extend_from_slice(b"AIFC");

        buf.extend_from_slice(b"COMM");
        buf.extend_from_slice(&comm_chunk_size.to_be_bytes());
        buf.extend_from_slice(&channels.to_be_bytes());
        buf.extend_from_slice(&frame_count.to_be_bytes());
        buf.extend_from_slice(&bits.to_be_bytes());
        buf.extend_from_slice(&encode_f80_be(sample_rate_hz as f64));
        buf.extend_from_slice(compression_type);
        // Pascal string with zero-length name: 0x00 + 1 byte pad to even.
        buf.push(0);
        buf.push(0);

        buf.extend_from_slice(b"SSND");
        buf.extend_from_slice(&ssnd_chunk_size.to_be_bytes());
        buf.extend_from_slice(&0u32.to_be_bytes());
        buf.extend_from_slice(&0u32.to_be_bytes());
        buf.resize(buf.len() + data_size as usize, 0);
        buf
    }

    #[test]
    fn parse_aifc_sowt_is_little_endian_pcm() {
        let buf = make_aifc(2, 48000, 16, 48000, b"sowt");
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_aiff(&mut fa));
        let a = |key: &str| fa.retrieve(StreamKind::Audio, 0, key).map(|z| z.as_str().to_owned());
        let g = |key: &str| fa.retrieve(StreamKind::General, 0, key).map(|z| z.as_str().to_owned());
        assert_eq!(g("Format").as_deref(), Some("AIFF"));
        assert_eq!(a("Format").as_deref(), Some("PCM"));
        assert_eq!(a("Format_Settings_Endianness").as_deref(), Some("Little"));
        assert_eq!(a("Format_Settings_Sign").as_deref(), Some("Signed"));
        assert_eq!(a("Channels").as_deref(), Some("2"));
        assert_eq!(a("SamplingRate").as_deref(), Some("48000"));
        assert_eq!(a("BitDepth").as_deref(), Some("16"));
    }

    #[test]
    fn parse_aifc_fl32_is_float_pcm() {
        let buf = make_aifc(1, 44100, 32, 44100, b"fl32");
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_aiff(&mut fa));
        let a = |key: &str| fa.retrieve(StreamKind::Audio, 0, key).map(|z| z.as_str().to_owned());
        assert_eq!(a("Format").as_deref(), Some("PCM"));
        assert_eq!(a("Format_Settings_Floating").as_deref(), Some("Yes"));
        // No Endianness/Sign fills for float per task spec.
        assert!(a("Format_Settings_Endianness").is_none());
        assert!(a("Format_Settings_Sign").is_none());
    }

    #[test]
    fn large_aiff_metadata_and_sound_data_access_stays_bounded() {
        let channels = 1u16;
        let bits = 16u16;
        let frame_count = (TEST_LARGE_CHUNK_SIZE / 2) as u32;
        let comm_chunk_size = 18u32;
        let ssnd_chunk_size = 8 + TEST_LARGE_CHUNK_SIZE as u32;
        let id3_chunk_size = TEST_LARGE_CHUNK_SIZE as u32;

        let mut buf = Vec::new();
        buf.extend_from_slice(b"FORM");
        let form_size = 4 + (8 + comm_chunk_size) + (8 + id3_chunk_size) + (8 + ssnd_chunk_size);
        buf.extend_from_slice(&form_size.to_be_bytes());
        buf.extend_from_slice(b"AIFF");

        buf.extend_from_slice(b"COMM");
        buf.extend_from_slice(&comm_chunk_size.to_be_bytes());
        buf.extend_from_slice(&channels.to_be_bytes());
        buf.extend_from_slice(&frame_count.to_be_bytes());
        buf.extend_from_slice(&bits.to_be_bytes());
        buf.extend_from_slice(&encode_f80_be(48000.0));

        buf.extend_from_slice(b"ID3 ");
        buf.extend_from_slice(&id3_chunk_size.to_be_bytes());
        buf.resize(buf.len() + TEST_LARGE_CHUNK_SIZE, 0);

        buf.extend_from_slice(b"SSND");
        buf.extend_from_slice(&ssnd_chunk_size.to_be_bytes());
        buf.extend_from_slice(&0u32.to_be_bytes());
        buf.extend_from_slice(&0u32.to_be_bytes());
        buf.resize(buf.len() + TEST_LARGE_CHUNK_SIZE, 0);

        assert!(buf.len() as u64 > AIFF_METADATA_ONLY_BUDGET);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_aiff(&mut fa));
        assert_eq!(
            fa.retrieve(StreamKind::Audio, 0, "StreamSize")
                .map(|z| z.as_str().to_owned())
                .as_deref(),
            Some("9437184")
        );

        let stats = fa.access_stats();
        assert!(stats.bytes_requested < AIFF_METADATA_ONLY_BUDGET, "{stats:?}");
        assert!(stats.bytes_returned < AIFF_METADATA_ONLY_BUDGET, "{stats:?}");
        assert!(stats.max_request_len <= 10, "{stats:?}");
    }

    #[test]
    fn parse_aifc_none_matches_aiff_defaults() {
        // AIFC with compressionType "NONE" should produce the same Format
        // metadata as a plain AIFF file.
        let buf = make_aifc(2, 48000, 24, 48000, b"NONE");
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_aiff(&mut fa));
        let a = |key: &str| fa.retrieve(StreamKind::Audio, 0, key).map(|z| z.as_str().to_owned());
        assert_eq!(a("Format").as_deref(), Some("PCM"));
        assert_eq!(a("Format_Settings_Endianness").as_deref(), Some("Big"));
        assert_eq!(a("Format_Settings_Sign").as_deref(), Some("Signed"));
    }

    #[test]
    fn encodes_44100_sample_rate_round_trip() {
        // Verifies our test encoder works against the canonical AIFF
        // 44100 Hz encoding (0x400E AC44 0000 0000 0000).
        let bytes = encode_f80_be(44100.0);
        assert_eq!(bytes, [0x40, 0x0E, 0xAC, 0x44, 0, 0, 0, 0, 0, 0]);
    }
}
