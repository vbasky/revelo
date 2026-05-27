//! Element trace tree — transliteration of MediaInfoLib's
//! `Element[Element_Level]` stack and `element_details::Element_Node` tree.
//!
//! In the C++ side this serves two roles: (1) building the `--trace` output
//! and (2) scoping sub-element parsing. This crate only handles the trace
//! tree role for now; the offset-scoping role is tied to the
//! `Header_Parse`/`Data_Parse` parser callback architecture and is handled
//! separately when that lands.

use crate::zenlib_re_export::int64u;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ElementInfo {
    /// `Some(name)` for field reads recorded via `Param` (i.e. `Get_B4(Size, "Size")`),
    /// `None` for ad-hoc info added via `Element_Info`.
    pub name: Option<String>,
    pub value: String,
    pub measure: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct ElementNode {
    pub name: String,
    pub size: int64u,
    pub infos: Vec<ElementInfo>,
    pub children: Vec<ElementNode>,
    pub has_error: bool,
}

impl ElementNode {
    pub fn new(name: impl Into<String>) -> Self {
        ElementNode {
            name: name.into(),
            size: 0,
            infos: Vec::new(),
            children: Vec::new(),
            has_error: false,
        }
    }
}

/// Stack-based element tree builder. Begin/end pairs must balance; pushing
/// a child on `End` makes the tree append-only from the consumer's view.
#[derive(Debug)]
pub struct ElementTree {
    stack: Vec<ElementNode>,
}

impl ElementTree {
    pub fn new() -> Self {
        ElementTree {
            stack: vec![ElementNode::new("")],
        }
    }

    pub fn Element_Begin(&mut self, name: impl Into<String>) {
        self.stack.push(ElementNode::new(name));
    }

    pub fn Element_End(&mut self) {
        if self.stack.len() <= 1 {
            return;
        }
        let child = self.stack.pop().expect("len > 1 checked above");
        self.stack
            .last_mut()
            .expect("len > 0 invariant")
            .children
            .push(child);
    }

    pub fn Element_Name(&mut self, name: impl Into<String>) {
        if let Some(last) = self.stack.last_mut() {
            last.name = name.into();
        }
    }

    pub fn Element_Info(&mut self, value: impl Into<String>, measure: Option<&str>) {
        if let Some(last) = self.stack.last_mut() {
            let value = value.into();
            // Match the C++ heuristic: value="NOK" or measure="Error" flags
            // the element as containing an error.
            if value == "NOK" || measure == Some("Error") {
                last.has_error = true;
            }
            last.infos.push(ElementInfo {
                name: None,
                value,
                measure: measure.map(String::from),
            });
        }
    }

    /// Record a field read (called by `Get_B*` / `Get_L*` / etc).
    pub fn Param(&mut self, name: impl Into<String>, value: impl Into<String>) {
        if let Some(last) = self.stack.last_mut() {
            last.infos.push(ElementInfo {
                name: Some(name.into()),
                value: value.into(),
                measure: None,
            });
        }
    }

    pub fn Element_Level(&self) -> usize {
        // C++ defines level 0 as "the implicit root", so depth = stack.len() - 1.
        self.stack.len().saturating_sub(1)
    }

    pub fn set_current_size(&mut self, size: int64u) {
        if let Some(last) = self.stack.last_mut() {
            last.size = size;
        }
    }

    /// Returns the root node. Only meaningful when all Begin/End pairs have
    /// balanced (i.e. `Element_Level() == 0`).
    pub fn root(&self) -> &ElementNode {
        &self.stack[0]
    }

    /// Mutable access to the current (top-of-stack) element. Used by
    /// `FileAnalyze::Get_B*` etc. to record param entries.
    pub fn current_mut(&mut self) -> &mut ElementNode {
        self.stack
            .last_mut()
            .expect("stack invariant: root always present")
    }
}

impl Default for ElementTree {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_tree_has_root_at_level_0() {
        let t = ElementTree::new();
        assert_eq!(t.Element_Level(), 0);
        assert_eq!(t.root().children.len(), 0);
    }

    #[test]
    fn begin_end_pair_appends_child_to_root() {
        let mut t = ElementTree::new();
        t.Element_Begin("atom");
        assert_eq!(t.Element_Level(), 1);
        t.Element_End();
        assert_eq!(t.Element_Level(), 0);
        assert_eq!(t.root().children.len(), 1);
        assert_eq!(t.root().children[0].name, "atom");
    }

    #[test]
    fn nested_begin_end_builds_tree() {
        let mut t = ElementTree::new();
        t.Element_Begin("moov");
        t.Element_Begin("trak");
        t.Element_Begin("tkhd");
        t.Element_End();
        t.Element_Begin("mdia");
        t.Element_End();
        t.Element_End();
        t.Element_End();

        let root = t.root();
        assert_eq!(root.children.len(), 1);
        let moov = &root.children[0];
        assert_eq!(moov.name, "moov");
        assert_eq!(moov.children.len(), 1);
        let trak = &moov.children[0];
        assert_eq!(trak.children.len(), 2);
        assert_eq!(trak.children[0].name, "tkhd");
        assert_eq!(trak.children[1].name, "mdia");
    }

    #[test]
    fn element_info_records_value_and_measure() {
        let mut t = ElementTree::new();
        t.Element_Begin("tkhd");
        t.Element_Info("1000", Some("ms"));
        t.Element_Info("42", None);
        t.Element_End();
        let tkhd = &t.root().children[0];
        assert_eq!(tkhd.infos.len(), 2);
        assert_eq!(tkhd.infos[0].name, None);
        assert_eq!(tkhd.infos[0].value, "1000");
        assert_eq!(tkhd.infos[0].measure.as_deref(), Some("ms"));
        assert_eq!(tkhd.infos[1].measure, None);
    }

    #[test]
    fn param_records_named_field_read() {
        let mut t = ElementTree::new();
        t.Element_Begin("mvhd");
        t.Param("Version", "0");
        t.Param("Flags", "0x000000");
        t.Element_End();
        let mvhd = &t.root().children[0];
        assert_eq!(mvhd.infos.len(), 2);
        assert_eq!(mvhd.infos[0].name.as_deref(), Some("Version"));
        assert_eq!(mvhd.infos[0].value, "0");
        assert_eq!(mvhd.infos[1].name.as_deref(), Some("Flags"));
    }

    #[test]
    fn nok_marks_element_as_error() {
        let mut t = ElementTree::new();
        t.Element_Begin("bad");
        t.Element_Info("NOK", None);
        t.Element_End();
        assert!(t.root().children[0].has_error);
    }

    #[test]
    fn measure_error_marks_element_as_error() {
        let mut t = ElementTree::new();
        t.Element_Begin("bad");
        t.Element_Info("0x1234", Some("Error"));
        t.Element_End();
        assert!(t.root().children[0].has_error);
    }

    #[test]
    fn element_name_renames_current_frame() {
        let mut t = ElementTree::new();
        t.Element_Begin("");
        t.Element_Name("renamed");
        t.Element_End();
        assert_eq!(t.root().children[0].name, "renamed");
    }

    #[test]
    fn extra_element_end_is_noop() {
        let mut t = ElementTree::new();
        t.Element_End();
        t.Element_End();
        assert_eq!(t.Element_Level(), 0);
    }
}
