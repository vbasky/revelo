//! DSDIFF (Direct Stream Digital Interchange File Format) — Philips .dff
//! container for 1-bit DSD audio streams used by SACDs.
//!
//! Same FORM-style container as AIFF, but with the magic "FRM8" instead of
//! "FORM" and 64-bit chunk sizes (vs AIFF's 32-bit). Mirrors the subset of
//! MediaInfoLib's `File_Dsdiff.cpp` needed for SamplingRate / Channels /
//! Format. ID3/COMT/DIIN tags are deliberately skipped at this commit —
//! the WHY: the harness ticket asks for header-walk only.
//!
//! Layout:
//!   "FRM8" <u64 BE form-size> "DSD "                                    // magic + form type
//!     chunks:
//!       "FVER" <u64 BE size> version<4>                                 // format version
//!       "PROP" <u64 BE size> "SND " then sub-chunks:                    // property container
//!         "FS  " <u64 BE size> sampleRate<u32 BE>
//!         "CHNL" <u64 BE size> numChannels<u16 BE> chID<4>*             // channel layout
//!         "CMPR" <u64 BE size> compressionType<4> count<1> name<count>  // codec id
//!         (ABSS / LSCO ignored at this commit)
//!       "DSD " <u64 BE size> samples                                    // 1-bit audio data
//!       "DST " <u64 BE size> ...                                        // compressed variant
//!   Chunk bodies are padded to even byte boundary (1-byte pad if size is odd).

use revelio_core::{FileAnalyze, StreamKind};
use zenlib::{int16u, int32u, int64u, int8u};

const FOURCC_FRM8: int32u = u32::from_be_bytes(*b"FRM8");
const FOURCC_DSD_FORMTYPE: int32u = u32::from_be_bytes(*b"DSD ");
const FOURCC_FVER: int32u = u32::from_be_bytes(*b"FVER");
const FOURCC_PROP: int32u = u32::from_be_bytes(*b"PROP");
const FOURCC_SND: int32u = u32::from_be_bytes(*b"SND ");
const FOURCC_FS: int32u = u32::from_be_bytes(*b"FS  ");
const FOURCC_CHNL: int32u = u32::from_be_bytes(*b"CHNL");
const FOURCC_CMPR: int32u = u32::from_be_bytes(*b"CMPR");
const FOURCC_DSD_DATA: int32u = u32::from_be_bytes(*b"DSD ");
const FOURCC_DST_DATA: int32u = u32::from_be_bytes(*b"DST ");

const CMPR_DSD: int32u = u32::from_be_bytes(*b"DSD ");
const CMPR_DST: int32u = u32::from_be_bytes(*b"DST ");

#[derive(Debug, Default)]
struct DsdiffInfo {
    sample_rate: int32u,
    num_channels: int16u,
    format: Option<&'static str>,
    audio_stream_size: int64u,
}

pub fn parse_dsdiff(fa: &mut FileAnalyze) -> bool {
    if fa.remain() < 16 {
        return false;
    }
    let mut magic: int32u = 0;
    fa.peek_b4(&mut magic);
    if magic != FOURCC_FRM8 {
        return false;
    }

    fa.element_begin("FRM8");
    let mut frm8_id: int32u = 0;
    fa.get_c4(&mut frm8_id, "ID");
    let mut form_size: int64u = 0;
    fa.get_b8(&mut form_size, "Size");
    let mut form_type: int32u = 0;
    fa.get_c4(&mut form_type, "FormType");

    if form_type != FOURCC_DSD_FORMTYPE {
        fa.element_end();
        return false;
    }

    let mut info = DsdiffInfo::default();

    // Top-level chunks under the DSD form.
    walk_chunks(fa, &mut info, /*inside_prop=*/ false);

    fa.element_end();

    // If CMPR was absent, MediaInfoLib leaves Audio.Format empty; we
    // default to "DSD" because the form type IS "DSD " — that's the
    // overwhelming common case (DST is rare) and matches the file's
    // declared identity.
    if info.format.is_none() {
        info.format = Some("DSD");
    }

    fill_streams(fa, &info);
    true
}

