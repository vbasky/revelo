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

use revelio_core::{FileAnalyze, Reader, StreamKind};

const FOURCC_FRM8: u32 = u32::from_be_bytes(*b"FRM8");
const FOURCC_DSD_FORMTYPE: u32 = u32::from_be_bytes(*b"DSD ");
const FOURCC_FVER: u32 = u32::from_be_bytes(*b"FVER");
const FOURCC_PROP: u32 = u32::from_be_bytes(*b"PROP");
const FOURCC_SND: u32 = u32::from_be_bytes(*b"SND ");
const FOURCC_FS: u32 = u32::from_be_bytes(*b"FS  ");
const FOURCC_CHNL: u32 = u32::from_be_bytes(*b"CHNL");
const FOURCC_CMPR: u32 = u32::from_be_bytes(*b"CMPR");
const FOURCC_DSD_DATA: u32 = u32::from_be_bytes(*b"DSD ");
const FOURCC_DST_DATA: u32 = u32::from_be_bytes(*b"DST ");

const CMPR_DSD: u32 = u32::from_be_bytes(*b"DSD ");
const CMPR_DST: u32 = u32::from_be_bytes(*b"DST ");

#[derive(Debug, Default)]
struct DsdiffInfo {
    sample_rate: u32,
    num_channels: u16,
    format: Option<&'static str>,
    audio_stream_size: u64,
}

/// Parse DSD Interchange File Format.
///
/// Detection: `FRM8` + DSD header.
/// Fills: Channels, sample rate, DSD properties.
pub fn parse_dsdiff(fa: &mut FileAnalyze) -> bool {
    parse(fa).is_some()
}

fn parse(fa: &mut FileAnalyze) -> Option<()> {
    let r = &mut Reader::wrap(fa);
    if r.remain() < 16 {
        return None;
    }
    if r.peek_be_u32()? != FOURCC_FRM8 {
        return None;
    }

    r.element_begin("FRM8");
    r.fourcc("ID")?;
    r.be_u64("Size")?;
    let form_type = r.fourcc("FormType")?;

    if form_type != FOURCC_DSD_FORMTYPE {
        r.element_end();
        return None;
    }

    let mut info = DsdiffInfo::default();

    // Top-level chunks under the DSD form.
    walk_chunks(r, &mut info, /*inside_prop=*/ false);

    r.element_end();

    // If CMPR was absent, MediaInfoLib leaves Audio.Format empty; we
    // default to "DSD" because the form type IS "DSD " — that's the
    // overwhelming common case (DST is rare) and matches the file's
    // declared identity.
    if info.format.is_none() {
        info.format = Some("DSD");
    }

    fill_streams(r, &info);
    Some(())
}

/// Walk a sequence of chunks until the buffer is exhausted. When
/// `inside_prop` is true, we're scanning the PROP/SND payload's
/// sub-chunks rather than top-level FRM8 children — same on-disk
/// shape, different semantics on a few IDs.
fn walk_chunks(r: &mut Reader<'_, '_>, info: &mut DsdiffInfo, inside_prop: bool) {
    while r.remain() >= 12 {
        let chunk_id = r.fourcc("ChunkID").unwrap_or(0);
        let chunk_size = r.be_u64("ChunkSize").unwrap_or(0);

        // Guard against malformed sizes that exceed the buffer.
        let body_len =
            if (chunk_size as usize) > r.remain() { r.remain() } else { chunk_size as usize };
        let body_start = r.element_offset();
        let body_end = body_start + body_len;

        if !inside_prop {
            match chunk_id {
                FOURCC_PROP => {
                    r.element_begin("PROP");
                    // PROP starts with the 4-byte propType ("SND ").
                    if r.remain() >= 4 {
                        let prop_type = r.fourcc("propType").unwrap_or(0);
                        if prop_type == FOURCC_SND {
                            // Recurse into sub-chunks until the PROP body ends.
                            let sub_end = body_end;
                            while r.element_offset() + 12 <= sub_end {
                                parse_prop_subchunk(r, info, sub_end);
                            }
                        }
                    }
                    // Skip any trailing bytes inside PROP we didn't consume.
                    if r.element_offset() < body_end {
                        r.skip(body_end - r.element_offset());
                    }
                    r.element_end();
                }
                FOURCC_DSD_DATA => {
                    r.element_begin("DSD");
                    info.audio_stream_size = body_len as u64;
                    r.skip(body_len);
                    r.element_end();
                }
                FOURCC_DST_DATA => {
                    r.element_begin("DST");
                    info.audio_stream_size = body_len as u64;
                    r.skip(body_len);
                    r.element_end();
                }
                FOURCC_FVER => {
                    r.skip(body_len);
                }
                _ => {
                    r.skip(body_len);
                }
            }
        }

        // Realign past any malformed gap (defensive — body parsers above
        // should always advance to body_end exactly).
        if r.element_offset() < body_end {
            r.skip(body_end - r.element_offset());
        }

        // Per-spec 1-byte pad when chunk size is odd.
        if chunk_size % 2 == 1 && r.remain() >= 1 {
            r.be_u8("pad");
        }
    }
}

