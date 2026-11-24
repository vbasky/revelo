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
//! | `MediaInfo_Inform` | Return the report. The handle owns the buffer. |
//! | `MediaInfo_Get` | Retrieve a single field value |
//! | `MediaInfo_Count_Get` | Count streams of a given kind |
//! | `MediaInfo_Option` | Set a runtime option (`Demux`, `TraceLevel`, …) |
//!
//! # C example
//!
//! ```c
//! void *h = MediaInfo_New();
//! if (MediaInfo_Open(h, "video.mp4")) {
//!     const char *report = MediaInfo_Inform(h, 0);
//!     printf("%s\n", report);
//!     /* The handle owns `report` until the next Get/Inform/Option call. */
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

use revelo_core::computed_fields::fill_computed_fields;
use revelo_core::config::MediaConfig;
use revelo_core::{
    FileAnalyze, FileLevelInfo, StreamCollection, StreamKind, fill_file_level_fields,
};
use revelo_dispatcher::detect;
use revelo_export::{to_html, to_json, to_text, to_xml};
use revelo_parsers_tag::parse_tags;

pub struct MediaInfoHandle {
    config: MediaConfig,
    streams: Option<StreamCollection>,
    /// Last string handed to C. The handle owns it; the next call replaces it.
    out: CString,
    /// `MediaInfo_Option("Output", ...)` value: Text, XML, JSON, or HTML.
    output: String,
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn MediaInfo_New() -> *mut c_void {
    let handle = Box::new(MediaInfoHandle {
        config: MediaConfig::default(),
        streams: None,
        out: CString::new("").unwrap(),
        output: String::new(),
    });
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
    let config = handle.config.clone();
    // Panic firewall: a parser panicking while unwinding across this
    // `extern "C"` boundary is undefined behavior. Catch it here and report
    // a clean open failure instead. `AssertUnwindSafe` is sound because the
    // captured state (`bytes`, `config`) is local and dropped on unwind.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut fa = FileAnalyze::new(bytes.as_slice());
        fa.set_config(config);
        parser(&mut fa);
        parse_tags(&mut fa);
        let meta = std::fs::metadata(path).ok();
        let info = file_level_from(path, meta.as_ref(), bytes.len() as u64);
        fill_file_level_fields(&mut fa, &info);
        fill_computed_fields(fa.streams_mut());
        fa.streams().clone()
    }));
    match result {
        Ok(streams) => {
            handle.streams = Some(streams);
            1
        }
        Err(_) => 0,
    }
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
    let handle = unsafe { &mut *(handle as *mut MediaInfoHandle) };
    let format = handle.output.clone();
    // Panic firewall around the formatter — unwinding across `extern "C"`
    // is UB. Report null (no output) on panic instead.
    let text = if let Some(streams) = handle.streams.as_ref() {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            inform_text(streams, &format)
        })) {
            Ok(t) => t,
            Err(_) => return std::ptr::null_mut(),
        }
    } else {
        String::new()
    };
    store_string(handle, &text)
}

fn inform_text(streams: &StreamCollection, format: &str) -> String {
    match format.to_ascii_lowercase().as_str() {
        "xml" => to_xml(streams, ""),
        "json" => to_json(streams, ""),
        "html" => to_html(streams, ""),
        _ => to_text(streams, ""),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn MediaInfo_Count_Get(
    handle: *mut c_void,
    stream_kind: c_int,
    stream_number: c_int,
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
    // libmediainfo: stream number `(size_t)-1` counts streams of this kind.
    // Any other number counts the fields of that one stream.
    if stream_number < 0 {
        streams.stream_count(kind) as c_int
    } else {
        streams
            .stream(kind, stream_number as usize)
            .map(|stream| stream.count() as c_int)
            .unwrap_or(0)
    }
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
    let handle = unsafe { &mut *(handle as *mut MediaInfoHandle) };
    let kind = match c_int_to_stream_kind(stream_kind) {
        Some(k) => k,
        None => return store_string(handle, ""),
    };
    let param = unsafe { CStr::from_ptr(parameter) }.to_string_lossy();
    let value = handle
        .streams
        .as_ref()
        .and_then(|streams| streams.stream(kind, stream_number))
        .and_then(|stream| lookup_param(stream, &param))
        .unwrap_or_default();
    store_string(handle, &value)
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
    if opt.eq_ignore_ascii_case("Output") {
        handle.output = val;
        return store_string(handle, "");
    }
    if opt.starts_with("Info_") {
        let result = match opt.as_ref() {
            "Info_Version" => env!("CARGO_PKG_VERSION"),
            // The Complete=1 parameter catalog is not reproduced here.
            "Info_Parameters" | "Info_Parameters_CSV" => "",
            _ => "",
        };
        return store_string(handle, result);
    }
    // Unknown options still warn inside `set_option`. libmediainfo returns
    // an empty string for both known and unknown options.
    let _recognised = handle.config.set_option(&opt, &val);
    store_string(handle, "")
}

/// Store `text` on the handle and return a pointer into that storage.
/// The pointer stays valid until the next call that stores a string, or
/// until `MediaInfo_Delete`.
fn store_string(handle: &mut MediaInfoHandle, text: &str) -> *mut c_char {
    let cleaned: String = text.chars().filter(|c| *c != '\0').collect();
    handle.out = CString::new(cleaned).unwrap_or_default();
    handle.out.as_ptr() as *mut c_char
}

fn file_level_from<'a>(
    path: &'a str,
    meta: Option<&std::fs::Metadata>,
    fallback_size: u64,
) -> FileLevelInfo<'a> {
    let extension = std::path::Path::new(path).extension().and_then(|ext| ext.to_str());
    let file_size = meta.map(|m| m.len()).unwrap_or(fallback_size);
    let modified_unix_secs = meta
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64);
    FileLevelInfo {
        file_size,
        extension,
        modified_unix_secs,
        local_offset_secs: local_offset_seconds(),
    }
}

