use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Serialize;

use crate::config::{BenchCase, Manifest};
use crate::util::{Result, err, run_checked, safe_id, which};

pub(crate) const MIN_SIZE: u64 = 4096;

#[derive(Debug, Serialize)]
pub(crate) struct FixtureList {
    pub(crate) fixtures: Vec<FixtureRecord>,
}

#[derive(Debug, Serialize)]
pub(crate) struct FixtureRecord {
    pub(crate) id: String,
    pub(crate) path: String,
    pub(crate) size_bytes: u64,
}

#[derive(Debug, Clone)]
struct FfmpegFixture {
    extension: &'static str,
    command: Vec<&'static str>,
    major_brand: Option<&'static [u8; 4]>,
}

pub(crate) fn generate_manifest_fixtures(
    manifest: &Manifest,
    out_dir: &Path,
) -> Result<FixtureList> {
    let mut fixtures = Vec::new();
    for case in &manifest.cases {
        if case.synthetic.is_some() {
            let path = generate_case_fixture(case, out_dir)?;
            fixtures.push(FixtureRecord {
                id: case.id.clone().unwrap_or_else(|| safe_id(&case.label)),
                size_bytes: path.metadata()?.len(),
                path: path.display().to_string(),
            });
        }
    }
    Ok(FixtureList { fixtures })
}

pub(crate) fn generate_case_fixture(case: &BenchCase, out_dir: &Path) -> Result<PathBuf> {
    let synthetic = case
        .synthetic
        .as_ref()
        .ok_or_else(|| err(format!("case {} missing synthetic object", case.label)))?;
    if synthetic.size_bytes < MIN_SIZE {
        return Err(err(format!("synthetic case {} size must be at least {MIN_SIZE}", case.label)));
    }
    let extension = extension_for_kind(&synthetic.kind)
        .ok_or_else(|| err(format!("unsupported synthetic fixture kind {}", synthetic.kind)))?;
    fs::create_dir_all(out_dir)?;
    let case_id = case.id.clone().unwrap_or_else(|| safe_id(&case.label));
    let path = out_dir.join(format!("{}.{}", safe_id(&case_id), extension));
    generate_kind(&synthetic.kind, &path, synthetic.size_bytes)?;
    Ok(path)
}

pub(crate) fn is_supported_kind(kind: &str) -> bool {
    extension_for_kind(kind).is_some()
}

pub(crate) fn supported_kinds() -> Vec<String> {
    let mut kinds = sparse_extensions()
        .keys()
        .chain(ffmpeg_fixtures().keys())
        .map(|value| (*value).to_owned())
        .collect::<Vec<_>>();
    kinds.sort();
    kinds
}

fn generate_kind(kind: &str, path: &Path, size_bytes: u64) -> Result<()> {
    match kind {
        "mp4_moov_front" => {
            write_mp4(path, size_bytes, b"isom", &[b"isom", b"iso2", b"mp41"], false)
        }
        "mp4_moov_tail" => write_mp4(path, size_bytes, b"isom", &[b"isom", b"iso2", b"mp41"], true),
        "mp4_snv2_tail" => {
            write_mp4(path, size_bytes, b"SNV2", &[b"SNV2", b"isom", b"iso2", b"mp42"], true)
        }
        "mov_moov_tail" => write_mp4(path, size_bytes, b"qt  ", &[b"qt  "], true),
        "fragmented_mp4" => generate_fragmented_mp4(path, size_bytes),
        "webm_sparse" => generate_ebml_sparse(path, size_bytes, b"webm"),
        "mkv_sparse" => generate_ebml_sparse(path, size_bytes, b"matroska"),
        "wav_list_id3_data" => generate_wav(path, size_bytes, WavOptions::default()),
        "wav_192khz_data" => generate_wav(
            path,
            size_bytes,
            WavOptions { sample_rate: 192_000, ..Default::default() },
        ),
        "wav_9ch_data" => {
            generate_wav(path, size_bytes, WavOptions { channels: 9, ..Default::default() })
        }
        "bwf_data" => {
            generate_wav(path, size_bytes, WavOptions { bext: true, ..Default::default() })
        }
        "rf64_ds64_data" => {
            generate_wav(path, size_bytes, WavOptions { rf64: true, ..Default::default() })
        }
        "aiff_ssnd" => generate_aiff(path, size_bytes, false, *b"NONE"),
        "aifc_ssnd" => generate_aiff(path, size_bytes, true, *b"NONE"),
        "aifc_mace3_ssnd" => generate_aiff(path, size_bytes, true, *b"MAC3"),
        "flac_large" => generate_flac(path, size_bytes),
        "mp3_id3_large" => generate_mp3(path, size_bytes),
        "ogg_large" => generate_ogg(path, size_bytes),
        "mpeg_ts_large" => generate_mpeg_ts(path, size_bytes),
        "mpeg_ps_large" => generate_mpeg_ps(path, size_bytes),
        "avi_large" => generate_ffmpeg(path, size_bytes, &avi_large_fixture()),
        other => {
            let fixtures = ffmpeg_fixtures();
            let fixture = fixtures
                .get(other)
                .ok_or_else(|| err(format!("unsupported synthetic fixture kind {other}")))?;
            generate_ffmpeg(path, size_bytes, fixture)
        }
    }
}

