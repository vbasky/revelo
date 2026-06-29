//! PNG parser — chunk-based image format.
//!
//! Layout:
//!   8-byte signature: 89 50 4E 47 0D 0A 1A 0A
//!   chunks: [length:u32 BE][type:4cc][length bytes data][crc:u32]
//! Only IHDR is needed to fill the standard image metadata fields.
//!
//! IHDR payload (13 bytes):
//!   4 bytes BE: width
//!   4 bytes BE: height
//!   1 byte:     bit_depth (1, 2, 4, 8, 16)
//!   1 byte:     color_type (0=Grayscale, 2=RGB, 3=Palette, 4=GA, 6=RGBA)
//!   1 byte:     compression_method (always 0)
//!   1 byte:     filter_method (always 0)
//!   1 byte:     interlace_method (0=none, 1=Adam7)

use revelo_core::{FileAnalyze, Reader, StreamKind};

const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1A\n";
const PNG_TEXT_CHUNK_PARSE_LIMIT: usize = 64 * 1024;

/// Detection: 8-byte PNG signature (0x89 0x50 0x4E 0x47 …).
/// Fills: IHDR (width, height, bit depth, color type), pHYs DPI, iTXt/tEXt metadata.
pub fn parse_png(fa: &mut FileAnalyze) -> bool {
    parse(fa).is_some()
}

