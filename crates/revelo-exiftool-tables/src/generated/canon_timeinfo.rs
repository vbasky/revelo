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
        0x0001 => Some("TimeZone"),
        0x0002 => Some("TimeZoneCity"),
        0x0003 => Some("DaylightSavings"),
        _ => None,
    }
}

/// Maker-note (tag id, integer value) -> ExifTool PrintConv string.
pub fn print_conv(id: u32, value: i64) -> Option<&'static str> {
    match (id, value) {
        (0x0002, 0) => Some("n/a"),
        (0x0002, 1) => Some("Chatham Islands"),
        (0x0002, 2) => Some("Wellington"),
        (0x0002, 3) => Some("Solomon Islands"),
        (0x0002, 4) => Some("Sydney"),
        (0x0002, 5) => Some("Adelaide"),
        (0x0002, 6) => Some("Tokyo"),
        (0x0002, 7) => Some("Hong Kong"),
        (0x0002, 8) => Some("Bangkok"),
        (0x0002, 9) => Some("Yangon"),
        (0x0002, 10) => Some("Dhaka"),
        (0x0002, 11) => Some("Kathmandu"),
        (0x0002, 12) => Some("Delhi"),
        (0x0002, 13) => Some("Karachi"),
        (0x0002, 14) => Some("Kabul"),
        (0x0002, 15) => Some("Dubai"),
        (0x0002, 16) => Some("Tehran"),
        (0x0002, 17) => Some("Moscow"),
        (0x0002, 18) => Some("Cairo"),
        (0x0002, 19) => Some("Paris"),
        (0x0002, 20) => Some("London"),
        (0x0002, 21) => Some("Azores"),
        (0x0002, 22) => Some("Fernando de Noronha"),
        (0x0002, 23) => Some("Sao Paulo"),
        (0x0002, 24) => Some("Newfoundland"),
        (0x0002, 25) => Some("Santiago"),
        (0x0002, 26) => Some("Caracas"),
        (0x0002, 27) => Some("New York"),
        (0x0002, 28) => Some("Chicago"),
        (0x0002, 29) => Some("Denver"),
        (0x0002, 30) => Some("Los Angeles"),
        (0x0002, 31) => Some("Anchorage"),
        (0x0002, 32) => Some("Honolulu"),
        (0x0002, 33) => Some("Samoa"),
        (0x0002, 32766) => Some("(not set)"),
        (0x0003, 0) => Some("Off"),
        (0x0003, 60) => Some("On"),
        _ => None,
    }
}