fn extension_for_kind(kind: &str) -> Option<&'static str> {
    sparse_extensions()
        .get(kind)
        .copied()
        .or_else(|| ffmpeg_fixtures().get(kind).map(|fixture| fixture.extension))
}

fn sparse_extensions() -> BTreeMap<&'static str, &'static str> {
    [
        ("mp4_moov_front", "mp4"),
        ("mp4_moov_tail", "mp4"),
        ("mp4_snv2_tail", "mp4"),
        ("mov_moov_tail", "mov"),
        ("fragmented_mp4", "mp4"),
        ("webm_sparse", "webm"),
        ("mkv_sparse", "mkv"),
        ("wav_list_id3_data", "wav"),
        ("wav_192khz_data", "wav"),
        ("wav_9ch_data", "wav"),
        ("bwf_data", "wav"),
        ("rf64_ds64_data", "wav"),
        ("aiff_ssnd", "aiff"),
        ("aifc_ssnd", "aifc"),
        ("aifc_mace3_ssnd", "aifc"),
        ("flac_large", "flac"),
        ("mp3_id3_large", "mp3"),
        ("ogg_large", "ogg"),
        ("mpeg_ts_large", "ts"),
        ("mpeg_ps_large", "vob"),
        ("avi_large", "avi"),
    ]
    .into()
}

fn write_exact_size(
    path: &Path,
    size_bytes: u64,
    writer: impl FnOnce(&mut File, u64) -> Result<()>,
) -> Result<()> {
    let mut file = File::create(path)?;
    writer(&mut file, size_bytes)?;
    let position = file.stream_position()?;
    if position > size_bytes {
        return Err(err(format!("fixture writer exceeded target size for {}", path.display())));
    }
    file.set_len(size_bytes)?;
    Ok(())
}

fn write_box(name: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(payload.len() + 8);
    out.extend_from_slice(&(payload.len() as u32 + 8).to_be_bytes());
    out.extend_from_slice(name);
    out.extend_from_slice(payload);
    out
}

fn full_box(name: &[u8; 4], version: u8, flags: u32, payload: &[u8]) -> Vec<u8> {
    let mut full = vec![version];
    full.extend_from_slice(&(flags & 0x00ff_ffff).to_be_bytes()[1..]);
    full.extend_from_slice(payload);
    write_box(name, &full)
}

fn make_ftyp(major: &[u8; 4], brands: &[&[u8; 4]]) -> Vec<u8> {
    let mut payload = Vec::new();
    payload.extend_from_slice(major);
    payload.extend_from_slice(&0x200u32.to_be_bytes());
    for brand in brands {
        payload.extend_from_slice(*brand);
    }
    write_box(b"ftyp", &payload)
}

fn make_moov() -> Vec<u8> {
    let mut payload = vec![0; 8];
    payload.extend_from_slice(&1000u32.to_be_bytes());
    payload.extend_from_slice(&1000u32.to_be_bytes());
    payload.extend_from_slice(&0x0001_0000u32.to_be_bytes());
    payload.extend_from_slice(&0x0100u16.to_be_bytes());
    payload.extend_from_slice(&[0; 10]);
    payload.extend_from_slice(&[
        0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0, 0, 0x40, 0, 0, 0,
    ]);
    payload.extend_from_slice(&[0; 24]);
    payload.extend_from_slice(&2u32.to_be_bytes());
    write_box(b"moov", &full_box(b"mvhd", 0, 0, &payload))
}

fn write_mp4(
    path: &Path,
    size_bytes: u64,
    major: &[u8; 4],
    brands: &[&[u8; 4]],
    moov_tail: bool,
) -> Result<()> {
    let ftyp = make_ftyp(major, brands);
    let moov = make_moov();
    write_exact_size(path, size_bytes, |file, target| {
        file.write_all(&ftyp)?;
        if moov_tail {
            let mdat_size = target
                .checked_sub(ftyp.len() as u64 + moov.len() as u64)
                .ok_or_else(|| err("target MP4 size too small"))?;
            if mdat_size < 8 {
                return Err(err("target MP4 size too small"));
            }
            file.write_all(&(mdat_size as u32).to_be_bytes())?;
            file.write_all(b"mdat")?;
            file.seek(SeekFrom::Start(target - moov.len() as u64))?;
            file.write_all(&moov)?;
        } else {
            file.write_all(&moov)?;
            let mdat_size = target
                .checked_sub(ftyp.len() as u64 + moov.len() as u64)
                .ok_or_else(|| err("target MP4 size too small"))?;
            if mdat_size < 8 {
                return Err(err("target MP4 size too small"));
            }
            file.write_all(&(mdat_size as u32).to_be_bytes())?;
            file.write_all(b"mdat")?;
        }
        Ok(())
    })
}

