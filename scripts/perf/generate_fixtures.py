#!/usr/bin/env python3
"""Generate sparse media fixtures for cross-tool benchmark comparisons."""

from __future__ import annotations

import argparse
import json
import shutil
import struct
import subprocess
import tempfile
from pathlib import Path
from typing import Any, Callable, NamedTuple


ROOT = Path(__file__).resolve().parents[2]
DEFAULT_FIXTURE_DIR = ROOT / "target" / "perf-fixtures"

MIN_SIZE = 4096


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path, help="manifest containing synthetic cases")
    parser.add_argument("--out-dir", type=Path, default=DEFAULT_FIXTURE_DIR)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()

    if args.self_test:
        return self_test()
    if args.manifest is None:
        parser.error("--manifest is required unless --self-test is used")

    manifest = json.loads(args.manifest.read_text(encoding="utf-8"))
    generated: list[dict[str, Any]] = []
    for case in manifest.get("cases", []):
        if "synthetic" not in case:
            continue
        path = generate_case_fixture(case, args.out_dir)
        generated.append({"id": case.get("id") or case["label"], "path": str(path), "size_bytes": path.stat().st_size})
    print(json.dumps({"fixtures": generated}, indent=2))
    return 0


def generate_case_fixture(case: dict[str, Any], out_dir: Path) -> Path:
    synthetic = case.get("synthetic")
    if not isinstance(synthetic, dict):
        raise SystemExit(f"case {case.get('id') or case.get('label')} missing synthetic object")
    kind = synthetic.get("kind")
    if not isinstance(kind, str):
        raise SystemExit(f"case {case.get('id') or case.get('label')} missing synthetic kind")
    size_bytes = int(synthetic.get("size_bytes", 0))
    if size_bytes < MIN_SIZE:
        raise SystemExit(f"synthetic case {case.get('id') or case.get('label')} size must be at least {MIN_SIZE}")

    generator = GENERATORS.get(kind)
    if generator is None:
        supported = ", ".join(sorted(GENERATORS))
        raise SystemExit(f"unsupported synthetic fixture kind {kind!r}; supported: {supported}")

    case_id = safe_id(str(case.get("id") or case.get("label") or kind))
    extension = EXTENSIONS.get(kind, "bin")
    path = out_dir / f"{case_id}.{extension}"
    path.parent.mkdir(parents=True, exist_ok=True)
    generator(path, size_bytes)
    return path


def safe_id(value: str) -> str:
    return "".join(char if char.isalnum() or char in ("-", "_") else "-" for char in value).strip("-") or "fixture"


def write_exact_size(path: Path, size_bytes: int, writer: Callable[[Any, int], None]) -> None:
    with path.open("wb") as file:
        writer(file, size_bytes)
        file.flush()
        if file.tell() > size_bytes:
            raise SystemExit(f"fixture writer exceeded target size for {path.name}")
        file.truncate(size_bytes)


def run_checked(command: list[str]) -> None:
    subprocess.run(command, check=True)


def require_tool(name: str) -> str:
    resolved = shutil.which(name)
    if resolved is None:
        raise SystemExit(f"{name} is required to generate this synthetic fixture")
    return resolved


def truncate_sparse(path: Path, size_bytes: int) -> None:
    with path.open("ab") as file:
        file.truncate(size_bytes)


def ffmpeg_base_video(size: str = "640x360", rate: str = "30") -> list[str]:
    return [
        require_tool("ffmpeg"),
        "-hide_banner",
        "-loglevel",
        "error",
        "-y",
        "-f",
        "lavfi",
        "-i",
        f"testsrc2=size={size}:rate={rate}",
        "-f",
        "lavfi",
        "-i",
        "sine=frequency=1000:sample_rate=48000",
        "-t",
        "2",
        "-shortest",
    ]


def ffmpeg_base_audio(source: str = "sine=frequency=1000:sample_rate=48000") -> list[str]:
    return [
        require_tool("ffmpeg"),
        "-hide_banner",
        "-loglevel",
        "error",
        "-y",
        "-f",
        "lavfi",
        "-i",
        source,
        "-t",
        "2",
    ]


def ffmpeg_metadata_args() -> list[str]:
    return [
        "-metadata",
        "title=Revelo benchmark fixture",
        "-metadata",
        "comment=generated public benchmark fixture",
    ]


