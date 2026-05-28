# Field Coverage Reference

Every field revelio's parsers write into the output stream, organized by
`StreamKind`. Generated from the codebase — grep through all `fa.Fill()`
call sites across every parser crate.

**Total unique fields: 185 across 6 stream kinds.** Last updated: 2026-05-28.

---

## General

Fields written at the container/file level.

| Field | Source | Notes |
|---|---|---|
| `Format` | All container parsers | e.g. "MPEG-4", "Matroska", "AVI" |
| `Format_Info` | Container-specific | "ISO Base Media" for MP4, "Enhanced Module" for audio modules |
| `Format_Profile` | ftyp brand mapping + post-parse | "Base Media / Version 2" from mp42/isom |
| `Format_Version` | Container header | Version number (Matroska, MP4 variant) |
| `CodecID` | Container header | ftyp major_brand, EBML DocType, etc. |
| `CodecID_Version` | Container metadata | e.g. Matroska DocType version |
| `FileSize` | File system | Byte count of input file |
| `FileExtension` | File system | Extension derived from path |
| `Duration` | Container metadata | In seconds (from mdhd/mvhd/Info/Segment duration) |
| `OverallBitRate` | Computed from StreamSize + Duration | Bits per second |
| `OverallBitRate_Mode` | Inferred from per-stream modes | "VBR" or "CBR" |
| `OverallBitRate_Maximum` | Post-parse aggregation | Sum of per-stream BitRate_Maximum |
| `OverallBitRate_Minimum` | Post-parse aggregation | Minimum per-stream BitRate |
| `FrameRate` | Container header or first video stream | FPS |
| `FrameCount` | Container | Total frames across all video |
| `StreamSize` | Payload tracking | Total byte count |
| `AudioCount` | Container | Number of audio streams |
| `VideoCount` | Container | Number of video streams |
| `TextCount` | Container | Number of subtitle/text streams |
| `MenuCount` | Container | Number of menu streams |
| `ImageCount` | Container | Number of image streams |
| `Encoded_Application` | Writing/muxing application | From mkv MuxingApp, MP4 ©too |
| `Encoded_Application_Name` | Application name separated | From application string |
| `Encoded_Library` | Writing library | e.g. "Lavf61.7.100" |
| `Encoded_Library_Settings` | Encoder settings | From x264/x265 SEI or container |
| `Encoded_Date` | mdat/mvhd creation time | 1904/UTC epoch conversion |
| `Encoded_By` | Container metadata | e.g. iTunMOVI writer |
| `Tagged_Date` | Container metadata | Modification date |
| `Encoded_Hardware_CompanyName` | Manufacture metadata | From Make field |
| `Encoded_Hardware_Model` | Hardware metadata | From Model field |
| `Recorded_Date` | Creation/original date | From DateTimeOriginal |
| `Mastered_Date` | Mastered/encoding date | e.g. from mov metadata |
| `Recorded_Location` | GPS metadata | From EXIF GPS |
| `TimeCode_FirstFrame` | Timecode track | Start timecode |
| `TimeCode_Source` | Timecode source | e.g. "Container" |
| `ServiceKind` | Audio metadata | e.g. "Main" / "Commentary" |
| `IsStreamable` | Container hint | Fast-start flag, web-optimized |
| `IsStreamable_Source` | Source of streamability | "Container" |
| `HDVideo` | Derived from height | Flag for HD resolution |
| `Cover` | Cover art present | Boolean |
| `Cover_Mime` | Cover MIME type | "image/jpeg", "image/png" |
| `Cover_Type` | Cover art type tag | e.g. "Cover (front)" |
| `Interleaved` | Container hint | Audio/video interleave |
| `Grouping` | Chapter grouping | Chapter edition info |
| `Title` | Metadata | From container or tags |
| `Album` | Metadata | From tags |
| `Artist` | Metadata | From tags |
| `Performer` | Metadata | From tags |
| `Composer` | Metadata | From tags |
| `Genre` | Metadata | From tags |
| `Track` | Metadata | Track number |
| `Part` | Metadata | Part number |
| `Part_Position` | Metadata | Part position in set |
| `Movie` | Metadata | Movie/series name |
| `TVShow` | Metadata | TV show name |
| `TVEpisode` | Metadata | Episode identifier |
| `BPM` | Metadata | Beats per minute from tags |
| `Lyrics` | Metadata | From Lyrics3 or VorbisComment |
| `Comment` | Metadata | From any tag source |
| `Description` | Metadata | From various metadata sources |
| `Copyright` | Metadata | From container or tags |
| `Keywords` | Metadata | From tags |
| `Subject` | Metadata | From container tags |
| `CatalogNumber` | Metadata | From tags |
| `Medium` | Metadata | From container or tags |
| `Product` | Metadata | e.g. from RIFF Info |
| `Source` | Metadata | Source media |
| `Publisher` | Metadata | Publishing label |
| `ProductionStudio` | Metadata | Studio name |
| `Make` | Metadata | Equipment make (EXIF) |
| `Model` | Metadata | Equipment model (EXIF) |
| `WriterVersion` | Encoder version | From container metadata |
| `Compilation` | Metadata | Compilation flag |
| `Gapless` | Audio metadata | Gapless playback flag |
| `Encryption` | DRM info | Present flag |
| `Encryption_Format` | DRM system | e.g. "DRM" / "FPS" |
| `Encryption_Length` | DRM encrypted size | Bytes |
| `Encryption_Method` | DRM method | Encryption algorithm |
| `Encryption_Mode` | DRM mode | e.g. "CBC" |
| `Encryption_Padding` | DRM padding | Padding scheme |
| `Truncated` | File integrity | if truncated |
| `UniqueID` | Container | Matroska/MP4 unique ID |
| `MenuID` | Menu track | Chapter/menu identifier |
| `Reel_Position` | Container metadata | e.g. from MOV reel |
| `Reader` | I/O layer | "File" / "Directory" / "HTTP" / "MMS" |

