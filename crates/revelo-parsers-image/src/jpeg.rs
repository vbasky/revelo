//! JPEG parser — segment-based image format. Mirrors the subset of
//! MediaInfoLib's `File_Jpeg.cpp` for plain JPEG/JFIF files.
//!
//! Layout: 0xFFD8 (SOI) + segments + 0xFFD9 (EOI).
//! Each non-fixed-length segment is `FF [marker:u8] [length:u16 BE]
//! [length-2 bytes payload]`.
//!
//! This module owns only JPEG *container structure*: SOFn geometry, chroma
//! subsampling, segment overhead, the JFIF version, the COM comment, and the
//! embedded EXIF thumbnail (IFD1). All EXIF/XMP/ICC *tag* extraction — and the
//! MediaInfo-vocabulary EXIF fields derived from it — is owned by the generic
//! `revelo-parsers-tag` crate, which runs as a second pass over every file.
//!
//! SOFn markers (Start Of Frame) carry the image geometry:
//!   FFC0 baseline DCT, FFC1 extended sequential, FFC2 progressive,
//!   FFC3 lossless, FFC5..FFC7 differential, FFC9..FFCB arithmetic,
//!   FFCD..FFCF differential arithmetic.
//! SOF payload: precision(1) + height(2 BE) + width(2 BE) + components(1)
//!   + per-component (id, h_v_sampling, qt_index) * components.

use revelo_core::{FileAnalyze, StreamKind};

const JPEG_SOF_PARSE_LIMIT: usize = 256;
const JPEG_COMMENT_PARSE_LIMIT: usize = 16 * 1024;
const JPEG_JFIF_HEADER_LEN: usize = 7;
const JPEG_EXIF_PARSE_LIMIT: usize = 64 * 1024;

/// Embedded-thumbnail geometry, read from the EXIF IFD1 chain. This is the
/// only part of the EXIF block the container parser needs (it drives the
/// `Image #2` stream, `ImageCount`, and the `StreamSize` deduction).
#[derive(Default)]
struct ThumbnailData {
    /// IFD1 ImageWidth (0x0100) — thumbnail geometry when present.
    thumbnail_width: Option<u32>,
    /// IFD1 ImageLength (0x0101).
    thumbnail_height: Option<u32>,
    /// JPEGInterchangeFormatLength (0x0202) — thumbnail byte size.
    thumbnail_size: Option<u32>,
    /// JPEGInterchangeFormat (0x0201) — offset of the embedded thumbnail JPEG
    /// within the TIFF buffer. Used to read the thumbnail SOF when IFD1 lacks
    /// explicit ImageWidth/Length tags.
    thumbnail_offset: Option<u32>,
}