def write_ffmpeg_sparse(path: Path, size_bytes: int, command: list[str]) -> None:
    path.unlink(missing_ok=True)
    tmp = path.with_name(f"{path.stem}.tmp{path.suffix}")
    tmp.unlink(missing_ok=True)
    try:
        run_checked([*command, str(tmp)])
        if tmp.stat().st_size > size_bytes:
            raise SystemExit(f"ffmpeg fixture exceeded target size for {path.name}")
        truncate_sparse(tmp, size_bytes)
        tmp.replace(path)
    finally:
        tmp.unlink(missing_ok=True)


def patch_major_brand(path: Path, brand: bytes) -> None:
    if len(brand) != 4:
        raise SystemExit("major brand must be exactly four bytes")
    with path.open("r+b") as file:
        header = file.read(16)
        if len(header) < 16 or header[4:8] != b"ftyp":
            raise SystemExit(f"{path.name} does not start with an ftyp box")
        file.seek(8)
        file.write(brand)


def box(name: bytes, payload: bytes) -> bytes:
    return struct.pack(">I4s", len(payload) + 8, name) + payload


def full_box(name: bytes, version: int, flags: int, payload: bytes) -> bytes:
    return box(name, bytes([version]) + flags.to_bytes(3, "big") + payload)


def make_ftyp(major: bytes = b"isom", brands: tuple[bytes, ...] = (b"isom", b"iso2", b"mp41")) -> bytes:
    return box(b"ftyp", major + b"\x00\x00\x02\x00" + b"".join(brands))


def make_moov() -> bytes:
    mvhd = full_box(
        b"mvhd",
        0,
        0,
        b"\x00" * 8
        + struct.pack(">II", 1000, 1000)
        + struct.pack(">I", 0x00010000)
        + struct.pack(">H", 0x0100)
        + b"\x00" * 10
        + b"\x00\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00"
        + b"\x00\x00\x00\x00\x00\x01\x00\x00\x00\x00\x00\x00"
        + b"\x00\x00\x00\x00\x00\x00\x00\x00\x40\x00\x00\x00"
        + b"\x00" * 24
        + struct.pack(">I", 2),
    )
    return box(b"moov", mvhd)


def write_mp4(path: Path, size_bytes: int, *, major: bytes, brands: tuple[bytes, ...], moov_tail: bool) -> None:
    ftyp = make_ftyp(major, brands)
    moov = make_moov()
    mdat_header_size = 8

    def writer(file: Any, target: int) -> None:
        file.write(ftyp)
        if moov_tail:
            mdat_size = target - len(ftyp) - len(moov)
            if mdat_size < mdat_header_size:
                raise SystemExit("target MP4 size too small")
            file.write(struct.pack(">I4s", mdat_size, b"mdat"))
            file.seek(target - len(moov))
            file.write(moov)
        else:
            file.write(moov)
            mdat_size = target - len(ftyp) - len(moov)
            if mdat_size < mdat_header_size:
                raise SystemExit("target MP4 size too small")
            file.write(struct.pack(">I4s", mdat_size, b"mdat"))

    write_exact_size(path, size_bytes, writer)


def generate_mp4_moov_front(path: Path, size_bytes: int) -> None:
    write_mp4(path, size_bytes, major=b"isom", brands=(b"isom", b"iso2", b"mp41"), moov_tail=False)


def generate_mp4_moov_tail(path: Path, size_bytes: int) -> None:
    write_mp4(path, size_bytes, major=b"isom", brands=(b"isom", b"iso2", b"mp41"), moov_tail=True)


def generate_mov_moov_tail(path: Path, size_bytes: int) -> None:
    write_mp4(path, size_bytes, major=b"qt  ", brands=(b"qt  ",), moov_tail=True)


def generate_mp4_snv2_tail(path: Path, size_bytes: int) -> None:
    write_mp4(path, size_bytes, major=b"SNV2", brands=(b"SNV2", b"isom", b"iso2", b"mp42"), moov_tail=True)


