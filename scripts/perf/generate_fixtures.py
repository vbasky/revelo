#!/usr/bin/env python3
"""Generate sparse media fixtures for cross-tool benchmark comparisons."""

from __future__ import annotations

import argparse
import json
import struct
import tempfile
from pathlib import Path
from typing import Any, Callable


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


def generate_wav(path: Path, size_bytes: int, *, rf64: bool = False, bext: bool = False) -> None:
    list_chunk = b"INFO" + b"INAM" + struct.pack("<I", 8) + b"fixture\x00"
    id3_chunk = b"ID3\x04\x00\x00\x00\x00\x00\x10" + b"\x00" * 16
    bext_chunk = b"benchmark".ljust(602, b"\x00") if bext else b""

    def chunk(name: bytes, payload: bytes) -> bytes:
        return name + struct.pack("<I", len(payload)) + payload + (b"\x00" if len(payload) % 2 else b"")

    fmt = chunk(b"fmt ", struct.pack("<HHIIHH", 1, 2, 48000, 192000, 4, 16))
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


def generate_aiff(path: Path, size_bytes: int, *, compressed: bool = False) -> None:
    form = b"AIFC" if compressed else b"AIFF"
    common_payload = b"\x00\x02\x00\x00\x00\x01\x00\x10@\x0e\xac\x44\x00\x00\x00\x00\x00\x00"
    if compressed:
        common_payload += b"NONE\x0enot compressed"
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
    streaminfo = b"\x00" * 34

    def writer(file: Any, target: int) -> None:
        file.write(b"fLaC")
        file.write(bytes([0x80]) + len(streaminfo).to_bytes(3, "big") + streaminfo)

    write_exact_size(path, size_bytes, writer)


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
    header = b"OggS\x00\x02" + b"\x00" * 8 + b"\x01\x00\x00\x00\x00\x00\x00\x00" + b"\x00\x00\x00\x00" + b"\x01" + b"\x1e" + b"\x01vorbis" + b"\x00" * 23

    def writer(file: Any, target: int) -> None:
        file.write(header)

    write_exact_size(path, size_bytes, writer)


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
    avih = b"avih" + struct.pack("<I", 56) + b"\x00" * 56
    hdrl = b"LIST" + struct.pack("<I", len(avih) + 4) + b"hdrl" + avih

    def writer(file: Any, target: int) -> None:
        movi_size = target - 12 - len(hdrl) - 12
        if movi_size < 0:
            raise SystemExit("target AVI size too small")
        file.write(b"RIFF")
        file.write(struct.pack("<I", min(target - 8, 0xFFFF_FFFF)))
        file.write(b"AVI ")
        file.write(hdrl)
        file.write(b"LIST")
        file.write(struct.pack("<I", min(movi_size + 4, 0xFFFF_FFFF)))
        file.write(b"movi")

    write_exact_size(path, size_bytes, writer)


def generate_webm_sparse(path: Path, size_bytes: int) -> None:
    header = bytes.fromhex("1A45DFA39F4286810142F7810142F2810442F381084282847765626D")
    segment = bytes.fromhex("18538067") + b"\x01\xff\xff\xff\xff\xff\xff\xff"
    info = bytes.fromhex("1549A966") + b"\x84" + b"\x2A\xD7\xB1\x81\x0F"
    cluster = bytes.fromhex("1F43B675") + b"\x01\xff\xff\xff\xff\xff\xff\xff"

    def writer(file: Any, target: int) -> None:
        file.write(header)
        file.write(segment)
        file.write(info)
        file.write(cluster)

    write_exact_size(path, size_bytes, writer)


EXTENSIONS = {
    "mp4_moov_front": "mp4",
    "mp4_moov_tail": "mp4",
    "mp4_snv2_tail": "mp4",
    "mov_moov_tail": "mov",
    "fragmented_mp4": "mp4",
    "webm_sparse": "webm",
    "mkv_sparse": "mkv",
    "wav_list_id3_data": "wav",
    "bwf_data": "wav",
    "rf64_ds64_data": "wav",
    "aiff_ssnd": "aiff",
    "aifc_ssnd": "aifc",
    "flac_large": "flac",
    "mp3_id3_large": "mp3",
    "ogg_large": "ogg",
    "mpeg_ts_large": "ts",
    "mpeg_ps_large": "vob",
    "avi_large": "avi",
}

GENERATORS: dict[str, Callable[[Path, int], None]] = {
    "mp4_moov_front": generate_mp4_moov_front,
    "mp4_moov_tail": generate_mp4_moov_tail,
    "mp4_snv2_tail": generate_mp4_snv2_tail,
    "mov_moov_tail": generate_mov_moov_tail,
    "fragmented_mp4": generate_fragmented_mp4,
    "webm_sparse": generate_webm_sparse,
    "mkv_sparse": generate_webm_sparse,
    "wav_list_id3_data": generate_wav_list_id3_data,
    "bwf_data": generate_bwf_data,
    "rf64_ds64_data": generate_rf64_ds64_data,
    "aiff_ssnd": generate_aiff_ssnd,
    "aifc_ssnd": generate_aifc_ssnd,
    "flac_large": generate_flac_large,
    "mp3_id3_large": generate_mp3_id3_large,
    "ogg_large": generate_ogg_large,
    "mpeg_ts_large": generate_mpeg_ts_large,
    "mpeg_ps_large": generate_mpeg_ps_large,
    "avi_large": generate_avi_large,
}


def self_test() -> int:
    with tempfile.TemporaryDirectory() as tmp:
        for kind in sorted(GENERATORS):
            case = {
                "id": f"self-test-{kind}",
                "label": f"self test {kind}",
                "synthetic": {"kind": kind, "size_bytes": 1048576},
            }
            path = generate_case_fixture(case, Path(tmp))
            assert path.exists()
            assert path.stat().st_size == 1048576
        mp4 = Path(tmp) / "self-test-mp4_snv2_tail.mp4"
        assert mp4.read_bytes()[4:8] == b"ftyp"
    print("fixture generator self-test ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
