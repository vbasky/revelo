// GENERATED FILE — DO NOT EDIT BY HAND.
//
// Derived from Image::ExifTool::Apple (Copyright Phil Harvey), which is
// distributed under the same terms as Perl: the Artistic License or the
// GNU General Public License. This file is therefore a derivative work
// under those terms and is distributed as part of the GPL-licensed
// `revelo-exiftool-tables` crate. See that crate's LICENSE and NOTICE.
//
// Regenerate with: crates/revelo-exiftool-tables/codegen/extract.pl

/// Maker-note tag id -> canonical ExifTool tag name.
pub fn tag_name(id: u32) -> Option<&'static str> {
    match id {
        0x0001 => Some("MakerNoteVersion"),
        0x0002 => Some("AEMatrix"),
        0x0003 => Some("RunTime"),
        0x0004 => Some("AEStable"),
        0x0005 => Some("AETarget"),
        0x0006 => Some("AEAverage"),
        0x0007 => Some("AFStable"),
        0x0008 => Some("AccelerationVector"),
        0x000a => Some("HDRImageType"),
        0x000b => Some("BurstUUID"),
        0x000c => Some("FocusDistanceRange"),
        0x000f => Some("OISMode"),
        0x0011 => Some("ContentIdentifier"),
        0x0014 => Some("ImageCaptureType"),
        0x0015 => Some("ImageUniqueID"),
        0x0017 => Some("LivePhotoVideoIndex"),
        0x0019 => Some("ImageProcessingFlags"),
        0x001a => Some("QualityHint"),
        0x001d => Some("LuminanceNoiseAmplitude"),
        0x001f => Some("PhotosAppFeatureFlags"),
        0x0020 => Some("ImageCaptureRequestID"),
        0x0021 => Some("HDRHeadroom"),
        0x0023 => Some("AFPerformance"),
        0x0025 => Some("SceneFlags"),
        0x0026 => Some("SignalToNoiseRatioType"),
        0x0027 => Some("SignalToNoiseRatio"),
        0x002b => Some("PhotoIdentifier"),
        0x002d => Some("ColorTemperature"),
        0x002e => Some("CameraType"),
        0x002f => Some("FocusPosition"),
        0x0030 => Some("HDRGain"),
        0x0038 => Some("AFMeasuredDepth"),
        0x003d => Some("AFConfidence"),
        0x003e => Some("ColorCorrectionMatrix"),
        0x003f => Some("GreenGhostMitigationStatus"),
        0x0040 => Some("SemanticStyle"),
        0x0041 => Some("SemanticStyleRenderingVer"),
        0x0042 => Some("SemanticStylePreset"),
        0x004e => Some("Apple_0x004e"),
        0x004f => Some("Apple_0x004f"),
        0x0054 => Some("Apple_0x0054"),
        0x005a => Some("Apple_0x005a"),
        _ => None,
    }
}

/// Maker-note (tag id, integer value) -> ExifTool PrintConv string.
pub fn print_conv(id: u32, value: i64) -> Option<&'static str> {
    match (id, value) {
        (0x0004, 0) => Some("No"),
        (0x0004, 1) => Some("Yes"),
        (0x0007, 0) => Some("No"),
        (0x0007, 1) => Some("Yes"),
        (0x000a, 3) => Some("HDR Image"),
        (0x000a, 4) => Some("Original Image"),
        (0x0014, 1) => Some("ProRAW"),
        (0x0014, 2) => Some("Portrait"),
        (0x0014, 10) => Some("Photo"),
        (0x0014, 11) => Some("Manual Focus"),
        (0x0014, 12) => Some("Scene"),
        (0x002e, 0) => Some("Back Wide Angle"),
        (0x002e, 1) => Some("Back Normal"),
        (0x002e, 6) => Some("Front"),
        _ => None,
    }
}
