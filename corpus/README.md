# Local sample corpus (not committed)

Real camera samples used to quantify EXIF / maker-note parity against the
`exiftool` oracle. **This directory is gitignored** — the files carry
private, non-transferable licenses ("NOT publication") and must never be
committed or redistributed.

Measure parity with:

```bash
cargo run -p revelo-exif-diff -- --verbose corpus/juleskers/<device>/*
```

## juleskers/ — contributed via issue #2

Donated by **Jules Kerssemakers** (@juleskers) for private in-project test use
only. See `juleskers/LICENSE.txt` for the exact terms. People-free, GPS-scrubbed
scenes across a deliberate breadth of devices.

| Device | Files | exiftool tags | revelo parity | Notes |
| --- | --- | --- | --- | --- |
| Fairphone 2 (stock cam) | 1 | 34 | 100% | plain EXIF, no maker note |
| Fairphone 3 (e/OS OpenCamera) | 1 | 37 | 100% | plain EXIF, no maker note |
| Fairphone 5 (e/OS OpenCamera) | 2 | 45 ea. | 100% | plain EXIF, no maker note |
| Fujifilm FinePix S2000HD | 2 | 73–75 | 98% (145/148) | Fujifilm maker note |
| Panasonic Lumix DMC-TZ61 | 4 | 149 ea. | 92% (137/149) | remaining gap is MPF (APP2) + 3 face-detect sub-IFD tags |
| Sony DSC-S750 | 2 | 53–54 | 79–80% | budget Sony writes **Olympus-format** maker notes |
| dino toy camera | 3 | 0 | n/a | untagged JPEG, no EXIF at all |

Measured with exiftool 13.55; `revelo-exif-diff` built with the
`exiftool-tables` feature.