/// Detection: SOI marker 0xFFD8.
/// Fills: SOFn dimensions, color space, chroma subsampling, JFIF version,
/// COM comment, and the EXIF thumbnail geometry.
pub fn parse_jpeg(fa: &mut FileAnalyze) -> bool {
    let head = fa.peek_raw(2);
    let Some(h) = head else { return false };
    if h != [0xFF, 0xD8] {
        return false;
    }

    let file_size = fa.remain();
    fa.skip_hexa(2, "SOI");

    let mut width: u16 = 0;
    let mut height: u16 = 0;
    let mut precision: u8 = 0;
    let mut components: u8 = 0;
    let mut sampling: Vec<(u8, u8)> = Vec::new();
    let mut found_sof = false;
    let mut comment: Option<String> = None;
    let mut jfif_version: Option<String> = None;
    let mut thumb = ThumbnailData::default();
    // Overhead = APP markers (0xE0..0xEF) + COM markers (0xFE).
    // Oracle treats SOI/EOI/DQT/DHT/SOF/SOS/entropy as image data.
    let mut overhead: usize = 0;

    while fa.remain() >= 4 {
        let marker_bytes = fa.read_raw(2).to_vec();
        if marker_bytes.len() < 2 || marker_bytes[0] != 0xFF {
            // Past markers — into entropy-coded data.
            break;
        }
        let marker = marker_bytes[1];
        if marker == 0xD9 {
            // EOI
            break;
        }
        if marker == 0xDA {
            // SOS — entropy-coded data follows. Stop marker scanning.
            break;
        }
        // Markers without payload length: D0..D7 (RSTn), 01 (TEM)
        if (0xD0..=0xD7).contains(&marker) || marker == 0x01 {
            continue;
        }
        // Read 2-byte BE length (includes the 2-byte length field itself).
        let len_bytes = fa.peek_raw(2);
        let Some(lb) = len_bytes else { break };
        let segment_len = u16::from_be_bytes([lb[0], lb[1]]) as usize;
        if segment_len < 2 || fa.remain() < segment_len {
            break;
        }
        // APP markers + COM marker count toward General.StreamSize.
        if (0xE0..=0xEF).contains(&marker) || marker == 0xFE {
            overhead += 2 + segment_len; // marker(2) + length+payload
        }
        fa.skip_hexa(2, "segment_length");
        let payload_size = segment_len - 2;

        // SOFn markers (geometry). Excludes FFC4 (DHT), FFC8 (JPG), FFCC (DAC).
        let is_sof = matches!(
            marker,
            0xC0 | 0xC1
                | 0xC2
                | 0xC3
                | 0xC5
                | 0xC6
                | 0xC7
                | 0xC9
                | 0xCA
                | 0xCB
                | 0xCD
                | 0xCE
                | 0xCF
        );
        if is_sof && !found_sof && payload_size >= 6 {
            let payload = fa.peek_raw(payload_size.min(JPEG_SOF_PARSE_LIMIT)).unwrap_or(&[]);
            precision = payload[0];
            height = u16::from_be_bytes([payload[1], payload[2]]);
            width = u16::from_be_bytes([payload[3], payload[4]]);
            components = payload[5];
            // Each component is 3 bytes: id, h_v_sampling, qt_idx.
            for c in 0..(components as usize) {
                let off = 6 + c * 3;
                if off + 1 < payload.len() {
                    let hv = payload[off + 1];
                    sampling.push(((hv >> 4) & 0xF, hv & 0xF));
                }
            }
            fa.skip_hexa(payload_size, "sof_segment");
            found_sof = true;
        } else if marker == 0xFE {
            // COM (Comment) segment — payload is UTF-8 text.
            let payload =
                fa.peek_raw(payload_size.min(JPEG_COMMENT_PARSE_LIMIT)).unwrap_or(&[]).to_vec();
            fa.skip_hexa(payload_size, "comment_segment");
            comment = Some(String::from_utf8_lossy(&payload).trim_end_matches('\0').to_string());
        } else if marker == 0xE0 && payload_size >= 7 {
            // APP0 JFIF: "JFIF\0" + version major + minor (rendered N.NN).
            let payload = fa.peek_raw(JPEG_JFIF_HEADER_LEN).unwrap_or(&[]);
            if &payload[..5] == b"JFIF\0" {
                jfif_version = Some(format!("{}.{:02}", payload[5], payload[6]));
            }
            fa.skip_hexa(payload_size, "jfif_segment");
        } else if marker == 0xE1 && payload_size >= 14 {
            // APP1 Exif: walk only the IFD1 thumbnail chain. All other EXIF
            // tags are handled by the generic tag pass (revelo-parsers-tag).
            if payload_size <= JPEG_EXIF_PARSE_LIMIT {
                let payload = fa.read_raw(payload_size).to_vec();
                if payload.len() >= 14 && &payload[..6] == b"Exif\0\0" {
                    parse_exif_thumbnail(&payload[6..], &mut thumb);
                }
            } else {
                fa.skip_hexa(payload_size, "exif_segment");
            }
        } else {
            fa.skip_hexa(payload_size, "segment");
        }
    }

    if !found_sof {
        return false;
    }

    fill_streams(
        fa,
        file_size,
        overhead,
        comment,
        &thumb,
        JpegFrame { width, height, precision, components },
        &sampling,
    );
    if let Some(v) = jfif_version {
        fa.set_field(StreamKind::General, 0, "JFIFVersion", v);
    }
    true
}

/// Parse only the IFD1 (thumbnail) chain of an EXIF TIFF block.
///
/// TIFF header (8 bytes): byte_order ("II"/"MM") + magic(42) + IFD0 offset.
/// IFD1 begins at the next-IFD offset stored after IFD0.
fn parse_exif_thumbnail(tiff: &[u8], out: &mut ThumbnailData) {
    if tiff.len() < 8 {
        return;
    }
    let little = match &tiff[..2] {
        b"II" => true,
        b"MM" => false,
        _ => return,
    };
    let r16 = |b: &[u8]| -> u16 {
        if little { u16::from_le_bytes([b[0], b[1]]) } else { u16::from_be_bytes([b[0], b[1]]) }
    };
    let r32 = |b: &[u8]| -> u32 {
        if little {
            u32::from_le_bytes([b[0], b[1], b[2], b[3]])
        } else {
            u32::from_be_bytes([b[0], b[1], b[2], b[3]])
        }
    };
    if r16(&tiff[2..4]) != 42 {
        return;
    }
    let ifd0_off = r32(&tiff[4..8]) as usize;
    if let Some(ifd1_off) = next_ifd_offset(tiff, ifd0_off, &r16, &r32)
        && ifd1_off != 0
    {
        walk_ifd1(tiff, ifd1_off as usize, &r16, &r32, out);
    }
}

