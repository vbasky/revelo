//! ExifTool-derived maker-note tag tables.
//!
//! **Licensing:** unlike the rest of revelo (BSD-2-Clause), this crate is
//! GPL/Artistic, because its tables are derived from Image::ExifTool. See
//! the crate `LICENSE` and `NOTICE`. It is kept as a separate, optional
//! crate so the BSD core never pulls copyleft code unless a build
//! explicitly opts in (the `exiftool-tables` feature in
//! `revelo-parsers-tag`).
//!
//! Two lookups per vendor, both derived from the vendor's `%Main` (or
//! equivalent) ExifTool tag table:
//! - [`tag_name`] — maker-note tag id → canonical ExifTool tag name.
//! - [`print_conv`] — (tag id, integer value) → ExifTool PrintConv string.
//!
//! Regenerate the tables with `codegen/extract.pl` against an installed
//! ExifTool.

mod generated;

/// A camera vendor whose ExifTool maker-note table is bundled here.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Vendor {
    Apple,
    Canon,
    Casio,
    Dji,
    Flir,
    Fujifilm,
    Minolta,
    Nikon,
    Olympus,
    Panasonic,
    Pentax,
    Samsung,
    Sigma,
    Sony,
}

impl Vendor {
    /// Resolve a vendor from an EXIF `Make` string (case-insensitive
    /// substring match), mirroring the dispatch in `revelo-parsers-tag`.
    /// Returns `None` for vendors with no bundled ExifTool table
    /// (e.g. GoPro, whose maker note uses the 4CC GPMF format).
    pub fn from_make(make: &str) -> Option<Vendor> {
        let m = make.to_uppercase();
        let has = |s: &str| m.contains(s);
        Some(if has("APPLE") {
            Vendor::Apple
        } else if has("CANON") {
            Vendor::Canon
        } else if has("CASIO") {
            Vendor::Casio
        } else if has("DJI") {
            Vendor::Dji
        } else if has("FLIR") {
            Vendor::Flir
        } else if has("FUJIFILM") || has("FUJI") {
            Vendor::Fujifilm
        } else if has("MINOLTA") || has("KONICA") {
            Vendor::Minolta
        } else if has("NIKON") {
            Vendor::Nikon
        } else if has("OLYMPUS") || has("OM DIGITAL") || has("OM SYSTEM") {
            Vendor::Olympus
        } else if has("PANASONIC") {
            Vendor::Panasonic
        } else if has("PENTAX") || has("RICOH") {
            Vendor::Pentax
        } else if has("SAMSUNG") {
            Vendor::Samsung
        } else if has("SIGMA") || has("FOVEON") {
            Vendor::Sigma
        } else if has("SONY") {
            Vendor::Sony
        } else {
            return None;
        })
    }
}

/// Maker-note tag id → canonical ExifTool tag name for `vendor`.
pub fn tag_name(vendor: Vendor, id: u32) -> Option<&'static str> {
    match vendor {
        Vendor::Apple => generated::apple::tag_name(id),
        Vendor::Canon => generated::canon::tag_name(id),
        Vendor::Casio => generated::casio::tag_name(id),
        Vendor::Dji => generated::dji::tag_name(id),
        Vendor::Flir => generated::flir::tag_name(id),
        Vendor::Fujifilm => generated::fujifilm::tag_name(id),
        Vendor::Minolta => generated::minolta::tag_name(id),
        Vendor::Nikon => generated::nikon::tag_name(id),
        Vendor::Olympus => generated::olympus::tag_name(id),
        Vendor::Panasonic => generated::panasonic::tag_name(id),
        Vendor::Pentax => generated::pentax::tag_name(id),
        Vendor::Samsung => generated::samsung::tag_name(id),
        Vendor::Sigma => generated::sigma::tag_name(id),
        Vendor::Sony => generated::sony::tag_name(id),
    }
}