fn generate_fragmented_mp4(path: &Path, size_bytes: u64) -> Result<()> {
    let ftyp = make_ftyp(b"iso6", &[b"iso6", b"mp41", b"dash"]);
    let moov = make_moov();
    let moof = write_box(b"moof", &write_box(b"mfhd", b"\x00\x00\x00\x00\x00\x00\x00\x01"));
    write_exact_size(path, size_bytes, |file, target| {
        file.write_all(&ftyp)?;
        file.write_all(&moov)?;
        file.write_all(&moof)?;
        let mdat_size = target
            .checked_sub(file.stream_position()?)
            .ok_or_else(|| err("target fragmented MP4 size too small"))?;
        if mdat_size < 8 {
            return Err(err("target fragmented MP4 size too small"));
        }
        file.write_all(&(mdat_size as u32).to_be_bytes())?;
        file.write_all(b"mdat")?;
        Ok(())
    })
}

#[derive(Clone, Copy)]
struct WavOptions {
    rf64: bool,
    bext: bool,
    channels: u16,
    sample_rate: u32,
    bits_per_sample: u16,
}

impl Default for WavOptions {
    fn default() -> Self {
        Self { rf64: false, bext: false, channels: 2, sample_rate: 48_000, bits_per_sample: 16 }
    }
}

fn riff_chunk(name: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(name);
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(payload);
    if payload.len() % 2 == 1 {
        out.push(0);
    }
    out
}

fn generate_wav(path: &Path, size_bytes: u64, options: WavOptions) -> Result<()> {
    let block_align = options.channels * options.bits_per_sample / 8;
    let byte_rate = options.sample_rate * u32::from(block_align);
    let mut fmt_payload = Vec::new();
    fmt_payload.extend_from_slice(&1u16.to_le_bytes());
    fmt_payload.extend_from_slice(&options.channels.to_le_bytes());
    fmt_payload.extend_from_slice(&options.sample_rate.to_le_bytes());
    fmt_payload.extend_from_slice(&byte_rate.to_le_bytes());
    fmt_payload.extend_from_slice(&block_align.to_le_bytes());
    fmt_payload.extend_from_slice(&options.bits_per_sample.to_le_bytes());
    let fmt = riff_chunk(b"fmt ", &fmt_payload);
    let list = riff_chunk(b"LIST", b"INFOINAM\x08\x00\x00\x00fixture\x00");
    let id3 =
        riff_chunk(b"ID3 ", b"ID3\x04\x00\x00\x00\x00\x00\x10\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0");
    let bext = if options.bext { riff_chunk(b"bext", &vec![0; 602]) } else { Vec::new() };
    write_exact_size(path, size_bytes, |file, target| {
        if options.rf64 {
            file.write_all(b"RF64\xff\xff\xff\xffWAVE")?;
            let data_bytes = target.saturating_sub(
                12 + 36
                    + fmt.len() as u64
                    + bext.len() as u64
                    + list.len() as u64
                    + id3.len() as u64
                    + 8,
            );
            let mut ds64_payload = Vec::new();
            ds64_payload.extend_from_slice(&(target - 8).to_le_bytes());
            ds64_payload.extend_from_slice(&data_bytes.to_le_bytes());
            ds64_payload.extend_from_slice(&0u64.to_le_bytes());
            ds64_payload.extend_from_slice(&0u32.to_le_bytes());
            file.write_all(&riff_chunk(b"ds64", &ds64_payload))?;
            file.write_all(&fmt)?;
            file.write_all(&bext)?;
            file.write_all(&list)?;
            file.write_all(&id3)?;
            file.write_all(b"data\xff\xff\xff\xff")?;
        } else {
            file.write_all(b"RIFF\xff\xff\xff\xffWAVE")?;
            file.write_all(&fmt)?;
            file.write_all(&bext)?;
            file.write_all(&list)?;
            file.write_all(&id3)?;
            file.write_all(b"data")?;
            let data_size = target.saturating_sub(file.stream_position()? + 4);
            file.write_all(&(data_size as u32).to_le_bytes())?;
        }
        Ok(())
    })
}

fn aiff_chunk(name: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(name);
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    out.extend_from_slice(payload);
    if payload.len() % 2 == 1 {
        out.push(0);
    }
    out
}

