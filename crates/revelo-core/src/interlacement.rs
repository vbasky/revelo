/// Video field ordering and interlacement tracking.
/// Mirrors MediaInfoLib's scan_order + interlacement field pair.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScanOrder {
    #[default]
    Progressive,
    Tff,
    Bff,
    Mixed,
}

impl ScanOrder {
    pub fn as_str(&self) -> &'static str {
        match self {
            ScanOrder::Progressive => "Progressive",
            ScanOrder::Tff => "TFF",
            ScanOrder::Bff => "BFF",
            ScanOrder::Mixed => "Mixed",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterlacementMode {
    Ppf,
    Interlaced,
    Tff,
    Bff,
    PsF,
}

impl InterlacementMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            InterlacementMode::Ppf => "PPF",
            InterlacementMode::Interlaced => "Interlaced",
            InterlacementMode::Tff => "TFF",
            InterlacementMode::Bff => "BFF",
            InterlacementMode::PsF => "PsF",
        }
    }
}

/// Tracks field sequencing across video frames.
#[derive(Debug, Default, Clone)]
pub struct FieldTracker {
    pub field_count: u64,
    pub top_field_count: u64,
    pub bottom_field_count: u64,
    pub progressive_count: u64,
    pub current_order: ScanOrder,
    pub stored_order: Option<ScanOrder>,
    pub stored_displayed_inverted: bool,
}

impl FieldTracker {
    pub fn new() -> Self {
        FieldTracker { current_order: ScanOrder::Progressive, ..Default::default() }
    }

    /// Register a field and determine scan order from top/bottom ratio.
    pub fn feed_field(&mut self, is_top: bool) {
        self.field_count += 1;
        if is_top {
            self.top_field_count += 1;
        } else {
            self.bottom_field_count += 1;
        }
    }

    /// Register a progressive frame.
    pub fn feed_progressive(&mut self) {
        self.field_count += 1;
        self.progressive_count += 1;
    }

    /// Determine scan order from field history.
    pub fn infer_scan_order(&self) -> ScanOrder {
        if self.progressive_count > 0 && self.top_field_count == 0 && self.bottom_field_count == 0 {
            return ScanOrder::Progressive;
        }
        if self.top_field_count > self.bottom_field_count {
            ScanOrder::Tff
        } else if self.bottom_field_count > self.top_field_count {
            ScanOrder::Bff
        } else if self.top_field_count > 0 {
            ScanOrder::Mixed
        } else {
            ScanOrder::Progressive
        }
    }

    pub fn interlacement(&self) -> InterlacementMode {
        match self.infer_scan_order() {
            ScanOrder::Progressive => InterlacementMode::Ppf,
            ScanOrder::Tff => InterlacementMode::Tff,
            ScanOrder::Bff => InterlacementMode::Bff,
            ScanOrder::Mixed => InterlacementMode::Interlaced,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_progressive() {
        let mut ft = FieldTracker::new();
        ft.feed_progressive();
        ft.feed_progressive();
        assert_eq!(ft.infer_scan_order(), ScanOrder::Progressive);
        assert_eq!(ft.interlacement(), InterlacementMode::Ppf);
    }
    #[test]
    fn test_tff() {
        let mut ft = FieldTracker::new();
        ft.feed_field(true);
        ft.feed_field(true);
        ft.feed_field(false);
        assert_eq!(ft.infer_scan_order(), ScanOrder::Tff);
        assert_eq!(ft.interlacement(), InterlacementMode::Tff);
    }
    #[test]
    fn test_mixed() {
        let mut ft = FieldTracker::new();
        ft.feed_field(true);
        ft.feed_field(false);
        assert_eq!(ft.infer_scan_order(), ScanOrder::Mixed);
    }
}
