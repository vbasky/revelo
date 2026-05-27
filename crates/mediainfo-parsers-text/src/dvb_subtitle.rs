//! DVB Subtitle parser — PES payload sync (ETSI EN 300 743).
//!
//! Layout (start of PES data field):
//!   0x20                 data_identifier
//!   0x00                 subtitle_stream_id
//!   0x0F or 0xFF         next segment sync_byte / end-of-PES marker
//!
//! The C++ `File_DvbSubtitle::Synched_Test` gates on `CC2 == 0x2000` and
//! then on the third byte being `0x0F` (segment start) or `0xFF` (end of
//! PES data field marker).

use mediainfo_core::{FileAnalyze, StreamKind};

pub fn parse_dvb_subtitle(fa: &mut FileAnalyze) -> bool {
    let head = fa.peek_raw(fa.Remain().min(3));
    let Some(h) = head else { return false };
    if h.len() < 3 {
        return false;
    }
    if h[0] != 0x20 || h[1] != 0x00 {
        return false;
    }
    // Third byte must be a segment sync (0x0F) or end-of-PES marker (0xFF);
    // anything else means this is not a DVB Subtitle PES payload.
    if h[2] != 0x0F && h[2] != 0xFF {
        return false;
    }

    fa.Stream_Prepare(StreamKind::General);
    fa.Fill(StreamKind::General, 0, "Format", "DVB Subtitle", false);

    fa.Stream_Prepare(StreamKind::Text);
    fa.Fill(StreamKind::Text, 0, "Format", "DVB Subtitle", false);

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_segment_start() {
        // data_identifier=0x20, stream_id=0x00, segment sync=0x0F
        let buf = [0x20u8, 0x00, 0x0F, 0x10, 0x00, 0x01, 0x00, 0x00];
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_dvb_subtitle(&mut fa));
    }

    #[test]
    fn accepts_end_of_pes_marker() {
        // 0xFF as third byte is the end-of-PES data field marker variant.
        let buf = [0x20u8, 0x00, 0xFF];
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_dvb_subtitle(&mut fa));
    }

    #[test]
    fn rejects_non_dvb_subtitle() {
        let mut fa = FileAnalyze::new(b"NOT DVB");
        assert!(!parse_dvb_subtitle(&mut fa));
    }
}
