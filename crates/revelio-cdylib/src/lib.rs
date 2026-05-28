//! C ABI shim — drop-in replacement for MediaInfoLib's `libmediainfo`.
//!
//! Function names and signatures match the upstream
//! C API exactly so existing consumers can swap the shared library
//! without source changes. Under the hood it delegates to revelio's
//! [`detect`] + [`FileAnalyze`] pipeline.

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int, c_void};

use revelio_core::{FileAnalyze, StreamCollection, StreamKind};
use revelio_dispatcher::detect;
use revelio_export::to_text;

pub struct MediaInfoHandle {
    streams: Option<StreamCollection>,
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn MediaInfo_New() -> *mut c_void {
    let handle = Box::new(MediaInfoHandle { streams: None });
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
    let mut fa = FileAnalyze::new(&bytes);
    parser(&mut fa);
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
    streams.count_get(kind) as c_int
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
    let value = stream.get(&param).map(|z| z.as_str().to_owned()).or_else(|| {
        stream.extras_iter().find(|(k, _)| *k == param.as_ref()).map(|(_, v)| v.as_str().to_owned())
    });
    match value {
        Some(v) => CString::new(v).unwrap_or_default().into_raw(),
        None => std::ptr::null_mut(),
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn MediaInfo_Option(
    handle: *mut c_void,
    option: *const c_char,
    _value: *const c_char,
) -> *mut c_char {
    if handle.is_null() || option.is_null() {
        return std::ptr::null_mut();
    }
    let opt = unsafe { CStr::from_ptr(option) }.to_string_lossy();
    let result = match opt.as_ref() {
        "Info_Version" => env!("CARGO_PKG_VERSION"),
        "Info_Parameters" => "",
        "Info_Parameters_CSV" => "",
        _ => "",
    };
    if result.is_empty() {
        return std::ptr::null_mut();
    }
    CString::new(result).unwrap_or_default().into_raw()
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
        _ => None,
    }
}
