pub mod archives;

pub use archives::{
    parse_zip, parse_rar, parse_7z, parse_tar, parse_gzip, parse_bzip2,
    parse_iso9660, parse_elf, parse_mach_o, parse_mz_exe, parse_ace,
};
