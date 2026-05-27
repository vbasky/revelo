//! JPEG parser — segment-based image format. Mirrors the subset of
//! MediaInfoLib's `File_Jpeg.cpp` for plain JPEG/JFIF files.
//!
//! Layout: 0xFFD8 (SOI) + segments + 0xFFD9 (EOI).
//! Each non-fixed-length segment is `FF [marker:u8] [length:u16 BE]
//! [length-2 bytes payload]`.
//!
//! SOFn markers (Start Of Frame) carry the image geometry:
//!   FFC0 baseline DCT, FFC1 extended sequential, FFC2 progressive,
//!   FFC3 lossless, FFC5..FFC7 differential, FFC9..FFCB arithmetic,
//!   FFCD..FFCF differential arithmetic.
//! SOF payload: precision(1) + height(2 BE) + width(2 BE) + components(1)
//!   + per-component (id, h_v_sampling, qt_index) * components.

use mediainfo_core::{FileAnalyze, StreamKind};

pub fn parse_jpeg(fa: &mut FileAnalyze) -> bool {
    let head = fa.peek_raw(2);
    let Some(h) = head else { return false };
    if h != [0xFF, 0xD8] {
        return false;
    }

    let file_size = fa.Remain();
    fa.Skip_Hexa(2, "SOI");

    let mut width: u16 = 0;
    let mut height: u16 = 0;
    let mut precision: u8 = 0;
    let mut components: u8 = 0;
    let mut sampling: Vec<(u8, u8)> = Vec::new();
    let mut found_sof = false;

    while fa.Remain() >= 4 {
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
            // SOS — entropy-coded data follows; stop scanning markers.
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
        if segment_len < 2 || fa.Remain() < segment_len {
            break;
        }
        fa.Skip_Hexa(2, "segment_length");
        let payload_size = segment_len - 2;

        // SOFn markers (geometry). Excludes FFC4 (DHT), FFC8 (JPG), FFCC (DAC).
        let is_sof = matches!(marker,
            0xC0 | 0xC1 | 0xC2 | 0xC3
            | 0xC5 | 0xC6 | 0xC7
            | 0xC9 | 0xCA | 0xCB
            | 0xCD | 0xCE | 0xCF);
        if is_sof && !found_sof && payload_size >= 6 {
            let payload = fa.read_raw(payload_size).to_vec();
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
            found_sof = true;
        } else {
            fa.Skip_Hexa(payload_size, "segment");
        }
    }

    if !found_sof {
        return false;
    }

    fill_streams(fa, file_size, width, height, precision, components, &sampling);
    true
}

fn fill_streams(
    fa: &mut FileAnalyze,
    file_size: usize,
    width: u16,
    height: u16,
    precision: u8,
    components: u8,
    sampling: &[(u8, u8)],
) {
    fa.Stream_Prepare(StreamKind::General);
    fa.Fill(StreamKind::General, 0, "Format", "JPEG", false);
    fa.Fill(StreamKind::General, 0, "ImageCount", "1", false);

    fa.Stream_Prepare(StreamKind::Image);
    fa.Fill(StreamKind::Image, 0, "Format", "JPEG", false);
    fa.Fill(StreamKind::Image, 0, "Width", width.to_string(), false);
    fa.Fill(StreamKind::Image, 0, "Height", height.to_string(), false);
    let color_space = match components {
        1 => "Y",
        3 => "YUV",
        4 => "CMYK",
        _ => "Unknown",
    };
    fa.Fill(StreamKind::Image, 0, "ColorSpace", color_space, false);
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
            fa.Fill(StreamKind::Image, 0, "ChromaSubsampling", s, false);
        }
    }
    fa.Fill(StreamKind::Image, 0, "BitDepth", precision.to_string(), false);
    fa.Fill(StreamKind::Image, 0, "Compression_Mode", "Lossy", false);
    // StreamSize and General.StreamSize/Comment require segment-size
    // accounting and APP/COM segment parsing — deferred. Filling
    // Image.StreamSize with FileSize here is a placeholder approximation
    // until that arrives; oracle's value subtracts the metadata overhead.
    let _ = file_size;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_jpeg_buffer() {
        let mut fa = FileAnalyze::new(b"NOT a JPEG file");
        assert!(!parse_jpeg(&mut fa));
    }
}
