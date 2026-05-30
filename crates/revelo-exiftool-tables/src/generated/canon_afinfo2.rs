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
        0x0000 => Some("AFInfoSize"),
        0x0001 => Some("AFAreaMode"),
        0x0002 => Some("NumAFPoints"),
        0x0003 => Some("ValidAFPoints"),
        0x0004 => Some("CanonImageWidth"),
        0x0005 => Some("CanonImageHeight"),
        0x0006 => Some("AFImageWidth"),
        0x0007 => Some("AFImageHeight"),
        0x0008 => Some("AFAreaWidths"),
        0x0009 => Some("AFAreaHeights"),
        0x000a => Some("AFAreaXPositions"),
        0x000b => Some("AFAreaYPositions"),
        0x000c => Some("AFPointsInFocus"),
        0x000d => Some("AFPointsSelected"),
        0x000e => Some("PrimaryAFPoint"),
        _ => None,
    }
}

/// Maker-note (tag id, integer value) -> ExifTool PrintConv string.
pub fn print_conv(id: u32, value: i64) -> Option<&'static str> {
    match (id, value) {
        (0x0001, 0) => Some("Off (Manual Focus)"),
        (0x0001, 1) => Some("AF Point Expansion (surround)"),
        (0x0001, 2) => Some("Single-point AF"),
        (0x0001, 4) => Some("Auto"),
        (0x0001, 5) => Some("Face Detect AF"),
        (0x0001, 6) => Some("Face + Tracking"),
        (0x0001, 7) => Some("Zone AF"),
        (0x0001, 8) => Some("AF Point Expansion (4 point)"),
        (0x0001, 9) => Some("Spot AF"),
        (0x0001, 10) => Some("AF Point Expansion (8 point)"),
        (0x0001, 11) => Some("Flexizone Multi (49 point)"),
        (0x0001, 12) => Some("Flexizone Multi (9 point)"),
        (0x0001, 13) => Some("Flexizone Single"),
        (0x0001, 14) => Some("Large Zone AF"),
        (0x0001, 16) => Some("Large Zone AF (vertical)"),
        (0x0001, 17) => Some("Large Zone AF (horizontal)"),
        (0x0001, 19) => Some("Flexible Zone AF 1"),
        (0x0001, 20) => Some("Flexible Zone AF 2"),
        (0x0001, 21) => Some("Flexible Zone AF 3"),
        (0x0001, 22) => Some("Whole Area AF"),
        _ => None,
    }
}
