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
        0x0000 => Some("AspectRatio"),
        0x0001 => Some("CroppedImageWidth"),
        0x0002 => Some("CroppedImageHeight"),
        0x0003 => Some("CroppedImageLeft"),
        0x0004 => Some("CroppedImageTop"),
        _ => None,
    }
}

/// Maker-note (tag id, integer value) -> ExifTool PrintConv string.
pub fn print_conv(id: u32, value: i64) -> Option<&'static str> {
    match (id, value) {
        (0x0000, 0) => Some("3:2"),
        (0x0000, 1) => Some("1:1"),
        (0x0000, 2) => Some("4:3"),
        (0x0000, 7) => Some("16:9"),
        (0x0000, 8) => Some("4:5"),
        (0x0000, 12) => Some("3:2 (APS-H crop)"),
        (0x0000, 13) => Some("3:2 (APS-C crop)"),
        (0x0000, 258) => Some("4:3 crop"),
        _ => None,
    }
}