/// Walk a sequence of chunks until the buffer is exhausted. When
/// `inside_prop` is true, we're scanning the PROP/SND payload's
/// sub-chunks rather than top-level FRM8 children — same on-disk
/// shape, different semantics on a few IDs.
fn walk_chunks(fa: &mut FileAnalyze, info: &mut DsdiffInfo, inside_prop: bool) {
    while fa.remain() >= 12 {
        let mut chunk_id: int32u = 0;
        fa.get_c4(&mut chunk_id, "ChunkID");
        let mut chunk_size: int64u = 0;
        fa.get_b8(&mut chunk_size, "ChunkSize");

        // Guard against malformed sizes that exceed the buffer.
        let body_len = if (chunk_size as usize) > fa.remain() {
            fa.remain()
        } else {
            chunk_size as usize
        };
        let body_start = fa.element_offset();
        let body_end = body_start + body_len;

        if !inside_prop {
            match chunk_id {
                FOURCC_PROP => {
                    fa.element_begin("PROP");
                    // PROP starts with the 4-byte propType ("SND ").
                    if fa.remain() >= 4 {
                        let mut prop_type: int32u = 0;
                        fa.get_c4(&mut prop_type, "propType");
                        if prop_type == FOURCC_SND {
                            // Recurse into sub-chunks until the PROP body ends.
                            let sub_end = body_end;
                            while fa.element_offset() + 12 <= sub_end {
                                parse_prop_subchunk(fa, info, sub_end);
                            }
                        }
                    }
                    // Skip any trailing bytes inside PROP we didn't consume.
                    if fa.element_offset() < body_end {
                        fa.skip_hexa(body_end - fa.element_offset(), "Unknown");
                    }
                    fa.element_end();
                }
                FOURCC_DSD_DATA => {
                    fa.element_begin("DSD");
                    info.audio_stream_size = body_len as int64u;
                    fa.skip_hexa(body_len, "DSDsoundData");
                    fa.element_end();
                }
                FOURCC_DST_DATA => {
                    fa.element_begin("DST");
                    info.audio_stream_size = body_len as int64u;
                    fa.skip_hexa(body_len, "DSTsoundData");
                    fa.element_end();
                }
                FOURCC_FVER => {
                    fa.skip_hexa(body_len, "FVER");
                }
                _ => {
                    fa.skip_hexa(body_len, "Unknown");
                }
            }
        }

        // Realign past any malformed gap (defensive — body parsers above
        // should always advance to body_end exactly).
        if fa.element_offset() < body_end {
            fa.skip_hexa(body_end - fa.element_offset(), "Trailer");
        }

        // Per-spec 1-byte pad when chunk size is odd.
        if chunk_size % 2 == 1 && fa.remain() >= 1 {
            let mut _pad: int8u = 0;
            fa.get_b1(&mut _pad, "pad");
        }
    }
}

