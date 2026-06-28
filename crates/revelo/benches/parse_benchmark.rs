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

fn rf64_header(data_size: u64) -> Vec<u8> {
    let sample_rate: u32 = 44100;
    let num_channels: u16 = 2;
    let bits_per_sample: u16 = 16;
    let block_align: u16 = num_channels * bits_per_sample / 8;
    let byte_rate: u32 = sample_rate * block_align as u32;
    let sample_count = data_size / block_align as u64;
    let riff_size = 4u64 + (8 + 28) + (8 + 16) + (8 + data_size);

    let mut buf = Vec::new();
    buf.extend_from_slice(b"RF64");
    buf.extend_from_slice(&u32::MAX.to_le_bytes());
    buf.extend_from_slice(b"WAVE");

    buf.extend_from_slice(b"ds64");
    buf.extend_from_slice(&28u32.to_le_bytes());
    buf.extend_from_slice(&riff_size.to_le_bytes());
    buf.extend_from_slice(&data_size.to_le_bytes());
    buf.extend_from_slice(&sample_count.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());

    buf.extend_from_slice(b"fmt ");
    buf.extend_from_slice(&16u32.to_le_bytes());
    buf.extend_from_slice(&1u16.to_le_bytes());
    buf.extend_from_slice(&num_channels.to_le_bytes());
    buf.extend_from_slice(&sample_rate.to_le_bytes());
    buf.extend_from_slice(&byte_rate.to_le_bytes());
    buf.extend_from_slice(&block_align.to_le_bytes());
    buf.extend_from_slice(&bits_per_sample.to_le_bytes());

    buf.extend_from_slice(b"data");
    buf.extend_from_slice(&u32::MAX.to_le_bytes());
    buf
}

fn full_rf64(data_size: usize) -> Vec<u8> {
    let mut buf = rf64_header(data_size as u64);
    buf.resize(buf.len() + data_size, 0);
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

fn write_temp_bytes(bytes: &[u8]) -> (NamedTempFile, u64) {
    let file = NamedTempFile::new().expect("tempfile");
    {
        let mut f = file.as_file();
        f.write_all(bytes).expect("write fixture");
        f.flush().expect("flush fixture");
    }
    (file, bytes.len() as u64)
}

fn write_sparse_temp(bytes: &[u8], total_len: u64) -> (NamedTempFile, u64) {
    let file = NamedTempFile::new().expect("tempfile");
    {
        let mut f = file.as_file();
        f.write_all(bytes).expect("write sparse fixture header");
        f.set_len(total_len).expect("set sparse fixture length");
        f.flush().expect("flush sparse fixture");
    }
    (file, total_len)
}

fn ebml_size(size: usize) -> Vec<u8> {
    if size <= 0x7f {
        vec![0x80 | size as u8]
    } else if size <= 0x3fff {
        vec![0x40 | ((size >> 8) as u8), (size & 0xff) as u8]
    } else if size <= 0x1f_ffff {
        vec![0x20 | ((size >> 16) as u8), ((size >> 8) & 0xff) as u8, (size & 0xff) as u8]
    } else {
        vec![
            0x10 | ((size >> 24) as u8),
            ((size >> 16) & 0xff) as u8,
            ((size >> 8) & 0xff) as u8,
            (size & 0xff) as u8,
        ]
    }
}

fn ebml_element(id: &[u8], payload: Vec<u8>) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(id);
    out.extend_from_slice(&ebml_size(payload.len()));
    out.extend_from_slice(&payload);
    out
}

fn webm_header(doc_type: &[u8]) -> Vec<u8> {
    ebml_element(&[0x1A, 0x45, 0xDF, 0xA3], ebml_element(&[0x42, 0x82], doc_type.to_vec()))
}

fn webm_track() -> Vec<u8> {
    let mut track = Vec::new();
    track.extend(ebml_element(&[0xD7], vec![1]));
    track.extend(ebml_element(&[0x83], vec![2]));
    track.extend(ebml_element(&[0x86], b"A_OPUS".to_vec()));
    ebml_element(&[0x16, 0x54, 0xAE, 0x6B], ebml_element(&[0xAE], track))
}