/// ExifTool PrintConv string for `(vendor, tag id, integer value)`, or
/// `None` when no enumerated mapping exists (the caller should then keep
/// the raw value).
pub fn print_conv(vendor: Vendor, id: u32, value: i64) -> Option<&'static str> {
    match vendor {
        Vendor::Apple => generated::apple::print_conv(id, value),
        Vendor::Canon => generated::canon::print_conv(id, value),
        Vendor::Casio => generated::casio::print_conv(id, value),
        Vendor::Dji => generated::dji::print_conv(id, value),
        Vendor::Flir => generated::flir::print_conv(id, value),
        Vendor::Fujifilm => generated::fujifilm::print_conv(id, value),
        Vendor::Minolta => generated::minolta::print_conv(id, value),
        Vendor::Nikon => generated::nikon::print_conv(id, value),
        Vendor::Olympus => generated::olympus::print_conv(id, value),
        Vendor::Panasonic => generated::panasonic::print_conv(id, value),
        Vendor::Pentax => generated::pentax::print_conv(id, value),
        Vendor::Samsung => generated::samsung::print_conv(id, value),
        Vendor::Sigma => generated::sigma::print_conv(id, value),
        Vendor::Sony => generated::sony::print_conv(id, value),
    }
}

/// An Olympus "type-2" maker-note sub-IFD (each pointed to by a tag in
/// the main IFD).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum OlympusSubTable {
    Equipment,
    CameraSettings,
    RawDevelopment,
    ImageProcessing,
    FocusInfo,
}

impl OlympusSubTable {
    /// Resolve the sub-IFD from its tag id in the main Olympus IFD.
    pub fn from_tag(tag_id: u16) -> Option<OlympusSubTable> {
        Some(match tag_id {
            0x2010 => OlympusSubTable::Equipment,
            0x2020 => OlympusSubTable::CameraSettings,
            0x2030 => OlympusSubTable::RawDevelopment,
            0x2040 => OlympusSubTable::ImageProcessing,
            0x2050 => OlympusSubTable::FocusInfo,
            _ => return None,
        })
    }
}

/// Olympus sub-IFD tag id → name.
pub fn olympus_sub_tag_name(table: OlympusSubTable, id: u32) -> Option<&'static str> {
    match table {
        OlympusSubTable::Equipment => generated::olympus_equipment::tag_name(id),
        OlympusSubTable::CameraSettings => generated::olympus_camerasettings::tag_name(id),
        OlympusSubTable::RawDevelopment => generated::olympus_rawdevelopment::tag_name(id),
        OlympusSubTable::ImageProcessing => generated::olympus_imageprocessing::tag_name(id),
        OlympusSubTable::FocusInfo => generated::olympus_focusinfo::tag_name(id),
    }
}

/// Olympus sub-IFD (tag id, value) → PrintConv.
pub fn olympus_sub_print_conv(table: OlympusSubTable, id: u32, value: i64) -> Option<&'static str> {
    match table {
        OlympusSubTable::Equipment => generated::olympus_equipment::print_conv(id, value),
        OlympusSubTable::CameraSettings => generated::olympus_camerasettings::print_conv(id, value),
        OlympusSubTable::RawDevelopment => generated::olympus_rawdevelopment::print_conv(id, value),
        OlympusSubTable::ImageProcessing => {
            generated::olympus_imageprocessing::print_conv(id, value)
        }
        OlympusSubTable::FocusInfo => generated::olympus_focusinfo::print_conv(id, value),
    }
}

/// Nikon AFInfo (0x0088) sub-table: index 0 = AFAreaMode, 1 = AFPoint,
/// 2 = AFPointsInFocus.
pub fn nikon_afinfo_tag_name(index: u32) -> Option<&'static str> {
    generated::nikon_afinfo::tag_name(index)
}

/// Nikon AFInfo (index, value) PrintConv.
pub fn nikon_afinfo_print_conv(index: u32, value: i64) -> Option<&'static str> {
    generated::nikon_afinfo::print_conv(index, value)
}

/// Map a standard EXIF tag name to ExifTool's name where the two diverge.
/// Returns `None` when ExifTool uses the same name (the common case). Used
/// to align revelo's EXIF stream with exiftool output under the feature.
/// Kept deliberately small: only well-known renames with no downstream
/// dependency in revelo (e.g. lens formatting still keys off the standard
/// `LensSpecification`, which is intentionally not remapped).
pub fn exif_name_alias(standard_name: &str) -> Option<&'static str> {
    Some(match standard_name {
        "DateTime" => "ModifyDate",
        "DateTimeDigitized" => "CreateDate",
        "PhotographicSensitivity" => "ISO",
        "PixelXDimension" => "ExifImageWidth",
        "PixelYDimension" => "ExifImageHeight",
        "InteroperabilityIndex" => "InteropIndex",
        "InteroperabilityVersion" => "InteropVersion",
        "CameraOwnerName" => "OwnerName",
        "ExposureBiasValue" => "ExposureCompensation",
        "FocalLengthIn35mmFilm" => "FocalLengthIn35mmFormat",
        _ => return None,
    })
}

