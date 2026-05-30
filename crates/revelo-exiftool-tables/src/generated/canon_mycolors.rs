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
        0x0002 => Some("MyColorMode"),
        _ => None,
    }
}

/// Maker-note (tag id, integer value) -> ExifTool PrintConv string.
pub fn print_conv(id: u32, value: i64) -> Option<&'static str> {
    match (id, value) {
        (0x0002, 0) => Some("Off"),
        (0x0002, 1) => Some("Positive Film"),
        (0x0002, 2) => Some("Light Skin Tone"),
        (0x0002, 3) => Some("Dark Skin Tone"),
        (0x0002, 4) => Some("Vivid Blue"),
        (0x0002, 5) => Some("Vivid Green"),
        (0x0002, 6) => Some("Vivid Red"),
        (0x0002, 7) => Some("Color Accent"),
        (0x0002, 8) => Some("Color Swap"),
        (0x0002, 9) => Some("Custom"),
        (0x0002, 12) => Some("Vivid"),
        (0x0002, 13) => Some("Neutral"),
        (0x0002, 14) => Some("Sepia"),
        (0x0002, 15) => Some("B&W"),
        _ => None,
    }
}
