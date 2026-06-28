use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use std::io::{Seek, SeekFrom, Write};
use tempfile::NamedTempFile;

const LARGE_DATA_SIZE: usize = 100 * 1024 * 1024;
const LARGE_CODEC_CONFIG_SIZE: usize = 256 * 1024 + 1024;
const LARGE_METADATA_SIZE: usize = 64 * 1024 + 1024;

/// Build just the 44-byte WAV header for PCM stereo 44100 Hz 16-bit.
/// Does NOT write sample data — the caller extends the file separately.
fn wav_header(data_size: u32) -> [u8; 44] {
    let sample_rate: u32 = 44100;
    let num_channels: u16 = 2;
    let bits_per_sample: u16 = 16;
    let block_align: u16 = num_channels * bits_per_sample / 8;
    let byte_rate: u32 = sample_rate * num_channels as u32 * 2;

    let file_size_minus_8: u32 = 4     // "WAVE"
        + 8 + 16                         // fmt  chunk
        + 8 + data_size; // data chunk

    let mut hdr = [0u8; 44];
    hdr[0..4].copy_from_slice(b"RIFF");
    hdr[4..8].copy_from_slice(&file_size_minus_8.to_le_bytes());
    hdr[8..12].copy_from_slice(b"WAVE");
    // fmt  sub-chunk
    hdr[12..16].copy_from_slice(b"fmt ");
    hdr[16..20].copy_from_slice(&16u32.to_le_bytes());
    hdr[20..22].copy_from_slice(&1u16.to_le_bytes()); // PCM
    hdr[22..24].copy_from_slice(&num_channels.to_le_bytes());
    hdr[24..28].copy_from_slice(&sample_rate.to_le_bytes());
    hdr[28..32].copy_from_slice(&byte_rate.to_le_bytes());
    hdr[32..34].copy_from_slice(&block_align.to_le_bytes());
    hdr[34..36].copy_from_slice(&bits_per_sample.to_le_bytes());
    // data sub-chunk
    hdr[36..40].copy_from_slice(b"data");
    hdr[40..44].copy_from_slice(&data_size.to_le_bytes());
    hdr
}

/// Full in-memory WAV — header + zero-filled data.
fn full_wav(data_size: usize) -> Vec<u8> {
    let padded = if data_size % 2 == 1 { data_size + 1 } else { data_size };
    let hdr = wav_header(padded as u32);
    let mut buf = Vec::with_capacity(44 + padded);
    buf.extend_from_slice(&hdr);
    buf.resize(44 + padded, 0u8);
    buf
}

/// Minimal MP4-like sparse file: ftyp followed by an mdat that extends to EOF.
/// This reproduces the large-container metadata case without storing media data.
fn mp4_ftyp_mdat_header() -> Vec<u8> {
    let mut buf = Vec::with_capacity(28);
    buf.extend_from_slice(&20u32.to_be_bytes());
    buf.extend_from_slice(b"ftyp");
    buf.extend_from_slice(b"M4A ");
    buf.extend_from_slice(&0u32.to_be_bytes());
    buf.extend_from_slice(b"isom");
    buf.extend_from_slice(&0u32.to_be_bytes());
    buf.extend_from_slice(b"mdat");
    buf
}

fn mp4_box(ty: &[u8; 4], payload: Vec<u8>) -> Vec<u8> {
    let mut buf = Vec::with_capacity(payload.len() + 8);
    buf.extend_from_slice(&((payload.len() + 8) as u32).to_be_bytes());
    buf.extend_from_slice(ty);
    buf.extend(payload);
    buf
}

fn ftyp_box(major: &[u8; 4], compatible: &[[u8; 4]]) -> Vec<u8> {
    let mut payload = Vec::with_capacity(8 + compatible.len() * 4);
    payload.extend_from_slice(major);
    payload.extend_from_slice(&0u32.to_be_bytes());
    for brand in compatible {
        payload.extend_from_slice(brand);
    }
    mp4_box(b"ftyp", payload)
}