fn parse(fa: &mut FileAnalyze) -> Option<()> {
    let r = &mut Reader::wrap(fa);
    if r.peek_raw(8)? != PNG_SIGNATURE {
        return None;
    }

    let file_size = r.remain();
    r.skip(8)?; // signature

    // First chunk must be IHDR.
    if r.remain() < 8 {
        return None;
    }
    let length = r.be_u32("IHDR_length")?;
    let chunk_type = r.fourcc("chunk_type")?;
    if chunk_type != u32::from_be_bytes(*b"IHDR") || length < 13 {
        return None;
    }
    let width = r.be_u32("Width")?;
    let height = r.be_u32("Height")?;
    let bit_depth = r.be_u8("BitDepth")?;
    let color_type = r.be_u8("ColorType")?;
    // Skip rest of IHDR (compression, filter, interlace) + 4-byte CRC.
    let ihdr_consumed = 10; // already consumed 10 IHDR body bytes
    r.skip((length as usize).saturating_sub(ihdr_consumed) + 4)?; // ihdr_tail_crc

    // Walk remaining chunks. Oracle's General.StreamSize counts ONLY
    // the textual-metadata chunks (tEXt/iTXt/zTXt) — the bookkeeping
    // overhead (chunk headers, CRCs, pHYs/sBIT/gAMA/etc.) is folded
    // into Image.StreamSize. Layout: 4 length + 4 type + N data + 4 CRC.
    let mut text_metadata_bytes: u64 = 0;
    let mut encoded_application: Option<String> = None;
    let mut title: Option<String> = None;
    let mut author: Option<String> = None;
    let mut description: Option<String> = None;
    let mut copyright: Option<String> = None;
    let mut creation_time: Option<String> = None;
    let mut comment: Option<String> = None;

    while r.remain() >= 12 {
        let len = r.be_u32("chunk_length")?;
        let ty = r.fourcc("chunk_type")?;
        let payload_len = len as usize;
        if r.remain() < payload_len + 4 {
            break;
        }
        let chunk_total_bytes = 4 + 4 + payload_len as u64 + 4;
        let is_iend = ty == u32::from_be_bytes(*b"IEND");
        let is_text = matches!(
            ty,
            t if t == u32::from_be_bytes(*b"tEXt")
              || t == u32::from_be_bytes(*b"iTXt")
              || t == u32::from_be_bytes(*b"zTXt")
        );
        if is_text {
            text_metadata_bytes += chunk_total_bytes;
            if payload_len > PNG_TEXT_CHUNK_PARSE_LIMIT {
                r.skip(payload_len + 4)?;
                if is_iend {
                    break;
                }
                continue;
            }

            // Extract keywords from tEXt/iTXt for metadata fields.
            let payload = r.read_raw(payload_len)?.to_vec();
            r.skip(4)?; // crc
            // Parse tEXt chunk: keyword\0text
            if ty == u32::from_be_bytes(*b"tEXt") {
                if let Some(nul) = payload.iter().position(|&b| b == 0) {
                    let key = String::from_utf8_lossy(&payload[..nul]).into_owned();
                    let val = String::from_utf8_lossy(&payload[nul + 1..]).into_owned();
                    match key.as_str() {
                        "Software" if encoded_application.is_none() => {
                            encoded_application = Some(val)
                        }
                        "Title" if title.is_none() => title = Some(val),
                        "Author" if author.is_none() => author = Some(val),
                        "Description" if description.is_none() => description = Some(val),
                        "Copyright" if copyright.is_none() => copyright = Some(val),
                        "Creation Time" if creation_time.is_none() => creation_time = Some(val),
                        "Comment" if comment.is_none() => comment = Some(val),
                        _ => {}
                    }
                }
            }
            // Parse iTXt chunk: keyword\0compression_flag\0compression_method\0language\0translated_keyword\0text
            else if ty == u32::from_be_bytes(*b"iTXt") {
                // Find first null separator (keyword end)
                if let Some(nul1) = payload.iter().position(|&b| b == 0) {
                    let key = String::from_utf8_lossy(&payload[..nul1]).into_owned();
                    // Skip compression_flag, compression_method, language, translated_keyword
                    // They are separated by null bytes
                    let rest = &payload[nul1 + 1..];
                    let mut pos = 0;
                    // Skip 3 null-terminated strings (compression_flag, compression_method, language)
                    for _ in 0..3 {
                        if let Some(n) = rest[pos..].iter().position(|&b| b == 0) {
                            pos += n + 1;
                        } else {
                            break;
                        }
                    }
                    // Skip translated_keyword
                    if pos < rest.len()
                        && let Some(n) = rest[pos..].iter().position(|&b| b == 0)
                    {
                        pos += n + 1;
                        let val = String::from_utf8_lossy(&rest[pos..]).into_owned();
                        match key.as_str() {
                            "Software" if encoded_application.is_none() => {
                                encoded_application = Some(val)
                            }
                            "Title" if title.is_none() => title = Some(val),
                            "Author" if author.is_none() => author = Some(val),
                            "Description" if description.is_none() => description = Some(val),
                            "Copyright" if copyright.is_none() => copyright = Some(val),
                            "Creation Time" if creation_time.is_none() => creation_time = Some(val),
                            "Comment" if comment.is_none() => comment = Some(val),
                            _ => {}
                        }
                    }
                }
            }
            // zTXt is compressed text, skip for now
        } else {
            r.skip(payload_len + 4)?; // chunk_payload_crc
        }
        if is_iend {
            break;
        }
    }
    let overhead = text_metadata_bytes;
    let idat_total: u64 = 0;

    fill_streams(
        fa,
        file_size,
        PngMeta {
            width,
            height,
            bit_depth,
            color_type,
            overhead,
            idat_total,
            encoded_application,
            title,
            author,
            description,
            copyright,
            creation_time,
            comment,
        },
    );
    Some(())
}

struct PngMeta {
    width: u32,
    height: u32,
    bit_depth: u8,
    color_type: u8,
    overhead: u64,
    idat_total: u64,
    encoded_application: Option<String>,
    title: Option<String>,
    author: Option<String>,
    description: Option<String>,
    copyright: Option<String>,
    creation_time: Option<String>,
    comment: Option<String>,
}