fn parse_prop_subchunk(fa: &mut FileAnalyze, info: &mut DsdiffInfo, sub_end: usize) {
    let mut sub_id: int32u = 0;
    fa.get_c4(&mut sub_id, "ChunkID");
    let mut sub_size: int64u = 0;
    fa.get_b8(&mut sub_size, "ChunkSize");

    let max_body = sub_end.saturating_sub(fa.element_offset());
    let body_len = (sub_size as usize).min(max_body);
    let body_start = fa.element_offset();
    let body_end = body_start + body_len;

    match sub_id {
        FOURCC_FS => {
            if body_len >= 4 {
                let mut sr: int32u = 0;
                fa.get_b4(&mut sr, "sampleRate");
                info.sample_rate = sr;
                if body_len > 4 {
                    fa.skip_hexa(body_len - 4, "Extra");
                }
            } else {
                fa.skip_hexa(body_len, "FS_truncated");
            }
        }
        FOURCC_CHNL => {
            if body_len >= 2 {
                let mut num_channels: int16u = 0;
                fa.get_b2(&mut num_channels, "numChannels");
                info.num_channels = num_channels;
                // chID list follows (4 bytes each). We don't need them
                // for the minimum-viable fields — skip to chunk end.
                let consumed = 2usize;
                if body_len > consumed {
                    fa.skip_hexa(body_len - consumed, "chIDs");
                }
            } else {
                fa.skip_hexa(body_len, "CHNL_truncated");
            }
        }
        FOURCC_CMPR => {
            if body_len >= 5 {
                let mut compression_type: int32u = 0;
                fa.get_b4(&mut compression_type, "compressionType");
                let mut name_count: int8u = 0;
                fa.get_b1(&mut name_count, "Count");
                let name_take = (name_count as usize).min(body_len.saturating_sub(5));
                if name_take > 0 {
                    fa.skip_hexa(name_take, "compressionName");
                }
                let consumed = 5 + name_take;
                if body_len > consumed {
                    fa.skip_hexa(body_len - consumed, "Extra");
                }
                info.format = Some(match compression_type {
                    CMPR_DSD => "DSD",
                    CMPR_DST => "DST",
                    _ => "DSD",
                });
            } else {
                fa.skip_hexa(body_len, "CMPR_truncated");
            }
        }
        _ => {
            fa.skip_hexa(body_len, "Unknown");
        }
    }

    // Defensive: ensure we land on body_end.
    if fa.element_offset() < body_end {
        fa.skip_hexa(body_end - fa.element_offset(), "Trailer");
    }

    // Pad if odd-sized sub-chunk.
    if sub_size % 2 == 1 && fa.element_offset() < sub_end {
        let mut _pad: int8u = 0;
        fa.get_b1(&mut _pad, "pad");
    }
}