fn parse_prop_subchunk(r: &mut Reader<'_, '_>, info: &mut DsdiffInfo, sub_end: usize) {
    let sub_id = r.fourcc("ChunkID").unwrap_or(0);
    let sub_size = r.be_u64("ChunkSize").unwrap_or(0);

    let max_body = sub_end.saturating_sub(r.element_offset());
    let body_len = (sub_size as usize).min(max_body);
    let body_start = r.element_offset();
    let body_end = body_start + body_len;

    match sub_id {
        FOURCC_FS => {
            if body_len >= 4 {
                info.sample_rate = r.be_u32("sampleRate").unwrap_or(0);
                if body_len > 4 {
                    r.skip(body_len - 4);
                }
            } else {
                r.skip(body_len);
            }
        }
        FOURCC_CHNL => {
            if body_len >= 2 {
                info.num_channels = r.be_u16("numChannels").unwrap_or(0);
                // chID list follows (4 bytes each). We don't need them
                // for the minimum-viable fields — skip to chunk end.
                let consumed = 2usize;
                if body_len > consumed {
                    r.skip(body_len - consumed);
                }
            } else {
                r.skip(body_len);
            }
        }
        FOURCC_CMPR => {
            if body_len >= 5 {
                let compression_type = r.be_u32("compressionType").unwrap_or(0);
                let name_count = r.be_u8("Count").unwrap_or(0);
                let name_take = (name_count as usize).min(body_len.saturating_sub(5));
                if name_take > 0 {
                    r.skip(name_take);
                }
                let consumed = 5 + name_take;
                if body_len > consumed {
                    r.skip(body_len - consumed);
                }
                info.format = Some(match compression_type {
                    CMPR_DSD => "DSD",
                    CMPR_DST => "DST",
                    _ => "DSD",
                });
            } else {
                r.skip(body_len);
            }
        }
        _ => {
            r.skip(body_len);
        }
    }

    // Defensive: ensure we land on body_end.
    if r.element_offset() < body_end {
        r.skip(body_end - r.element_offset());
    }

    // Pad if odd-sized sub-chunk.
    if sub_size % 2 == 1 && r.element_offset() < sub_end {
        r.be_u8("pad");
    }
}

fn fill_streams(r: &mut Reader<'_, '_>, info: &DsdiffInfo) {
    r.stream_prepare(StreamKind::General);
    r.set_field(StreamKind::General, 0, "Format", "DSDIFF");

    r.stream_prepare(StreamKind::Audio);
    if let Some(fmt) = info.format {
        r.set_field(StreamKind::Audio, 0, "Format", fmt);
    }
    if info.sample_rate > 0 {
        r.set_field(StreamKind::Audio, 0, "SamplingRate", info.sample_rate.to_string());
    }
    if info.num_channels > 0 {
        r.set_field(StreamKind::Audio, 0, "Channels", info.num_channels.to_string());
    }
    // DSD is by construction 1-bit-per-sample-per-channel; this is the
    // defining property of Direct Stream Digital and the reason MediaInfo
    // doesn't carry a BitDepth field in CMPR.
    r.set_field(StreamKind::Audio, 0, "BitDepth", "1");
    r.set_field(StreamKind::Audio, 0, "Compression_Mode", "Lossless");
    r.set_field(StreamKind::Audio, 0, "BitRate_Mode", "CBR");

    if info.audio_stream_size > 0 {
        r.set_field(StreamKind::Audio, 0, "StreamSize", info.audio_stream_size.to_string());
    }

    r.set_field(StreamKind::General, 0, "AudioCount", "1");
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