## Audio

| Field | Source | Notes |
|---|---|---|
| `Format` | Parser | "AAC", "MPEG Audio", "FLAC", "Opus", etc. |
| `Format_Info` | Stream header | e.g. "Advanced Audio Codec" |
| `Format_Profile` | Codec config | "LC", "HE-AAC", "Main", "Core" |
| `Format_Version` | Codec version | Version number |
| `Format_Settings` | Codec flags | e.g. Endianness, Floating |
| `Format_Settings_Endianness` | PCM parser | "Big" / "Little" |
| `Format_Settings_Floating` | PCM parser | "Yes" when float |
| `Format_Settings_Sign` | PCM parser | "Signed" / "Unsigned" |
| `Format_Settings_Mode` | Joint stereo/etc. | e.g. "Joint Stereo" |
| `Format_Settings_ModeExtension` | Mode details | e.g. "Intensity Stereo" |
| `Format_Settings_Emphasis` | Emphasis flag | "50/15ms" / "CCITT J.17" |
| `Format_Settings_Floor` | Vorbis codec | "Floor0" / "Floor1" |
| `Format_AdditionalFeatures` | Extended features | e.g. DTS-HD Master Audio |
| `Format_Commercial_IfAny` | Commercial brand | e.g. Dolby branding |
| `Codec` | 2CC/4CC | Short codec identifier |
| `Codec_Settings` | Codec-specific | e.g. Dolby E program config |
| `CodecID` | Container mapping | e.g. "A_OPUS", "mp4a" |
| `Duration` | Container / estimate | Seconds |
| `BitRate` | Bitrate fields | bps (VBR average) |
| `BitRate_Maximum` | Container hint | esds maxBitrate or equivalent |
| `BitRate_Mode` | VBR/CBR | "VBR" or "CBR" |
| `Channels` | Stream header | Channel count |
| `ChannelPositions` | Channel layout | "Front: L C R" etc. |
| `ChannelLayout` | Standard layout | e.g. "L R C LFE Ls Rs" |
| `SamplingRate` | Stream header | Hz |
| `SamplingCount` | Sample count | Total PCM samples |
| `BitDepth` | Stream header | Bit depth |
| `SamplesPerFrame` | Codec-specific | AAC/MP3 frame size |
| `FrameCount` | Stream analysis | Total audio frames |
| `FrameRate` | Derived | Audio frame rate |
| `Compression_Mode` | Lossy/Lossless | From codec |
| `StreamSize` | Payload tracking | Bytes |
| `StreamOrder` | Stream ordering | Stream index |
| `ID` | Stream ID | Per-stream identifier |
| `UniqueID` | Container | Stream UUID |
| `Language` | Language descriptor | ISO 639-2 |
| `Default` | Default flag | Forced/default stream |
| `Forced` | Forced flag | Forced subtitle/audio |
| `Title` | Metadata | Stream title |
| `Encoded_Date` | Creation date | Per stream |
| `Tagged_Date` | Modification date | Per stream |
| `Encoded_Library` | Encoder string | e.g. "LAME3.100" |
| `Encoded_Library_Settings` | Encoder settings | LAME preset etc. |
| `Delay` | Encoder delay | Samples |
| `Delay_Source` | How delay was determined | "Container" / "Stream" |
| `Video_Delay` | Audio/video sync | ms delay relative to video |
| `Alignment` | Frame alignment | "Aligned" / "Split" |
| `ServiceKind` | Audio service | "Main" / "Commentary" / etc. |
| `ReplayGain_Gain` | LAME Gaia / ID3v2 | Track replay gain (dB) |
| `ReplayGain_Peak` | LAME Gaia / ID3v2 | Track peak (float) |
| `MD5_Unencoded` | FLAC STREAMINFO | Unencoded audio MD5 |
| `Encryption` | DRM flag | Encrypted stream |
| `TimeCode_FirstFrame` | Timecode | Start timecode |
| `TimeCode_LastFrame` | Timecode | End timecode |
| `acmod` | AC-3/Dolby | Audio coding mode |
| `bsid` | AC-3/Dolby | Bitstream ID |
| `lfeon` | AC-3/Dolby | LFE channel present |
| `dialnorm` | AC-3/Dolby | Dialogue normalization |
| `dialnorm_Average` | AC-3/Dolby | Average dialogue level |
| `dialnorm_Minimum` | AC-3/Dolby | Minimum dialogue level |
| `dsurmod` | AC-3/Dolby | Dolby Surround mode |
| `Codec` (short) | 2CC/4CC | Short codec identifier |