fn local_offset_seconds() -> i64 {
    #[cfg(unix)]
    if let Some(offset) = local_offset_seconds_unix() {
        return offset;
    }
    0
}

#[cfg(unix)]
fn local_offset_seconds_unix() -> Option<i64> {
    use std::os::raw::{c_int, c_long};

    type TimeT = i64;

    #[repr(C)]
    struct Tm {
        tm_sec: c_int,
        tm_min: c_int,
        tm_hour: c_int,
        tm_mday: c_int,
        tm_mon: c_int,
        tm_year: c_int,
        tm_wday: c_int,
        tm_yday: c_int,
        tm_isdst: c_int,
        tm_gmtoff: c_long,
        tm_zone: *const c_char,
    }

    unsafe extern "C" {
        fn time(tloc: *mut TimeT) -> TimeT;
        fn localtime_r(timep: *const TimeT, result: *mut Tm) -> *mut Tm;
    }

    // SAFETY: `time(null)` returns the current timestamp. `localtime_r`
    // writes into the stack `Tm`, which uses the Unix C layout.
    unsafe {
        let now = time(std::ptr::null_mut());
        if now == -1 {
            return None;
        }
        let mut tm = std::mem::zeroed::<Tm>();
        if localtime_r(&now, &mut tm).is_null() {
            return None;
        }
        Some(tm.tm_gmtoff as i64)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    #[test]
    fn open_reports_file_size() {
        let mut bytes = 16u32.to_be_bytes().to_vec();
        bytes.extend_from_slice(b"ftyp");
        bytes.extend_from_slice(b"isom");
        bytes.extend_from_slice(&0u32.to_be_bytes());
        let path = std::env::temp_dir().join(format!("revelo-cdylib-{}.mp4", std::process::id()));
        std::fs::write(&path, &bytes).unwrap();
        let c_path = CString::new(path.to_str().unwrap()).unwrap();
        unsafe {
            let handle = MediaInfo_New();
            assert_eq!(MediaInfo_Open(handle, c_path.as_ptr()), 1);
            let param = CString::new("FileSize").unwrap();
            let value = MediaInfo_Get(handle, 0, 0, param.as_ptr(), 0, 0);
            assert!(!value.is_null());
            let text = CStr::from_ptr(value).to_string_lossy().into_owned();
            assert_eq!(text, bytes.len().to_string());

            let missing = CString::new("NoSuchField").unwrap();
            let empty = MediaInfo_Get(handle, 0, 0, missing.as_ptr(), 0, 0);
            assert!(!empty.is_null());
            assert_eq!(CStr::from_ptr(empty).to_bytes(), b"");

            assert_eq!(MediaInfo_Count_Get(handle, 0, -1), 1);
            assert!(MediaInfo_Count_Get(handle, 0, 0) > 0);

            let unknown = CString::new("nope").unwrap();
            let warned = MediaInfo_Option(handle, unknown.as_ptr(), std::ptr::null());
            assert!(!warned.is_null());
            assert_eq!(CStr::from_ptr(warned).to_bytes(), b"");

            let output = CString::new("Output").unwrap();
            let json = CString::new("JSON").unwrap();
            let _ = MediaInfo_Option(handle, output.as_ptr(), json.as_ptr());
            let report = MediaInfo_Inform(handle, 0);
            assert!(!report.is_null());
            let report = CStr::from_ptr(report).to_string_lossy();
            assert!(report.contains("\"media\""), "{report}");

            MediaInfo_Close(handle);
            MediaInfo_Delete(handle);
        }
        let _ = std::fs::remove_file(path);
    }
}
