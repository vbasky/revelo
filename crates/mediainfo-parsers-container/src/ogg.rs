//! Ogg container parser. Mirrors the subset of MediaInfoLib's
//! `File_Ogg.cpp` for plain Ogg/Vorbis files. The container is a
//! sequence of pages, each starting with the magic "OggS"; each page
//! belongs to a logical stream identified by its 32-bit serial_number.
//!
//! Page layout:
//!   "OggS"               (4 bytes)
//!   version              (1 byte, currently 0)
//!   header_type          (1 byte; bit 0=continued, bit 1=BOS, bit 2=EOS)
//!   granule_position     (8 bytes LE — codec-defined; for audio it's
//!                         the sample count up to and including this page)
//!   serial_number        (4 bytes LE — identifies the logical stream)
//!   sequence_number      (4 bytes LE)
//!   checksum             (4 bytes LE)
//!   segments_count       (1 byte)
//!   segment_table        (segments_count bytes; each = segment length)
//!   segment_data         (sum of segment_table bytes)
//!
//! For BOS pages, the first packet identifies the codec:
//!   Vorbis: 0x01 "vorbis"
//!   Opus:   "OpusHead"
//!   FLAC:   0x7F "FLAC"
//!   Theora: 0x80 "theora"

use mediainfo_core::{FileAnalyze, StreamKind};

const OGG_MAGIC: &[u8; 4] = b"OggS";

pub fn parse_ogg(fa: &mut FileAnalyze) -> bool {
    let head = fa.peek_raw(4);
    let Some(h) = head else { return false };
    if h != OGG_MAGIC {
        return false;
    }

    let mut streams: Vec<OggStream> = Vec::new();

    while fa.Remain() >= 27 {
        let start = fa.Element_Offset();
        let h = match fa.peek_raw(27) {
            Some(b) => b.to_vec(),
            None => break,
        };
        if &h[0..4] != OGG_MAGIC {
            // Resync attempt or end of valid pages.
            break;
        }
        let header_type = h[5];
        let granule_position = u64::from_le_bytes([h[6], h[7], h[8], h[9], h[10], h[11], h[12], h[13]]);
        let serial = u32::from_le_bytes([h[14], h[15], h[16], h[17]]);
        let _seq = u32::from_le_bytes([h[18], h[19], h[20], h[21]]);
        let _crc = u32::from_le_bytes([h[22], h[23], h[24], h[25]]);
        let segments_count = h[26] as usize;

        let table_len = segments_count;
        if fa.Remain() < 27 + table_len {
            break;
        }
        // Read segment table to compute total payload size.
        fa.Skip_Hexa(27, "OggS_header");
        let table = fa.read_raw(table_len).to_vec();
        let payload_size: usize = table.iter().map(|&b| b as usize).sum();
        if fa.Remain() < payload_size {
            break;
        }

        let is_bos = (header_type & 0x02) != 0;
        let is_eos = (header_type & 0x04) != 0;

        let stream_idx = match streams.iter().position(|s| s.serial == serial) {
            Some(i) => i,
            None => {
                streams.push(OggStream::new(serial));
                streams.len() - 1
            }
        };

        if is_bos {
            // First packet of this stream identifies the codec.
            let packet_bytes = fa.read_raw(payload_size).to_vec();
            identify_codec_and_parse_header(&packet_bytes, &mut streams[stream_idx]);
        } else {
            fa.Skip_Hexa(payload_size, "page_payload");
        }

        if granule_position != u64::MAX {
            streams[stream_idx].last_granule = granule_position;
        }
        if is_eos {
            streams[stream_idx].eos_seen = true;
        }

        // Defensive: ensure forward progress.
        let consumed = fa.Element_Offset() - start;
        if consumed == 0 {
            break;
        }
    }

    fill_streams(fa, &streams);
    true
}

#[derive(Debug, Default)]
struct OggStream {
    serial: u32,
    codec: Option<OggCodec>,
    channels: u8,
    sample_rate: u32,
    bitrate_nominal: u32,
    last_granule: u64,
    eos_seen: bool,
}

