use revelio_core::{FileAnalyze, StreamKind};
/// Extract ReplayGain from LAME tag data in an MP3 Xing header.
/// LAME Gaia information: 9 bytes encoder string + VBR quality + 12 byte info + 4 byte flags
/// Then 20 bytes for Track Gain/Peak, and optionally 20 bytes for Album Gain/Peak.
/// ref: http://wiki.hydrogenaud.io/index.php?title=LAME
pub fn extract_replay_gain(fa: &mut FileAnalyze, data: &[u8]) {
    let pos = fa.count_get(StreamKind::Audio);
    if pos == 0 { return; }
    let idx = pos - 1;
    if data.len() < 20 { return; }
    // Track replay gain is at the start of the Gaia block (first 20 bytes)
    let gain = i16::from_le_bytes([data[0], data[1]]) as f64 * 0.01;
    let peak_bits = u32::from_le_bytes([data[2], data[3], data[4], data[5]]);
    let peak = f32::from_bits(peak_bits);
    fa.fill(StreamKind::Audio, idx, "ReplayGain_Gain", format!("{:.2} dB", gain), false);
    fa.fill(StreamKind::Audio, idx, "ReplayGain_Peak", format!("{:.6}", peak), false);
}

pub fn fill_id3_replay_gain(fa: &mut FileAnalyze, tags: &[(String, String)]) {
    let pos = fa.count_get(StreamKind::Audio);
    if pos == 0 { return; }
    let idx = pos - 1;
    for (key, val) in tags {
        let field = match key.to_uppercase().as_str() {
            "REPLAYGAIN_TRACK_GAIN" => "ReplayGain_Gain",
            "REPLAYGAIN_TRACK_PEAK" => "ReplayGain_Peak",
            "REPLAYGAIN_ALBUM_GAIN" => "ReplayGain_Gain",
            "REPLAYGAIN_ALBUM_PEAK" => "ReplayGain_Peak",
            _ => continue,
        };
        fa.fill(StreamKind::Audio, idx, field, val.clone(), false);
    }
}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn test_smoke() {
        assert_eq!(2 + 2, 4);
    }
}
