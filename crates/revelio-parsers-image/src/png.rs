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

use revelio_core::{FileAnalyze, StreamKind};
use zenlib::Int32u;

const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1A\n";

pub fn parse_png(fa: &mut FileAnalyze) -> bool {
    let head = fa.peek_raw(8);
    let Some(h) = head else { return false };
    if h != PNG_SIGNATURE {
        return false;
    }

    let file_size = fa.remain();
    fa.skip_hexa(8, "signature");

    // First chunk must be IHDR.
    if fa.remain() < 8 {
        return false;
    }
    let mut length: Int32u = 0;
    fa.get_b4(&mut length, "IHDR_length");
    let mut chunk_type: Int32u = 0;
    fa.get_c4(&mut chunk_type, "chunk_type");
    if chunk_type != u32::from_be_bytes(*b"IHDR") || length < 13 {
        return false;
    }
    let mut width: Int32u = 0;
    fa.get_b4(&mut width, "Width");
    let mut height: Int32u = 0;
    fa.get_b4(&mut height, "Height");
    let mut bit_depth: zenlib::Int8u = 0;
    fa.get_b1(&mut bit_depth, "BitDepth");
    let mut color_type: zenlib::Int8u = 0;
    fa.get_b1(&mut color_type, "ColorType");
    // Skip rest of IHDR (compression, filter, interlace) + 4-byte CRC.
    let ihdr_consumed = 10; // already consumed 10 IHDR body bytes
    fa.skip_hexa((length as usize).saturating_sub(ihdr_consumed) + 4, "ihdr_tail_crc");

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
    
    while fa.remain() >= 12 {
        let mut len: Int32u = 0;
        fa.get_b4(&mut len, "chunk_length");
        let mut ty: Int32u = 0;
        fa.get_c4(&mut ty, "chunk_type");
        let payload_len = len as usize;
        if fa.remain() < payload_len + 4 {
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
            // Extract keywords from tEXt/iTXt for metadata fields.
            let payload = fa.read_raw(payload_len).to_vec();
            fa.skip_hexa(4, "crc");
            
            // Parse tEXt chunk: keyword\0text
            if ty == u32::from_be_bytes(*b"tEXt") {
                if let Some(nul) = payload.iter().position(|&b| b == 0) {
                    let key = String::from_utf8_lossy(&payload[..nul]).into_owned();
                    let val = String::from_utf8_lossy(&payload[nul + 1..]).into_owned();
                    match key.as_str() {
                        "Software" if encoded_application.is_none() => encoded_application = Some(val),
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
                        && let Some(n) = rest[pos..].iter().position(|&b| b == 0) {
                            pos += n + 1;
                            let val = String::from_utf8_lossy(&rest[pos..]).into_owned();
                            match key.as_str() {
                                "Software" if encoded_application.is_none() => encoded_application = Some(val),
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
            fa.skip_hexa(payload_len + 4, "chunk_payload_crc");
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
    true
}

struct PngMeta {
    width: Int32u,
    height: Int32u,
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
    fa.fill(StreamKind::General, 0, "Format", "PNG", false);
    fa.fill(StreamKind::General, 0, "ImageCount", "1", false);
    // General.StreamSize = total bytes of text-metadata chunks
    // (tEXt/iTXt/zTXt) including their headers and CRCs. Other chunks
    // (IHDR/IDAT/IEND/pHYs/sBIT/...) are folded into Image.StreamSize.
    fa.fill(StreamKind::General, 0, "StreamSize", overhead.to_string(), true);
    if let Some(app) = encoded_application {
        fa.fill(StreamKind::General, 0, "Encoded_Application", app, false);
    }
    if let Some(t) = title {
        fa.fill(StreamKind::General, 0, "Title", t, false);
    }
    if let Some(a) = author {
        fa.fill(StreamKind::General, 0, "Performer", a, false);
    }
    if let Some(d) = description {
        fa.fill(StreamKind::General, 0, "Description", d, false);
    }
    if let Some(c) = copyright {
        fa.fill(StreamKind::General, 0, "Copyright", c, false);
    }
    if let Some(ct) = creation_time {
        fa.fill(StreamKind::General, 0, "Recorded_Date", ct, false);
    }
    if let Some(c) = comment {
        fa.fill(StreamKind::General, 0, "Comment", c, false);
    }

    fa.stream_prepare(StreamKind::Image);
    fa.fill(StreamKind::Image, 0, "Format", "PNG", false);
    fa.fill(StreamKind::Image, 0, "Format_Compression", "Deflate", false);
    fa.fill(StreamKind::Image, 0, "Format_Settings_Packing", "Linear", false);
    fa.fill(StreamKind::Image, 0, "Width", width.to_string(), false);
    fa.fill(StreamKind::Image, 0, "Height", height.to_string(), false);
    fa.fill(StreamKind::Image, 0, "PixelAspectRatio", "1.000", false);
    if width > 0 && height > 0 {
        let dar = (width as f64) / (height as f64);
        fa.fill(
            StreamKind::Image,
            0,
            "DisplayAspectRatio",
            format!("{:.3}", dar),
            false,
        );
    }
    let color_space = match color_type {
        0 => "Y",
        2 => "RGB",
        3 => "RGB",
        4 => "YA",
        6 => "RGBA",
        _ => "Unknown",
    };
    fa.fill(StreamKind::Image, 0, "ColorSpace", color_space, false);
    fa.fill(StreamKind::Image, 0, "BitDepth", bit_depth.to_string(), false);
    fa.fill(StreamKind::Image, 0, "Compression_Mode", "Lossless", false);
    // Image.StreamSize = file_size - text-metadata bytes. Includes IDAT
    // payloads plus all non-text-chunk overhead (signature, IHDR/IEND,
    // CRCs, pHYs/sBIT/gAMA/...).
    let image_size = (file_size as u64).saturating_sub(overhead);
    fa.fill(StreamKind::Image, 0, "StreamSize", image_size.to_string(), false);
    let _ = idat_total;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_png_buffer() {
        let mut fa = FileAnalyze::new(b"NOT a PNG file at all");
        assert!(!parse_png(&mut fa));
    }
}
