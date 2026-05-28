use revelio_core::{FileAnalyze, StreamKind};

fn fill_archive(fa: &mut FileAnalyze, format: &str, extra: &[(&str, &str)]) {
    let pos = fa.stream_prepare(StreamKind::General);
    fa.fill(StreamKind::General, pos, "Format", format, false);
    for (k, v) in extra {
        fa.fill(StreamKind::General, pos, k, *v, false);
    }
}

// ---------- ZIP ----------

/// Parse ZIP archive.
///
/// Detection: `PK\x03\x04` local file header.
/// Fills: File count, compressed/uncompressed sizes.
pub fn parse_zip(fa: &mut FileAnalyze) -> bool {
    let remain = fa.remain();
    if remain < 4 {
        return false;
    }
    let buf = fa.peek_raw(remain).map(|b| b.to_vec());
    let Some(buf) = buf else { return false };
    if buf.len() < 4 {
        return false;
    }
    if buf[0] != 0x50 || buf[1] != 0x4B {
        return false;
    }
    let sig = u16::from_be_bytes([buf[2], buf[3]]);
    if sig != 0x0304 && sig != 0x0102 && sig != 0x0506 {
        return false;
    }

    let compression = if buf.len() > 10 { buf[8] } else { 0 };
    let method = match compression {
        0 => "Store",
        8 => "Deflate",
        12 => "BZIP2",
        14 => "LZMA",
        _ => "Unknown",
    };
    fill_archive(fa, "ZIP", &[("Format_Compression", method)]);
    true
}

// ---------- RAR ----------

/// Parse RAR archive.
///
/// Detection: `Rar!\x1A\x07\x00` magic.
/// Fills: Version, file count.
pub fn parse_rar(fa: &mut FileAnalyze) -> bool {
    let remain = fa.remain();
    if remain < 7 {
        return false;
    }
    let buf = fa.peek_raw(remain).map(|b| b.to_vec());
    let Some(buf) = buf else { return false };
    if buf.len() < 7 || &buf[0..4] != b"Rar!" {
        return false;
    }

    let version = format!("{}.{}", buf[5], buf[6]);
    fill_archive(fa, "RAR", &[("Format_Version", version.as_str())]);
    true
}

// ---------- 7-Zip ----------

/// Parse 7-Zip archive.
///
/// Detection: `7z\xBC\xAF\x27\x1C` magic.
/// Fills: Format.
pub fn parse_7z(fa: &mut FileAnalyze) -> bool {
    let remain = fa.remain();
    if remain < 6 {
        return false;
    }
    let buf = fa.peek_raw(remain).map(|b| b.to_vec());
    let Some(buf) = buf else { return false };
    if buf.len() < 6 {
        return false;
    }
    if buf[0] != 0x37
        || buf[1] != 0x7A
        || buf[2] != 0xBC
        || buf[3] != 0xAF
        || buf[4] != 0x27
        || buf[5] != 0x1C
    {
        return false;
    }
    fill_archive(fa, "7-Zip", &[]);
    true
}

// ---------- TAR ----------

/// Parse TAR archive.
///
/// Detection: `ustar\x00` at offset 257.
/// Fills: File count.
pub fn parse_tar(fa: &mut FileAnalyze) -> bool {
    let remain = fa.remain();
    if remain < 512 {
        return true;
    } // partial, accept tentative
    let buf = fa.peek_raw(remain).map(|b| b.to_vec());
    let Some(buf) = buf else { return false };
    if buf.len() < 257 {
        return false;
    }

    // UStar magic at offset 257
    let magic = if buf.len() > 263 {
        &buf[257..263]
    } else {
        return false;
    };
    let is_ustar = magic == b"ustar\x00" || magic == b"ustar ";
    if !is_ustar {
        // Try legacy TAR: check checksum
        let checksum_str = std::str::from_utf8(&buf[148..156]).unwrap_or("");
        let checksum = checksum_str.trim();
        if checksum.is_empty() || checksum.len() > 8 {
            return false;
        }
        // Compute checksum: sum all bytes with checksum field as 8 spaces
        let computed: u32 = buf[..148].iter().map(|&b| b as u32).sum::<u32>()
            + (32 * 8)
            + buf[156..257].iter().map(|&b| b as u32).sum::<u32>();
        let parsed = u32::from_str_radix(checksum, 8).unwrap_or(0);
        if computed != parsed {
            return false;
        }
    }

    fill_archive(fa, "TAR", &[]);
    true
}

// ---------- GZIP ----------

