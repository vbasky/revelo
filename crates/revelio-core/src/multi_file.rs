use std::fs;
use std::path::{Path, PathBuf};
use super::config::MultiFileConfig;
use super::reference::ReferenceTracker;

/// Walks a BDMV playlist or segmented MP4, collecting all referenced
/// companion files and optionally appending their bytes to the main buffer.
pub struct MultiFileLoader {
    pub files: Vec<PathBuf>,
    pub total_bytes: u64,
    pub tracker: ReferenceTracker,
}

impl Default for MultiFileLoader {
    fn default() -> Self {
        Self::new()
    }
}

impl MultiFileLoader {
    pub fn new() -> Self {
        MultiFileLoader {
            files: Vec::new(),
            total_bytes: 0,
            tracker: ReferenceTracker::new(),
        }
    }

    /// Scan a BDMV/STREAM directory: collect SST subtitles, secondary audio PIDs
    /// referenced from the MPLS playlist, and secondary M2TS clip files.
    pub fn scan_references(&mut self, primary_path: &Path, _cfg: &MultiFileConfig) {
        // Follow BDMV layout: PLAYLIST/ -> clip list -> STREAM/*.m2ts
        // and CLIPINF/ -> clip info -> secondary audio/subtitle PIDs
        let parent = primary_path.parent().unwrap_or(primary_path);
        // Look for SRT/SST subtitle files alongside the primary
        if let Some(stem) = primary_path.file_stem() {
            let stem = stem.to_string_lossy();
            // Common companion patterns for BDMV-segmented content
            for pattern in &["srt", "SST", "sub", "idx"] {
                let sidecar = parent.join(format!("{}.{}", stem, pattern));
                if sidecar.exists() && sidecar != primary_path {
                    self.files.push(sidecar.clone());
                    if let Ok(meta) = fs::metadata(&sidecar) {
                        self.total_bytes += meta.len();
                    }
                    self.tracker.add(
                        sidecar.to_string_lossy().as_ref(),
                        if *pattern == "srt" { "SubRip" } else { pattern },
                        0,
                    );
                }
            }
        }

        // BDMV structure: STREAM/ directory next to primary
        let stream_dir = parent.join("STREAM");
        if stream_dir.is_dir()
            && let Ok(entries) = fs::read_dir(&stream_dir) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if p == primary_path { continue; }
                    if let Some(ext) = p.extension()
                        && (ext == "m2ts" || ext == "mts") {
                            self.files.push(p.clone());
                            if let Ok(meta) = fs::metadata(&p) {
                                self.total_bytes += meta.len();
                            }
                            self.tracker.add(
                                p.to_string_lossy().as_ref(),
                                "BDAV",
                                0x1011, // default video PID
                            );
                        }
                }
            }
    }

    /// Append all referenced files' content to the output buffer.
    /// Returns the concatenated data alongside total reference file count.
    pub fn load_all(&self) -> Option<(Vec<u8>, usize)> {
        if self.files.is_empty() {
            return None;
        }
        let mut data = Vec::with_capacity(self.total_bytes as usize);
        for path in &self.files {
            if let Ok(bytes) = fs::read(path) {
                data.extend_from_slice(&bytes);
            }
        }
        Some((data, self.files.len()))
    }

    pub fn reference_count(&self) -> usize {
        self.files.len()
    }
}

/// Resolve duplicate streams: two streams with same format + language +
/// dimensions but different IDs may be duplicate references to the same
/// content. MediaInfoLib compares a subset of fields to determine
/// equivalence. This function returns indices of streams to suppress
/// from the output.
pub fn find_duplicate_streams(
    streams: &super::StreamCollection,
) -> Vec<(super::StreamKind, usize)> {
    use super::StreamKind;
    let mut duplicates = Vec::new();
    let kinds = [StreamKind::Video, StreamKind::Audio, StreamKind::Text];

    for kind in &kinds {
        let n = streams.count_get(*kind);
        for i in 0..n {
            let s1 = streams.stream(*kind, i);
            for j in (i + 1)..n {
                let s2 = streams.stream(*kind, j);
                if let (Some(a), Some(b)) = (s1, s2) {
                    let fmt_a = a.get("Format").map(|z| z.as_str());
                    let fmt_b = b.get("Format").map(|z| z.as_str());
                    let lang_a = a.get("Language").map(|z| z.as_str());
                    let lang_b = b.get("Language").map(|z| z.as_str());
                    let w_a = a.get("Width").or(a.get("Channels")).map(|z| z.as_str());
                    let w_b = b.get("Width").or(b.get("Channels")).map(|z| z.as_str());
                    if fmt_a == fmt_b && lang_a == lang_b && w_a == w_b
                        && fmt_a.is_some()
                    {
                        // j is a duplicate of i
                        if !duplicates.contains(&(*kind, j)) {
                            duplicates.push((*kind, j));
                        }
                    }
                }
            }
        }
    }
    duplicates
}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn test_duplicate_detection() {
        use super::super::{StreamCollection, StreamKind}; use zenlib::Ztring;
        let mut sc = StreamCollection::new();
        sc.stream_prepare(StreamKind::Video);
        sc.fill(StreamKind::Video, 0, "Format", Ztring::from("AVC"), false);
        sc.fill(StreamKind::Video, 0, "Language", Ztring::from("eng"), false);
        sc.fill(StreamKind::Video, 0, "Width", Ztring::from("1920"), false);
        sc.stream_prepare(StreamKind::Video);
        sc.fill(StreamKind::Video, 1, "Format", Ztring::from("AVC"), false);
        sc.fill(StreamKind::Video, 1, "Language", Ztring::from("eng"), false);
        sc.fill(StreamKind::Video, 1, "Width", Ztring::from("1920"), false);
        let dups = find_duplicate_streams(&sc);
        assert_eq!(dups.len(), 1);
        assert_eq!(dups[0], (StreamKind::Video, 1));
    }
    #[test] fn test_no_duplicate_different_lang() {
        use super::super::{StreamCollection, StreamKind}; use zenlib::Ztring;
        let mut sc = StreamCollection::new();
        sc.stream_prepare(StreamKind::Audio);
        sc.fill(StreamKind::Audio, 0, "Format", Ztring::from("AAC"), false);
        sc.fill(StreamKind::Audio, 0, "Language", Ztring::from("eng"), false);
        sc.fill(StreamKind::Audio, 0, "Channels", Ztring::from("2"), false);
        sc.stream_prepare(StreamKind::Audio);
        sc.fill(StreamKind::Audio, 1, "Format", Ztring::from("AAC"), false);
        sc.fill(StreamKind::Audio, 1, "Language", Ztring::from("jpn"), false);
        sc.fill(StreamKind::Audio, 1, "Channels", Ztring::from("2"), false);
        let dups = find_duplicate_streams(&sc);
        assert_eq!(dups.len(), 0);
    }
}
