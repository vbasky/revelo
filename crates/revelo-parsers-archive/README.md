# revelo-parsers-archive

Archive, compressed-file, and executable-format parsers for
[**revelo**](https://github.com/vbasky/revelo) — a fast, safe, pure-Rust port of
[MediaInfoLib](https://mediaarea.net/en/MediaInfo). This crate covers the
probe-and-parse layer for archive containers, compression wrappers, and native
executable formats: it detects a file's format from its magic bytes and fills
the `FileAnalyze` stream graph with format, version, and structural fields.

Part of the [**revelo**](https://github.com/vbasky/revelo) project — see the
[project README](https://github.com/vbasky/revelo#readme) for the full picture.

## Normal use

Most users should depend on the [`revelo`](https://crates.io/crates/revelo)
facade crate rather than this crate directly. The facade re-exports every parser
and wires them into the dispatcher automatically.

## Supported formats

### Archives and compression

| Function | Format | Detection magic |
| --- | --- | --- |
| `parse_zip` | ZIP | `PK\x03\x04` local file header |
| `parse_rar` | RAR (v4 / v5) | `Rar!\x1A\x07` |
| `parse_7z` | 7-Zip | `7z\xBC\xAF\x27\x1C` |
| `parse_tar` | TAR (ustar / legacy) | `ustar` at offset 257, or checksum |
| `parse_gzip` | GZip | `\x1F\x8B` |
| `parse_bzip2` | BZip2 | `BZh` + block-size digit |
| `parse_ace` | ACE | `**ACE**` |
| `parse_iso9660` | ISO 9660 CD-ROM filesystem | `CD001` at sector 16 (0x8001) |

### Executables and shared objects

| Function | Format | Detection magic |
| --- | --- | --- |
| `parse_elf` | ELF (Linux/BSD — 32/64-bit, LE/BE, x86/x86-64/ARM/AArch64/MIPS/PPC) | `\x7FELF` |
| `parse_mach_o` | Mach-O (macOS/iOS — 32/64-bit, LE/BE, fat/universal binary) | `FEEDFACE` / `CEFAEDFE` / `FEEDFACF` / `CFFAEDFE` / `BEBAFECA` |
| `parse_mz_exe` | MZ DOS / Windows PE | `MZ` + `PE\0\0` at offset from header |

## Usage

```no_run
use revelo_parsers_archive::parse_zip;
use revelo_core::FileAnalyze;

let data: Vec<u8> = std::fs::read("archive.zip").unwrap();
let mut fa = FileAnalyze::new(&data);
if parse_zip(&mut fa) {
    // fa now contains a General stream with Format = "ZIP"
    // and Format_Compression set to "Store", "Deflate", "BZIP2", or "LZMA"
}
```

Prefer the `revelo` facade for everyday use — it handles format detection and
dispatches to the right parser automatically.

## Safety

`#![deny(unsafe_code)]` — zero unsafe blocks.

## License

BSD-2-Clause — see [LICENSE](https://github.com/vbasky/revelo/blob/main/LICENSE).
