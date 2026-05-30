// GENERATED FILE — DO NOT EDIT BY HAND.
//
// Derived from Image::ExifTool::DJI (Copyright Phil Harvey), which is
// distributed under the same terms as Perl: the Artistic License or the
// GNU General Public License. This file is therefore a derivative work
// under those terms and is distributed as part of the GPL-licensed
// `revelo-exiftool-tables` crate. See that crate's LICENSE and NOTICE.
//
// Regenerate with: crates/revelo-exiftool-tables/codegen/extract.pl

/// Maker-note tag id -> canonical ExifTool tag name.
pub fn tag_name(id: u32) -> Option<&'static str> {
    match id {
        0x0001 => Some("Make"),
        0x0003 => Some("SpeedX"),
        0x0004 => Some("SpeedY"),
        0x0005 => Some("SpeedZ"),
        0x0006 => Some("Pitch"),
        0x0007 => Some("Yaw"),
        0x0008 => Some("Roll"),
        0x0009 => Some("CameraPitch"),
        0x000a => Some("CameraYaw"),
        0x000b => Some("CameraRoll"),
        _ => None,
    }
}

/// Maker-note (tag id, integer value) -> ExifTool PrintConv string.
pub fn print_conv(id: u32, value: i64) -> Option<&'static str> {
    match (id, value) {
        _ => None,
    }
}