def generate_fragmented_mp4(path: Path, size_bytes: int) -> None:
    ftyp = make_ftyp(b"iso6", (b"iso6", b"mp41", b"dash"))
    moov = make_moov()
    moof = box(b"moof", box(b"mfhd", b"\x00\x00\x00\x00\x00\x00\x00\x01"))

    def writer(file: Any, target: int) -> None:
        file.write(ftyp)
        file.write(moov)
        file.write(moof)
        mdat_size = target - file.tell()
        if mdat_size < 8:
            raise SystemExit("target fragmented MP4 size too small")
        file.write(struct.pack(">I4s", mdat_size, b"mdat"))

    write_exact_size(path, size_bytes, writer)


def generate_wav(
    path: Path,
    size_bytes: int,
    *,
    rf64: bool = False,
    bext: bool = False,
    channels: int = 2,
    sample_rate: int = 48_000,
    bits_per_sample: int = 16,
) -> None:
    list_chunk = b"INFO" + b"INAM" + struct.pack("<I", 8) + b"fixture\x00"
    id3_chunk = b"ID3\x04\x00\x00\x00\x00\x00\x10" + b"\x00" * 16
    bext_chunk = b"benchmark".ljust(602, b"\x00") if bext else b""

    def chunk(name: bytes, payload: bytes) -> bytes:
        return name + struct.pack("<I", len(payload)) + payload + (b"\x00" if len(payload) % 2 else b"")

    block_align = channels * bits_per_sample // 8
    byte_rate = sample_rate * block_align
    fmt = chunk(b"fmt ", struct.pack("<HHIIHH", 1, channels, sample_rate, byte_rate, block_align, bits_per_sample))
    metadata = chunk(b"LIST", list_chunk) + chunk(b"ID3 ", id3_chunk)
    if bext:
        metadata = chunk(b"bext", bext_chunk) + metadata

    def writer(file: Any, target: int) -> None:
        if rf64:
            file.write(b"RF64\xff\xff\xff\xffWAVE")
            data_bytes = target - (12 + 36 + len(fmt) + len(metadata) + 8)
            ds64 = struct.pack("<QQQI", target - 8, data_bytes, 0, 0)
            file.write(chunk(b"ds64", ds64))
            file.write(fmt)
            file.write(metadata)
            file.write(b"data\xff\xff\xff\xff")
        else:
            file.write(b"RIFF\xff\xff\xff\xffWAVE")
            file.write(fmt)
            file.write(metadata)
            file.write(b"data")
            data_size = target - file.tell() - 4
            file.write(struct.pack("<I", max(0, data_size)))

    write_exact_size(path, size_bytes, writer)


def generate_wav_list_id3_data(path: Path, size_bytes: int) -> None:
    generate_wav(path, size_bytes)


def generate_bwf_data(path: Path, size_bytes: int) -> None:
    generate_wav(path, size_bytes, bext=True)


def generate_rf64_ds64_data(path: Path, size_bytes: int) -> None:
    generate_wav(path, size_bytes, rf64=True)


def generate_aiff(path: Path, size_bytes: int, *, compressed: bool = False, compression_type: bytes = b"NONE") -> None:
    form = b"AIFC" if compressed else b"AIFF"
    common_payload = b"\x00\x02\x00\x00\x00\x01\x00\x10@\x0e\xac\x44\x00\x00\x00\x00\x00\x00"
    if compressed:
        if len(compression_type) != 4:
            raise SystemExit("AIFF-C compression type must be four bytes")
        common_payload += compression_type + b"\x0enot compressed"
    common = aiff_chunk(b"COMM", common_payload)
    name = aiff_chunk(b"NAME", b"fixture")
    id3 = aiff_chunk(b"ID3 ", b"ID3\x04\x00\x00\x00\x00\x00\x10" + b"\x00" * 16)

    def writer(file: Any, target: int) -> None:
        header_size = 12 + len(common) + len(name) + len(id3) + 16
        ssnd_payload_size = target - header_size
        if ssnd_payload_size < 0:
            raise SystemExit("target AIFF size too small")
        file.write(b"FORM")
        file.write(struct.pack(">I", target - 8))
        file.write(form)
        file.write(common)
        file.write(name)
        file.write(id3)
        file.write(struct.pack(">I4sII", ssnd_payload_size + 16, b"SSND", 0, 0))

    write_exact_size(path, size_bytes, writer)


