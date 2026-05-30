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
        0x0000 => Some("RawDevVersion"),
        0x0100 => Some("RawDevExposureBiasValue"),
        0x0101 => Some("RawDevWhiteBalanceValue"),
        0x0102 => Some("RawDevWBFineAdjustment"),
        0x0103 => Some("RawDevGrayPoint"),
        0x0104 => Some("RawDevSaturationEmphasis"),
        0x0105 => Some("RawDevMemoryColorEmphasis"),
        0x0106 => Some("RawDevContrastValue"),
        0x0107 => Some("RawDevSharpnessValue"),
        0x0108 => Some("RawDevColorSpace"),
        0x0109 => Some("RawDevEngine"),
        0x010a => Some("RawDevNoiseReduction"),
        0x010b => Some("RawDevEditStatus"),
        0x010c => Some("RawDevSettings"),
        _ => None,
    }
}

/// Maker-note (tag id, integer value) -> ExifTool PrintConv string.
pub fn print_conv(id: u32, value: i64) -> Option<&'static str> {
    match (id, value) {
        (0x0108, 0) => Some("sRGB"),
        (0x0108, 1) => Some("Adobe RGB"),
        (0x0108, 2) => Some("Pro Photo RGB"),
        (0x0109, 0) => Some("High Speed"),
        (0x0109, 1) => Some("High Function"),
        (0x0109, 2) => Some("Advanced High Speed"),
        (0x0109, 3) => Some("Advanced High Function"),
        (0x010a, 0) => Some("(none)"),
        (0x010b, 0) => Some("Original"),
        (0x010b, 1) => Some("Edited (Landscape)"),
        (0x010b, 6) => Some("Edited (Portrait)"),
        (0x010b, 8) => Some("Edited (Portrait)"),
        (0x010c, 0) => Some("(none)"),
        _ => None,
    }
}
