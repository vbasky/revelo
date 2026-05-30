// GENERATED FILE — DO NOT EDIT BY HAND.
//
// Derived from Image::ExifTool::Nikon (Copyright Phil Harvey), which is
// distributed under the same terms as Perl: the Artistic License or the
// GNU General Public License. This file is therefore a derivative work
// under those terms and is distributed as part of the GPL-licensed
// `revelo-exiftool-tables` crate. See that crate's LICENSE and NOTICE.
//
// Regenerate with: crates/revelo-exiftool-tables/codegen/extract.pl

/// Maker-note tag id -> canonical ExifTool tag name.
pub fn tag_name(id: u32) -> Option<&'static str> {
    match id {
        0x0000 => Some("AFAreaMode"),
        0x0001 => Some("AFPoint"),
        0x0002 => Some("AFPointsInFocus"),
        _ => None,
    }
}

/// Maker-note (tag id, integer value) -> ExifTool PrintConv string.
pub fn print_conv(id: u32, value: i64) -> Option<&'static str> {
    match (id, value) {
        (0x0000, 0) => Some("Single Area"),
        (0x0000, 1) => Some("Dynamic Area"),
        (0x0000, 2) => Some("Dynamic Area (closest subject)"),
        (0x0000, 3) => Some("Group Dynamic"),
        (0x0000, 4) => Some("Single Area (wide)"),
        (0x0000, 5) => Some("Dynamic Area (wide)"),
        (0x0001, 0) => Some("Center"),
        (0x0001, 1) => Some("Top"),
        (0x0001, 2) => Some("Bottom"),
        (0x0001, 3) => Some("Mid-left"),
        (0x0001, 4) => Some("Mid-right"),
        (0x0001, 5) => Some("Upper-left"),
        (0x0001, 6) => Some("Upper-right"),
        (0x0001, 7) => Some("Lower-left"),
        (0x0001, 8) => Some("Lower-right"),
        (0x0001, 9) => Some("Far Left"),
        (0x0001, 10) => Some("Far Right"),
        (0x0002, 0) => Some("(none)"),
        (0x0002, 2047) => Some("All 11 Points"),
        _ => None,
    }
}
