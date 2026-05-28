use revelio_core::{FileAnalyze, StreamKind};

pub fn parse_file_reader(fa: &mut FileAnalyze) -> bool {
    let pos = fa.Stream_Prepare(StreamKind::General);
    fa.Fill(StreamKind::General, pos, "Reader", "File", false);
    true
}

pub fn parse_directory_reader(fa: &mut FileAnalyze) -> bool {
    let pos = fa.Stream_Prepare(StreamKind::General);
    fa.Fill(StreamKind::General, pos, "Reader", "Directory", false);
    true
}

pub fn parse_http_reader(fa: &mut FileAnalyze) -> bool {
    let pos = fa.Stream_Prepare(StreamKind::General);
    fa.Fill(StreamKind::General, pos, "Reader", "HTTP", false);
    true
}

pub fn parse_mms_reader(fa: &mut FileAnalyze) -> bool {
    let pos = fa.Stream_Prepare(StreamKind::General);
    fa.Fill(StreamKind::General, pos, "Reader", "MMS", false);
    true
}
