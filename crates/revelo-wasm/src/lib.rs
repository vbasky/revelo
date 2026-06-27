//! WebAssembly bindings for [revelo](https://github.com/vbasky/revelo).
//!
//! Exposes metadata parsing to JavaScript via `wasm-bindgen`. The browser
//! supplies a `Uint8Array` and receives JSON back.
//!
//! # Usage (JS)
//!
//! ```js
//! import init, { parse } from "revelo-wasm";
//! await init();
//!
//! const input = document.querySelector('input[type=file]');
//! input.onchange = async (e) => {
//!     const buf = await e.target.files[0].arrayBuffer();
//!     const json = parse(new Uint8Array(buf));
//!     console.log(JSON.parse(json));
//! };
//! ```

#![deny(unsafe_code)]

use revelo::Metadata;
use serde::Serialize;
use wasm_bindgen::prelude::*;

#[derive(Serialize)]
struct Field<'a> {
    name: &'a str,
    value: &'a str,
}

#[derive(Serialize)]
struct StreamOutput<'a> {
    kind: &'a str,
    fields: Vec<Field<'a>>,
}

/// Parse a media file buffer and return metadata as JSON.
///
/// The input is a byte slice (typically from a JS `Uint8Array` created
/// from a `File.arrayBuffer()` call). Returns a JSON string with the
/// same structure as the MediaInfo JSON output.
///
/// Returns `null` if the format is not recognized.
#[wasm_bindgen]
pub fn parse(data: &[u8]) -> Option<String> {
    let meta = Metadata::from_bytes(data)?;

    let mut streams: Vec<StreamOutput> = Vec::new();

    let general_fields: Vec<Field> =
        meta.general().map(|(k, v)| Field { name: k, value: v }).collect();
    if !general_fields.is_empty() {
        streams.push(StreamOutput { kind: "General", fields: general_fields });
    }

    let video_fields: Vec<Field> = meta.video().map(|(k, v)| Field { name: k, value: v }).collect();
    if !video_fields.is_empty() {
        streams.push(StreamOutput { kind: "Video", fields: video_fields });
    }

    let audio_fields: Vec<Field> = meta.audio().map(|(k, v)| Field { name: k, value: v }).collect();
    if !audio_fields.is_empty() {
        streams.push(StreamOutput { kind: "Audio", fields: audio_fields });
    }

    let text_fields: Vec<Field> = meta.text().map(|(k, v)| Field { name: k, value: v }).collect();
    if !text_fields.is_empty() {
        streams.push(StreamOutput { kind: "Text", fields: text_fields });
    }

    let image_fields: Vec<Field> = meta.image().map(|(k, v)| Field { name: k, value: v }).collect();
    if !image_fields.is_empty() {
        streams.push(StreamOutput { kind: "Image", fields: image_fields });
    }

    let exif_fields: Vec<Field> = meta.exif().map(|(k, v)| Field { name: k, value: v }).collect();
    if !exif_fields.is_empty() {
        streams.push(StreamOutput { kind: "EXIF", fields: exif_fields });
    }

    serde_json::to_string(&streams).ok()
}

/// Return the revelo version string.
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}