fn fill_streams(fa: &mut FileAnalyze, info: &DsdiffInfo) {
    fa.stream_prepare(StreamKind::General);
    fa.fill(StreamKind::General, 0, "Format", "DSDIFF", false);

    fa.stream_prepare(StreamKind::Audio);
    if let Some(fmt) = info.format {
        fa.fill(StreamKind::Audio, 0, "Format", fmt, false);
    }
    if info.sample_rate > 0 {
        fa.fill(
            StreamKind::Audio,
            0,
            "SamplingRate",
            info.sample_rate.to_string(),
            false,
        );
    }
    if info.num_channels > 0 {
        fa.fill(
            StreamKind::Audio,
            0,
            "Channels",
            info.num_channels.to_string(),
            false,
        );
    }
    // DSD is by construction 1-bit-per-sample-per-channel; this is the
    // defining property of Direct Stream Digital and the reason MediaInfo
    // doesn't carry a BitDepth field in CMPR.
    fa.fill(StreamKind::Audio, 0, "BitDepth", "1", false);
    fa.fill(StreamKind::Audio, 0, "Compression_Mode", "Lossless", false);
    fa.fill(StreamKind::Audio, 0, "BitRate_Mode", "CBR", false);

    if info.audio_stream_size > 0 {
        fa.fill(
            StreamKind::Audio,
            0,
            "StreamSize",
            info.audio_stream_size.to_string(),
            false,
        );
    }

    fa.fill(StreamKind::General, 0, "AudioCount", "1", false);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal DSDIFF: FRM8 + DSD form, PROP/SND with FS/CHNL/CMPR,
    /// then a DSD sound-data chunk of `audio_size` bytes. Odd-sized chunks
    /// get a 1-byte pad per the DSDIFF spec (matches MediaInfoLib's
    /// `Size%2 ? Size++` behavior in File_Dsdiff.cpp's Header_Parse).
    fn push_chunk(buf: &mut Vec<u8>, id: &[u8; 4], body: &[u8]) {
        buf.extend_from_slice(id);
        buf.extend_from_slice(&(body.len() as u64).to_be_bytes());
        buf.extend_from_slice(body);
        if body.len() % 2 == 1 {
            buf.push(0);
        }
    }

    fn make_dsdiff(
        sample_rate: u32,
        num_channels: u16,
        ch_ids: &[&[u8; 4]],
        cmpr: &[u8; 4],
        audio_size: usize,
    ) -> Vec<u8> {
        // PROP body = "SND " + FS-chunk + CHNL-chunk + CMPR-chunk
        let mut prop_body = Vec::new();
        prop_body.extend_from_slice(b"SND ");
        // FS chunk (4-byte body)
        let mut fs_body = Vec::new();
        fs_body.extend_from_slice(&sample_rate.to_be_bytes());
        push_chunk(&mut prop_body, b"FS  ", &fs_body);
        // CHNL chunk (2 + n*4)
        let mut chnl_body = Vec::new();
        chnl_body.extend_from_slice(&num_channels.to_be_bytes());
        for id in ch_ids {
            chnl_body.extend_from_slice(*id);
        }
        push_chunk(&mut prop_body, b"CHNL", &chnl_body);
        // CMPR chunk (5 bytes — odd, so pad-byte will be appended)
        let mut cmpr_body = Vec::new();
        cmpr_body.extend_from_slice(cmpr);
        cmpr_body.push(0); // name count = 0
        push_chunk(&mut prop_body, b"CMPR", &cmpr_body);

        // Top-level chunks: PROP, DSD
        let mut top = Vec::new();
        push_chunk(&mut top, b"PROP", &prop_body);
        let audio_body = vec![0u8; audio_size];
        push_chunk(&mut top, b"DSD ", &audio_body);

        // FRM8 header: "FRM8" + size(u64) + form-type("DSD ") + top
        let mut buf = Vec::new();
        buf.extend_from_slice(b"FRM8");
        let form_size: u64 = 4 + top.len() as u64; // 4 = form-type
        buf.extend_from_slice(&form_size.to_be_bytes());
        buf.extend_from_slice(b"DSD ");
        buf.extend_from_slice(&top);
        buf
    }

    #[test]
    fn parse_minimal_dsdiff_stereo_dsd64() {
        // 2.8224 MHz = 64 * 44100 — canonical DSD64.
        let buf = make_dsdiff(2_822_400, 2, &[b"SLFT", b"SRGT"], b"DSD ", 1024);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_dsdiff(&mut fa));

        let g = |k: &str| fa.retrieve(StreamKind::General, 0, k).map(|z| z.as_str().to_owned());
        let a = |k: &str| fa.retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());

        assert_eq!(g("Format").as_deref(), Some("DSDIFF"));
        assert_eq!(g("AudioCount").as_deref(), Some("1"));
        assert_eq!(a("Format").as_deref(), Some("DSD"));
        assert_eq!(a("SamplingRate").as_deref(), Some("2822400"));
        assert_eq!(a("Channels").as_deref(), Some("2"));
        assert_eq!(a("BitDepth").as_deref(), Some("1"));
        assert_eq!(a("Compression_Mode").as_deref(), Some("Lossless"));
        assert_eq!(a("BitRate_Mode").as_deref(), Some("CBR"));
        assert_eq!(a("StreamSize").as_deref(), Some("1024"));
    }

    #[test]
    fn rejects_non_frm8_buffer() {
        let mut fa = FileAnalyze::new(b"NOTAFRM8headerXXXXXXXX");
        assert!(!parse_dsdiff(&mut fa));
    }

    #[test]
    fn rejects_frm8_with_non_dsd_form_type() {
        // FRM8 magic but form-type "FOO " — not a DSDIFF file.
        let mut buf = Vec::new();
        buf.extend_from_slice(b"FRM8");
        buf.extend_from_slice(&4u64.to_be_bytes());
        buf.extend_from_slice(b"FOO ");
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_dsdiff(&mut fa));
    }

    #[test]
    fn maps_dst_compression_to_dst_format() {
        let buf = make_dsdiff(2_822_400, 2, &[b"SLFT", b"SRGT"], b"DST ", 512);
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_dsdiff(&mut fa));
        let a = |k: &str| fa.retrieve(StreamKind::Audio, 0, k).map(|z| z.as_str().to_owned());
        assert_eq!(a("Format").as_deref(), Some("DST"));
    }
}
