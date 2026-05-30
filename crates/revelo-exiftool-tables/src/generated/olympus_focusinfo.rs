// GENERATED FILE — DO NOT EDIT BY HAND.
//
// Derived from Image::ExifTool::Olympus (Copyright Phil Harvey), which is
// distributed under the same terms as Perl: the Artistic License or the
// GNU General Public License. This file is therefore a derivative work
// under those terms and is distributed as part of the GPL-licensed
// `revelo-exiftool-tables` crate. See that crate's LICENSE and NOTICE.
//
// Regenerate with: crates/revelo-exiftool-tables/codegen/extract.pl

/// Maker-note tag id -> canonical ExifTool tag name.
pub fn tag_name(id: u32) -> Option<&'static str> {
    match id {
        0x0000 => Some("FocusInfoVersion"),
        0x0209 => Some("AutoFocus"),
        0x0210 => Some("SceneDetect"),
        0x0211 => Some("SceneArea"),
        0x0212 => Some("SceneDetectData"),
        0x0300 => Some("ZoomStepCount"),
        0x0301 => Some("FocusStepCount"),
        0x0303 => Some("FocusStepInfinity"),
        0x0304 => Some("FocusStepNear"),
        0x0305 => Some("FocusDistance"),
        0x0308 => Some("AFPoint"),
        0x031b => Some("AFPointDetails"),
        0x0328 => Some("AFInfo"),
        0x1201 => Some("ExternalFlash"),
        0x1203 => Some("ExternalFlashGuideNumber"),
        0x1204 => Some("ExternalFlashBounce"),
        0x1205 => Some("ExternalFlashZoom"),
        0x1208 => Some("InternalFlash"),
        0x1209 => Some("ManualFlash"),
        0x120a => Some("MacroLED"),
        0x1500 => Some("SensorTemperature"),
        0x1600 => Some("ImageStabilization"),
        0x2100 => Some("AntiShockWaitingTime"),
        _ => None,
    }
}

/// Maker-note (tag id, integer value) -> ExifTool PrintConv string.
pub fn print_conv(id: u32, value: i64) -> Option<&'static str> {
    match (id, value) {
        (0x0209, 0) => Some("Off"),
        (0x0209, 1) => Some("On"),
        (0x1204, 0) => Some("Bounce or Off"),
        (0x1204, 1) => Some("Direct"),
        (0x1208, 0) => Some("Off"),
        (0x1208, 1) => Some("On"),
        (0x120a, 0) => Some("Off"),
        (0x120a, 1) => Some("On"),
        _ => None,
    }
}
