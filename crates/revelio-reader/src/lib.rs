#![deny(unsafe_code)]

use revelio_core::{FileAnalyze, StreamKind};

pub fn parse_file_reader(fa: &mut FileAnalyze) -> bool {
    let pos = fa.stream_prepare(StreamKind::General);
    fa.set_field(StreamKind::General, pos, "Reader", "File");
    true
}

pub fn parse_directory_reader(fa: &mut FileAnalyze) -> bool {
    let pos = fa.stream_prepare(StreamKind::General);
    fa.set_field(StreamKind::General, pos, "Reader", "Directory");
    true
}

pub fn parse_http_reader(fa: &mut FileAnalyze) -> bool {
    let pos = fa.stream_prepare(StreamKind::General);
    fa.set_field(StreamKind::General, pos, "Reader", "HTTP");
    true
}

pub fn parse_mms_reader(fa: &mut FileAnalyze) -> bool {
    let pos = fa.stream_prepare(StreamKind::General);
    fa.set_field(StreamKind::General, pos, "Reader", "MMS");
    true
}
