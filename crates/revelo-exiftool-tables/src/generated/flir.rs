// GENERATED FILE — DO NOT EDIT BY HAND.
//
// Derived from Image::ExifTool::FLIR (Copyright Phil Harvey), which is
// distributed under the same terms as Perl: the Artistic License or the
// GNU General Public License. This file is therefore a derivative work
// under those terms and is distributed as part of the GPL-licensed
// `revelo-exiftool-tables` crate. See that crate's LICENSE and NOTICE.
//
// Regenerate with: crates/revelo-exiftool-tables/codegen/extract.pl

/// Maker-note tag id -> canonical ExifTool tag name.
pub fn tag_name(id: u32) -> Option<&'static str> {
    match id {
        0x0001 => Some("ImageTemperatureMax"),
        0x0002 => Some("ImageTemperatureMin"),
        0x0003 => Some("Emissivity"),
        0x0004 => Some("UnknownTemperature"),
        0x0005 => Some("CameraTemperatureRangeMax"),
        0x0006 => Some("CameraTemperatureRangeMin"),
        _ => None,
    }
}

/// Maker-note (tag id, integer value) -> ExifTool PrintConv string.
pub fn print_conv(id: u32, value: i64) -> Option<&'static str> {
    match (id, value) {
        _ => None,
    }
}