/// Map a revelo Canon Main maker-note tag name to ExifTool's name where
/// they diverge. Deliberately excludes `CanonLensType`/`LensSpecification`,
/// which revelo's lens-formatting post-pass keys off by name.
pub fn canon_main_name_alias(revelo_name: &str) -> Option<&'static str> {
    Some(match revelo_name {
        "CanonImageNumber" => "FileNumber",
        "CanonOwnerName" => "OwnerName",
        "CanonThumbnailValidArea" => "ThumbnailImageValidArea",
        "CanonSerialNumber" => "SerialNumber",
        "CanonSerialNumberFormat" => "SerialNumberFormat",
        _ => return None,
    })
}

/// A Canon maker-note `ProcessBinaryData` sub-table — a flat `int16s`
/// array where each entry index maps to a tag. Resolved from the parent
/// maker-note tag id (e.g. 0x0001 = CameraSettings).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CanonSubTable {
    CameraSettings,
    ShotInfo,
    FocalLength,
    AfInfo,
    AfInfo2,
    FileInfo,
    MyColors,
    Panorama,
    ContrastInfo,
    TimeInfo,
    AspectInfo,
    FaceDetect3,
}

impl CanonSubTable {
    /// Map a Canon maker-note tag id to its binary sub-table, if one is
    /// bundled. Only fixed-layout `int16s` arrays are mapped here, so a
    /// flat index→entry decode is correct. `AfInfo2`/`FileInfo` are
    /// generated and available via [`canon_sub_tag_name`] but intentionally
    /// NOT auto-dispatched, because they contain variable-length sections
    /// (DATAMEMBER-driven) that a naive flat decode would misalign.
    pub fn from_tag(parent_tag_id: u16) -> Option<CanonSubTable> {
        Some(match parent_tag_id {
            0x0001 => CanonSubTable::CameraSettings,
            0x0002 => CanonSubTable::FocalLength,
            0x0004 => CanonSubTable::ShotInfo,
            0x001d => CanonSubTable::MyColors,
            0x0027 => CanonSubTable::ContrastInfo,
            0x0035 => CanonSubTable::TimeInfo,
            0x009a => CanonSubTable::AspectInfo,
            0x002f => CanonSubTable::FaceDetect3,
            _ => return None,
        })
    }

    /// ExifTool `FIRST_ENTRY` for this table — the array index of the
    /// first tagged entry. Tables with `FIRST_ENTRY == 1` reserve index 0
    /// for the record's count/size header.
    pub fn first_entry(self) -> u32 {
        match self {
            CanonSubTable::CameraSettings
            | CanonSubTable::ShotInfo
            | CanonSubTable::TimeInfo
            | CanonSubTable::FaceDetect3 => 1,
            _ => 0,
        }
    }

    /// Element size in bytes (ExifTool `FORMAT`): 2 for the `int16*`
    /// tables, 4 for the `int32*` tables (TimeInfo, AspectInfo).
    pub fn element_size(self) -> usize {
        match self {
            CanonSubTable::TimeInfo | CanonSubTable::AspectInfo => 4,
            _ => 2,
        }
    }

    /// Whether the record begins with a 16-bit byte-count header (the
    /// int16 `FIRST_ENTRY == 1` tables: CameraSettings, ShotInfo),
    /// allowing it to be self-located.
    pub fn has_byte_count_header(self) -> bool {
        matches!(self, CanonSubTable::CameraSettings | CanonSubTable::ShotInfo)
    }
}

/// Sub-table entry index → tag name.
pub fn canon_sub_tag_name(table: CanonSubTable, index: u32) -> Option<&'static str> {
    match table {
        CanonSubTable::CameraSettings => generated::canon_camerasettings::tag_name(index),
        CanonSubTable::ShotInfo => generated::canon_shotinfo::tag_name(index),
        CanonSubTable::FocalLength => generated::canon_focallength::tag_name(index),
        CanonSubTable::AfInfo => generated::canon_afinfo::tag_name(index),
        CanonSubTable::AfInfo2 => generated::canon_afinfo2::tag_name(index),
        CanonSubTable::FileInfo => generated::canon_fileinfo::tag_name(index),
        CanonSubTable::MyColors => generated::canon_mycolors::tag_name(index),
        CanonSubTable::Panorama => generated::canon_panorama::tag_name(index),
        CanonSubTable::ContrastInfo => generated::canon_contrastinfo::tag_name(index),
        CanonSubTable::TimeInfo => generated::canon_timeinfo::tag_name(index),
        CanonSubTable::AspectInfo => generated::canon_aspectinfo::tag_name(index),
        CanonSubTable::FaceDetect3 => generated::canon_facedetect3::tag_name(index),
    }
}