fn ilst_metadata_box(payload_len: usize) -> Vec<u8> {
    let mut data_body = Vec::with_capacity(payload_len + 8);
    data_body.extend_from_slice(&1u32.to_be_bytes());
    data_body.extend_from_slice(&0u32.to_be_bytes());
    data_body.resize(data_body.len() + payload_len, b'x');

    let item_type = 0xA9_74_6F_6Fu32.to_be_bytes(); // ©too
    mp4_box(&item_type, mp4_box(b"data", data_body))
}

fn visual_entry(entry_type: &[u8; 4], codec_box_type: &[u8; 4], codec_body: Vec<u8>) -> Vec<u8> {
    let mut payload = Vec::new();
    payload.resize(6, 0); // reserved
    payload.extend_from_slice(&1u16.to_be_bytes()); // data_reference_index
    payload.resize(payload.len() + 16, 0); // pre_defined/reserved
    payload.extend_from_slice(&1920u16.to_be_bytes());
    payload.extend_from_slice(&1080u16.to_be_bytes());
    payload.extend_from_slice(&0x0048_0000u32.to_be_bytes()); // horizresolution
    payload.extend_from_slice(&0x0048_0000u32.to_be_bytes()); // vertresolution
    payload.extend_from_slice(&0u32.to_be_bytes());
    payload.extend_from_slice(&1u16.to_be_bytes()); // frame_count
    payload.resize(payload.len() + 32, 0); // compressorname
    payload.extend_from_slice(&24u16.to_be_bytes()); // depth
    payload.extend_from_slice(&0xFFFFu16.to_be_bytes()); // pre_defined
    payload.extend(mp4_box(codec_box_type, codec_body));
    mp4_box(entry_type, payload)
}

fn mp4a_entry(esds_body: Vec<u8>) -> Vec<u8> {
    let mut payload = Vec::new();
    payload.resize(6, 0); // reserved
    payload.extend_from_slice(&1u16.to_be_bytes()); // data_reference_index
    payload.extend_from_slice(&0u16.to_be_bytes()); // version
    payload.extend_from_slice(&0u16.to_be_bytes()); // revision
    payload.extend_from_slice(&0u32.to_be_bytes()); // vendor
    payload.extend_from_slice(&2u16.to_be_bytes()); // channel_count
    payload.extend_from_slice(&16u16.to_be_bytes()); // sample_size
    payload.extend_from_slice(&0u16.to_be_bytes()); // pre_defined
    payload.extend_from_slice(&0u16.to_be_bytes()); // packet_size
    payload.extend_from_slice(&(48_000u32 << 16).to_be_bytes());
    payload.extend(mp4_box(b"esds", esds_body));
    mp4_box(b"mp4a", payload)
}

fn stsd_box(entry: Vec<u8>) -> Vec<u8> {
    let mut payload = Vec::new();
    payload.extend_from_slice(&0u32.to_be_bytes()); // version_flags
    payload.extend_from_slice(&1u32.to_be_bytes()); // entry_count
    payload.extend(entry);
    mp4_box(b"stsd", payload)
}

fn trak_with_stsd(entry: Vec<u8>) -> Vec<u8> {
    mp4_box(b"trak", mp4_box(b"mdia", mp4_box(b"minf", mp4_box(b"stbl", stsd_box(entry)))))
}