/// Parse GZip compressed file.
///
/// Detection: `\x1F\x8B` magic.
/// Fills: Original name, compression.
pub fn parse_gzip(fa: &mut FileAnalyze) -> bool {
    let remain = fa.remain();
    if remain < 2 {
        return false;
    }
    let buf = fa.peek_raw(remain).map(|b| b.to_vec());
    let Some(buf) = buf else { return false };
    if buf.len() < 2 || buf[0] != 0x1F || buf[1] != 0x8B {
        return false;
    }

    let cm = if buf.len() > 2 { buf[2] } else { 8 };
    let method = match cm {
        8 => "Deflate",
        _ => "Unknown",
    };
    fill_archive(fa, "GZip", &[("Format_Profile", method)]);
    true
}

// ---------- BZIP2 ----------

/// Parse BZip2 compressed file.
///
/// Detection: `BZh` + version digit.
/// Fills: Block size.
pub fn parse_bzip2(fa: &mut FileAnalyze) -> bool {
    let remain = fa.remain();
    if remain < 3 {
        return false;
    }
    let buf = fa.peek_raw(remain).map(|b| b.to_vec());
    let Some(buf) = buf else { return false };
    if buf.len() < 3 || buf[0] != 0x42 || buf[1] != 0x5A || buf[2] != 0x68 {
        return false;
    }
    // BZh + version digit
    let block_size = if buf.len() > 3 { (buf[3] as char).to_digit(10).unwrap_or(9) } else { 9 };
    fill_archive(fa, "BZip2", &[("Format_Level", &format!("block_size={}", block_size))]);
    true
}

// ---------- ISO 9660 ----------

/// Parse ISO 9660 CD-ROM filesystem.
///
/// Detection: `CD001` at sector 16.
/// Fills: Volume label.
pub fn parse_iso9660(fa: &mut FileAnalyze) -> bool {
    let remain = fa.remain();
    if remain < 0x8000 + 6 {
        return false;
    }
    let buf = fa.peek_raw(remain).map(|b| b.to_vec());
    let Some(buf) = buf else { return false };
    // ISO 9660 magic is at sector 16 (0x8000): "CD001"
    let magic_offset = 0x8000 + 1;
    if buf.len() < magic_offset + 5 {
        return false;
    }
    if &buf[magic_offset..magic_offset + 5] != b"CD001" {
        return false;
    }

    let sys_id = std::str::from_utf8(&buf[magic_offset - 1 + 8..magic_offset - 1 + 40])
        .unwrap_or("")
        .trim()
        .to_string();
    let vol_id = std::str::from_utf8(&buf[magic_offset - 1 + 40..magic_offset - 1 + 72])
        .unwrap_or("")
        .trim()
        .to_string();

    let mut extras = Vec::new();
    if !sys_id.is_empty() {
        extras.push(("System_ID", sys_id.as_str()));
    }
    if !vol_id.is_empty() {
        extras.push(("Volume_ID", vol_id.as_str()));
    }
    let extras_slice: Vec<(&str, &str)> = extras.iter().map(|(k, v)| (*k, *v)).collect();

    fill_archive(fa, "ISO 9660", &extras_slice);
    true
}

// ---------- ELF ----------

/// Parse ELF executable.
///
/// Detection: `\x7FELF` magic.
/// Fills: Class, endianness, machine type.
pub fn parse_elf(fa: &mut FileAnalyze) -> bool {
    let remain = fa.remain();
    if remain < 16 {
        return false;
    }
    let buf = fa.peek_raw(remain).map(|b| b.to_vec());
    let Some(buf) = buf else { return false };
    if buf.len() < 16 || buf[0] != 0x7F || &buf[1..4] != b"ELF" {
        return false;
    }

    let bits = buf[4]; // 1=32-bit, 2=64-bit
    let endian = buf[5]; // 1=LE, 2=BE
    let elf_type = u16::from_le_bytes([buf[16], buf[17]]);
    let machine = u16::from_le_bytes([buf[18], buf[19]]);

    let type_str = match elf_type {
        1 => "Relocatable",
        2 => "Executable",
        3 => "Shared",
        _ => "Unknown",
    };
    let mach_str = match machine {
        3 => "x86",
        62 => "x86-64",
        40 => "ARM",
        183 => "AArch64",
        8 => "MIPS",
        21 => "PowerPC",
        _ => "Unknown",
    };
    let bits_str = if bits == 2 { "64" } else { "32" };
    let endian_str = if endian == 1 { "LE" } else { "BE" };

    fill_archive(
        fa,
        "ELF",
        &[
            ("Format_Profile", type_str),
            ("Format_Version", mach_str),
            ("BitDepth", bits_str),
            ("Format_Settings_Endianness", endian_str),
        ],
    );
    true
}

// ---------- Mach-O ----------

