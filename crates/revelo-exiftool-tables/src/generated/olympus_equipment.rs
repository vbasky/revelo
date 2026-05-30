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
        0x0000 => Some("EquipmentVersion"),
        0x0100 => Some("CameraType2"),
        0x0101 => Some("SerialNumber"),
        0x0102 => Some("InternalSerialNumber"),
        0x0103 => Some("FocalPlaneDiagonal"),
        0x0104 => Some("BodyFirmwareVersion"),
        0x0201 => Some("LensType"),
        0x0202 => Some("LensSerialNumber"),
        0x0203 => Some("LensModel"),
        0x0204 => Some("LensFirmwareVersion"),
        0x0205 => Some("MaxApertureAtMinFocal"),
        0x0206 => Some("MaxApertureAtMaxFocal"),
        0x0207 => Some("MinFocalLength"),
        0x0208 => Some("MaxFocalLength"),
        0x020a => Some("MaxAperture"),
        0x020b => Some("LensProperties"),
        0x0301 => Some("Extender"),
        0x0302 => Some("ExtenderSerialNumber"),
        0x0303 => Some("ExtenderModel"),
        0x0304 => Some("ExtenderFirmwareVersion"),
        0x0403 => Some("ConversionLens"),
        0x1000 => Some("FlashType"),
        0x1001 => Some("FlashModel"),
        0x1002 => Some("FlashFirmwareVersion"),
        0x1003 => Some("FlashSerialNumber"),
        _ => None,
    }
}

/// Maker-note (tag id, integer value) -> ExifTool PrintConv string.
pub fn print_conv(id: u32, value: i64) -> Option<&'static str> {
    match (id, value) {
        (0x1000, 0) => Some("None"),
        (0x1000, 2) => Some("Simple E-System"),
        (0x1000, 3) => Some("E-System"),
        (0x1000, 4) => Some("E-System (body powered)"),
        (0x1001, 0) => Some("None"),
        (0x1001, 1) => Some("FL-20"),
        (0x1001, 2) => Some("FL-50"),
        (0x1001, 3) => Some("RF-11"),
        (0x1001, 4) => Some("TF-22"),
        (0x1001, 5) => Some("FL-36"),
        (0x1001, 6) => Some("FL-50R"),
        (0x1001, 7) => Some("FL-36R"),
        (0x1001, 9) => Some("FL-14"),
        (0x1001, 11) => Some("FL-600R"),
        (0x1001, 13) => Some("FL-LM3"),
        (0x1001, 15) => Some("FL-900R"),
        _ => None,
    }
}