fn structured_moov() -> Vec<u8> {
    let metadata = mp4_box(
        b"udta",
        mp4_box(b"meta", mp4_box(b"ilst", ilst_metadata_box(LARGE_METADATA_SIZE))),
    );
    let avcc = {
        let mut body = vec![1, 0x64, 0, 0x1F, 0xFF, 0xE0, 0];
        body.resize(LARGE_CODEC_CONFIG_SIZE, 0);
        trak_with_stsd(visual_entry(b"avc1", b"avcC", body))
    };
    let hvcc = {
        let mut body = vec![0; 23];
        body[0] = 1;
        body[1] = 0x21;
        body[12] = 0x5D;
        body[16] = 1;
        body[17] = 2;
        body.resize(LARGE_CODEC_CONFIG_SIZE, 0);
        trak_with_stsd(visual_entry(b"hvc1", b"hvcC", body))
    };
    let esds = {
        let mut body = vec![0; LARGE_CODEC_CONFIG_SIZE];
        body[4] = 0x03;
        trak_with_stsd(mp4a_entry(body))
    };
    mp4_box(b"moov", [metadata, avcc, hvcc, esds].concat())
}

fn write_structured_mp4(
    file: &NamedTempFile,
    major_brand: &[u8; 4],
    moov_first: bool,
) -> std::io::Result<u64> {
    let ftyp = ftyp_box(major_brand, &[*b"isom", *b"mp42"]);
    let moov = structured_moov();
    let mdat_size = 8 + LARGE_DATA_SIZE as u32;
    let mut file = file.as_file();
    if moov_first {
        file.write_all(&ftyp)?;
        file.write_all(&moov)?;
        file.write_all(&mdat_size.to_be_bytes())?;
        file.write_all(b"mdat")?;
        let total = (ftyp.len() + moov.len() + 8 + LARGE_DATA_SIZE) as u64;
        file.set_len(total)?;
        Ok(total)
    } else {
        file.write_all(&ftyp)?;
        file.write_all(&mdat_size.to_be_bytes())?;
        file.write_all(b"mdat")?;
        let moov_start = (ftyp.len() + 8 + LARGE_DATA_SIZE) as u64;
        file.seek(SeekFrom::Start(moov_start))?;
        file.write_all(&moov)?;
        Ok(moov_start + moov.len() as u64)
    }
}

