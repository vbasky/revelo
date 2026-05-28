pub struct SmpteTimeCode {
    pub hours: u8, pub minutes: u8, pub seconds: u8, pub frames: u8,
    pub drop_frame: bool, pub fps: u8,
}
impl SmpteTimeCode {
    pub fn parse(value: &str, fps: u8) -> Option<Self> {
        let df = value.contains(';');
        if df {
            let mut parts = value.splitn(2, ';');
            let time_part = parts.next()?;
            let frames_str = parts.next()?;
            let hms: Vec<&str> = time_part.split(':').collect();
            if hms.len() != 3 { return None; }
            Some(SmpteTimeCode { hours: hms[0].parse().ok()?, minutes: hms[1].parse().ok()?,
                seconds: hms[2].parse().ok()?, frames: frames_str.parse().ok()?,
                drop_frame: df, fps })
        } else {
            let pos = value.rfind(':')?;
            let frames_str = &value[pos+1..];
            let hms: Vec<&str> = value[..pos].split(':').collect();
            if hms.len() != 3 { return None; }
            Some(SmpteTimeCode { hours: hms[0].parse().ok()?, minutes: hms[1].parse().ok()?,
                seconds: hms[2].parse().ok()?, frames: frames_str.parse().ok()?,
                drop_frame: df, fps })
        }
    }
    pub fn to_milliseconds(&self) -> u64 {
        let total_frames = (self.hours as u64 * 3600 + self.minutes as u64 * 60 + self.seconds as u64) * self.fps as u64 + self.frames as u64;
        total_frames * 1000 / self.fps as u64
    }
}
#[cfg(test)] mod tests { use super::*;
    #[test] fn test_ndf() { let tc = SmpteTimeCode::parse("00:01:30:00", 30).unwrap(); assert!(!tc.drop_frame); }
    #[test] fn test_df() { let tc = SmpteTimeCode::parse("01:00:00;00", 30).unwrap(); assert!(tc.drop_frame); }
    #[test] fn test_ndf4() { let tc = SmpteTimeCode::parse("01:02:03:04", 30).unwrap(); assert_eq!(tc.frames, 4); }
}
