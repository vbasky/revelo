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
        0x0000 => Some("ImageProcessingVersion"),
        0x0100 => Some("WB_RBLevels"),
        0x0102 => Some("WB_RBLevels3000K"),
        0x0103 => Some("WB_RBLevels3300K"),
        0x0104 => Some("WB_RBLevels3600K"),
        0x0105 => Some("WB_RBLevels3900K"),
        0x0106 => Some("WB_RBLevels4000K"),
        0x0107 => Some("WB_RBLevels4300K"),
        0x0108 => Some("WB_RBLevels4500K"),
        0x0109 => Some("WB_RBLevels4800K"),
        0x010a => Some("WB_RBLevels5300K"),
        0x010b => Some("WB_RBLevels6000K"),
        0x010c => Some("WB_RBLevels6600K"),
        0x010d => Some("WB_RBLevels7500K"),
        0x010e => Some("WB_RBLevelsCWB1"),
        0x010f => Some("WB_RBLevelsCWB2"),
        0x0110 => Some("WB_RBLevelsCWB3"),
        0x0111 => Some("WB_RBLevelsCWB4"),
        0x0113 => Some("WB_GLevel3000K"),
        0x0114 => Some("WB_GLevel3300K"),
        0x0115 => Some("WB_GLevel3600K"),
        0x0116 => Some("WB_GLevel3900K"),
        0x0117 => Some("WB_GLevel4000K"),
        0x0118 => Some("WB_GLevel4300K"),
        0x0119 => Some("WB_GLevel4500K"),
        0x011a => Some("WB_GLevel4800K"),
        0x011b => Some("WB_GLevel5300K"),
        0x011c => Some("WB_GLevel6000K"),
        0x011d => Some("WB_GLevel6600K"),
        0x011e => Some("WB_GLevel7500K"),
        0x011f => Some("WB_GLevel"),
        0x0200 => Some("ColorMatrix"),
        0x0300 => Some("Enhancer"),
        0x0301 => Some("EnhancerValues"),
        0x0310 => Some("CoringFilter"),
        0x0311 => Some("CoringValues"),
        0x0600 => Some("BlackLevel2"),
        0x0610 => Some("GainBase"),
        0x0611 => Some("ValidBits"),
        0x0612 => Some("CropLeft"),
        0x0613 => Some("CropTop"),
        0x0614 => Some("CropWidth"),
        0x0615 => Some("CropHeight"),
        0x0635 => Some("UnknownBlock1"),
        0x0636 => Some("UnknownBlock2"),
        0x0805 => Some("SensorCalibration"),
        0x1010 => Some("NoiseReduction2"),
        0x1011 => Some("DistortionCorrection2"),
        0x1012 => Some("ShadingCompensation2"),
        0x101c => Some("MultipleExposureMode"),
        0x1103 => Some("UnknownBlock3"),
        0x1104 => Some("UnknownBlock4"),
        0x1112 => Some("AspectRatio"),
        0x1113 => Some("AspectFrame"),
        0x1200 => Some("FacesDetected"),
        0x1201 => Some("FaceDetectArea"),
        0x1202 => Some("MaxFaces"),
        0x1203 => Some("FaceDetectFrameSize"),
        0x1207 => Some("FaceDetectFrameCrop"),
        0x1306 => Some("CameraTemperature"),
        0x1900 => Some("KeystoneCompensation"),
        0x1901 => Some("KeystoneDirection"),
        0x1906 => Some("KeystoneValue"),
        0x2110 => Some("GNDFilterType"),
        _ => None,
    }
}

/// Maker-note (tag id, integer value) -> ExifTool PrintConv string.
pub fn print_conv(id: u32, value: i64) -> Option<&'static str> {
    match (id, value) {
        (0x1010, 0) => Some("(none)"),
        (0x1011, 0) => Some("Off"),
        (0x1011, 1) => Some("On"),
        (0x1012, 0) => Some("Off"),
        (0x1012, 1) => Some("On"),
        (0x1901, 0) => Some("Vertical"),
        (0x1901, 1) => Some("Horizontal"),
        (0x2110, 0) => Some("High"),
        (0x2110, 1) => Some("Medium"),
        (0x2110, 2) => Some("Soft"),
        _ => None,
    }
}
