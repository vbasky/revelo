//! Metadata tag parsers: ID3v1, ID3v2, APE tags, Vorbis Comment, Lyrics3.
//!
//! These parse embedded metadata within media files rather than
//! standalone formats. Each parser returns `Option<u32>` (tag size in
//! bytes) or `bool` indicating whether tag data was found.

#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]

pub mod tags;

pub use tags::{
    parse_id3v1, parse_id3v2, parse_ape_tag, parse_vorbis_comment,
    parse_lyrics3, parse_tags,
};