fn next_ifd_offset(
    tiff: &[u8],
    ifd_off: usize,
    r16: &impl Fn(&[u8]) -> u16,
    r32: &impl Fn(&[u8]) -> u32,
) -> Option<u32> {
    if ifd_off + 2 > tiff.len() {
        return None;
    }
    let count = r16(&tiff[ifd_off..ifd_off + 2]) as usize;
    let next_pos = ifd_off + 2 + count * 12;
    if next_pos + 4 > tiff.len() {
        return None;
    }
    Some(r32(&tiff[next_pos..next_pos + 4]))
}

fn walk_ifd1(
    tiff: &[u8],
    ifd_off: usize,
    r16: &impl Fn(&[u8]) -> u16,
    r32: &impl Fn(&[u8]) -> u32,
    out: &mut ThumbnailData,
) {
    if ifd_off + 2 > tiff.len() {
        return;
    }
    let count = r16(&tiff[ifd_off..ifd_off + 2]) as usize;
    for i in 0..count {
        let entry = ifd_off + 2 + i * 12;
        if entry + 12 > tiff.len() {
            break;
        }
        let tag = r16(&tiff[entry..entry + 2]);
        let cnt = r32(&tiff[entry + 4..entry + 8]);
        let val = &tiff[entry + 8..entry + 12];
        match tag {
            // ImageWidth / ImageLength in IFD1 describe the thumbnail geometry.
            0x0100 if cnt == 1 => out.thumbnail_width = Some(r32(val)),
            0x0101 if cnt == 1 => out.thumbnail_height = Some(r32(val)),
            // JPEGInterchangeFormat = thumbnail data offset.
            0x0201 if cnt == 1 => out.thumbnail_offset = Some(r32(val)),
            // JPEGInterchangeFormatLength = thumbnail byte size.
            0x0202 if cnt == 1 => out.thumbnail_size = Some(r32(val)),
            _ => {}
        }
    }
    // If IFD1 only carries the JPEG offset/length but no ImageWidth/Length,
    // parse the embedded JPEG's SOF for geometry. Common in compact-camera
    // thumbnails (e.g. Acer C01).
    if out.thumbnail_width.is_none()
        && let (Some(off), Some(sz)) = (out.thumbnail_offset, out.thumbnail_size)
    {
        let lo = off as usize;
        let hi = (off + sz) as usize;
        if hi <= tiff.len()
            && let Some((w, h)) = scan_jpeg_sof(&tiff[lo..hi])
        {
            out.thumbnail_width = Some(w as u32);
            out.thumbnail_height = Some(h as u32);
        }
    }
}

/// Scan a JPEG byte stream for the first SOF marker and return its
/// (width, height). Returns None if no SOF found.
fn scan_jpeg_sof(buf: &[u8]) -> Option<(u16, u16)> {
    if buf.len() < 4 || buf[0] != 0xFF || buf[1] != 0xD8 {
        return None;
    }
    let mut i = 2usize;
    while i + 4 <= buf.len() {
        if buf[i] != 0xFF {
            return None;
        }
        let m = buf[i + 1];
        if m == 0xD9 || m == 0xDA {
            return None;
        }
        if (0xD0..=0xD7).contains(&m) || m == 0x01 {
            i += 2;
            continue;
        }
        if i + 4 > buf.len() {
            return None;
        }
        let seg = u16::from_be_bytes([buf[i + 2], buf[i + 3]]) as usize;
        let is_sof = matches!(
            m,
            0xC0 | 0xC1
                | 0xC2
                | 0xC3
                | 0xC5
                | 0xC6
                | 0xC7
                | 0xC9
                | 0xCA
                | 0xCB
                | 0xCD
                | 0xCE
                | 0xCF
        );
        if is_sof && i + 4 + 5 <= buf.len() {
            // SOF payload = precision(1) + height(2) + width(2) + ...
            let h = u16::from_be_bytes([buf[i + 5], buf[i + 6]]);
            let w = u16::from_be_bytes([buf[i + 7], buf[i + 8]]);
            return Some((w, h));
        }
        i += 2 + seg;
    }
    None
}

struct JpegFrame {
    width: u16,
    height: u16,
    precision: u8,
    components: u8,
}

