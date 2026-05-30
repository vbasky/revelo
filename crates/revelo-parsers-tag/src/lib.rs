//! Metadata tag parsers: ID3v1, ID3v2, APE tags, Vorbis Comment, Lyrics3,
//! EXIF, XMP, ICC profiles, C2PA, IIM/IPTC, Apple PropertyList,
//! SphericalVideo.
//!
//! These parse embedded metadata within media files. Each parser
//! returns `Option<u32>` (tag size), `bool`, or nothing for VorbisComment.

#![allow(non_snake_case)]
#![deny(unsafe_code)]

pub mod tags;

pub use tags::{
    parse_ape_tag, parse_c2pa, parse_exif, parse_icc, parse_id3v1, parse_id3v2, parse_iim,
    parse_iim_buf, parse_lyrics3, parse_property_list, parse_spherical_video, parse_tags,
    parse_vorbis_comment, parse_vorbis_comment_from_buf, parse_xmp,
};