fn small_webm() -> Vec<u8> {
    small_mkv_container(b"webm")
}

fn small_matroska() -> Vec<u8> {
    small_mkv_container(b"matroska")
}

fn small_mkv_container(doc_type: &[u8]) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.extend(webm_header(doc_type));
    buf.extend(ebml_element(&[0x18, 0x53, 0x80, 0x67], webm_track()));
    buf
}

fn mkv_tags(payload_len: usize) -> Vec<u8> {
    ebml_element(
        &[0x12, 0x54, 0xC3, 0x67],
        ebml_element(
            &[0x73, 0x73],
            ebml_element(
                &[0x67, 0xC8],
                [
                    ebml_element(&[0x45, 0xA3], b"TITLE".to_vec()),
                    ebml_element(&[0x44, 0x87], vec![b'x'; payload_len]),
                ]
                .concat(),
            ),
        ),
    )
}

fn mkv_attachment(payload_len: usize) -> Vec<u8> {
    ebml_element(
        &[0x19, 0x41, 0xA4, 0x69],
        ebml_element(
            &[0x61, 0xA7],
            [
                ebml_element(&[0x46, 0x7E], b"cover".to_vec()),
                ebml_element(&[0x46, 0x6E], b"cover.jpg".to_vec()),
                ebml_element(&[0x46, 0x60], b"image/jpeg".to_vec()),
                ebml_element(&[0x46, 0x5C], vec![0; payload_len]),
            ]
            .concat(),
        ),
    )
}

fn large_mkv_sparse_header(doc_type: &[u8], media_size: usize) -> (Vec<u8>, u64) {
    let mut segment_prefix = Vec::new();
    segment_prefix.extend(webm_track());
    segment_prefix.extend(mkv_tags(LARGE_METADATA_SIZE));
    segment_prefix.extend(ebml_element(&[0x1C, 0x53, 0xBB, 0x6B], vec![0; LARGE_METADATA_SIZE]));
    segment_prefix.extend(mkv_attachment(LARGE_METADATA_SIZE));
    segment_prefix.extend_from_slice(&[0x1F, 0x43, 0xB6, 0x75]);
    segment_prefix.extend(ebml_size(media_size));

    let segment_payload_len = segment_prefix.len() + media_size;

    let mut buf = Vec::new();
    buf.extend(webm_header(doc_type));
    buf.extend_from_slice(&[0x18, 0x53, 0x80, 0x67]);
    buf.extend(ebml_size(segment_payload_len));
    buf.extend(segment_prefix);

    let total_len = buf.len() as u64 + media_size as u64;
    (buf, total_len)
}

fn encode_f80_be(value: f64) -> [u8; 10] {
    debug_assert!(value > 0.0 && value < 2f64.powi(63));
    let int_part = value.trunc() as u64;
    let e = 63 - int_part.leading_zeros() as i32;
    let scaled = value * 2f64.powi(63 - e);
    let mantissa = scaled.round() as u64;
    let biased_exp = (16383 + e) as u16;
    let mut out = [0u8; 10];
    out[0] = ((biased_exp >> 8) & 0x7F) as u8;
    out[1] = (biased_exp & 0xFF) as u8;
    out[2..10].copy_from_slice(&mantissa.to_be_bytes());
    out
}

fn aiff_header(data_size: usize) -> Vec<u8> {
    let channels = 2u16;
    let bits = 24u16;
    let block_align = channels * (bits / 8);
    let frame_count = (data_size / block_align as usize) as u32;
    let ssnd_chunk_size = 8 + data_size as u32;
    let comm_chunk_size = 18u32;

    let mut buf = Vec::new();
    buf.extend_from_slice(b"FORM");
    let form_size = 4 + (8 + comm_chunk_size) + (8 + ssnd_chunk_size);
    buf.extend_from_slice(&form_size.to_be_bytes());
    buf.extend_from_slice(b"AIFF");
    buf.extend_from_slice(b"COMM");
    buf.extend_from_slice(&comm_chunk_size.to_be_bytes());
    buf.extend_from_slice(&channels.to_be_bytes());
    buf.extend_from_slice(&frame_count.to_be_bytes());
    buf.extend_from_slice(&bits.to_be_bytes());
    buf.extend_from_slice(&encode_f80_be(48_000.0));
    buf.extend_from_slice(b"SSND");
    buf.extend_from_slice(&ssnd_chunk_size.to_be_bytes());
    buf.extend_from_slice(&0u32.to_be_bytes());
    buf.extend_from_slice(&0u32.to_be_bytes());
    buf
}