fn generate_aiff(
    path: &Path,
    size_bytes: u64,
    compressed: bool,
    compression_type: [u8; 4],
) -> Result<()> {
    let mut common_payload =
        b"\x00\x02\x00\x00\x00\x01\x00\x10@\x0e\xac\x44\x00\x00\x00\x00\x00\x00".to_vec();
    if compressed {
        common_payload.extend_from_slice(&compression_type);
        common_payload.extend_from_slice(b"\x0enot compressed");
    }
    let common = aiff_chunk(b"COMM", &common_payload);
    let name = aiff_chunk(b"NAME", b"fixture");
    let id3 =
        aiff_chunk(b"ID3 ", b"ID3\x04\x00\x00\x00\x00\x00\x10\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0");
    write_exact_size(path, size_bytes, |file, target| {
        let header_size = 12 + common.len() as u64 + name.len() as u64 + id3.len() as u64 + 16;
        let ssnd_payload_size =
            target.checked_sub(header_size).ok_or_else(|| err("target AIFF size too small"))?;
        file.write_all(b"FORM")?;
        file.write_all(&((target - 8) as u32).to_be_bytes())?;
        file.write_all(if compressed { b"AIFC" } else { b"AIFF" })?;
        file.write_all(&common)?;
        file.write_all(&name)?;
        file.write_all(&id3)?;
        file.write_all(&((ssnd_payload_size + 16) as u32).to_be_bytes())?;
        file.write_all(b"SSND")?;
        file.write_all(&0u32.to_be_bytes())?;
        file.write_all(&0u32.to_be_bytes())?;
        Ok(())
    })
}

fn generate_flac(path: &Path, size_bytes: u64) -> Result<()> {
    write_exact_size(path, size_bytes, |file, target| {
        file.write_all(b"fLaC")?;
        let audio_size = target.saturating_sub(42).max(4);
        let streaminfo = flac_streaminfo(audio_size);
        file.write_all(&[0x80])?;
        file.write_all(&(streaminfo.len() as u32).to_be_bytes()[1..])?;
        file.write_all(&streaminfo)?;
        Ok(())
    })
}

fn flac_streaminfo(audio_size: u64) -> Vec<u8> {
    let samples = (audio_size / 4).max(1);
    let mut packed = 0u128;
    packed |= 48_000u128 << (3 + 5 + 36);
    packed |= 1u128 << (5 + 36);
    packed |= 15u128 << 36;
    packed |= u128::from(samples) & ((1u128 << 36) - 1);
    let mut out = Vec::new();
    out.extend_from_slice(b"\x00\x10\x10\x00\x00\x00\x00\x00\x00\x00");
    out.extend_from_slice(&packed.to_be_bytes()[8..]);
    out.extend_from_slice(&[0; 16]);
    out
}

fn generate_mp3(path: &Path, size_bytes: u64) -> Result<()> {
    let tag_payload = b"TIT2\x00\x00\x00\x08\x00\x00fixture\x00";
    let tag_size = tag_payload.len();
    let syncsafe = [
        ((tag_size >> 21) & 0x7f) as u8,
        ((tag_size >> 14) & 0x7f) as u8,
        ((tag_size >> 7) & 0x7f) as u8,
        (tag_size & 0x7f) as u8,
    ];
    write_exact_size(path, size_bytes, |file, _| {
        file.write_all(b"ID3\x04\x00\x00")?;
        file.write_all(&syncsafe)?;
        file.write_all(tag_payload)?;
        file.write_all(b"\xff\xfb\x90\x64")?;
        Ok(())
    })
}

fn generate_ogg(path: &Path, size_bytes: u64) -> Result<()> {
    let opus_head = [
        b"OpusHead".as_slice(),
        &[1, 2],
        &312u16.to_le_bytes(),
        &48_000u32.to_le_bytes(),
        &0u16.to_le_bytes(),
        &[0],
    ]
    .concat();
    let opus_tags = [b"OpusTags".as_slice(), &0u32.to_le_bytes(), &0u32.to_le_bytes()].concat();
    let header = [
        ogg_page(&opus_head, 2, 0, 0),
        ogg_page(&opus_tags, 0, 1, 0),
        ogg_page(b"\xfc\xff\xfe", 4, 2, 960),
    ]
    .concat();
    write_exact_size(path, size_bytes, |file, _| {
        file.write_all(&header)?;
        Ok(())
    })
}

fn ogg_page(packet: &[u8], header_type: u8, sequence: u32, granule_position: u64) -> Vec<u8> {
    let mut segments = Vec::new();
    let mut remaining = packet.len();
    while remaining >= 255 {
        segments.push(255);
        remaining -= 255;
    }
    segments.push(remaining as u8);
    let mut page = Vec::new();
    page.extend_from_slice(b"OggS");
    page.extend_from_slice(&[0, header_type]);
    page.extend_from_slice(&granule_position.to_le_bytes());
    page.extend_from_slice(&1u32.to_le_bytes());
    page.extend_from_slice(&sequence.to_le_bytes());
    page.extend_from_slice(&0u32.to_le_bytes());
    page.push(segments.len() as u8);
    page.extend_from_slice(&segments);
    page.extend_from_slice(packet);
    let crc = ogg_crc(&page);
    page[22..26].copy_from_slice(&crc.to_le_bytes());
    page
}

