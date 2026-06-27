use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use std::io::Write;
use tempfile::NamedTempFile;

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
    let large_data = 100 * 1024 * 1024usize; // 100 MiB
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
