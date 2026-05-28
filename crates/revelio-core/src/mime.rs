/// MIME type mapping from container/codec to IANA media type.
pub fn mime_for_container(fourcc: &str) -> Option<&'static str> {
    Some(match fourcc {
        "mp42" | "isom" | "avc1" => "video/mp4",
        "qt  " => "video/quicktime",
        "3gp4" => "video/3gpp",
        "mkv" | "matroska" | "webm" => "video/x-matroska",
        "avi" | "AVI " => "video/x-msvideo",
        "wav" | "WAVE" => "audio/wav",
        "flv" | "FLV " => "video/x-flv",
        "ogg" | "OggS" => "audio/ogg",
        "asf " | "WMA " | "WMV " => "video/x-ms-asf",
        "mpeg" | "ts" => "video/mp2t",
        "dvd" | "vob" => "video/mpeg",
        "heic" | "mif1" => "image/heif",
        "jp2 " => "image/jp2",
        "MXF " => "application/mxf",
        _ => return None,
    })
}
pub fn mime_for_codec(fourcc: &str) -> Option<&'static str> {
    Some(match fourcc {
        "mp4a" => "audio/mp4",
        "avc1" | "avc3" | "h264" => "video/H264",
        "hvc1" | "hev1" | "h265" => "video/H265",
        "av01" => "video/AV1",
        "vp09" | "vp9 " | "vp8 " => "video/VP9",
        "mp3 " => "audio/mpeg",
        "aac " | "AAC " => "audio/aac",
        "ac-3" | "ac3 " => "audio/ac3",
        "ec-3" | "eac3" => "audio/eac3",
        "dts " | "dtsc" => "audio/dts",
        "flac" => "audio/flac",
        "opus" => "audio/opus",
        "vorb" => "audio/vorbis",
        "theo" => "video/theora",
        "jpeg" => "image/jpeg",
        "png " => "image/png",
        _ => return None,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_mp4() {
        assert_eq!(mime_for_container("mp42"), Some("video/mp4"));
    }
    #[test]
    fn test_heif() {
        assert_eq!(mime_for_container("heic"), Some("image/heif"));
    }
    #[test]
    fn test_av1() {
        assert_eq!(mime_for_codec("av01"), Some("video/AV1"));
    }
    #[test]
    fn test_unknown() {
        assert_eq!(mime_for_container("xunk"), None);
    }
}
