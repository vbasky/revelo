//! Drop-in replacement for `libmediainfo` — exposes the `MediaInfo_*` C ABI
//! backed entirely by revelo's pure-Rust engine.
//!
//! Function names and signatures match the upstream MediaInfoLib C API exactly,
//! so existing C/C++/Python/FFI consumers can swap the shared library without
//! source changes. Under the hood every call delegates to
//! `revelo-dispatcher`'s [`detect`] + `revelo-core`'s [`FileAnalyze`] pipeline.
//!
//! # Exported symbols
//!
//! | Symbol | Description |
//! |--------|-------------|
//! | `MediaInfo_New` | Allocate a handle |
//! | `MediaInfo_Delete` | Free a handle |
//! | `MediaInfo_Open` | Read and parse a file (returns 1 on success) |
//! | `MediaInfo_Close` | Discard parsed state |
//! | `MediaInfo_Inform` | Return the text report as a C string (caller frees) |
//! | `MediaInfo_Get` | Retrieve a single field value |
//! | `MediaInfo_Count_Get` | Count streams of a given kind |
//! | `MediaInfo_Option` | Set a runtime option (`Demux`, `TraceLevel`, …) |
//!
//! # C example
//!
//! ```c
//! void *h = MediaInfo_New();
//! if (MediaInfo_Open(h, "video.mp4")) {
//!     char *report = MediaInfo_Inform(h, 0);
//!     printf("%s\n", report);
//!     free(report);
//!     MediaInfo_Close(h);
//! }
//! MediaInfo_Delete(h);
//! ```
//!
//! # Stream kind values
//!
//! `MediaInfo_Get` / `MediaInfo_Count_Get` accept `stream_kind` as an integer:
//! `0` = General, `1` = Video, `2` = Audio, `3` = Text, `4` = Other,
//! `5` = Image, `6` = Menu (MediaInfo-compatible). revelo extends this with
//! `7` = Exif, `8` = Iptc, `9` = Xmp, `10` = Icc, `11` = C2pa,
//! `12` = MakerNotes.
//!
//! # Field aliases
//!
//! `MediaInfo_Get` resolves common cross-version aliases transparently
//! (e.g. `SampleRate` → `SamplingRate`, `Codec` → `CodecID`,
//! `Resolution` → `BitDepth`). See `resolve_alias` in the source for the
//! full list.

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int, c_void};

use revelo_core::config::MediaConfig;
use revelo_core::{FileAnalyze, StreamCollection, StreamKind};
use revelo_dispatcher::detect;
use revelo_export::to_text;
use revelo_parsers_tag::parse_tags;

