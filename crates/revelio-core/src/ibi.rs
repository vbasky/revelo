/// Index of Binary Information — maps stream positions to timestamps
/// for frame-accurate seeking, used by MPEG-TS and MPEG-PS containers.
#[derive(Debug, Default)]
pub struct Ibi {
    pub frames: Vec<IbiFrame>,
}
#[derive(Debug, Clone)]
pub struct IbiFrame {
    pub stream_offset: u64,
    pub timestamp_ms: u64,
    pub is_keyframe: bool,
}
impl Ibi {
    pub fn add(&mut self, offset: u64, ts_ms: u64, keyframe: bool) {
        self.frames.push(IbiFrame {
            stream_offset: offset,
            timestamp_ms: ts_ms,
            is_keyframe: keyframe,
        });
    }
    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }
    pub fn last_frame_duration(&self) -> Option<u64> {
        if self.frames.len() < 2 {
            return None;
        }
        Some(self.frames.last()?.timestamp_ms - self.frames[self.frames.len() - 2].timestamp_ms)
    }
    pub fn duration_ms(&self) -> Option<u64> {
        if self.frames.is_empty() {
            return None;
        }
        Some(self.frames.last()?.timestamp_ms - self.frames[0].timestamp_ms)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_base() {
        let mut ibi = Ibi::default();
        ibi.add(0, 0, true);
        ibi.add(1000, 40, false);
        assert_eq!(ibi.frame_count(), 2);
        assert_eq!(ibi.duration_ms(), Some(40));
    }
}
