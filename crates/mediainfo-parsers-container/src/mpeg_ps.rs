//! MPEG-PS (Program Stream, ISO/IEC 13818-1) parser.
//!
//! Identified by the pack start code 0x00 0x00 0x01 0xBA at the file
//! start (or in MPEG-1 PS variants, also by leading PES packets with
//! start codes 0x000001E0/C0/BD/FA/FD/FE).
//!
//! Stream IDs (the 4th byte of an 0x000001XX start code):
//!   0xBA           = pack header
//!   0xBB           = system header
//!   0xBC           = program stream map
//!   0xBD           = private stream 1 (AC-3, DTS, LPCM in DVD)
//!   0xBE / 0xBF    = padding / private stream 2
//!   0xC0..=0xDF    = MPEG audio (32 streams)
//!   0xE0..=0xEF    = MPEG video (16 streams)
//!
//! The minimal parser:
//!   1. Confirm magic start code.
//!   2. Walk packets, collecting the set of unique stream IDs.
//!   3. Emit one Audio/Video stream per ID.

use mediainfo_core::{FileAnalyze, StreamKind};
use std::collections::BTreeSet;

const PACK_SC: [u8; 4] = [0x00, 0x00, 0x01, 0xBA];

pub fn parse_mpeg_ps(fa: &mut FileAnalyze) -> bool {
    let total = fa.Remain();
    let buf = match fa.peek_raw(total) {
        Some(b) => b,
        None => return false,
    };
    if buf.len() < 14 || buf[..4] != PACK_SC {
        // Allow MPEG-1 PS files starting with a PES start code directly.
        if !starts_with_pes(buf) {
            return false;
        }
    }

    let mut audio_ids: BTreeSet<u8> = BTreeSet::new();
    let mut video_ids: BTreeSet<u8> = BTreeSet::new();
    let mut private1_seen = false;

    // Walk start codes. Bound the scan to first 1 MB for speed.
    let max_scan = total.min(1_048_576);
    let mut i = 0usize;
    while i + 4 <= max_scan {
        if buf[i] == 0x00 && buf[i + 1] == 0x00 && buf[i + 2] == 0x01 {
            let sid = buf[i + 3];
            match sid {
                0xBA => {
                    // Pack header. MPEG-2 layout: 10 bytes after the SC,
                    // then stuffing[bytes 13 & 0x7]. MPEG-1: 8 bytes.
                    if i + 14 <= buf.len() {
                        let is_mpeg2 = (buf[i + 4] & 0xC0) == 0x40;
                        if is_mpeg2 {
                            let stuff = buf[i + 13] & 0x07;
                            i += 14 + stuff as usize;
                        } else {
                            i += 12; // MPEG-1 pack header
                        }
                        continue;
                    } else {
                        break;
                    }
                }
                0xB9 => {
                    // MPEG_program_end_code — done.
                    break;
                }
                0xC0..=0xDF => { audio_ids.insert(sid); }
                0xE0..=0xEF => { video_ids.insert(sid); }
                0xBD => { private1_seen = true; }
                _ => {}
            }
            // PES packet: skip past it using the declared length field.
            if i + 6 <= buf.len() && sid != 0xBA {
                let pes_len = ((buf[i + 4] as usize) << 8) | (buf[i + 5] as usize);
                if pes_len == 0 {
                    // Unbounded video PES (allowed) — search for next SC.
                    let mut j = i + 6;
                    while j + 4 <= max_scan {
                        if buf[j] == 0 && buf[j + 1] == 0 && buf[j + 2] == 1 {
                            break;
                        }
                        j += 1;
                    }
                    i = j;
                    continue;
                }
                i += 6 + pes_len;
            } else {
                i += 4;
            }
        } else {
            i += 1;
        }
    }

    if audio_ids.is_empty() && video_ids.is_empty() && !private1_seen && buf[..4] != PACK_SC {
        return false;
    }

    fa.Stream_Prepare(StreamKind::General);
    fa.Fill(StreamKind::General, 0, "Format", "MPEG-PS", false);
    if !video_ids.is_empty() {
        fa.Fill(
            StreamKind::General,
            0,
            "VideoCount",
            video_ids.len().to_string(),
            false,
        );
    }
    let audio_count = audio_ids.len() + (if private1_seen { 1 } else { 0 });
    if audio_count > 0 {
        fa.Fill(StreamKind::General, 0, "AudioCount", audio_count.to_string(), false);
    }

    for sid in &video_ids {
        let pos = fa.Stream_Prepare(StreamKind::Video);
        fa.Fill(StreamKind::Video, pos, "Format", "MPEG Video", false);
        fa.Fill(StreamKind::Video, pos, "ID", sid.to_string(), false);
    }
    for sid in &audio_ids {
        let pos = fa.Stream_Prepare(StreamKind::Audio);
        fa.Fill(StreamKind::Audio, pos, "Format", "MPEG Audio", false);
        fa.Fill(StreamKind::Audio, pos, "ID", sid.to_string(), false);
    }
    if private1_seen {
        // Private stream 1 carries AC-3/DTS/LPCM in DVD VOBs. Without
        // sub-stream sniffing we just label it "Private".
        let pos = fa.Stream_Prepare(StreamKind::Audio);
        fa.Fill(StreamKind::Audio, pos, "Format", "Private", false);
        fa.Fill(StreamKind::Audio, pos, "ID", "189", false); // 0xBD
    }

    true
}