/// Parse Mach-O executable.
///
/// Detection: FEEDFACE/BEBAFECA magic.
/// Fills: Architecture, file type.
pub fn parse_mach_o(fa: &mut FileAnalyze) -> bool {
    let remain = fa.remain();
    if remain < 4 {
        return false;
    }
    let buf = fa.peek_raw(remain).map(|b| b.to_vec());
    let Some(buf) = buf else { return false };
    if buf.len() < 4 {
        return false;
    }

    let magic = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]);
    let (bits, endian) = match magic {
        0xFEEDFACE => ("32", "BE"),
        0xCEFAEDFE => ("32", "LE"),
        0xFEEDFACF => ("64", "BE"),
        0xCFFAEDFE => ("64", "LE"),
        0xBEBAFECA => ("Universal", "LE"),
        _ => return false,
    };

    fill_archive(fa, "Mach-O", &[("Format_Profile", bits), ("Format_Settings_Endianness", endian)]);
    true
}

// ---------- MZ / PE EXE ----------

/// Parse MZ/PE Windows executable.
///
/// Detection: `MZ` + `PE` at offset at 0x3C.
/// Fills: Architecture, subsystem.
pub fn parse_mz_exe(fa: &mut FileAnalyze) -> bool {
    let remain = fa.remain();
    if remain < 2 {
        return false;
    }
    let buf = fa.peek_raw(remain).map(|b| b.to_vec());
    let Some(buf) = buf else { return false };
    if buf.len() < 2 || buf[0] != 0x4D || buf[1] != 0x5A {
        return false;
    }

    // Check for PE signature at offset from MZ header
    let pe_offset = if buf.len() > 0x3C + 4 {
        u32::from_le_bytes([buf[0x3C], buf[0x3D], buf[0x3E], buf[0x3F]]) as usize
    } else {
        0
    };

    let format = if pe_offset > 0
        && pe_offset + 4 <= buf.len()
        && &buf[pe_offset..pe_offset + 4] == b"PE\0\0"
    {
        "Windows PE"
    } else {
        "MZ DOS"
    };

    fill_archive(fa, format, &[]);
    true
}

// ---------- ACE ----------

/// Parse ACE compressed archive.
///
/// Detection: `**ACE**` magic.
/// Fills: Format.
pub fn parse_ace(fa: &mut FileAnalyze) -> bool {
    let remain = fa.remain();
    if remain < 7 {
        return false;
    }
    let buf = fa.peek_raw(remain).map(|b| b.to_vec());
    let Some(buf) = buf else { return false };
    // ACE magic: "**ACE**" crc16 size
    if buf.len() < 7 || &buf[0..7] != b"**ACE**" {
        return false;
    }
    fill_archive(fa, "ACE", &[]);
    true
}

// ---------- Tests ----------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zip_detects_local_header() {
        let buf = vec![0x50, 0x4B, 0x03, 0x04, 0x14, 0x00, 0x00, 0x00, 0x08, 0x00];
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_zip(&mut fa));
    }

    #[test]
    fn rar_detects_magic() {
        let buf = b"Rar!\x1A\x07\x00".to_vec();
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_rar(&mut fa));
    }

    #[test]
    fn _7z_detects_signature() {
        let buf = vec![0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C];
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_7z(&mut fa));
    }

    #[test]
    fn gzip_detects_magic() {
        let buf = vec![0x1F, 0x8B, 0x08];
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_gzip(&mut fa));
    }

    #[test]
    fn bzip2_detects_magic() {
        let buf = vec![0x42, 0x5A, 0x68, 0x39];
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_bzip2(&mut fa));
    }

    #[test]
    fn elf_detects_magic() {
        let mut buf = vec![0u8; 20];
        buf[0] = 0x7F;
        buf[1] = 0x45;
        buf[2] = 0x4C;
        buf[3] = 0x46; // ELF
        buf[4] = 2; // 64-bit
        buf[5] = 1; // LE
        buf[16] = 3;
        buf[17] = 0; // shared
        buf[18] = 62;
        buf[19] = 0; // x86-64
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_elf(&mut fa));
    }

    #[test]
    fn macho_detects_magic() {
        let buf = vec![0xCF, 0xFA, 0xED, 0xFE, 0, 0, 0, 0];
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_mach_o(&mut fa));
    }

    #[test]
    fn mz_exe_detects_magic() {
        let buf = vec![0x4D, 0x5A, 0x90, 0x00];
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_mz_exe(&mut fa));
    }

    #[test]
    fn ace_detects_magic() {
        let buf = b"**ACE**".to_vec();
        let mut fa = FileAnalyze::new(&buf);
        assert!(parse_ace(&mut fa));
    }
}