impl OggStream {
    fn new(serial: u32) -> Self {
        OggStream {
            serial,
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum OggCodec {
    Vorbis,
    Opus,
    Flac,
    Theora,
}

fn identify_codec_and_parse_header(packet: &[u8], stream: &mut OggStream) {
    if packet.len() >= 7 && packet[0] == 0x01 && &packet[1..7] == b"vorbis" {
        stream.codec = Some(OggCodec::Vorbis);
        parse_vorbis_ident(packet, stream);
    } else if packet.len() >= 8 && &packet[0..8] == b"OpusHead" {
        stream.codec = Some(OggCodec::Opus);
        parse_opus_ident(packet, stream);
    } else if packet.len() >= 5 && packet[0] == 0x7F && &packet[1..5] == b"FLAC" {
        stream.codec = Some(OggCodec::Flac);
    } else if packet.len() >= 7 && packet[0] == 0x80 && &packet[1..7] == b"theora" {
        stream.codec = Some(OggCodec::Theora);
    }
}

/// Vorbis identification header layout (after 1 byte type + 6 bytes "vorbis"):
///   4 bytes LE: vorbis_version (0)
///   1 byte:     audio_channels
///   4 bytes LE: audio_sample_rate
///   4 bytes LE: bitrate_maximum (signed)
///   4 bytes LE: bitrate_nominal (signed)
///   4 bytes LE: bitrate_minimum (signed)
///   1 byte:     blocksize_0 / blocksize_1
///   1 byte:     framing_flag
fn parse_vorbis_ident(packet: &[u8], stream: &mut OggStream) {
    if packet.len() < 30 {
        return;
    }
    stream.channels = packet[11];
    stream.sample_rate = u32::from_le_bytes([packet[12], packet[13], packet[14], packet[15]]);
    stream.bitrate_nominal = u32::from_le_bytes([packet[20], packet[21], packet[22], packet[23]]);
}

/// Opus identification header: "OpusHead" + 1 byte version + 1 byte
/// channel_count + 2 bytes pre_skip + 4 bytes input_sample_rate +
/// 2 bytes output_gain + 1 byte channel_mapping_family ...
fn parse_opus_ident(packet: &[u8], stream: &mut OggStream) {
    if packet.len() < 19 {
        return;
    }
    stream.channels = packet[9];
    // input_sample_rate (the "original sample rate before encoding")
    stream.sample_rate = u32::from_le_bytes([packet[12], packet[13], packet[14], packet[15]]);
}

fn fill_streams(fa: &mut FileAnalyze, streams: &[OggStream]) {
    fa.Stream_Prepare(StreamKind::General);
    fa.Fill(StreamKind::General, 0, "Format", "Ogg", false);

    let mut audio_count: u32 = 0;
    let mut video_count: u32 = 0;
    for stream in streams {
        match stream.codec {
            Some(OggCodec::Vorbis) | Some(OggCodec::Opus) | Some(OggCodec::Flac) => {
                let pos = fa.Stream_Prepare(StreamKind::Audio);
                fa.Fill(StreamKind::Audio, pos, "ID", stream.serial.to_string(), false);
                let fmt = match stream.codec.unwrap() {
                    OggCodec::Vorbis => "Vorbis",
                    OggCodec::Opus => "Opus",
                    OggCodec::Flac => "FLAC",
                    _ => unreachable!(),
                };
                fa.Fill(StreamKind::Audio, pos, "Format", fmt, false);
                fa.Fill(StreamKind::Audio, pos, "BitRate_Mode", "VBR", false);
                if stream.bitrate_nominal > 0 {
                    fa.Fill(
                        StreamKind::Audio,
                        pos,
                        "BitRate",
                        stream.bitrate_nominal.to_string(),
                        false,
                    );
                }
                if stream.channels > 0 {
                    fa.Fill(StreamKind::Audio, pos, "Channels", stream.channels.to_string(), false);
                }
                if stream.sample_rate > 0 {
                    fa.Fill(
                        StreamKind::Audio,
                        pos,
                        "SamplingRate",
                        stream.sample_rate.to_string(),
                        false,
                    );
                }
                if stream.last_granule > 0 && stream.sample_rate > 0 {
                    // Oracle derives SamplingCount from the truncated
                    // Duration_ms, not from the raw granule (which
                    // includes encoder priming + trailing partial
                    // samples). So compute Duration first.
                    let duration_ms =
                        (stream.last_granule * 1000) / (stream.sample_rate as u64);
                    let sampling_count =
                        duration_ms * (stream.sample_rate as u64) / 1000;
                    fa.Fill(
                        StreamKind::Audio,
                        pos,
                        "SamplingCount",
                        sampling_count.to_string(),
                        false,
                    );
                    fa.Fill(
                        StreamKind::Audio,
                        pos,
                        "Duration",
                        duration_ms.to_string(),
                        false,
                    );
                    // For Vorbis-in-Ogg the oracle reports StreamSize
                    // = bitrate_nominal/8 * Duration_seconds, not the
                    // actual byte total (which would include Ogg page
                    // overhead). This is the "encoded payload size at
                    // the nominal rate" convention.
                    if stream.bitrate_nominal > 0 {
                        let stream_size =
                            (stream.bitrate_nominal as u64) * duration_ms / 8000;
                        fa.Fill(
                            StreamKind::Audio,
                            pos,
                            "StreamSize",
                            stream_size.to_string(),
                            false,
                        );
                    }
                }
                fa.Fill(StreamKind::Audio, pos, "Compression_Mode", "Lossy", false);
                audio_count += 1;
            }
            Some(OggCodec::Theora) => video_count += 1,
            None => {}
        }
    }

    if audio_count > 0 {
        fa.Fill(StreamKind::General, 0, "AudioCount", audio_count.to_string(), false);
        // Ogg-wrapped lossy audio (Vorbis/Opus) is VBR; the oracle
        // emits OverallBitRate_Mode for the General track.
        fa.Fill(StreamKind::General, 0, "OverallBitRate_Mode", "VBR", false);
    }
    if video_count > 0 {
        fa.Fill(StreamKind::General, 0, "VideoCount", video_count.to_string(), false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_ogg_buffer() {
        let mut fa = FileAnalyze::new(b"NOT an Ogg file at all");
        assert!(!parse_ogg(&mut fa));
    }
}