fn bench_parse(c: &mut Criterion) {
    // ── small file (44 B header + 100 B samples) ────────────────
    let small_data = 100usize;
    let small = full_wav(small_data);
    let small_file = NamedTempFile::new().expect("tempfile");
    {
        let mut f = small_file.as_file();
        f.write_all(&small).expect("write small");
        f.flush().expect("flush");
    }

    // ── large file (44 B header + 100 MiB samples, sparse) ─────
    let large_data = LARGE_DATA_SIZE; // 100 MiB
    let large_file = NamedTempFile::new().expect("tempfile");
    let large_total = 44u64 + large_data as u64;
    {
        let mut f = large_file.as_file();
        f.write_all(&wav_header(large_data as u32)).expect("write header");
        f.set_len(large_total).expect("set_len"); // sparse — no allocation
        f.flush().expect("flush");
    }
    // Also build a full in-memory large buffer (pre-allocated for fairness)
    let large_bytes = full_wav(large_data);

    // ── sparse MP4-like large file: ftyp + mdat-to-EOF ─────────
    let large_mp4_file = NamedTempFile::new().expect("tempfile");
    let large_mp4_header = mp4_ftyp_mdat_header();
    let large_mp4_total = large_mp4_header.len() as u64 + large_data as u64;
    {
        let mut f = large_mp4_file.as_file();
        f.write_all(&large_mp4_header).expect("write mp4 header");
        f.set_len(large_mp4_total).expect("set_len mp4");
        f.flush().expect("flush mp4");
    }

    // ── structured sparse MP4/MOV-like files with moov metadata ─
    let mp4_moov_front_file = NamedTempFile::new().expect("tempfile");
    let mp4_moov_front_total =
        write_structured_mp4(&mp4_moov_front_file, b"isom", true).expect("write mp4 moov front");

    let mp4_moov_tail_file = NamedTempFile::new().expect("tempfile");
    let mp4_moov_tail_total =
        write_structured_mp4(&mp4_moov_tail_file, b"qt  ", false).expect("write mp4 moov tail");

    let mp4_snv2_tail_file = NamedTempFile::new().expect("tempfile");
    let mp4_snv2_tail_total =
        write_structured_mp4(&mp4_snv2_tail_file, b"SNV2", false).expect("write snv2 mp4 tail");

    // ── group: from_bytes (in-memory buffer) ────────────────────
    let mut group = c.benchmark_group("from_bytes");
    group.throughput(Throughput::Bytes(small.len() as u64));
    group.bench_function(BenchmarkId::new("small", "144 B"), |b| {
        b.iter(|| black_box(revelo::Metadata::from_bytes(black_box(&small))));
    });

    group.throughput(Throughput::Bytes(large_bytes.len() as u64));
    group.bench_function(BenchmarkId::new("large", "100 MiB"), |b| {
        b.iter(|| black_box(revelo::Metadata::from_bytes(black_box(large_bytes.as_slice()))));
    });
    group.finish();

    // ── group: from_file (mmap) ─────────────────────────────────
    let mut group = c.benchmark_group("from_file_mmap");
    let small_path = small_file.path().to_str().expect("path").to_string();
    group.throughput(Throughput::Bytes(small.len() as u64));
    group.bench_function(BenchmarkId::new("small", "144 B"), |b| {
        b.iter(|| black_box(revelo::Metadata::from_file(black_box(&small_path))));
    });

    let large_path = large_file.path().to_str().expect("path").to_string();
    group.throughput(Throughput::Bytes(large_total));
    group.bench_function(BenchmarkId::new("large", "100 MiB"), |b| {
        b.iter(|| black_box(revelo::Metadata::from_file(black_box(&large_path))));
    });
    let large_mp4_path = large_mp4_file.path().to_str().expect("path").to_string();
    group.throughput(Throughput::Bytes(large_mp4_total));
    group.bench_function(BenchmarkId::new("large_mp4_sparse", "100 MiB"), |b| {
        b.iter(|| black_box(revelo::Metadata::from_file(black_box(&large_mp4_path))));
    });
    let mp4_moov_front_path = mp4_moov_front_file.path().to_str().expect("path").to_string();
    group.throughput(Throughput::Bytes(mp4_moov_front_total));
    group.bench_function(BenchmarkId::new("large_mp4_moov_front", "100 MiB"), |b| {
        b.iter(|| black_box(revelo::Metadata::from_file(black_box(&mp4_moov_front_path))));
    });
    let mp4_moov_tail_path = mp4_moov_tail_file.path().to_str().expect("path").to_string();
    group.throughput(Throughput::Bytes(mp4_moov_tail_total));
    group.bench_function(BenchmarkId::new("large_mov_moov_tail", "100 MiB"), |b| {
        b.iter(|| black_box(revelo::Metadata::from_file(black_box(&mp4_moov_tail_path))));
    });
    let mp4_snv2_tail_path = mp4_snv2_tail_file.path().to_str().expect("path").to_string();
    group.throughput(Throughput::Bytes(mp4_snv2_tail_total));
    group.bench_function(BenchmarkId::new("large_snv2_moov_tail", "100 MiB"), |b| {
        b.iter(|| black_box(revelo::Metadata::from_file(black_box(&mp4_snv2_tail_path))));
    });
    group.finish();

    // ── group: from_file_owned (full read → from_bytes) ─────────
    let mut group = c.benchmark_group("from_file_owned");
    group.throughput(Throughput::Bytes(small.len() as u64));
    group.bench_function(BenchmarkId::new("small", "144 B"), |b| {
        b.iter(|| black_box(revelo::Metadata::from_file_owned(black_box(&small_path))));
    });

    group.throughput(Throughput::Bytes(large_total));
    group.bench_function(BenchmarkId::new("large", "100 MiB"), |b| {
        b.iter(|| black_box(revelo::Metadata::from_file_owned(black_box(&large_path))));
    });
    group.finish();
}

criterion_group!(benches, bench_parse);
criterion_main!(benches);