fn full_aiff(data_size: usize) -> Vec<u8> {
    let mut buf = aiff_header(data_size);
    buf.resize(buf.len() + data_size, 0);
    buf
}

fn pack_flac_streaminfo(sample_rate: u32, channels_m1: u8, bps_m1: u8, samples: u64) -> [u8; 8] {
    let mut packed: u64 = 0;
    packed |= (sample_rate as u64) << (3 + 5 + 36);
    packed |= (channels_m1 as u64) << (5 + 36);
    packed |= (bps_m1 as u64) << 36;
    packed |= samples & ((1u64 << 36) - 1);
    packed.to_be_bytes()
}

fn flac_header(audio_size: usize) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.extend_from_slice(b"fLaC");
    buf.push(0x80);
    buf.extend_from_slice(&[0, 0, 34]);
    buf.extend_from_slice(&[0, 0]);
    buf.extend_from_slice(&[0, 0]);
    buf.extend_from_slice(&[0, 0, 0]);
    buf.extend_from_slice(&[0, 0, 0]);
    let samples = (audio_size as u64 / 4).max(1);
    buf.extend_from_slice(&pack_flac_streaminfo(48_000, 1, 15, samples));
    buf.extend_from_slice(&[0u8; 16]);
    buf
}

fn full_flac(audio_size: usize) -> Vec<u8> {
    let mut buf = flac_header(audio_size);
    buf.resize(buf.len() + audio_size, 0);
    buf
}

fn mp3_frame() -> Vec<u8> {
    let mut frame = vec![0u8; 384];
    frame[0..4].copy_from_slice(&[0xFF, 0xFB, 0x94, 0x44]);
    frame
}

fn id3v2_tag(payload_size: usize) -> Vec<u8> {
    let syncsafe = [
        ((payload_size >> 21) & 0x7F) as u8,
        ((payload_size >> 14) & 0x7F) as u8,
        ((payload_size >> 7) & 0x7F) as u8,
        (payload_size & 0x7F) as u8,
    ];
    let mut buf = Vec::new();
    buf.extend_from_slice(b"ID3");
    buf.extend_from_slice(&[4, 0, 0]);
    buf.extend_from_slice(&syncsafe);
    buf
}

fn full_mp3(id3_payload_size: usize, audio_tail_size: usize) -> Vec<u8> {
    let mut buf = id3v2_tag(id3_payload_size);
    buf.resize(buf.len() + id3_payload_size, 0);
    buf.extend(mp3_frame());
    buf.resize(buf.len() + audio_tail_size, 0);
    buf
}

fn ogg_page(header_type: u8, granule: u64, serial: u32, seq: u32, payload: &[u8]) -> Vec<u8> {
    assert!(payload.len() <= 255);
    let mut buf = Vec::new();
    buf.extend_from_slice(b"OggS");
    buf.push(0);
    buf.push(header_type);
    buf.extend_from_slice(&granule.to_le_bytes());
    buf.extend_from_slice(&serial.to_le_bytes());
    buf.extend_from_slice(&seq.to_le_bytes());
    buf.extend_from_slice(&0u32.to_le_bytes());
    buf.push(1);
    buf.push(payload.len() as u8);
    buf.extend_from_slice(payload);
    buf
}