## Video

| Field | Source | Notes |
|---|---|---|
| `Format` | Parser | "AVC", "HEVC", "AV1", "VP9", etc. |
| `Format_Info` | Stream header | Descriptive name |
| `Format_Profile` | Codec config | "High@L4.1", "Main 10" |
| `Format_Level` | Level indicator | "4.1", "5.0" |
| `Format_Tier` | HEVC/VVC tier | "Main" / "High" |
| `Format_Version` | Version | Codec version |
| `Format_Settings` | Flags | Combined settings |
| `Format_Settings_CABAC` | AVC entropy | "Yes" / "No" |
| `Format_Settings_RefFrames` | SPS | Number of reference frames |
| `Format_Settings_GOP` | GOP analysis | "M=3, N=12" |
| `Format_Settings_BVOP` | MPEG-4 Visual | B-frames used |
| `Format_Settings_QPel` | MPEG-4 Visual | Quarter-pixel motion |
| `Format_Settings_GMC` | MPEG-4 Visual | Global motion compensation |
| `Format_Settings_Matrix` | MPEG-4 Visual | Quantization matrix |
| `Codec` | 4CC | Short codec identifier |
| `CodecID` | Container mapping | "avc1", "V_MPEG4/ISO/AVC" |
| `Width` | Stream header | Display width |
| `Height` | Stream header | Display height |
| `Sampled_Width` | Coding resolution | Coded width |
| `Sampled_Height` | Coding resolution | Coded height |
| `DisplayAspectRatio` | SAR + dimensions | e.g. "1.778" |
| `PixelAspectRatio` | SAR | e.g. "1.000" |
| `BitDepth` | Stream header | 8/10/12 bit |
| `ChromaSubsampling` | Stream header | "4:2:0" / "4:2:2" / "4:4:4" |
| `ColorSpace` | Colour info | "YUV" / "RGB" |
| `ScanType` | Field info | "Progressive" / "Interlaced" |
| `ScanOrder` | Interlacement | "TFF", "BFF", "Mixed" |
| `Duration` | Container / estimate | Seconds |
| `BitRate` | Computed | bps |
| `BitRate_Maximum` | Container hint | From max bitrate field |
| `BitRate_Minimum` | Post-parse | Half of nominal |
| `BitRate_Mode` | VBR/CBR | Bitrate mode |
| `BitRate_Nominal` | Nominal rate | From container or codec |
| `Bits_Pixel_Frame` | Computed | BitRate ÷ (W × H × FR) |
| `Compression_Mode` | Lossy/Lossless | "Lossy" / "Lossless" |
| `FrameRate` | Stream header | e.g. "25.000" |
| `FrameRate_Mode` | Rate type | "CFR" / "VFR" |
| `FrameRate_Mode_Original` | Original before CFR override | "CFR" / "VFR" |
| `FrameRate_Num` | Rate numerator | e.g. 24000 |
| `FrameRate_Den` | Rate denominator | e.g. 1001 |
| `FrameCount` | Container/analyse | Total frames |
| `StreamSize` | Payload | Bytes in stream |
| `StreamOrder` | Container | Stream index |
| `ID` | Container | Track/stream ID |
| `UniqueID` | Matroska | Per-track UUID |
| `Language` | Descriptor | ISO 639-2 |
| `Default` | Container | Default stream |
| `Forced` | Container | Forced stream |
| `Title` | Metadata | Track title |
| `Encoded_Date` | Container | Creation date |
| `Tagged_Date` | Container | Modification date |
| `Encoded_Library` | SEI / codec | "x264 core 164", "x265 3.5+1" |
| `Encoded_Library_Name` | SEI decomposition | "x264", "x265" |
| `Encoded_Library_Version` | SEI decomposition | Version string |
| `Encoded_Library_Settings` | SEI decomposition | "cabac=1 / ref=5 / …" |
| `Delay` | Container | Sync delay |
| `GOP_Detect` | AVC slice analysis | e.g. "M=3, N=12" |
| `colour_range` | SPS VUI | "Limited" / "Full" |
| `colour_range_Source` | Source hint | "Container / Stream" |
| `colour_primaries` | SPS VUI | "BT.709", "BT.2020", etc. |
| `colour_primaries_Source` | Source hint | |
| `transfer_characteristics` | SPS VUI | "PQ", "HLG", "BT.709", etc. |
| `transfer_characteristics_Source` | Source hint | |
| `matrix_coefficients` | SPS VUI | "BT.709", "BT.2020 non-constant" |
| `matrix_coefficients_Source` | Source hint | |
| `colour_description_present` | SPS VUI | Colour desc presence flag |
| `colour_description_present_Source` | Source hint | |
| `HDR_Format` | SEI / container | "SMPTE ST 2086", "Dolby Vision" |
| `HDR_Format_Compatibility` | Profile info | "HDR10", "BL:4" |
| `HDR_Format_Profile` | DV profile | "Dolby Vision 8.1" |
| `HDR_Format_Level` | DV level | Level number |
| `HDR_Format_Version` | Version | e.g. "1.0" |
| `MasteringDisplay_ColorPrimaries` | HDR10 SEI | Display primaries |
| `MasteringDisplay_Luminance` | HDR10 SEI | Min/max luminance |
| `MasteringDisplay_Luminance_Max` | Decomposed | Max luminance only |
| `MasteringDisplay_Luminance_Min` | Decomposed | Min luminance only |
| `MaxCLL` | HDR10 SEI | Max content light level |
| `MaxFALL` | HDR10 SEI | Max frame average light level |

