use revelio_core::{FileAnalyze, StreamKind};

pub fn parse_scc(fa: &mut FileAnalyze) -> bool {
    let buf = match fa.peek_raw(fa.Remain() as usize) {
        Some(b) => b,
        None => return false,
    };

    if buf.len() < 18 {
        return false;
    }

    let magic = std::str::from_utf8(&buf[0..18]).unwrap_or("");
    if magic != "Scenarist_SCC V1.0" {
        return false;
    }

    let pos = fa.Stream_Prepare(StreamKind::Text);
    fa.Fill(StreamKind::Text, pos, "Format", "SCC", false);
    fa.Fill(StreamKind::Text, pos, "MuxingMode", "SCC", false);

    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use revelio_core::FileAnalyze;

    #[test]
    fn scc_detects_magic() {
        let buf = b"Scenarist_SCC V1.0\r\n00:00:00:00\t9420 9420".to_vec();
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_scc(&mut fa));
        assert_eq!(
            fa.Retrieve(StreamKind::Text, 0, "Format").map(|z| z.as_str().to_owned()),
            Some("SCC".into())
        );
    }

    #[test]
    fn scc_rejects_garbage() {
        let buf = b"Not an SCC file".to_vec();
        let mut fa = FileAnalyze::new(&buf);
        assert!(!parse_scc(&mut fa));
    }
}