pub struct MediaInfoHandle {
    config: MediaConfig,
    streams: Option<StreamCollection>,
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn MediaInfo_New() -> *mut c_void {
    let handle = Box::new(MediaInfoHandle { config: MediaConfig::default(), streams: None });
    Box::into_raw(handle) as *mut c_void
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn MediaInfo_Delete(handle: *mut c_void) {
    if !handle.is_null() {
        unsafe {
            drop(Box::from_raw(handle as *mut MediaInfoHandle));
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn MediaInfo_Open(handle: *mut c_void, filename: *const c_char) -> c_int {
    if handle.is_null() || filename.is_null() {
        return 0;
    }
    let handle = unsafe { &mut *(handle as *mut MediaInfoHandle) };
    let path = match unsafe { CStr::from_ptr(filename) }.to_str() {
        Ok(s) => s,
        Err(_) => return 0,
    };
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(_) => return 0,
    };

    let Some(parser) = detect(&bytes) else {
        return 0;
    };
    let mut fa = FileAnalyze::new(bytes.as_slice());
    fa.set_config(handle.config.clone());
    parser(&mut fa);
    parse_tags(&mut fa);
    handle.streams = Some(fa.streams().clone());
    1
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn MediaInfo_Close(handle: *mut c_void) {
    if !handle.is_null() {
        let handle = unsafe { &mut *(handle as *mut MediaInfoHandle) };
        handle.streams = None;
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn MediaInfo_Inform(handle: *mut c_void, _reserved: u32) -> *mut c_char {
    if handle.is_null() {
        return std::ptr::null_mut();
    }
    let handle: &MediaInfoHandle = unsafe { &*(handle as *const MediaInfoHandle) };
    let streams = match &handle.streams {
        Some(s) => s,
        None => return std::ptr::null_mut(),
    };
    let text = to_text(streams, "");
    match CString::new(text) {
        Ok(c) => c.into_raw(),
        Err(_) => std::ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn MediaInfo_Count_Get(
    handle: *mut c_void,
    stream_kind: c_int,
    _stream_number: c_int,
) -> c_int {
    if handle.is_null() {
        return 0;
    }
    let handle: &MediaInfoHandle = unsafe { &*(handle as *const MediaInfoHandle) };
    let streams = match &handle.streams {
        Some(s) => s,
        None => return 0,
    };
    let kind = match c_int_to_stream_kind(stream_kind) {
        Some(k) => k,
        None => return 0,
    };
    streams.stream_count(kind) as c_int
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn MediaInfo_Get(
    handle: *mut c_void,
    stream_kind: c_int,
    stream_number: usize,
    parameter: *const c_char,
    _info_kind: c_int,
    _search_kind: c_int,
) -> *mut c_char {
    if handle.is_null() || parameter.is_null() {
        return std::ptr::null_mut();
    }
    let handle: &MediaInfoHandle = unsafe { &*(handle as *const MediaInfoHandle) };
    let streams = match &handle.streams {
        Some(s) => s,
        None => return std::ptr::null_mut(),
    };
    let kind = match c_int_to_stream_kind(stream_kind) {
        Some(k) => k,
        None => return std::ptr::null_mut(),
    };
    let param = unsafe { CStr::from_ptr(parameter) }.to_string_lossy();
    let stream = match streams.stream(kind, stream_number) {
        Some(s) => s,
        None => return std::ptr::null_mut(),
    };
    let value = lookup_param(stream, &param);
    match value {
        Some(v) => CString::new(v).unwrap_or_default().into_raw(),
        None => std::ptr::null_mut(),
    }
}

fn lookup_param(stream: &revelo_core::Stream, param: &str) -> Option<String> {
    if let Some(v) = stream.get(param) {
        return Some(v.as_str().to_owned());
    }
    if let Some(v) = stream.extras_iter().find(|(k, _)| *k == param) {
        return Some(v.1.as_str().to_owned());
    }
    let alias = resolve_alias(param);
    if alias != param {
        if let Some(v) = stream.get(alias) {
            return Some(v.as_str().to_owned());
        }
        if let Some(v) = stream.extras_iter().find(|(k, _)| *k == alias) {
            return Some(v.1.as_str().to_owned());
        }
    }
    None
}

fn resolve_alias(param: &str) -> &str {
    match param {
        "WritingApplication" | "Writing_Application" => "Encoded_Application",
        "WritingLibrary" | "Writing_Library" => "Encoded_Library",
        "MimeType" => "InternetMediaType",
        "ColorPrimaries" | "Color_primaries" => "colour_primaries",
        "TransferCharacteristics" | "Transfer_Characteristics" => "transfer_characteristics",
        "MatrixCoefficients" | "Matrix_Coefficients" => "matrix_coefficients",
        "ColorRange" | "Range" => "colour_range",
        "Coded_Width" | "Stored_Width" => "Sampled_Width",
        "Coded_Height" | "Stored_Height" => "Sampled_Height",
        "Codec_Profile" => "Format_Profile",
        "Codec_Level" => "Format_Level",
        "Resolution" => "BitDepth",
        "Channel(s)" | "Channel_s_" => "Channels",
        "Channel_Layout" => "ChannelLayout",
        "PixelFormat" | "Colorimetry" => "Format_Settings_CABAC",
        "FrameRate_Original" | "FrameRate_Nominal" => "FrameRate",
        "BitRate_Nominal" => "BitRate",
        "Video_Delay" | "Video0_Delay" => "Delay",
        "Format_Settings/String" | "Format_Settings_Encoding" => "Format_Settings",
        "Codec" => "CodecID",
        "Track" => "Title",
        "Source_FrameCount" => "FrameCount",
        "Sampling_Rate" => "SamplingRate",
        "SampleRate" => "SamplingRate",
        "ImageLength" => "ImageHeight",
        _ => param,
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn MediaInfo_Option(
    handle: *mut c_void,
    option: *const c_char,
    value: *const c_char,
) -> *mut c_char {
    if handle.is_null() || option.is_null() {
        return std::ptr::null_mut();
    }
    let handle = unsafe { &mut *(handle as *mut MediaInfoHandle) };
    let opt = unsafe { CStr::from_ptr(option) }.to_string_lossy();
    let val = if value.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(value) }.to_string_lossy().to_string()
    };
    if opt.starts_with("Info_") {
        let result = match opt.as_ref() {
            "Info_Version" => env!("CARGO_PKG_VERSION"),
            "Info_Parameters" => "",
            "Info_Parameters_CSV" => "",
            _ => "",
        };
        if result.is_empty() {
            return std::ptr::null_mut();
        }
        return CString::new(result).unwrap_or_default().into_raw();
    }
    if handle.config.set_option(&opt, &val) {
        CString::new("").unwrap_or_default().into_raw()
    } else {
        std::ptr::null_mut()
    }
}

fn c_int_to_stream_kind(val: c_int) -> Option<StreamKind> {
    match val {
        0 => Some(StreamKind::General),
        1 => Some(StreamKind::Video),
        2 => Some(StreamKind::Audio),
        3 => Some(StreamKind::Text),
        4 => Some(StreamKind::Other),
        5 => Some(StreamKind::Image),
        6 => Some(StreamKind::Menu),
        7 => Some(StreamKind::Exif),
        8 => Some(StreamKind::Iptc),
        9 => Some(StreamKind::Xmp),
        10 => Some(StreamKind::Icc),
        11 => Some(StreamKind::C2pa),
        12 => Some(StreamKind::MakerNotes),
        _ => None,
    }
}