fn ogg_crc(data: &[u8]) -> u32 {
    let mut crc = 0u32;
    for byte in data {
        crc ^= u32::from(*byte) << 24;
        for _ in 0..8 {
            if crc & 0x8000_0000 != 0 {
                crc = (crc << 1) ^ 0x04c1_1db7;
            } else {
                crc <<= 1;
            }
        }
    }
    crc
}

fn generate_mpeg_ts(path: &Path, size_bytes: u64) -> Result<()> {
    let packet = [vec![0x47, 0x40, 0x00, 0x10], vec![0xff; 184]].concat();
    write_exact_size(path, size_bytes, |file, target| {
        for _ in 0..std::cmp::min(16, target / packet.len() as u64) {
            file.write_all(&packet)?;
        }
        Ok(())
    })
}

fn generate_mpeg_ps(path: &Path, size_bytes: u64) -> Result<()> {
    write_exact_size(path, size_bytes, |file, _| {
        file.write_all(b"\x00\x00\x01\xba\x44\x00\x04\x00\x04\x01\x89\xc3\xf8")?;
        Ok(())
    })
}

fn generate_ebml_sparse(path: &Path, size_bytes: u64, doc_type: &[u8]) -> Result<()> {
    let mut header_payload = Vec::new();
    header_payload.extend_from_slice(&hex("4286810142F7810142F2810442F38108"));
    header_payload.extend_from_slice(&hex("4282"));
    header_payload.extend_from_slice(&vint_size(doc_type.len())?);
    header_payload.extend_from_slice(doc_type);
    let mut header = hex("1A45DFA3");
    header.extend_from_slice(&vint_size(header_payload.len())?);
    header.extend_from_slice(&header_payload);
    write_exact_size(path, size_bytes, |file, _| {
        file.write_all(&header)?;
        file.write_all(&hex("1853806701FFFFFFFFFFFFFF"))?;
        file.write_all(&hex("1549A966842AD7B1810F"))?;
        file.write_all(&hex("1F43B67501FFFFFFFFFFFFFF"))?;
        Ok(())
    })
}

fn vint_size(size: usize) -> Result<Vec<u8>> {
    if size < 0x7f {
        Ok(vec![0x80 | size as u8])
    } else if size < 0x3fff {
        Ok(vec![0x40 | (size >> 8) as u8, size as u8])
    } else {
        Err(err(format!("EBML fixture size field too large: {size}")))
    }
}

fn hex(value: &str) -> Vec<u8> {
    value
        .as_bytes()
        .chunks(2)
        .map(|chunk| u8::from_str_radix(std::str::from_utf8(chunk).unwrap(), 16).unwrap())
        .collect()
}

fn generate_ffmpeg(path: &Path, size_bytes: u64, fixture: &FfmpegFixture) -> Result<()> {
    let ffmpeg = which("ffmpeg")
        .ok_or_else(|| err("ffmpeg is required to generate this synthetic fixture"))?;
    let tmp = path.with_file_name(format!(
        "{}.tmp.{}",
        path.file_stem().and_then(|value| value.to_str()).unwrap_or("fixture"),
        path.extension().and_then(|value| value.to_str()).unwrap_or("media")
    ));
    let _ = fs::remove_file(path);
    let _ = fs::remove_file(&tmp);
    let mut command = Command::new(ffmpeg);
    command.args(["-hide_banner", "-loglevel", "error", "-y"]).args(&fixture.command).arg(&tmp);
    run_checked(&mut command)?;
    if tmp.metadata()?.len() > size_bytes {
        return Err(err(format!("ffmpeg fixture exceeded target size for {}", path.display())));
    }
    truncate_sparse(&tmp, size_bytes)?;
    if let Some(brand) = fixture.major_brand {
        patch_major_brand(&tmp, brand)?;
    }
    fs::rename(&tmp, path)?;
    Ok(())
}

fn truncate_sparse(path: &Path, size_bytes: u64) -> Result<()> {
    OpenOptions::new().write(true).open(path)?.set_len(size_bytes)?;
    Ok(())
}

fn patch_major_brand(path: &Path, brand: &[u8; 4]) -> Result<()> {
    use std::io::Read;
    let mut file = OpenOptions::new().read(true).write(true).open(path)?;
    let mut header = [0; 16];
    file.read_exact(&mut header)?;
    if &header[4..8] != b"ftyp" {
        return Err(err(format!("{} does not start with an ftyp box", path.display())));
    }
    file.seek(SeekFrom::Start(8))?;
    file.write_all(brand)?;
    Ok(())
}

