use crate::events::{DemuxLevel, TraceFormat};

/// Global configuration mirroring MediaInfoLib's MediaInfo_Config.
/// Drives Demux/Trace level activation, output format, and I/O options.
#[derive(Debug, Clone)]
pub struct MediaConfig {
    pub demux_level: DemuxLevel,
    pub trace_level: u32,
    pub trace_format: TraceFormat,
    pub trace_activated: bool,
    pub trace_time_section_only_first: bool,
    pub file_path: Option<String>,
    pub parse_speed: f64,
    pub show_complete: bool,
    pub show_complete_helper: bool,
    pub multi_file: MultiFileConfig,
}

#[derive(Debug, Clone, Default)]
pub struct MultiFileConfig {
    pub enabled: bool,
    pub max_files: usize,
    pub follow_symlinks: bool,
    pub sort_by_size: bool,
}

impl Default for MediaConfig {
    fn default() -> Self {
        MediaConfig {
            demux_level: DemuxLevel::FRAME,
            trace_level: 0,
            trace_format: TraceFormat::Tree,
            trace_activated: false,
            trace_time_section_only_first: false,
            file_path: None,
            parse_speed: 1.0,
            show_complete: false,
            show_complete_helper: false,
            multi_file: MultiFileConfig::default(),
        }
    }
}

impl MediaConfig {
    pub fn set_demux(&mut self, level: &str) {
        self.demux_level = match level.to_lowercase().as_str() {
            "none" | "0" => DemuxLevel(0),
            "frame" | "1" => DemuxLevel::FRAME,
            "container" | "2" => DemuxLevel::CONTAINER,
            "elementary" | "4" => DemuxLevel::ELEMENTARY,
            "ancillary" | "8" => DemuxLevel::ANCILLARY,
            "all" | "15" => DemuxLevel(255),
            _ => DemuxLevel::FRAME,
        };
    }

    pub fn set_trace_format(&mut self, format: &str) {
        self.trace_format = match format.to_lowercase().as_str() {
            "tree" => TraceFormat::Tree,
            "csv" => TraceFormat::Csv,
            "xml" => TraceFormat::Xml,
            "microxml" | "micro_xml" => TraceFormat::MicroXml,
            _ => TraceFormat::Tree,
        };
    }

    pub fn set_trace_level(&mut self, level: &str) {
        self.trace_level = level.parse().unwrap_or(0);
        self.trace_activated = self.trace_level > 0;
    }

    pub fn set_option(&mut self, key: &str, value: &str) -> bool {
        match key.to_lowercase().as_str() {
            "demux" => { self.set_demux(value); true }
            "trace_level" => { self.set_trace_level(value); true }
            "trace_format" => { self.set_trace_format(value); true }
            "trace_time_section_only_first" => {
                self.trace_time_section_only_first = value == "1";
                true
            }
            "parse_speed" => {
                self.parse_speed = value.parse().unwrap_or(1.0);
                true
            }
            "show_complete" => {
                self.show_complete = value == "1";
                true
            }
            "multi_file" => {
                self.multi_file.enabled = value == "1";
                true
            }
            "multi_file_max" => {
                self.multi_file.max_files = value.parse().unwrap_or(0);
                true
            }
            _ => false,
        }
    }

    /// Whether demux is active at the given level.
    pub fn demux_active(&self, level: DemuxLevel) -> bool {
        self.demux_level.contains(level)
    }
}

#[cfg(test)] mod tests {
    use super::*;
    #[test] fn test_default_config() {
        let cfg = MediaConfig::default();
        assert_eq!(cfg.demux_level, DemuxLevel::FRAME);
        assert!(!cfg.trace_activated);
        assert!(!cfg.multi_file.enabled);
    }
    #[test] fn test_demux_container() {
        let mut cfg = MediaConfig::default();
        cfg.set_demux("container");
        assert!(cfg.demux_active(DemuxLevel::CONTAINER));
        assert!(!cfg.demux_active(DemuxLevel::ELEMENTARY));
    }
    #[test] fn test_demux_all() {
        let mut cfg = MediaConfig::default();
        cfg.set_demux("all");
        assert!(cfg.demux_active(DemuxLevel::FRAME));
        assert!(cfg.demux_active(DemuxLevel::ELEMENTARY));
    }
    #[test] fn test_trace_format() {
        let mut cfg = MediaConfig::default();
        cfg.set_trace_format("csv");
        assert!(matches!(cfg.trace_format, TraceFormat::Csv));
    }
    #[test] fn test_set_option_unknown() {
        let mut cfg = MediaConfig::default();
        assert!(!cfg.set_option("nope", "value"));
    }
}