fn small_ogg_vorbis() -> Vec<u8> {
    let mut ident = Vec::new();
    ident.extend_from_slice(b"\x01vorbis");
    ident.extend_from_slice(&0u32.to_le_bytes());
    ident.push(2);
    ident.extend_from_slice(&48_000u32.to_le_bytes());
    ident.extend_from_slice(&0u32.to_le_bytes());
    ident.extend_from_slice(&128_000u32.to_le_bytes());
    ident.extend_from_slice(&0u32.to_le_bytes());
    ident.extend_from_slice(&[0xB0, 1]);

    let mut comment = Vec::new();
    comment.extend_from_slice(b"\x03vorbis");
    comment.extend_from_slice(&5u32.to_le_bytes());
    comment.extend_from_slice(b"bench");
    comment.extend_from_slice(&0u32.to_le_bytes());

    let mut buf = Vec::new();
    buf.extend(ogg_page(0x02, u64::MAX, 1, 0, &ident));
    buf.extend(ogg_page(0x00, u64::MAX, 1, 1, &comment));
    buf.extend(ogg_page(0x04, 48_000, 1, 2, &[0]));
    buf
}

fn ts_packet(pid: u16, payload_unit_start: bool, payload: &[u8]) -> Vec<u8> {
    let mut pkt = vec![0xFFu8; 188];
    pkt[0] = 0x47;
    pkt[1] = if payload_unit_start { 0x40 } else { 0x00 } | ((pid >> 8) as u8 & 0x1F);
    pkt[2] = pid as u8;
    pkt[3] = 0x10;
    let copy = payload.len().min(184);
    pkt[4..4 + copy].copy_from_slice(&payload[..copy]);
    pkt
}

fn psi_packet(pid: u16, section: &[u8]) -> Vec<u8> {
    let mut payload = Vec::with_capacity(section.len() + 1);
    payload.push(0);
    payload.extend_from_slice(section);
    ts_packet(pid, true, &payload)
}

fn pat_section(program_number: u16, pmt_pid: u16) -> Vec<u8> {
    let section_length: u16 = 13;
    let mut s = Vec::new();
    s.push(0x00);
    s.push(0xB0 | ((section_length >> 8) as u8 & 0x0F));
    s.push(section_length as u8);
    s.extend_from_slice(&[0x00, 0x01, 0xC1, 0x00, 0x00]);
    s.extend_from_slice(&program_number.to_be_bytes());
    s.extend_from_slice(&(0xE000u16 | (pmt_pid & 0x1FFF)).to_be_bytes());
    s.extend_from_slice(&[0, 0, 0, 0]);
    s
}

fn pmt_section(program_number: u16, pcr_pid: u16, streams: &[(u8, u16)]) -> Vec<u8> {
    let body_len: u16 = streams.iter().map(|_| 5u16).sum();
    let section_length: u16 = 9 + body_len + 4;
    let mut s = Vec::new();
    s.push(0x02);
    s.push(0xB0 | ((section_length >> 8) as u8 & 0x0F));
    s.push(section_length as u8);
    s.extend_from_slice(&program_number.to_be_bytes());
    s.extend_from_slice(&[0xC1, 0x00, 0x00]);
    s.extend_from_slice(&(0xE000u16 | (pcr_pid & 0x1FFF)).to_be_bytes());
    s.extend_from_slice(&[0xF0, 0x00]);
    for &(stream_type, pid) in streams {
        s.push(stream_type);
        s.extend_from_slice(&(0xE000u16 | (pid & 0x1FFF)).to_be_bytes());
        s.extend_from_slice(&[0xF0, 0x00]);
    }
    s.extend_from_slice(&[0, 0, 0, 0]);
    s
}

fn small_mpeg_ts() -> Vec<u8> {
    let mut out = Vec::new();
    out.extend(psi_packet(0, &pat_section(1, 0x1000)));
    out.extend(psi_packet(0x1000, &pmt_section(1, 0x101, &[(0x02, 0x101), (0x0F, 0x102)])));
    for _ in 0..14 {
        out.extend(ts_packet(0x1FFF, false, &[]));
    }
    out
}

fn small_mpeg_ps() -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&[0x00, 0x00, 0x01, 0xBA, 0x44, 0, 4, 0, 4, 1, 0x89, 0xC3, 0xF8, 0]);
    out.extend_from_slice(&[0x00, 0x00, 0x01, 0xE0, 0x00, 0x08]);
    out.extend_from_slice(&[0x00, 0x00, 0x01, 0xB3, 0x2D, 0x02, 0x40, 0x33]);
    out
}

