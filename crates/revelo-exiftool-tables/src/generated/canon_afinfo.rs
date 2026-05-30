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
        0x0000 => Some("NumAFPoints"),
        0x0001 => Some("ValidAFPoints"),
        0x0002 => Some("CanonImageWidth"),
        0x0003 => Some("CanonImageHeight"),
        0x0004 => Some("AFImageWidth"),
        0x0005 => Some("AFImageHeight"),
        0x0006 => Some("AFAreaWidth"),
        0x0007 => Some("AFAreaHeight"),
        0x0008 => Some("AFAreaXPositions"),
        0x0009 => Some("AFAreaYPositions"),
        0x000a => Some("AFPointsInFocus"),
        0x000b => Some("PrimaryAFPoint"),
        0x000c => Some("PrimaryAFPoint"),
        _ => None,
    }
}

/// Maker-note (tag id, integer value) -> ExifTool PrintConv string.
pub fn print_conv(id: u32, value: i64) -> Option<&'static str> {
    match (id, value) {
        _ => None,
    }
}