## Text / Subtitles

| Field | Source | Notes |
|---|---|---|
| `Format` | Parser | "SubRip", "PGS", "WebVTT", "EIA-608", etc. |
| `Format_Info` | Detailed info | "UTF-8", "Presentation Graphic Stream" |
| `Codec` | Codec ID | Short identifier |
| `Language` | Descriptor | ISO 639-2 |
| `Language_More` | Additional languages | Multiple language codes |
| `MuxingMode` | Multiplex info | e.g. "zlib" / "Muxed in Video" |

## Image

| Field | Source | Notes |
|---|---|---|
| `Format` | Parser | "JPEG", "PNG", "TIFF", "BMP", etc. |
| `Format_Profile` | Profile | e.g. "Baseline", "Progressive" |
| `Format_Version` | Version | Format version number |
| `Format_Compression` | Compression | "Lossy", "Lossless", "LZW", "RLE" |
| `Format_Settings_Endianness` | Byte order | "Big" / "Little" |
| `Format_Settings_Packing` | Packing | Bits packing info |
| `Width` | Image header | Pixel width |
| `Height` | Image header | Pixel height |
| `BitDepth` | Image header | e.g. 8, 24, 32 |
| `DisplayAspectRatio` | Dimensions | Width/Height ratio |
| `PixelAspectRatio` | SAR | e.g. "1.000" |
| `ColorSpace` | Colour space | "YUV", "RGB", "CMYK", "Grayscale" |
| `ChromaSubsampling` | JPEG | "4:4:4" / "4:2:2" / "4:2:0" |
| `Compression_Mode` | Lossy/Lossless | |
| `StreamSize` | Payload | Bytes |
| `CodecID` | Codec 4CC | e.g. "JPEG", "PNG " |
| `Encoded_Library` | Software | From EXIF Software tag |
| `WriterVersion` | Software | From EXIF Software tag |
| `colour_description_present` | ICC/colour | Flag |
| `colour_primaries` | ICC/colour | |
| `Density_X` | Physical size | DPI/pixel density X |
| `Density_Y` | Physical size | DPI/pixel density Y |
| `Density_Unit` | Unit indicator | "ppi" / "dpm" / "dpcm" |
| `Type` | Image type | "Cover", "Thumbnail" |
| `AlternateHdrHeadroom` | AVIF gain map | Alternate headroom |
| `BaseHdrHeadroom` | AVIF gain map | Base headroom |
| `MuxingMode` | Encoding method | Encoding mode string |