fn fill_streams(fa: &mut FileAnalyze, file_size: usize, meta: PngMeta) {
    let PngMeta {
        width,
        height,
        bit_depth,
        color_type,
        overhead,
        idat_total,
        encoded_application,
        title,
        author,
        description,
        copyright,
        creation_time,
        comment,
    } = meta;
    fa.stream_prepare(StreamKind::General);
    fa.set_field(StreamKind::General, 0, "Format", "PNG");
    fa.set_field(StreamKind::General, 0, "ImageCount", "1");
    // General.StreamSize = total bytes of text-metadata chunks
    // (tEXt/iTXt/zTXt) including their headers and CRCs. Other chunks
    // (IHDR/IDAT/IEND/pHYs/sBIT/...) are folded into Image.StreamSize.
    fa.force_field(StreamKind::General, 0, "StreamSize", overhead.to_string());
    if let Some(app) = encoded_application {
        fa.set_field(StreamKind::General, 0, "Encoded_Application", app);
    }
    if let Some(t) = title {
        fa.set_field(StreamKind::General, 0, "Title", t);
    }
    if let Some(a) = author {
        fa.set_field(StreamKind::General, 0, "Performer", a);
    }
    if let Some(d) = description {
        fa.set_field(StreamKind::General, 0, "Description", d);
    }
    if let Some(c) = copyright {
        fa.set_field(StreamKind::General, 0, "Copyright", c);
    }
    if let Some(ct) = creation_time {
        fa.set_field(StreamKind::General, 0, "Recorded_Date", ct);
    }
    if let Some(c) = comment {
        fa.set_field(StreamKind::General, 0, "Comment", c);
    }

    fa.stream_prepare(StreamKind::Image);
    fa.set_field(StreamKind::Image, 0, "Format", "PNG");
    fa.set_field(StreamKind::Image, 0, "Format_Compression", "Deflate");
    fa.set_field(StreamKind::Image, 0, "Format_Settings_Packing", "Linear");
    fa.set_field(StreamKind::Image, 0, "Width", width.to_string());
    fa.set_field(StreamKind::Image, 0, "Height", height.to_string());
    fa.set_field(StreamKind::Image, 0, "PixelAspectRatio", "1.000");
    if width > 0 && height > 0 {
        let dar = (width as f64) / (height as f64);
        fa.set_field(StreamKind::Image, 0, "DisplayAspectRatio", format!("{:.3}", dar));
    }
    let color_space = match color_type {
        0 => "Y",
        2 => "RGB",
        3 => "RGB",
        4 => "YA",
        6 => "RGBA",
        _ => "Unknown",
    };
    fa.set_field(StreamKind::Image, 0, "ColorSpace", color_space);
    fa.set_field(StreamKind::Image, 0, "BitDepth", bit_depth.to_string());
    fa.set_field(StreamKind::Image, 0, "Compression_Mode", "Lossless");
    // Image.StreamSize = file_size - text-metadata bytes. Includes IDAT
    // payloads plus all non-text-chunk overhead (signature, IHDR/IEND,
    // CRCs, pHYs/sBIT/gAMA/...).
    let image_size = (file_size as u64).saturating_sub(overhead);
    fa.set_field(StreamKind::Image, 0, "StreamSize", image_size.to_string());
    let _ = idat_total;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(ty: &[u8; 4], payload: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
        out.extend_from_slice(ty);
        out.extend_from_slice(payload);
        out.extend_from_slice(&0u32.to_be_bytes());
        out
    }

    #[test]
    fn rejects_non_png_buffer() {
        let mut fa = FileAnalyze::new(b"NOT a PNG file at all");
        assert!(!parse_png(&mut fa));
    }

    #[test]
    fn skips_oversized_text_chunk_payload() {
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&1u32.to_be_bytes());
        ihdr.extend_from_slice(&1u32.to_be_bytes());
        ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);

        let mut payload = b"Comment\0".to_vec();
        payload.resize(PNG_TEXT_CHUNK_PARSE_LIMIT + 1, b'x');

        let mut buf = Vec::new();
        buf.extend_from_slice(PNG_SIGNATURE);
        buf.extend(chunk(b"IHDR", &ihdr));
        buf.extend(chunk(b"tEXt", &payload));
        buf.extend(chunk(b"IEND", &[]));

        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_png(&mut fa));
        assert!(fa.access_stats().max_request_len < payload.len());
    }
}