fn audio_base(source: &'static str) -> Vec<&'static str> {
    vec!["-f", "lavfi", "-i", source, "-t", "2"]
}

fn video_base() -> Vec<&'static str> {
    vec![
        "-f",
        "lavfi",
        "-i",
        "testsrc2=size=640x360:rate=30",
        "-f",
        "lavfi",
        "-i",
        "sine=frequency=1000:sample_rate=48000",
        "-t",
        "2",
        "-shortest",
    ]
}

fn with_metadata(mut args: Vec<&'static str>) -> Vec<&'static str> {
    args.extend([
        "-metadata",
        "title=Revelo benchmark fixture",
        "-metadata",
        "comment=generated public benchmark fixture",
    ]);
    args
}

fn fixture(extension: &'static str, command: Vec<&'static str>) -> FfmpegFixture {
    FfmpegFixture { extension, command, major_brand: None }
}

fn brand_fixture(
    extension: &'static str,
    command: Vec<&'static str>,
    brand: &'static [u8; 4],
) -> FfmpegFixture {
    FfmpegFixture { extension, command, major_brand: Some(brand) }
}

fn avi_large_fixture() -> FfmpegFixture {
    fixture(
        "avi",
        with_metadata(
            [
                video_base(),
                vec![
                    "-c:v",
                    "mpeg4",
                    "-q:v",
                    "5",
                    "-c:a",
                    "libmp3lame",
                    "-b:a",
                    "128k",
                    "-f",
                    "avi",
                ],
            ]
            .concat(),
        ),
    )
}

fn ffmpeg_fixtures() -> BTreeMap<&'static str, FfmpegFixture> {
    let mp4_h264 = || {
        with_metadata(
            [
                video_base(),
                vec![
                    "-c:v",
                    "libx264",
                    "-preset",
                    "ultrafast",
                    "-pix_fmt",
                    "yuv420p",
                    "-c:a",
                    "aac",
                    "-b:a",
                    "96k",
                    "-movflags",
                    "+faststart",
                ],
            ]
            .concat(),
        )
    };
    [
        ("ffmpeg_mp4_avc_faststart", fixture("mp4", mp4_h264())),
        ("ffmpeg_mp4_snv2_faststart", brand_fixture("mp4", mp4_h264(), b"SNV2")),
        (
            "ffmpeg_mp4_hevc10_faststart",
            fixture(
                "mp4",
                with_metadata(
                    [
                        video_base(),
                        vec![
                            "-c:v",
                            "libx265",
                            "-preset",
                            "ultrafast",
                            "-x265-params",
                            "log-level=error",
                            "-pix_fmt",
                            "yuv420p10le",
                            "-c:a",
                            "aac",
                            "-b:a",
                            "96k",
                            "-movflags",
                            "+faststart",
                        ],
                    ]
                    .concat(),
                ),
            ),
        ),
        (
            "ffmpeg_mp4_av1_faststart",
            fixture(
                "mp4",
                with_metadata(
                    [
                        video_base(),
                        vec![
                            "-c:v",
                            "libsvtav1",
                            "-preset",
                            "13",
                            "-crf",
                            "45",
                            "-pix_fmt",
                            "yuv420p10le",
                            "-c:a",
                            "aac",
                            "-b:a",
                            "96k",
                            "-movflags",
                            "+faststart",
                        ],
                    ]
                    .concat(),
                ),
            ),
        ),
        (
            "ffmpeg_mp4_aac_audio",
            fixture(
                "m4a",
                with_metadata(
                    [
                        audio_base("sine=frequency=1000:sample_rate=48000"),
                        vec!["-c:a", "aac", "-b:a", "128k", "-f", "mp4"],
                    ]
                    .concat(),
                ),
            ),
        ),
        (
            "ffmpeg_mp4_tail",
            fixture(
                "mp4",
                with_metadata(
                    [
                        video_base(),
                        vec![
                            "-c:v",
                            "libx264",
                            "-preset",
                            "ultrafast",
                            "-pix_fmt",
                            "yuv420p",
                            "-c:a",
                            "aac",
                            "-b:a",
                            "96k",
                        ],
                    ]
                    .concat(),
                ),
            ),
        ),
        (
            "ffmpeg_mov_mpeg4_pcm",
            fixture(
                "mov",
                with_metadata(
                    [
                        video_base(),
                        vec!["-c:v", "mpeg4", "-q:v", "5", "-c:a", "pcm_s16be", "-f", "mov"],
                    ]
                    .concat(),
                ),
            ),
        ),
        (
            "ffmpeg_fragmented_mp4",
            fixture(
                "mp4",
                with_metadata(
                    [
                        video_base(),
                        vec![
                            "-c:v",
                            "libx264",
                            "-preset",
                            "ultrafast",
                            "-pix_fmt",
                            "yuv420p",
                            "-c:a",
                            "aac",
                            "-movflags",
                            "frag_keyframe+empty_moov+default_base_moof",
                        ],
                    ]
                    .concat(),
                ),
            ),
        ),
        (
            "ffmpeg_mkv_h264_aac",
            fixture(
                "mkv",
                with_metadata(
                    [
                        video_base(),
                        vec![
                            "-c:v",
                            "libx264",
                            "-preset",
                            "ultrafast",
                            "-c:a",
                            "aac",
                            "-f",
                            "matroska",
                        ],
                    ]
                    .concat(),
                ),
            ),
        ),
        (
            "ffmpeg_mkv_ffv1_flac",
            fixture(
                "mkv",
                with_metadata(
                    [
                        video_base(),
                        vec!["-c:v", "ffv1", "-level", "3", "-c:a", "flac", "-f", "matroska"],
                    ]
                    .concat(),
                ),
            ),
        ),
        (
            "ffmpeg_webm_vp8_opus",
            fixture(
                "webm",
                with_metadata(
                    [
                        video_base(),
                        vec![
                            "-c:v",
                            "libvpx",
                            "-deadline",
                            "realtime",
                            "-cpu-used",
                            "8",
                            "-b:v",
                            "500k",
                            "-c:a",
                            "libopus",
                            "-b:a",
                            "64k",
                            "-f",
                            "webm",
                        ],
                    ]
                    .concat(),
                ),
            ),
        ),
        (
            "ffmpeg_webm_av1_opus",
            fixture(
                "webm",
                with_metadata(
                    [
                        video_base(),
                        vec![
                            "-c:v",
                            "libsvtav1",
                            "-preset",
                            "13",
                            "-crf",
                            "45",
                            "-pix_fmt",
                            "yuv420p10le",
                            "-c:a",
                            "libopus",
                            "-b:a",
                            "64k",
                            "-f",
                            "webm",
                        ],
                    ]
                    .concat(),
                ),
            ),
        ),
        (
            "ffmpeg_flv_nellymoser",
            fixture(
                "flv",
                with_metadata(
                    [
                        audio_base("sine=frequency=1000:sample_rate=44100"),
                        vec!["-c:a", "nellymoser", "-f", "flv"],
                    ]
                    .concat(),
                ),
            ),
        ),
        (
            "ffmpeg_asf_wma",
            fixture(
                "asf",
                with_metadata(
                    [
                        audio_base("sine=frequency=1000:sample_rate=48000"),
                        vec!["-c:a", "wmav2", "-f", "asf"],
                    ]
                    .concat(),
                ),
            ),
        ),
        (
            "ffmpeg_flac",
            fixture(
                "flac",
                with_metadata(
                    [audio_base("anoisesrc=r=48000:a=0.25"), vec!["-c:a", "flac", "-f", "flac"]]
                        .concat(),
                ),
            ),
        ),
        (
            "ffmpeg_mp3",
            fixture(
                "mp3",
                with_metadata(
                    [
                        audio_base("anoisesrc=r=48000:a=0.25"),
                        vec![
                            "-c:a",
                            "libmp3lame",
                            "-b:a",
                            "320k",
                            "-write_id3v2",
                            "1",
                            "-id3v2_version",
                            "3",
                            "-f",
                            "mp3",
                        ],
                    ]
                    .concat(),
                ),
            ),
        ),
        (
            "ffmpeg_ogg_vorbis",
            fixture(
                "ogg",
                with_metadata(
                    [
                        audio_base("anullsrc=r=48000:cl=stereo"),
                        vec!["-strict", "-2", "-c:a", "vorbis", "-q:a", "5", "-f", "ogg"],
                    ]
                    .concat(),
                ),
            ),
        ),
        (
            "ffmpeg_ogg_opus",
            fixture(
                "ogg",
                with_metadata(
                    [
                        audio_base("anullsrc=r=48000:cl=stereo"),
                        vec!["-c:a", "libopus", "-b:a", "128k", "-f", "ogg"],
                    ]
                    .concat(),
                ),
            ),
        ),
        (
            "ffmpeg_ogg_flac",
            fixture(
                "oga",
                with_metadata(
                    [audio_base("anoisesrc=r=48000:a=0.25"), vec!["-c:a", "flac", "-f", "ogg"]]
                        .concat(),
                ),
            ),
        ),
        (
            "ffmpeg_mpeg_ts",
            fixture(
                "ts",
                with_metadata(
                    [
                        video_base(),
                        vec![
                            "-c:v",
                            "mpeg2video",
                            "-b:v",
                            "2M",
                            "-c:a",
                            "mp2",
                            "-b:a",
                            "128k",
                            "-f",
                            "mpegts",
                        ],
                    ]
                    .concat(),
                ),
            ),
        ),
        (
            "ffmpeg_mpeg_ts_ac3",
            fixture(
                "ts",
                with_metadata(
                    [
                        video_base(),
                        vec![
                            "-c:v",
                            "mpeg2video",
                            "-b:v",
                            "2M",
                            "-c:a",
                            "ac3",
                            "-b:a",
                            "192k",
                            "-f",
                            "mpegts",
                        ],
                    ]
                    .concat(),
                ),
            ),
        ),
        (
            "ffmpeg_m2ts",
            fixture(
                "m2ts",
                with_metadata(
                    [
                        video_base(),
                        vec![
                            "-c:v",
                            "mpeg2video",
                            "-b:v",
                            "2M",
                            "-c:a",
                            "mp2",
                            "-b:a",
                            "128k",
                            "-mpegts_m2ts_mode",
                            "1",
                            "-f",
                            "mpegts",
                        ],
                    ]
                    .concat(),
                ),
            ),
        ),
        (
            "ffmpeg_mpeg_ps",
            fixture(
                "mpg",
                with_metadata(
                    [
                        video_base(),
                        vec![
                            "-c:v",
                            "mpeg2video",
                            "-b:v",
                            "2M",
                            "-c:a",
                            "mp2",
                            "-b:a",
                            "128k",
                            "-f",
                            "mpeg",
                        ],
                    ]
                    .concat(),
                ),
            ),
        ),
        (
            "ffmpeg_mpeg_ps_ac3",
            fixture(
                "mpg",
                with_metadata(
                    [
                        video_base(),
                        vec![
                            "-c:v",
                            "mpeg2video",
                            "-b:v",
                            "2M",
                            "-c:a",
                            "ac3",
                            "-b:a",
                            "192k",
                            "-f",
                            "mpeg",
                        ],
                    ]
                    .concat(),
                ),
            ),
        ),
        (
            "ffmpeg_vob",
            fixture(
                "vob",
                with_metadata(
                    [
                        vec![
                            "-f",
                            "lavfi",
                            "-i",
                            "testsrc2=size=720x480:rate=30000/1001",
                            "-f",
                            "lavfi",
                            "-i",
                            "sine=frequency=1000:sample_rate=48000",
                            "-t",
                            "2",
                            "-shortest",
                        ],
                        vec![
                            "-c:v",
                            "mpeg2video",
                            "-b:v",
                            "4M",
                            "-c:a",
                            "mp2",
                            "-b:a",
                            "192k",
                            "-f",
                            "vob",
                        ],
                    ]
                    .concat(),
                ),
            ),
        ),
        ("ffmpeg_avi_mpeg4_mp3", avi_large_fixture()),
        (
            "ffmpeg_avi_mpeg4_wma",
            fixture(
                "avi",
                with_metadata(
                    [
                        video_base(),
                        vec![
                            "-c:v", "mpeg4", "-q:v", "5", "-c:a", "wmav2", "-b:a", "128k", "-f",
                            "avi",
                        ],
                    ]
                    .concat(),
                ),
            ),
        ),
        (
            "ffmpeg_raw_aac",
            fixture(
                "aac",
                [
                    audio_base("sine=frequency=1000:sample_rate=48000"),
                    vec!["-c:a", "aac", "-b:a", "128k", "-f", "adts"],
                ]
                .concat(),
            ),
        ),
        (
            "ffmpeg_raw_ac3",
            fixture(
                "ac3",
                [
                    audio_base("sine=frequency=1000:sample_rate=48000"),
                    vec!["-c:a", "ac3", "-b:a", "192k", "-f", "ac3"],
                ]
                .concat(),
            ),
        ),
        (
            "ffmpeg_raw_eac3",
            fixture(
                "eac3",
                [
                    audio_base("sine=frequency=1000:sample_rate=48000"),
                    vec!["-c:a", "eac3", "-b:a", "192k", "-f", "eac3"],
                ]
                .concat(),
            ),
        ),
        (
            "ffmpeg_opus",
            fixture(
                "opus",
                with_metadata(
                    [
                        audio_base("sine=frequency=1000:sample_rate=48000"),
                        vec!["-c:a", "libopus", "-b:a", "64k", "-f", "opus"],
                    ]
                    .concat(),
                ),
            ),
        ),
    ]
    .into()
}

pub(crate) fn self_test() -> Result<()> {
    let temp = tempfile::tempdir()?;
    for kind in ["mp4_snv2_tail", "webm_sparse", "mkv_sparse", "flac_large", "ogg_large"] {
        let case = BenchCase {
            id: Some(format!("self-test-{kind}")),
            label: format!("self test {kind}"),
            class_name: "synthetic".to_owned(),
            format: None,
            container: None,
            codec: None,
            layout: None,
            source: None,
            path: None,
            synthetic: Some(crate::config::SyntheticCase {
                kind: kind.to_owned(),
                size_bytes: 8 * 1024 * 1024,
            }),
        };
        let path = generate_case_fixture(&case, temp.path())?;
        assert_eq!(path.metadata()?.len(), 8 * 1024 * 1024);
    }
    Ok(())
}
