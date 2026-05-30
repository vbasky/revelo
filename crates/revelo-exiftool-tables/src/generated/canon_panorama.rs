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
        0x0002 => Some("PanoramaFrameNumber"),
        0x0005 => Some("PanoramaDirection"),
        _ => None,
    }
}

/// Maker-note (tag id, integer value) -> ExifTool PrintConv string.
pub fn print_conv(id: u32, value: i64) -> Option<&'static str> {
    match (id, value) {
        (0x0005, 0) => Some("Left to Right"),
        (0x0005, 1) => Some("Right to Left"),
        (0x0005, 2) => Some("Bottom to Top"),
        (0x0005, 3) => Some("Top to Bottom"),
        (0x0005, 4) => Some("2x2 Matrix (Clockwise)"),
        _ => None,
    }
}