fn starts_with_pes(buf: &[u8]) -> bool {
    if buf.len() < 4 || buf[0] != 0 || buf[1] != 0 || buf[2] != 1 {
        return false;
    }
    let sid = buf[3];
    // Allow MPEG-1 PS files starting with a PES start code per the C++
    // detection logic (File_MpegPs.cpp:929).
    matches!(sid, 0xE0..=0xEF | 0xC0..=0xDF | 0xBD | 0xFA | 0xFD | 0xFE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_ps() {
        let mut fa = FileAnalyze::new(b"NOT AN MPEG-PS FILE...........");
        assert!(!parse_mpeg_ps(&mut fa));
    }

    fn build_minimal_ps_with_video_audio() -> Vec<u8> {
        // Pack header (MPEG-2)
        let mut buf = vec![0x00, 0x00, 0x01, 0xBA];
        // 10 bytes MPEG-2 header: first byte must have bits 01 in top 2 → 0x44
        buf.extend_from_slice(&[0x44, 0x00, 0x04, 0x00, 0x04, 0x00, 0x01, 0x00, 0x00, 0x00]);
        // Video PES (stream id E0, length 4, payload 4 bytes)
        buf.extend_from_slice(&[0x00, 0x00, 0x01, 0xE0, 0x00, 0x04, 0xAA, 0xAA, 0xAA, 0xAA]);
        // Audio PES (stream id C0, length 4)
        buf.extend_from_slice(&[0x00, 0x00, 0x01, 0xC0, 0x00, 0x04, 0xBB, 0xBB, 0xBB, 0xBB]);
        // Program end code
        buf.extend_from_slice(&[0x00, 0x00, 0x01, 0xB9]);
        buf
    }

    #[test]
    fn parses_synthetic_program_stream() {
        let buf = build_minimal_ps_with_video_audio();
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_mpeg_ps(&mut fa));
        assert_eq!(
            fa.Retrieve(StreamKind::General, 0, "Format").map(|z| z.as_str().to_owned()),
            Some("MPEG-PS".into())
        );
        assert_eq!(fa.Count_Get(StreamKind::Video), 1);
        assert_eq!(fa.Count_Get(StreamKind::Audio), 1);
        assert_eq!(
            fa.Retrieve(StreamKind::Video, 0, "Format").map(|z| z.as_str().to_owned()),
            Some("MPEG Video".into())
        );
        assert_eq!(
            fa.Retrieve(StreamKind::Audio, 0, "Format").map(|z| z.as_str().to_owned()),
            Some("MPEG Audio".into())
        );
    }
}
