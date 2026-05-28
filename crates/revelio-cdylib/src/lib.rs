use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int, c_void};

use revelio_core::{FileAnalyze, StreamCollection, StreamKind};
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
        unsafe { drop(Box::from_raw(handle as *mut MediaInfoHandle)); }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn MediaInfo_Open(
    handle: *mut c_void,
    filename: *const c_char,
) -> c_int {
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

    let parsers: [fn(&mut FileAnalyze) -> bool; 114] = build_parser_table();
    for parser in parsers {
        let mut fa = FileAnalyze::new(&bytes);
        if parser(&mut fa) {
            handle.streams = Some(fa.streams().clone());
            return 1;
        }
    }
    0
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn MediaInfo_Close(handle: *mut c_void) {
    if !handle.is_null() {
        let handle = unsafe { &mut *(handle as *mut MediaInfoHandle) };
        handle.streams = None;
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn MediaInfo_Inform(
    handle: *mut c_void,
    _reserved: u32,
) -> *mut c_char {
    if handle.is_null() {
        return std::ptr::null_mut();
    }
    let handle: &MediaInfoHandle = unsafe { &*(handle as *const MediaInfoHandle) };
    let streams = match &handle.streams {
        Some(s) => s,
        None => return std::ptr::null_mut(),
    };
    let text = to_text(streams);
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
    streams.Count_Get(kind) as c_int
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
    let value = stream
        .get(&param)
        .map(|z| z.as_str().to_owned())
        .or_else(|| {
            stream
                .extras_iter()
                .find(|(k, _)| *k == param.as_ref())
                .map(|(_, v)| v.as_str().to_owned())
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

fn build_parser_table() -> [fn(&mut FileAnalyze) -> bool; 114] {
    use revelio_parsers_audio::*;
    use revelio_parsers_container::*;
    use revelio_parsers_image::*;
    use revelio_parsers_text::*;
    use revelio_parsers_video::*;

    [
        parse_wav, parse_avi, parse_cdxa, parse_amv, parse_webp, parse_aiff, parse_flac,
        parse_dsdiff, parse_caf, parse_mp4, parse_mkv, parse_ogg, parse_mpeg_ts, parse_mpeg_ps,
        parse_swf, parse_skm, parse_dpg, parse_hds_f4m, parse_hls, parse_dash_mpd, parse_dcp_am,
        parse_dcp_cpl, parse_ibi, parse_dxw, parse_aaf, parse_mxf, parse_bdmv, parse_dvdv,
        parse_dv_dif, parse_flv, parse_lxf, parse_nut, parse_wm, parse_wtv, parse_rm, parse_ivf,
        parse_ism, parse_mi_xml, parse_p2_clip, parse_xdcam_clip, parse_sequence_info, parse_ptx,
        parse_nsv, parse_pmp, parse_gxf, parse_cdp, parse_pgs, parse_dvb_subtitle,
        parse_arib_std_b24_b37, parse_kate, parse_cmml, parse_ttml, parse_n19, parse_sub_rip,
        parse_other_text, parse_dsf, parse_png, parse_jpeg, parse_bmp, parse_gif, parse_tiff,
        parse_ico, parse_psd, parse_dpx, parse_dds, parse_exr, parse_bpg, parse_pcx,
        parse_arriraw, parse_amiga_icon, parse_y4m, parse_vc1, parse_mpeg2, parse_av1, parse_avc,
        parse_hevc, parse_vp8, parse_vp9, parse_theora, parse_ac3, parse_ac4, parse_dts,
        parse_dts_uhd, parse_aac_adts, parse_iab, parse_iamf, parse_als, parse_ape, parse_au,
        parse_amr, parse_speex, parse_mpc, parse_la, parse_tak, parse_tta, parse_wvpk,
        parse_twin_vq, parse_extended_module, parse_dat, parse_rkau, parse_aptx100, parse_open_mg,
        parse_midi, parse_module, parse_impulse_tracker, parse_scream_tracker3, parse_mp3,
        parse_tga, parse_gain_map, parse_rle, parse_adpcm, parse_eia608, parse_eia708, parse_vbi,
    ]
}
