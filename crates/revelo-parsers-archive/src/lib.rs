#![deny(unsafe_code)]

pub mod archives;

pub use archives::{
    parse_7z, parse_ace, parse_bzip2, parse_elf, parse_gzip, parse_iso9660, parse_mach_o,
    parse_mz_exe, parse_rar, parse_tar, parse_zip,
};