fn bytes_label(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{} MiB", bytes / (1024 * 1024))
    } else if bytes >= 1024 {
        format!("{} KiB", bytes / 1024)
    } else {
        format!("{bytes} B")
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

    // ── extended small/large corpus for format-family coverage ───
    let (small_webm_file, small_webm_total) = write_temp_bytes(&small_webm());
    let (large_webm_header, large_webm_total) = large_mkv_sparse_header(b"webm", large_data);
    let (large_webm_file, large_webm_total) =
        write_sparse_temp(&large_webm_header, large_webm_total);

    let (small_matroska_file, small_matroska_total) = write_temp_bytes(&small_matroska());
    let (large_matroska_header, large_matroska_total) =
        large_mkv_sparse_header(b"matroska", large_data);
    let (large_matroska_file, large_matroska_total) =
        write_sparse_temp(&large_matroska_header, large_matroska_total);

    let small_rf64 = full_rf64(100);
    let (small_rf64_file, small_rf64_total) = write_temp_bytes(&small_rf64);
    let large_rf64_header = rf64_header(large_data as u64);
    let large_rf64_total = large_rf64_header.len() as u64 + large_data as u64;
    let (large_rf64_file, large_rf64_total) =
        write_sparse_temp(&large_rf64_header, large_rf64_total);

    let small_aiff = full_aiff(100);
    let (small_aiff_file, small_aiff_total) = write_temp_bytes(&small_aiff);
    let large_aiff_header = aiff_header(large_data);
    let large_aiff_total = large_aiff_header.len() as u64 + large_data as u64;
    let (large_aiff_file, large_aiff_total) =
        write_sparse_temp(&large_aiff_header, large_aiff_total);

    let small_flac = full_flac(100);
    let (small_flac_file, small_flac_total) = write_temp_bytes(&small_flac);
    let large_flac_header = flac_header(large_data);
    let large_flac_total = large_flac_header.len() as u64 + large_data as u64;
    let (large_flac_file, large_flac_total) =
        write_sparse_temp(&large_flac_header, large_flac_total);

    let small_mp3 = full_mp3(0, 0);
    let (small_mp3_file, small_mp3_total) = write_temp_bytes(&small_mp3);
    let large_mp3_header = full_mp3(1024, 0);
    let large_mp3_total = large_mp3_header.len() as u64 + large_data as u64;
    let (large_mp3_file, large_mp3_total) = write_sparse_temp(&large_mp3_header, large_mp3_total);

    let small_ogg = small_ogg_vorbis();
    let (small_ogg_file, small_ogg_total) = write_temp_bytes(&small_ogg);
    let large_ogg_total = small_ogg.len() as u64 + large_data as u64;
    let (large_ogg_file, large_ogg_total) = write_sparse_temp(&small_ogg, large_ogg_total);

    let small_ts = small_mpeg_ts();
    let (small_ts_file, small_ts_total) = write_temp_bytes(&small_ts);
    let large_ts_total = small_ts.len() as u64 + large_data as u64;
    let (large_ts_file, large_ts_total) = write_sparse_temp(&small_ts, large_ts_total);

    let small_ps = small_mpeg_ps();
    let (small_ps_file, small_ps_total) = write_temp_bytes(&small_ps);
    let large_ps_total = small_ps.len() as u64 + large_data as u64;
    let (large_ps_file, large_ps_total) = write_sparse_temp(&small_ps, large_ps_total);

    // ── group: from_bytes (in-memory buffer) ────────────────────
    let mut group = c.benchmark_group("from_bytes");
    group.throughput(Throughput::Bytes(small.len() as u64));
    group.bench_function(BenchmarkId::new("small_wav", "144 B"), |b| {
        b.iter(|| black_box(revelo::Metadata::from_bytes(black_box(&small))));
    });

    group.throughput(Throughput::Bytes(large_bytes.len() as u64));
    group.bench_function(BenchmarkId::new("large_wav_in_memory", "100 MiB"), |b| {
        b.iter(|| black_box(revelo::Metadata::from_bytes(black_box(large_bytes.as_slice()))));
    });
    group.finish();

    // ── group: from_file (mmap) ─────────────────────────────────
    let mut group = c.benchmark_group("from_file_mmap");
    let small_path = small_file.path().to_str().expect("path").to_string();
    group.throughput(Throughput::Bytes(small.len() as u64));
    group.bench_function(BenchmarkId::new("small_wav", "144 B"), |b| {
        b.iter(|| black_box(revelo::Metadata::from_file(black_box(&small_path))));
    });

    let large_path = large_file.path().to_str().expect("path").to_string();
    group.throughput(Throughput::Bytes(large_total));
    group.bench_function(BenchmarkId::new("large_wav_sparse", "100 MiB"), |b| {
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

    let small_webm_path = small_webm_file.path().to_str().expect("path").to_string();
    group.throughput(Throughput::Bytes(small_webm_total));
    group.bench_function(BenchmarkId::new("small_webm", bytes_label(small_webm_total)), |b| {
        b.iter(|| black_box(revelo::Metadata::from_file(black_box(&small_webm_path))));
    });
    let large_webm_path = large_webm_file.path().to_str().expect("path").to_string();
    group.throughput(Throughput::Bytes(large_webm_total));
    group.bench_function(
        BenchmarkId::new("large_webm_sparse", bytes_label(large_webm_total)),
        |b| {
            b.iter(|| black_box(revelo::Metadata::from_file(black_box(&large_webm_path))));
        },
    );
    let small_matroska_path = small_matroska_file.path().to_str().expect("path").to_string();
    group.throughput(Throughput::Bytes(small_matroska_total));
    group.bench_function(
        BenchmarkId::new("small_matroska", bytes_label(small_matroska_total)),
        |b| {
            b.iter(|| black_box(revelo::Metadata::from_file(black_box(&small_matroska_path))));
        },
    );
    let large_matroska_path = large_matroska_file.path().to_str().expect("path").to_string();
    group.throughput(Throughput::Bytes(large_matroska_total));
    group.bench_function(
        BenchmarkId::new("large_matroska_sparse", bytes_label(large_matroska_total)),
        |b| {
            b.iter(|| black_box(revelo::Metadata::from_file(black_box(&large_matroska_path))));
        },
    );

    let small_rf64_path = small_rf64_file.path().to_str().expect("path").to_string();
    group.throughput(Throughput::Bytes(small_rf64_total));
    group.bench_function(BenchmarkId::new("small_rf64", bytes_label(small_rf64_total)), |b| {
        b.iter(|| black_box(revelo::Metadata::from_file(black_box(&small_rf64_path))));
    });
    let large_rf64_path = large_rf64_file.path().to_str().expect("path").to_string();
    group.throughput(Throughput::Bytes(large_rf64_total));
    group.bench_function(
        BenchmarkId::new("large_rf64_sparse", bytes_label(large_rf64_total)),
        |b| {
            b.iter(|| black_box(revelo::Metadata::from_file(black_box(&large_rf64_path))));
        },
    );

    let small_aiff_path = small_aiff_file.path().to_str().expect("path").to_string();
    group.throughput(Throughput::Bytes(small_aiff_total));
    group.bench_function(BenchmarkId::new("small_aiff", bytes_label(small_aiff_total)), |b| {
        b.iter(|| black_box(revelo::Metadata::from_file(black_box(&small_aiff_path))));
    });
    let large_aiff_path = large_aiff_file.path().to_str().expect("path").to_string();
    group.throughput(Throughput::Bytes(large_aiff_total));
    group.bench_function(
        BenchmarkId::new("large_aiff_sparse", bytes_label(large_aiff_total)),
        |b| {
            b.iter(|| black_box(revelo::Metadata::from_file(black_box(&large_aiff_path))));
        },
    );

    let small_flac_path = small_flac_file.path().to_str().expect("path").to_string();
    group.throughput(Throughput::Bytes(small_flac_total));
    group.bench_function(BenchmarkId::new("small_flac", bytes_label(small_flac_total)), |b| {
        b.iter(|| black_box(revelo::Metadata::from_file(black_box(&small_flac_path))));
    });
    let large_flac_path = large_flac_file.path().to_str().expect("path").to_string();
    group.throughput(Throughput::Bytes(large_flac_total));
    group.bench_function(
        BenchmarkId::new("large_flac_sparse", bytes_label(large_flac_total)),
        |b| {
            b.iter(|| black_box(revelo::Metadata::from_file(black_box(&large_flac_path))));
        },
    );

    let small_mp3_path = small_mp3_file.path().to_str().expect("path").to_string();
    group.throughput(Throughput::Bytes(small_mp3_total));
    group.bench_function(BenchmarkId::new("small_mp3", bytes_label(small_mp3_total)), |b| {
        b.iter(|| black_box(revelo::Metadata::from_file(black_box(&small_mp3_path))));
    });
    let large_mp3_path = large_mp3_file.path().to_str().expect("path").to_string();
    group.throughput(Throughput::Bytes(large_mp3_total));
    group.bench_function(
        BenchmarkId::new("large_mp3_sparse_tail", bytes_label(large_mp3_total)),
        |b| {
            b.iter(|| black_box(revelo::Metadata::from_file(black_box(&large_mp3_path))));
        },
    );

    let small_ogg_path = small_ogg_file.path().to_str().expect("path").to_string();
    group.throughput(Throughput::Bytes(small_ogg_total));
    group.bench_function(BenchmarkId::new("small_ogg_vorbis", bytes_label(small_ogg_total)), |b| {
        b.iter(|| black_box(revelo::Metadata::from_file(black_box(&small_ogg_path))));
    });
    let large_ogg_path = large_ogg_file.path().to_str().expect("path").to_string();
    group.throughput(Throughput::Bytes(large_ogg_total));
    group.bench_function(
        BenchmarkId::new("large_ogg_sparse_tail", bytes_label(large_ogg_total)),
        |b| {
            b.iter(|| black_box(revelo::Metadata::from_file(black_box(&large_ogg_path))));
        },
    );

    let small_ts_path = small_ts_file.path().to_str().expect("path").to_string();
    group.throughput(Throughput::Bytes(small_ts_total));
    group.bench_function(BenchmarkId::new("small_mpeg_ts", bytes_label(small_ts_total)), |b| {
        b.iter(|| black_box(revelo::Metadata::from_file(black_box(&small_ts_path))));
    });
    let large_ts_path = large_ts_file.path().to_str().expect("path").to_string();
    group.throughput(Throughput::Bytes(large_ts_total));
    group.bench_function(
        BenchmarkId::new("large_mpeg_ts_sparse", bytes_label(large_ts_total)),
        |b| {
            b.iter(|| black_box(revelo::Metadata::from_file(black_box(&large_ts_path))));
        },
    );

    let small_ps_path = small_ps_file.path().to_str().expect("path").to_string();
    group.throughput(Throughput::Bytes(small_ps_total));
    group.bench_function(BenchmarkId::new("small_mpeg_ps", bytes_label(small_ps_total)), |b| {
        b.iter(|| black_box(revelo::Metadata::from_file(black_box(&small_ps_path))));
    });
    let large_ps_path = large_ps_file.path().to_str().expect("path").to_string();
    group.throughput(Throughput::Bytes(large_ps_total));
    group.bench_function(
        BenchmarkId::new("large_mpeg_ps_sparse", bytes_label(large_ps_total)),
        |b| {
            b.iter(|| black_box(revelo::Metadata::from_file(black_box(&large_ps_path))));
        },
    );
    group.finish();

    // ── group: from_file_owned (full read → from_bytes) ─────────
    let mut group = c.benchmark_group("from_file_owned");
    group.throughput(Throughput::Bytes(small.len() as u64));
    group.bench_function(BenchmarkId::new("small_wav", "144 B"), |b| {
        b.iter(|| black_box(revelo::Metadata::from_file_owned(black_box(&small_path))));
    });

    group.throughput(Throughput::Bytes(large_total));
    group.bench_function(BenchmarkId::new("large_wav_owned", "100 MiB"), |b| {
        b.iter(|| black_box(revelo::Metadata::from_file_owned(black_box(&large_path))));
    });
    group.finish();
}

criterion_group!(benches, bench_parse);
criterion_main!(benches);