fn fill_streams(
    fa: &mut FileAnalyze,
    file_size: usize,
    overhead: usize,
    comment: Option<String>,
    thumb: &ThumbnailData,
    frame: JpegFrame,
    sampling: &[(u8, u8)],
) {
    let JpegFrame { width, height, precision, components } = frame;
    fa.stream_prepare(StreamKind::General);
    fa.set_field(StreamKind::General, 0, "Format", "JPEG");
    let has_thumbnail = thumb.thumbnail_width.is_some() && thumb.thumbnail_height.is_some();
    let image_count = if has_thumbnail { 2 } else { 1 };
    fa.set_field(StreamKind::General, 0, "ImageCount", image_count.to_string());
    // General.StreamSize = file overhead. When a thumbnail is embedded its
    // bytes are part of the EXIF APP1 segment but oracle deducts them from
    // General (and attributes them to the thumbnail Image stream instead).
    let thumb_bytes = thumb.thumbnail_size.unwrap_or(0) as usize;
    let general_overhead = overhead.saturating_sub(thumb_bytes);
    fa.force_field(StreamKind::General, 0, "StreamSize", general_overhead.to_string());

    fa.stream_prepare(StreamKind::Image);
    fa.set_field(StreamKind::Image, 0, "Format", "JPEG");
    fa.set_field(StreamKind::Image, 0, "Width", width.to_string());
    fa.set_field(StreamKind::Image, 0, "Height", height.to_string());
    let color_space = match components {
        1 => "Y",
        3 => "YUV",
        4 => "CMYK",
        _ => "Unknown",
    };
    fa.set_field(StreamKind::Image, 0, "ColorSpace", color_space);
    if components == 3 && sampling.len() == 3 {
        let y = sampling[0];
        let cb = sampling[1];
        let cr = sampling[2];
        // Standard subsampling ratios derived from Y's H×V vs Cb/Cr.
        let subsampling = if cb == cr && cb == (1, 1) {
            match y {
                (1, 1) => Some("4:4:4"),
                (2, 1) => Some("4:2:2"),
                (2, 2) => Some("4:2:0"),
                (4, 1) => Some("4:1:1"),
                (4, 2) => Some("4:1:0"),
                _ => None,
            }
        } else {
            None
        };
        if let Some(s) = subsampling {
            fa.set_field(StreamKind::Image, 0, "ChromaSubsampling", s);
        }
    }
    fa.set_field(StreamKind::Image, 0, "BitDepth", precision.to_string());
    fa.set_field(StreamKind::Image, 0, "Compression_Mode", "Lossy");
    // Main Image.StreamSize = file - all-APP-overhead (the full overhead,
    // including the embedded thumbnail bytes). When a thumbnail exists its
    // bytes are NOT main-image data — keep using the full overhead.
    let image_size = file_size.saturating_sub(overhead);
    fa.set_field(StreamKind::Image, 0, "StreamSize", image_size.to_string());
    if let Some(c) = comment {
        // The COM-segment comment lands on the General stream (last field),
        // not the Image stream — matching the oracle's placement.
        fa.set_field(StreamKind::General, 0, "Comment", c);
    }

    // EXIF thumbnail → second Image stream (oracle labels it Type=Thumbnail,
    // MuxingMode=Exif). StreamSize comes from JPEGInterchangeFormatLength.
    if let (Some(tw), Some(th)) = (thumb.thumbnail_width, thumb.thumbnail_height) {
        let tpos = fa.stream_prepare(StreamKind::Image);
        fa.set_field(StreamKind::Image, tpos, "Type", "Thumbnail");
        fa.set_field(StreamKind::Image, tpos, "MuxingMode", "Exif");
        fa.set_field(StreamKind::Image, tpos, "Format", "JPEG");
        fa.set_field(StreamKind::Image, tpos, "Width", tw.to_string());
        fa.set_field(StreamKind::Image, tpos, "Height", th.to_string());
        if let Some(sz) = thumb.thumbnail_size {
            fa.set_field(StreamKind::Image, tpos, "StreamSize", sz.to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segment(marker: u8, payload: &[u8]) -> Vec<u8> {
        let mut out = vec![0xFF, marker];
        out.extend_from_slice(&((payload.len() + 2) as u16).to_be_bytes());
        out.extend_from_slice(payload);
        out
    }

    #[test]
    fn rejects_non_jpeg_buffer() {
        let mut fa = FileAnalyze::new(b"NOT a JPEG file");
        assert!(!parse_jpeg(&mut fa));
    }

    #[test]
    fn comment_segment_is_capped() {
        let sof = [8, 0, 1, 0, 1, 1, 1, 0x11, 0];
        let mut comment = Vec::new();
        comment.resize(JPEG_COMMENT_PARSE_LIMIT + 256, b'a');

        let mut buf = Vec::new();
        buf.extend_from_slice(&[0xFF, 0xD8]);
        buf.extend(segment(0xC0, &sof));
        buf.extend(segment(0xFE, &comment));
        buf.extend_from_slice(&[0xFF, 0xD9]);

        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_jpeg(&mut fa));
        assert!(fa.access_stats().max_request_len <= JPEG_COMMENT_PARSE_LIMIT);
    }
}
