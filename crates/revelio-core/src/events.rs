use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DemuxLevel(pub u8);

impl DemuxLevel {
    pub const FRAME: DemuxLevel = DemuxLevel(1);
    pub const CONTAINER: DemuxLevel = DemuxLevel(2);
    pub const ELEMENTARY: DemuxLevel = DemuxLevel(4);
    pub const ANCILLARY: DemuxLevel = DemuxLevel(8);

    pub fn contains(self, level: DemuxLevel) -> bool {
        self.0 & level.0 != 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentType {
    MainStream,
    SubStream,
    Header,
    Synchro,
}

#[derive(Debug, Clone)]
pub struct DemuxEvent {
    pub content_type: ContentType,
    pub stream_ids: Vec<u16>,
    pub pts: Option<u64>,
    pub dts: Option<u64>,
    pub duration: Option<u64>,
    pub offset_stream: u64,
    pub offset_content: u64,
    pub random_access: bool,
}

pub struct DemuxState {
    pub active_level: DemuxLevel,
    pub events: Vec<DemuxEvent>,
    pub frame_number: u64,
    pub field_count: u64,
    pub total_bytes: u64,
    pub first_dts: Option<u64>,
    pub unpacketize: bool,
}

impl DemuxState {
    pub fn new(level: DemuxLevel) -> Self {
        DemuxState {
            active_level: level,
            events: Vec::new(),
            frame_number: 0,
            field_count: 0,
            total_bytes: 0,
            first_dts: None,
            unpacketize: false,
        }
    }

    pub fn emit(&mut self, event: DemuxEvent) {
        if self.first_dts.is_none() {
            self.first_dts = event.dts;
        }
        self.total_bytes += event.offset_content;
        self.frame_number += 1;
        self.events.push(event);
    }

    pub fn event_count(&self) -> usize { self.events.len() }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraceFormat {
    Tree,
    Csv,
    Xml,
    MicroXml,
}

#[derive(Debug, Clone)]
pub struct TraceNode {
    pub name: String,
    pub file_offset: u64,
    pub size: u64,
    pub value: Option<String>,
    pub infos: Vec<(String, String)>,
    pub children: Vec<TraceNode>,
}

impl TraceNode {
    pub fn new(name: &str, file_offset: u64) -> Self {
        TraceNode {
            name: name.to_string(),
            file_offset,
            size: 0,
            value: None,
            infos: Vec::new(),
            children: Vec::new(),
        }
    }

    pub fn set_value(&mut self, val: &str) { self.value = Some(val.to_string()); }
    pub fn set_size(&mut self, size: u64) { self.size = size; }
    pub fn add_info(&mut self, key: &str, val: &str) {
        self.infos.push((key.to_string(), val.to_string()));
    }
    pub fn add_child(&mut self, child: TraceNode) { self.children.push(child); }

    pub fn render(&self, format: TraceFormat, depth: usize) -> String {
        match format {
            TraceFormat::Tree => self.render_tree(depth),
            TraceFormat::Csv => self.render_csv(depth),
            TraceFormat::Xml => self.render_xml(depth),
            TraceFormat::MicroXml => self.render_micro_xml(),
        }
    }

    fn render_tree(&self, depth: usize) -> String {
        let indent = "  ".repeat(depth);
        let mut s = format!("{}{}", indent, self.name);
        if let Some(ref v) = self.value {
            s.push_str(&format!(": {}", v));
        }
        for (k, v) in &self.infos {
            s.push_str(&format!(" [{}={}]", k, v));
        }
        s.push('\n');
        for child in &self.children {
            s.push_str(&child.render_tree(depth + 1));
        }
        s
    }

    fn render_csv(&self, depth: usize) -> String {
        let val = self.value.as_deref().unwrap_or("");
        let mut s = format!("{},{},{},{},{}\n", depth, self.file_offset, self.name, self.size, val);
        for (k, v) in &self.infos {
            s.push_str(&format!("  {},\"info\",{},{}\n", depth, k, v));
        }
        for child in &self.children {
            s.push_str(&child.render_csv(depth + 1));
        }
        s
    }

    fn render_xml(&self, depth: usize) -> String {
        let indent = "  ".repeat(depth);
        let val = self.value.as_deref().unwrap_or("");
        if val.is_empty() && self.children.is_empty() {
            format!("{}<{} offset=\"{}\" size=\"{}\"/>\n", indent, self.name, self.file_offset, self.size)
        } else if val.is_empty() {
            let children: String = self.children.iter().map(|c| c.render_xml(depth + 1)).collect();
            format!("{}<{} offset=\"{}\" size=\"{}\">\n{}{}</{}>\n", indent, self.name, self.file_offset, self.size, children, indent, self.name)
        } else {
            let children: String = self.children.iter().map(|c| c.render_xml(depth + 1)).collect();
            format!("{}<{} offset=\"{}\" size=\"{}\">\n{}  {}\n{}{}</{}>\n", indent, self.name, self.file_offset, self.size, indent, val, children, indent, self.name)
        }
    }

    fn render_micro_xml(&self) -> String {
        let val = self.value.as_deref().unwrap_or("");
        if self.children.is_empty() {
            format!("<{} o=\"{}\" s=\"{}\" v=\"{}\"/>\n", self.name, self.file_offset, self.size, val)
        } else {
            let children: String = self.children.iter().map(|c| c.render_micro_xml()).collect();
            format!("<{} o=\"{}\" s=\"{}\">\n{}</{}>\n", self.name, self.file_offset, self.size, children, self.name)
        }
    }
}

#[cfg(test)] mod tests {
    use super::*;

    #[test] fn test_demux_level() {
        let full = DemuxLevel(0x0F);
        assert!(full.contains(DemuxLevel::FRAME));
        assert!(full.contains(DemuxLevel::CONTAINER));
        assert!(full.contains(DemuxLevel::ELEMENTARY));
        assert!(full.contains(DemuxLevel::ANCILLARY));
    }

    #[test] fn test_demux_state() {
        let mut state = DemuxState::new(DemuxLevel::CONTAINER);
        state.emit(DemuxEvent {
            content_type: ContentType::MainStream,
            stream_ids: vec![0x1011],
            pts: Some(0),
            dts: Some(0),
            duration: Some(40),
            offset_stream: 0,
            offset_content: 1024,
            random_access: true,
        });
        assert_eq!(state.event_count(), 1);
        assert_eq!(state.frame_number, 1);
    }

    #[test] fn test_trace_tree() {
        let mut root = TraceNode::new("mp4", 0);
        root.set_size(1024);
        let mut child = TraceNode::new("moov", 32);
        child.set_size(512);
        child.add_info("timescale", "1000");
        root.add_child(child);
        let tree = root.render(TraceFormat::Tree, 0);
        assert!(tree.contains("mp4"));
        assert!(tree.contains("moov"));
        assert!(tree.contains("timescale=1000"));
    }

    #[test] fn test_trace_micro_xml() {
        let mut root = TraceNode::new("ftyp", 0);
        root.set_value("mp42");
        root.set_size(28);
        let xml = root.render(TraceFormat::MicroXml, 0);
        assert!(xml.contains("ftyp"));
        assert!(xml.contains("mp42"));
    }
}