/// Sub-table (entry index, int16 value) → PrintConv string.
pub fn canon_sub_print_conv(table: CanonSubTable, index: u32, value: i64) -> Option<&'static str> {
    match table {
        CanonSubTable::CameraSettings => generated::canon_camerasettings::print_conv(index, value),
        CanonSubTable::ShotInfo => generated::canon_shotinfo::print_conv(index, value),
        CanonSubTable::FocalLength => generated::canon_focallength::print_conv(index, value),
        CanonSubTable::AfInfo => generated::canon_afinfo::print_conv(index, value),
        CanonSubTable::AfInfo2 => generated::canon_afinfo2::print_conv(index, value),
        CanonSubTable::FileInfo => generated::canon_fileinfo::print_conv(index, value),
        CanonSubTable::MyColors => generated::canon_mycolors::print_conv(index, value),
        CanonSubTable::Panorama => generated::canon_panorama::print_conv(index, value),
        CanonSubTable::ContrastInfo => generated::canon_contrastinfo::print_conv(index, value),
        CanonSubTable::TimeInfo => generated::canon_timeinfo::print_conv(index, value),
        CanonSubTable::AspectInfo => generated::canon_aspectinfo::print_conv(index, value),
        CanonSubTable::FaceDetect3 => generated::canon_facedetect3::print_conv(index, value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apple_tag_and_printconv_resolve() {
        // Spot-check values extracted from Image::ExifTool::Apple.
        assert_eq!(tag_name(Vendor::Apple, 0x000a), Some("HDRImageType"));
        assert_eq!(print_conv(Vendor::Apple, 0x000a, 3), Some("HDR Image"));
        assert_eq!(print_conv(Vendor::Apple, 0x0014, 1), Some("ProRAW"));
        assert_eq!(tag_name(Vendor::Apple, 0xFFFF), None);
    }

    #[test]
    fn canon_camerasettings_subtable_resolves() {
        let t = CanonSubTable::from_tag(0x0001).unwrap();
        assert_eq!(t, CanonSubTable::CameraSettings);
        assert_eq!(canon_sub_tag_name(t, 1), Some("MacroMode"));
        // MacroMode PrintConv: 1 => 'Macro', 2 => 'Normal'
        assert_eq!(canon_sub_print_conv(t, 1, 1), Some("Macro"));
        assert_eq!(canon_sub_print_conv(t, 1, 2), Some("Normal"));
        assert_eq!(CanonSubTable::from_tag(0xABCD), None);
    }

    #[test]
    fn name_aliases() {
        assert_eq!(exif_name_alias("DateTime"), Some("ModifyDate"));
        assert_eq!(exif_name_alias("InteroperabilityIndex"), Some("InteropIndex"));
        assert_eq!(exif_name_alias("Make"), None);
        assert_eq!(canon_main_name_alias("CanonOwnerName"), Some("OwnerName"));
        // Lens fields must NOT be remapped (post-pass depends on the name).
        assert_eq!(canon_main_name_alias("CanonLensType"), None);
    }

    #[test]
    fn afinfo_subtable_names() {
        let t = CanonSubTable::AfInfo;
        assert_eq!(canon_sub_tag_name(t, 0), Some("NumAFPoints"));
        assert_eq!(canon_sub_tag_name(t, 8), Some("AFAreaXPositions"));
        assert!(!t.has_byte_count_header()); // FIRST_ENTRY 0
    }

    #[test]
    fn make_string_dispatch() {
        assert_eq!(Vendor::from_make("Canon"), Some(Vendor::Canon));
        assert_eq!(Vendor::from_make("NIKON CORPORATION"), Some(Vendor::Nikon));
        assert_eq!(Vendor::from_make("FUJIFILM"), Some(Vendor::Fujifilm));
        assert_eq!(Vendor::from_make("GoPro"), None);
    }
}