## Other / Menu

**Other streams** (timed metadata, timecodes, teletext):

| Field | Source | Notes |
|---|---|---|
| `Format` | Parser | "TimeCode", "Teletext", etc. |
| `CodecID` | Container | Stream codec ID |
| `ID` | Container | Stream ID |
| `Title` | Metadata | Stream title |
| `Type` | Metadata | Usage type |
| `Default` | Container | Default stream flag |
| `Encoded_Date` | Container | Creation date |
| `Tagged_Date` | Container | Modification date |
| `Language` | Container | ISO 639-2 |
| `BitRate_Mode` | Container | Rate mode |
| `StreamSize` | Payload | Bytes |
| `FrameCount` | Analysis | Total frames |
| `Compression_Ratio` | Computed | Post-parse |

**Menu streams** (chapters, DVD/BD menus):

| Field | Source | Notes |
|---|---|---|
| `Format` | Container | "Menu", "Text" |
| `ID` | Container | Chapter/menu ID |
| `MenuID` | Container | Menu identifier |
| `StreamOrder` | Container | Display order |

---

## Computed Fields

These are calculated in a post-parse pass, not by individual parsers:

| Field | Formula | Stream Kind |
|---|---|---|
| `Bits_Pixel_Frame` | BitRate ÷ (Width × Height × FrameRate) | Video |
| `Compression_Ratio` | Uncompressed ÷ StreamSize | Audio, Video |
| `FrameRate_Mode_Original` | Copies FrameRate_Mode before any override | Video |
| `Format_Profile` (General) | ftyp major_brand → label | General |
| `BitRate_Minimum` | BitRate ÷ 2 (when not parser-filled) | Audio, Video |
| `OverallBitRate_Maximum` | Sum of per-stream BitRate_Maximum | General |
| `OverallBitRate_Minimum` | Min of per-stream BitRate | General |

## Tag-Sourced Fields

Fields populated from embedded metadata (ID3v2, VorbisComment, APE Tag,
EXIF, XMP, Lyrics3, QuickTime udta):

| Field | Tag sources |
|---|---|
| `Title` | ID3v2 TIT2, VorbisComment TITLE, iTunes ©nam, APE Title |
| `Artist` | ID3v2 TPE1, VorbisComment ARTIST, APE Artist |
| `Album` | ID3v2 TALB, VorbisComment ALBUM, APE Album |
| `Genre` | ID3v2 TCON, VorbisComment GENRE, APE Genre |
| `Track` | ID3v2 TRCK, VorbisComment TRACKNUMBER |
| `Composer` | ID3v2 TCOM, VorbisComment COMPOSER |
| `Performer` | VorbisComment PERFORMER, ID3v2 TPE2 |
| `Comment` | ID3v2 COMM, VorbisComment COMMENT |
| `Copyright` | ID3v2 TCOP, VorbisComment COPYRIGHT, RIFF ICOP |
| `Description` | ID3v2 TIT3 (subtitle), QuickTime desc |
| `Cover` / `Cover_Mime` / `Cover_Type` | ID3v2 APIC, VorbisComment METADATA_BLOCK_PICTURE, iTunes covr |
| `Lyrics` | Lyrics3 LYR200, VorbisComment LYRICS |
| `BPM` | ID3v2 TBPM, VorbisComment BPM |
| `Compilation` | ID3v2 TCMP, VorbisComment COMPILATION |
| `ReplayGain_Gain` / `ReplayGain_Peak` | LAME Gaia header (MP3 Xing), ID3v2 TXXX "replaygain_*" |
| `Encoded_Date` | ID3v2 TDRC, VorbisComment DATE, QuickTime ©day, MP4 mvhd |
| `Tagged_Date` | QuickTime ©day (modification), MP4 mvhd modification time |
| `Recorded_Date` | EXIF DateTimeOriginal, QuickTime com.apple.quicktime.creationdate |
| `Recorded_Location` | EXIF GPS IFD → latitude/longitude |
| `Make` / `Model` | EXIF IFD tags 0x010F / 0x0110 |
| `Encoded_Library` | MP3 LAME info frame, x264/x265 SEI string |
| `Encoded_Library_Settings` | LAME preset/extras, x264 option string |