def aiff_chunk(name: bytes, payload: bytes) -> bytes:
    padding = b"\x00" if len(payload) % 2 else b""
    return name + struct.pack(">I", len(payload)) + payload + padding


def generate_aiff_ssnd(path: Path, size_bytes: int) -> None:
    generate_aiff(path, size_bytes, compressed=False)


def generate_aifc_ssnd(path: Path, size_bytes: int) -> None:
    generate_aiff(path, size_bytes, compressed=True)


def generate_flac_large(path: Path, size_bytes: int) -> None:
    def writer(file: Any, target: int) -> None:
        file.write(b"fLaC")
        audio_size = max(target - 42, 4)
        streaminfo = flac_streaminfo_payload(audio_size)
        file.write(bytes([0x80]) + len(streaminfo).to_bytes(3, "big") + streaminfo)

    write_exact_size(path, size_bytes, writer)


def flac_streaminfo_payload(audio_size: int) -> bytes:
    samples = max(audio_size // 4, 1)
    packed = pack_flac_streaminfo(48_000, 1, 15, samples)
    return b"\x00\x10" + b"\x10\x00" + b"\x00\x00\x00" + b"\x00\x00\x00" + packed + b"\x00" * 16


def pack_flac_streaminfo(sample_rate: int, channels_m1: int, bps_m1: int, samples: int) -> bytes:
    packed = 0
    packed |= sample_rate << (3 + 5 + 36)
    packed |= channels_m1 << (5 + 36)
    packed |= bps_m1 << 36
    packed |= samples & ((1 << 36) - 1)
    return packed.to_bytes(8, "big")


def generate_mp3_id3_large(path: Path, size_bytes: int) -> None:
    tag_payload = b"TIT2\x00\x00\x00\x08\x00\x00fixture\x00"
    tag_size = len(tag_payload)
    syncsafe = bytes([
        (tag_size >> 21) & 0x7F,
        (tag_size >> 14) & 0x7F,
        (tag_size >> 7) & 0x7F,
        tag_size & 0x7F,
    ])

    def writer(file: Any, target: int) -> None:
        file.write(b"ID3\x04\x00\x00" + syncsafe + tag_payload)
        file.write(b"\xff\xfb\x90\x64")

    write_exact_size(path, size_bytes, writer)


def generate_ogg_large(path: Path, size_bytes: int) -> None:
    opus_head = (
        b"OpusHead"
        + bytes([1, 2])
        + (312).to_bytes(2, "little")
        + (48_000).to_bytes(4, "little")
        + (0).to_bytes(2, "little")
        + bytes([0])
    )
    opus_tags = b"OpusTags" + (0).to_bytes(4, "little") + (0).to_bytes(4, "little")
    opus_packet = b"\xfc\xff\xfe"
    header = (
        ogg_page(opus_head, header_type=2, sequence=0, granule_position=0)
        + ogg_page(opus_tags, header_type=0, sequence=1, granule_position=0)
        + ogg_page(opus_packet, header_type=4, sequence=2, granule_position=960)
    )

    def writer(file: Any, target: int) -> None:
        file.write(header)

    write_exact_size(path, size_bytes, writer)


def ogg_page(packet: bytes, *, header_type: int, sequence: int, granule_position: int) -> bytes:
    segments = []
    remaining = len(packet)
    while remaining >= 255:
        segments.append(255)
        remaining -= 255
    segments.append(remaining)
    header = (
        b"OggS"
        + bytes([0, header_type])
        + granule_position.to_bytes(8, "little")
        + (1).to_bytes(4, "little")
        + sequence.to_bytes(4, "little")
        + b"\x00\x00\x00\x00"
        + bytes([len(segments)])
        + bytes(segments)
    )
    page = header + packet
    crc = ogg_crc(page)
    return page[:22] + crc.to_bytes(4, "little") + page[26:]


def ogg_crc(data: bytes) -> int:
    crc = 0
    for byte in data:
        crc ^= byte << 24
        for _ in range(8):
            if crc & 0x8000_0000:
                crc = ((crc << 1) ^ 0x04C1_1DB7) & 0xFFFF_FFFF
            else:
                crc = (crc << 1) & 0xFFFF_FFFF
    return crc


def generate_mpeg_ts_large(path: Path, size_bytes: int) -> None:
    packet = bytes([0x47, 0x40, 0x00, 0x10]) + b"\xff" * 184

    def writer(file: Any, target: int) -> None:
        repeats = min(16, target // len(packet))
        for _ in range(repeats):
            file.write(packet)

    write_exact_size(path, size_bytes, writer)


def generate_mpeg_ps_large(path: Path, size_bytes: int) -> None:
    header = b"\x00\x00\x01\xba" + b"\x44\x00\x04\x00\x04\x01\x89\xc3\xf8"

    def writer(file: Any, target: int) -> None:
        file.write(header)

    write_exact_size(path, size_bytes, writer)


def generate_avi_large(path: Path, size_bytes: int) -> None:
    write_ffmpeg_sparse(
        path,
        size_bytes,
        ffmpeg_base_video()
        + [
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
    )


def generate_wav_192khz_data(path: Path, size_bytes: int) -> None:
    generate_wav(path, size_bytes, sample_rate=192_000)


def generate_wav_9ch_data(path: Path, size_bytes: int) -> None:
    generate_wav(path, size_bytes, channels=9)


def generate_aifc_mace3_ssnd(path: Path, size_bytes: int) -> None:
    generate_aiff(path, size_bytes, compressed=True, compression_type=b"MAC3")


def generate_webm_sparse(path: Path, size_bytes: int) -> None:
    generate_ebml_sparse(path, size_bytes, doc_type=b"webm")


def generate_mkv_sparse(path: Path, size_bytes: int) -> None:
    generate_ebml_sparse(path, size_bytes, doc_type=b"matroska")


def generate_ebml_sparse(path: Path, size_bytes: int, *, doc_type: bytes) -> None:
    header_payload = (
        bytes.fromhex("42868101")
        + bytes.fromhex("42F78101")
        + bytes.fromhex("42F28104")
        + bytes.fromhex("42F38108")
        + bytes.fromhex("4282")
        + vint_size(len(doc_type))
        + doc_type
    )
    header = bytes.fromhex("1A45DFA3") + vint_size(len(header_payload)) + header_payload
    segment = bytes.fromhex("18538067") + b"\x01\xff\xff\xff\xff\xff\xff\xff"
    info = bytes.fromhex("1549A966") + b"\x84" + b"\x2A\xD7\xB1\x81\x0F"
    cluster = bytes.fromhex("1F43B675") + b"\x01\xff\xff\xff\xff\xff\xff\xff"

    def writer(file: Any, target: int) -> None:
        file.write(header)
        file.write(segment)
        file.write(info)
        file.write(cluster)

    write_exact_size(path, size_bytes, writer)


def vint_size(size: int) -> bytes:
    if size < 0x7F:
        return bytes([0x80 | size])
    if size < 0x3FFF:
        return bytes([0x40 | (size >> 8), size & 0xFF])
    raise SystemExit(f"EBML fixture size field too large: {size}")


class FfmpegFixture(NamedTuple):
    extension: str
    command: Callable[[], list[str]]
    major_brand: bytes | None = None


def with_metadata(command: list[str]) -> list[str]:
    return [*command, *ffmpeg_metadata_args()]


def make_ffmpeg_generator(fixture: FfmpegFixture) -> Callable[[Path, int], None]:
    def generate(path: Path, size_bytes: int) -> None:
        write_ffmpeg_sparse(path, size_bytes, fixture.command())
        if fixture.major_brand is not None:
            patch_major_brand(path, fixture.major_brand)

    return generate


FFMPEG_FIXTURES: dict[str, FfmpegFixture] = {
    "ffmpeg_mp4_avc_faststart": FfmpegFixture(
        "mp4",
        lambda: with_metadata(
            ffmpeg_base_video()
            + ["-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p", "-c:a", "aac", "-b:a", "96k", "-movflags", "+faststart"]
        ),
    ),
    "ffmpeg_mp4_snv2_faststart": FfmpegFixture(
        "mp4",
        lambda: with_metadata(
            ffmpeg_base_video()
            + ["-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p", "-c:a", "aac", "-b:a", "96k", "-movflags", "+faststart"]
        ),
        b"SNV2",
    ),
    "ffmpeg_mp4_hevc10_faststart": FfmpegFixture(
        "mp4",
        lambda: with_metadata(
            ffmpeg_base_video()
            + [
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
            ]
        ),
    ),
    "ffmpeg_mp4_av1_faststart": FfmpegFixture(
        "mp4",
        lambda: with_metadata(
            ffmpeg_base_video()
            + [
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
            ]
        ),
    ),
    "ffmpeg_mp4_aac_audio": FfmpegFixture("m4a", lambda: with_metadata(ffmpeg_base_audio() + ["-c:a", "aac", "-b:a", "128k", "-f", "mp4"])),
    "ffmpeg_mp4_tail": FfmpegFixture(
        "mp4",
        lambda: with_metadata(ffmpeg_base_video() + ["-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p", "-c:a", "aac", "-b:a", "96k"]),
    ),
    "ffmpeg_mov_mpeg4_pcm": FfmpegFixture("mov", lambda: with_metadata(ffmpeg_base_video() + ["-c:v", "mpeg4", "-q:v", "5", "-c:a", "pcm_s16be", "-f", "mov"])),
    "ffmpeg_fragmented_mp4": FfmpegFixture(
        "mp4",
        lambda: with_metadata(
            ffmpeg_base_video() + ["-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p", "-c:a", "aac", "-movflags", "frag_keyframe+empty_moov+default_base_moof"]
        ),
    ),
    "ffmpeg_mkv_h264_aac": FfmpegFixture("mkv", lambda: with_metadata(ffmpeg_base_video() + ["-c:v", "libx264", "-preset", "ultrafast", "-c:a", "aac", "-f", "matroska"])),
    "ffmpeg_mkv_ffv1_flac": FfmpegFixture("mkv", lambda: with_metadata(ffmpeg_base_video() + ["-c:v", "ffv1", "-level", "3", "-c:a", "flac", "-f", "matroska"])),
    "ffmpeg_webm_vp8_opus": FfmpegFixture(
        "webm",
        lambda: with_metadata(
            ffmpeg_base_video() + ["-c:v", "libvpx", "-deadline", "realtime", "-cpu-used", "8", "-b:v", "500k", "-c:a", "libopus", "-b:a", "64k", "-f", "webm"]
        ),
    ),
    "ffmpeg_webm_av1_opus": FfmpegFixture(
        "webm",
        lambda: with_metadata(
            ffmpeg_base_video()
            + ["-c:v", "libsvtav1", "-preset", "13", "-crf", "45", "-pix_fmt", "yuv420p10le", "-c:a", "libopus", "-b:a", "64k", "-f", "webm"]
        ),
    ),
    "ffmpeg_flv_nellymoser": FfmpegFixture(
        "flv",
        lambda: with_metadata(ffmpeg_base_audio("sine=frequency=1000:sample_rate=44100") + ["-c:a", "nellymoser", "-f", "flv"]),
    ),
    "ffmpeg_asf_wma": FfmpegFixture("asf", lambda: with_metadata(ffmpeg_base_audio() + ["-c:a", "wmav2", "-f", "asf"])),
    "ffmpeg_flac": FfmpegFixture("flac", lambda: with_metadata(ffmpeg_base_audio("anoisesrc=r=48000:a=0.25") + ["-c:a", "flac", "-f", "flac"])),
    "ffmpeg_mp3": FfmpegFixture(
        "mp3",
        lambda: with_metadata(
            ffmpeg_base_audio("anoisesrc=r=48000:a=0.25") + ["-c:a", "libmp3lame", "-b:a", "320k", "-write_id3v2", "1", "-id3v2_version", "3", "-f", "mp3"]
        ),
    ),
    "ffmpeg_ogg_vorbis": FfmpegFixture(
        "ogg",
        lambda: with_metadata(ffmpeg_base_audio("anullsrc=r=48000:cl=stereo") + ["-strict", "-2", "-c:a", "vorbis", "-q:a", "5", "-f", "ogg"]),
    ),
    "ffmpeg_ogg_opus": FfmpegFixture(
        "ogg",
        lambda: with_metadata(ffmpeg_base_audio("anullsrc=r=48000:cl=stereo") + ["-c:a", "libopus", "-b:a", "128k", "-f", "ogg"]),
    ),
    "ffmpeg_ogg_flac": FfmpegFixture("oga", lambda: with_metadata(ffmpeg_base_audio("anoisesrc=r=48000:a=0.25") + ["-c:a", "flac", "-f", "ogg"])),
    "ffmpeg_mpeg_ts": FfmpegFixture("ts", lambda: with_metadata(ffmpeg_base_video() + ["-c:v", "mpeg2video", "-b:v", "2M", "-c:a", "mp2", "-b:a", "128k", "-f", "mpegts"])),
    "ffmpeg_mpeg_ts_ac3": FfmpegFixture("ts", lambda: with_metadata(ffmpeg_base_video() + ["-c:v", "mpeg2video", "-b:v", "2M", "-c:a", "ac3", "-b:a", "192k", "-f", "mpegts"])),
    "ffmpeg_m2ts": FfmpegFixture(
        "m2ts",
        lambda: with_metadata(ffmpeg_base_video() + ["-c:v", "mpeg2video", "-b:v", "2M", "-c:a", "mp2", "-b:a", "128k", "-mpegts_m2ts_mode", "1", "-f", "mpegts"]),
    ),
    "ffmpeg_mpeg_ps": FfmpegFixture("mpg", lambda: with_metadata(ffmpeg_base_video() + ["-c:v", "mpeg2video", "-b:v", "2M", "-c:a", "mp2", "-b:a", "128k", "-f", "mpeg"])),
    "ffmpeg_mpeg_ps_ac3": FfmpegFixture("mpg", lambda: with_metadata(ffmpeg_base_video() + ["-c:v", "mpeg2video", "-b:v", "2M", "-c:a", "ac3", "-b:a", "192k", "-f", "mpeg"])),
    "ffmpeg_vob": FfmpegFixture(
        "vob",
        lambda: with_metadata(ffmpeg_base_video("720x480", "30000/1001") + ["-c:v", "mpeg2video", "-b:v", "4M", "-c:a", "mp2", "-b:a", "192k", "-f", "vob"]),
    ),
    "ffmpeg_avi_mpeg4_mp3": FfmpegFixture("avi", lambda: with_metadata(ffmpeg_base_video() + ["-c:v", "mpeg4", "-q:v", "5", "-c:a", "libmp3lame", "-b:a", "128k", "-f", "avi"])),
    "ffmpeg_avi_mpeg4_wma": FfmpegFixture("avi", lambda: with_metadata(ffmpeg_base_video() + ["-c:v", "mpeg4", "-q:v", "5", "-c:a", "wmav2", "-b:a", "128k", "-f", "avi"])),
    "ffmpeg_raw_aac": FfmpegFixture("aac", lambda: ffmpeg_base_audio() + ["-c:a", "aac", "-b:a", "128k", "-f", "adts"]),
    "ffmpeg_raw_ac3": FfmpegFixture("ac3", lambda: ffmpeg_base_audio() + ["-c:a", "ac3", "-b:a", "192k", "-f", "ac3"]),
    "ffmpeg_raw_eac3": FfmpegFixture("eac3", lambda: ffmpeg_base_audio() + ["-c:a", "eac3", "-b:a", "192k", "-f", "eac3"]),
    "ffmpeg_opus": FfmpegFixture("opus", lambda: with_metadata(ffmpeg_base_audio() + ["-c:a", "libopus", "-b:a", "64k", "-f", "opus"])),
}


SPARSE_EXTENSIONS = {
    "mp4_moov_front": "mp4",
    "mp4_moov_tail": "mp4",
    "mp4_snv2_tail": "mp4",
    "mov_moov_tail": "mov",
    "fragmented_mp4": "mp4",
    "webm_sparse": "webm",
    "mkv_sparse": "mkv",
    "wav_list_id3_data": "wav",
    "wav_192khz_data": "wav",
    "wav_9ch_data": "wav",
    "bwf_data": "wav",
    "rf64_ds64_data": "wav",
    "aiff_ssnd": "aiff",
    "aifc_ssnd": "aifc",
    "aifc_mace3_ssnd": "aifc",
    "flac_large": "flac",
    "mp3_id3_large": "mp3",
    "ogg_large": "ogg",
    "mpeg_ts_large": "ts",
    "mpeg_ps_large": "vob",
    "avi_large": "avi",
}


EXTENSIONS = {**SPARSE_EXTENSIONS, **{kind: fixture.extension for kind, fixture in FFMPEG_FIXTURES.items()}}


SPARSE_GENERATORS: dict[str, Callable[[Path, int], None]] = {
    "mp4_moov_front": generate_mp4_moov_front,
    "mp4_moov_tail": generate_mp4_moov_tail,
    "mp4_snv2_tail": generate_mp4_snv2_tail,
    "mov_moov_tail": generate_mov_moov_tail,
    "fragmented_mp4": generate_fragmented_mp4,
    "webm_sparse": generate_webm_sparse,
    "mkv_sparse": generate_mkv_sparse,
    "wav_list_id3_data": generate_wav_list_id3_data,
    "wav_192khz_data": generate_wav_192khz_data,
    "wav_9ch_data": generate_wav_9ch_data,
    "bwf_data": generate_bwf_data,
    "rf64_ds64_data": generate_rf64_ds64_data,
    "aiff_ssnd": generate_aiff_ssnd,
    "aifc_ssnd": generate_aifc_ssnd,
    "aifc_mace3_ssnd": generate_aifc_mace3_ssnd,
    "flac_large": generate_flac_large,
    "mp3_id3_large": generate_mp3_id3_large,
    "ogg_large": generate_ogg_large,
    "mpeg_ts_large": generate_mpeg_ts_large,
    "mpeg_ps_large": generate_mpeg_ps_large,
    "avi_large": generate_avi_large,
}


GENERATORS: dict[str, Callable[[Path, int], None]] = {
    **SPARSE_GENERATORS,
    **{kind: make_ffmpeg_generator(fixture) for kind, fixture in FFMPEG_FIXTURES.items()},
}

SELF_TEST_KINDS = (
    "mp4_snv2_tail",
    "webm_sparse",
    "mkv_sparse",
    "flac_large",
    "ogg_large",
    "avi_large",
)


def self_test() -> int:
    assert set(GENERATORS) == set(EXTENSIONS)
    with tempfile.TemporaryDirectory() as tmp:
        for kind in SELF_TEST_KINDS:
            case = {
                "id": f"self-test-{kind}",
                "label": f"self test {kind}",
                "synthetic": {"kind": kind, "size_bytes": 8 * 1024 * 1024},
            }
            path = generate_case_fixture(case, Path(tmp))
            assert path.exists()
            assert path.stat().st_size == 8 * 1024 * 1024
        mp4 = Path(tmp) / "self-test-mp4_snv2_tail.mp4"
        assert mp4.read_bytes()[4:8] == b"ftyp"
        webm = Path(tmp) / "self-test-webm_sparse.webm"
        mkv = Path(tmp) / "self-test-mkv_sparse.mkv"
        assert webm.read_bytes()[:5] == bytes.fromhex("1A45DFA397")
        assert b"\x42\x82\x84webm" in webm.read_bytes()[:64]
        assert b"\x42\x82\x88matroska" in mkv.read_bytes()[:64]
        flac = Path(tmp) / "self-test-flac_large.flac"
        flac_bytes = flac.read_bytes()[:42]
        assert flac_bytes[:4] == b"fLaC"
        assert flac_bytes[4:8] == bytes([0x80, 0, 0, 34])
        assert flac_bytes[18:26] != b"\x00" * 8
        ogg = Path(tmp) / "self-test-ogg_large.ogg"
        ogg_bytes = ogg.read_bytes()
        assert ogg_bytes[:4] == b"OggS"
        segment_count = ogg_bytes[26]
        first_page_len = 27 + segment_count + sum(ogg_bytes[27 : 27 + segment_count])
        first_page = ogg_bytes[:first_page_len]
        assert ogg_crc(first_page[:22] + b"\x00\x00\x00\x00" + first_page[26:]) == int.from_bytes(
            first_page[22:26],
            "little",
        )
        avi = Path(tmp) / "self-test-avi_large.avi"
        avi_header = avi.read_bytes()[:16]
        assert avi_header[:4] == b"RIFF"
        assert avi_header[8:12] == b"AVI "
    print("fixture generator self-test ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
