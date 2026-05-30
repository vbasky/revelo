// GENERATED FILE — DO NOT EDIT BY HAND.
//
// Derived from Image::ExifTool::Canon (Copyright Phil Harvey), which is
// distributed under the same terms as Perl: the Artistic License or the
// GNU General Public License. This file is therefore a derivative work
// under those terms and is distributed as part of the GPL-licensed
// `revelo-exiftool-tables` crate. See that crate's LICENSE and NOTICE.
//
// Regenerate with: crates/revelo-exiftool-tables/codegen/extract.pl

/// Maker-note tag id -> canonical ExifTool tag name.
pub fn tag_name(id: u32) -> Option<&'static str> {
    match id {
        0x0001 => Some("AutoISO"),
        0x0002 => Some("BaseISO"),
        0x0003 => Some("MeasuredEV"),
        0x0004 => Some("TargetAperture"),
        0x0005 => Some("TargetExposureTime"),
        0x0006 => Some("ExposureCompensation"),
        0x0007 => Some("WhiteBalance"),
        0x0008 => Some("SlowShutter"),
        0x0009 => Some("SequenceNumber"),
        0x000a => Some("OpticalZoomCode"),
        0x000c => Some("CameraTemperature"),
        0x000d => Some("FlashGuideNumber"),
        0x000e => Some("AFPointsInFocus"),
        0x000f => Some("FlashExposureComp"),
        0x0010 => Some("AutoExposureBracketing"),
        0x0011 => Some("AEBBracketValue"),
        0x0012 => Some("ControlMode"),
        0x0013 => Some("FocusDistanceUpper"),
        0x0014 => Some("FocusDistanceLower"),
        0x0015 => Some("FNumber"),
        0x0016 => Some("ExposureTime"),
        0x0017 => Some("MeasuredEV2"),
        0x0018 => Some("BulbDuration"),
        0x001a => Some("CameraType"),
        0x001b => Some("AutoRotate"),
        0x001c => Some("NDFilter"),
        0x001d => Some("SelfTimer2"),
        0x0021 => Some("FlashOutput"),
        _ => None,
    }
}

/// Maker-note (tag id, integer value) -> ExifTool PrintConv string.
pub fn print_conv(id: u32, value: i64) -> Option<&'static str> {
    match (id, value) {
        (0x0007, 0) => Some("Auto"),
        (0x0007, 1) => Some("Daylight"),
        (0x0007, 2) => Some("Cloudy"),
        (0x0007, 3) => Some("Tungsten"),
        (0x0007, 4) => Some("Fluorescent"),
        (0x0007, 5) => Some("Flash"),
        (0x0007, 6) => Some("Custom"),
        (0x0007, 7) => Some("Black & White"),
        (0x0007, 8) => Some("Shade"),
        (0x0007, 9) => Some("Manual Temperature (Kelvin)"),
        (0x0007, 10) => Some("PC Set1"),
        (0x0007, 11) => Some("PC Set2"),
        (0x0007, 12) => Some("PC Set3"),
        (0x0007, 14) => Some("Daylight Fluorescent"),
        (0x0007, 15) => Some("Custom 1"),
        (0x0007, 16) => Some("Custom 2"),
        (0x0007, 17) => Some("Underwater"),
        (0x0007, 18) => Some("Custom 3"),
        (0x0007, 19) => Some("Custom 4"),
        (0x0007, 20) => Some("PC Set4"),
        (0x0007, 21) => Some("PC Set5"),
        (0x0007, 23) => Some("Auto (ambience priority)"),
        (0x0008, -1) => Some("n/a"),
        (0x0008, 0) => Some("Off"),
        (0x0008, 1) => Some("Night Scene"),
        (0x0008, 2) => Some("On"),
        (0x0008, 3) => Some("None"),
        (0x000e, 12288) => Some("None (MF)"),
        (0x000e, 12289) => Some("Right"),
        (0x000e, 12290) => Some("Center"),
        (0x000e, 12291) => Some("Center+Right"),
        (0x000e, 12292) => Some("Left"),
        (0x000e, 12293) => Some("Left+Right"),
        (0x000e, 12294) => Some("Left+Center"),
        (0x000e, 12295) => Some("All"),
        (0x0010, -1) => Some("On"),
        (0x0010, 0) => Some("Off"),
        (0x0010, 1) => Some("On (shot 1)"),
        (0x0010, 2) => Some("On (shot 2)"),
        (0x0010, 3) => Some("On (shot 3)"),
        (0x0012, 0) => Some("n/a"),
        (0x0012, 1) => Some("Camera Local Control"),
        (0x0012, 3) => Some("Computer Remote Control"),
        (0x001a, 0) => Some("n/a"),
        (0x001a, 248) => Some("EOS High-end"),
        (0x001a, 250) => Some("Compact"),
        (0x001a, 252) => Some("EOS Mid-range"),
        (0x001a, 255) => Some("DV Camera"),
        (0x001b, -1) => Some("n/a"),
        (0x001b, 0) => Some("None"),
        (0x001b, 1) => Some("Rotate 90 CW"),
        (0x001b, 2) => Some("Rotate 180"),
        (0x001b, 3) => Some("Rotate 270 CW"),
        (0x001c, -1) => Some("n/a"),
        (0x001c, 0) => Some("Off"),
        (0x001c, 1) => Some("On"),
        _ => None,
    }
}
