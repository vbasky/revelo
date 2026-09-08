use revelo::Metadata;

struct XorShift(u64);
impl XorShift {
    fn next_u32(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        (x >> 32) as u32
    }
}

fn run(bytes: &[u8]) {
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = Metadata::from_bytes(bytes);
    }));
    if r.is_err() {
        let msg = match r {
            Ok(_) => String::new(),
            Err(a) => a
                .downcast_ref::<&str>()
                .map(|s| s.to_string())
                .or_else(|| a.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "<non-str panic>".into()),
        };
        panic!("PANIC on input {:?} (len {}) :: {}", bytes, bytes.len(), msg);
    }
}

fn seeds() -> Vec<Vec<u8>> {
    let mut v: Vec<Vec<u8>> = vec![
        b"RIFF\x00\x00\x00\x00WAVE".to_vec(),
        b"RIFF\x00\x00\x00\x00AVI ".to_vec(),
        b"\x00\x00\x00\x20ftypisom".to_vec(),
        b"\x00\x00\x00\x20ftypM4A ".to_vec(),
        b"\x1a\x45\xdf\xa3".to_vec(),
        b"OggS".to_vec(),
        b"fLaC".to_vec(),
        b"ID3\x04\x00".to_vec(),
        b"FLV\x01\x05\x00\x00\x00\x09".to_vec(),
        b"\x47\x40\x00\x10".to_vec(),
        b"\x00\x00\x01\xba".to_vec(),
        b"\x89PNG\r\n\x1a\n".to_vec(),
        b"\xff\xd8\xff\xe0".to_vec(),
        b"GIF89a".to_vec(),
        b"BM".to_vec(),
        b"\x1f\x8b".to_vec(),
        b"PK\x03\x04".to_vec(),
        b"II*\x00".to_vec(),
        b"MM\x00*".to_vec(),
        b"JP\x20\x20".to_vec(),
        b"\x00\x00\x01\x0f".to_vec(),
        b"\x00\x00\x00\x14ftypavif".to_vec(),
        b"\x0a\x0a\x0a\x0a\x0a\x0a\x0a\x0a\x0a\x0a".to_vec(),
        b"FORM\x00\x00\x00\x00AIFF".to_vec(),
        b"FORM\x00\x00\x00\x00AIFC".to_vec(),
        b"RIFX\x00\x00\x00\x00WAVE".to_vec(),
        b"RF64\x00\x00\x00\x00WAVE".to_vec(),
        b"fwav".to_vec(),
        b"\x43\x4f\x4e\x54".to_vec(),
        b"OggS\x00\x02".to_vec(),
        b"ID3\x03\x00\x00".to_vec(),
        b"\xff\xfb\x90\x00".to_vec(),
        b"\x00\x00\x00\x18ftypmp42".to_vec(),
        b"\x00\x00\x00\x24ftypqt  ".to_vec(),
        b"mocha".to_vec(),
        b"jP  ".to_vec(),
        b"\x00\x00\x00\x0cjP  \r\n\x87\n".to_vec(),
        b"\xff\xd8\xff\xdb".to_vec(),
        b"\xff\xd8\xff\xc0".to_vec(),
        b"GIF87a".to_vec(),
        b"RIFF".to_vec(),
        b"\x00\x00\x01\xb3".to_vec(),
        b"\x00\x00\x01\xb5".to_vec(),
        b"\x00\x00\x01\x65".to_vec(),
        b"\x00\x00\x00\x01\x67".to_vec(),
        b"\x00\x00\x00\x01\x68".to_vec(),
        b"\x00\x00\x00\x01\x09".to_vec(),
        b"\x00\x00\x00\x01\x06".to_vec(),
        b"\x00\x00\x00\x01\x65\x88".to_vec(),
        b"\x4f\x70\x75\x73\x48\x65\x61\x64".to_vec(),
        b"FORM\x00\x00\x00\x00CDXA".to_vec(),
        b"AMV\x00".to_vec(),
        b"SKM\x00\x00\x00\x00\x00\x00\x00\x00\x00".to_vec(),
        b"DPG".to_vec(),
        b"\x46\x57\x45\x53".to_vec(),
        b"DCPP".to_vec(),
        b"\x06\x0e\x2b\x34".to_vec(),
        b"\x00\x00\x00\x00\x00\x00\x00\x00".to_vec(),
        b"\x00\x00\x00\x00\x00\x00\x00\x18".to_vec(),
        b"\x52\x49\x46\x46".to_vec(),
        b"\x30\x26\xb2\x75\x8e\x66\xcf\x11".to_vec(),
        b"\x30\x26\xb2\x75\x8e\x66\xcf\x11\xa6\xd9\x00\xaa\x00\x62\xce\x6c".to_vec(),
        b"\x75\x73\x74\x61\x72".to_vec(),
        b"\x1f\x8b\x08".to_vec(),
        b"\x42\x5a\x68".to_vec(),
        b"\xfd\x37\x7a\x58\x5a\x00".to_vec(),
        b"\x52\x61\x72\x21\x1a\x07\x01\x00".to_vec(),
        b"\x7fELF".to_vec(),
        b"\xca\xfe\xba\xbe".to_vec(),
        b"\x25\x50\x44\x46".to_vec(),
        b"\x7b\x0a".to_vec(),
        b"\xef\xbb\xbf".to_vec(),
        b"\x49\x49".to_vec(),
        b"\x4d\x4d".to_vec(),
        b"\x00\x00\x00\x00\x6d\x6f\x6f\x76".to_vec(),
        b"\x00\x00\x00\x00\x6d\x64\x61\x74".to_vec(),
        b"\x00\x00\x00\x00\x74\x72\x61\x6b".to_vec(),
        b"\x00\x00\x00\x00\x6d\x6f\x6f\x66".to_vec(),
        b"\x00\x00\x00\x00\x74\x72\x75\x6e".to_vec(),
        b"\x00\x00\x00\x00\x74\x66\x68\x64".to_vec(),
        b"\x00\x00\x00\x00\x61\x76\x63\x43".to_vec(),
        b"\x00\x00\x00\x00\x68\x76\x63\x43".to_vec(),
        b"\x00\x00\x00\x00\x76\x70\x63\x43".to_vec(),
        b"\x00\x00\x00\x00\x65\x73\x64\x73".to_vec(),
        b"\x00\x00\x00\x00\x68\x64\x6c\x72".to_vec(),
        b"\x00\x00\x00\x00\x73\x74\x62\x6c".to_vec(),
        b"\x00\x00\x00\x00\x73\x74\x63\x6f".to_vec(),
        b"\x00\x00\x00\x00\x73\x74\x74\x73".to_vec(),
        b"\x00\x00\x00\x00\x73\x74\x73\x7a".to_vec(),
        b"\x00\x00\x00\x00\x73\x74\x73\x63".to_vec(),
        b"\x00\x00\x00\x00\x73\x64\x74\x70".to_vec(),
        b"\x00\x00\x00\x00\x73\x74\x70\x64".to_vec(),
        b"\x00\x00\x00\x00\x6d\x76\x65\x78".to_vec(),
        b"\x00\x00\x00\x00\x74\x72\x65\x78".to_vec(),
        b"\x00\x00\x00\x00\x74\x6b\x68\x64".to_vec(),
        b"\x00\x00\x00\x00\x6d\x64\x68\x64".to_vec(),
        b"\x00\x00\x00\x00\x6d\x76\x68\x64".to_vec(),
        b"\x00\x00\x00\x01\x42".to_vec(),
        b"\x00\x00\x00\x01\x44".to_vec(),
        b"\x00\x00\x00\x01\x26".to_vec(),
        b"\x00\x00\x00\x01\x40".to_vec(),
        b"\x00\x00\x00\x01\x4a".to_vec(),
        b"\x00\x00\x00\x01\x0c".to_vec(),
        b"\x00\x00\x00\x01\x0e".to_vec(),
        b"\x00\x00\x00\x01\x61".to_vec(),
        b"\x00\x00\x01\x61".to_vec(),
        b"\x00\x00\x01\x42\x01\x01\x60\x00\x00\x03\x00\x90\x00\x00\x03\x00\x00\x03\x00\x78".to_vec(),
        b"\x00\x00\x01\x42\x01\x01\x01\x60\x00\x00\x03\x00\x90\x00\x00\x03\x00\x00\x03\x00\x78\xa0".to_vec(),
        b"\x0b\x77\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00".to_vec(),
        b"\x51\x2c\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00".to_vec(),
        b"\x7c\xa5\x8c\x07".to_vec(),
        b"\x8a\x31\x2f\x11".to_vec(),
        b"\xd0\x06\x2f\x00".to_vec(),
        b"\x43\x53\x53\x44\x00\x00\x00\x00".to_vec(),
        b"\x53\x45\x53\x53\x00\x00\x00\x00".to_vec(),
        b"\x2e\x52\x4d\x46".to_vec(),
        b"\x46\x4f\x52\x4d\x00\x00\x00\x00\x41\x49\x43\x4c".to_vec(),
        b"\x46\x4f\x52\x4d\x00\x00\x00\x00\x4d\x4f\x56\x49".to_vec(),
        b"\x46\x4f\x52\x4d\x00\x00\x00\x00\x41\x49\x46\x46".to_vec(),
        b"\x2e\x73\x6e\x64".to_vec(),
        b"\x64\x6e\x73\x2e".to_vec(),
        b"\x4d\x54\x68\x64".to_vec(),
        b"\xff\xf1".to_vec(),
        b"\xff\xf9".to_vec(),
        b"\x0b\x77\x00".to_vec(),
        b"\x0b\x77\x02".to_vec(),
        b"\x49\x44\x33\x02\x00\x00\x00\x00\x00\x00".to_vec(),
        b"\x24\x00\x00\x00".to_vec(),
        b"\x24\x00\x00\x00\x00\x00\x00\x00".to_vec(),
        b"\x1a\x45\xdf\xa3\x93\x42\x82\x88".to_vec(),
        b"\x1a\x45\xdf\xa3\x93\x42\x82\x88\x6d\x61\x74\x72\x6f\x73\x6b\x61".to_vec(),
        b"\x1a\x45\xdf\xa3\x01\x00\x00\x00\x00\x00\x00\x00\x15\x80\x00\x00\x00\x00\x00\x00\x42\x82\x88".to_vec(),
    ];
    let mut out = Vec::new();
    for (i, s) in v.drain(..).enumerate() {
        out.push(s.clone());
        let mut ext = s.clone();
        ext.extend_from_slice(&(i as u32).to_be_bytes());
        ext.extend_from_slice(&[0u8; 64]);
        out.push(ext);
    }
    out
}

#[test]
fn fuzz_metadata_never_panics_truncated_magic() {
    for s in seeds() {
        for len in 0..=s.len() {
            run(&s[..len]);
        }
    }
}

#[test]
fn fuzz_metadata_never_panics_flipped_and_random() {
    let all_seeds = seeds();
    let pass_seeds: [u64; 12] = [
        0xDEAD_BEEF_CAFE_F00D,
        0x1234_5678_9ABC_DEF0,
        0xFEDC_BA98_7654_3210,
        0x0BAD_0F00_D00D_0001,
        0xCAFE_CAFE_CAFE_CAFE,
        0x6666_6666_6666_6666,
        0x8888_8888_8888_8888,
        0x1111_1111_1111_1111,
        0xABCD_ABCD_ABCD_ABCD,
        0x4242_4242_4242_4242,
        0x5555_5555_5555_5555,
        0x7777_7777_7777_7777,
    ];
    for ps in pass_seeds {
        let mut rng = XorShift(ps);
        for _ in 0..12000 {
            let seed = &all_seeds[(rng.next_u32() as usize) % all_seeds.len()];
            let mut buf = seed.clone();
            // byte flips / truncation / length extension
            let nflips = 1 + (rng.next_u32() % 4) as usize;
            for _ in 0..nflips {
                if buf.is_empty() {
                    buf.push(rng.next_u32() as u8);
                } else {
                    let idx = (rng.next_u32() as usize) % buf.len();
                    buf[idx] ^= (1u8) << (rng.next_u32() % 8);
                }
            }
            if rng.next_u32().is_multiple_of(3) {
                let keep = (rng.next_u32() as usize) % buf.len().saturating_add(1);
                buf.truncate(keep);
            }
            if rng.next_u32().is_multiple_of(4) {
                let extra = (rng.next_u32() % 300) as usize;
                let mut tail = Vec::with_capacity(extra);
                for _ in 0..extra {
                    tail.push(rng.next_u32() as u8);
                }
                buf.extend_from_slice(&tail);
            }
            run(&buf);
        }
    }
}

#[test]
fn fuzz_metadata_never_panics_degenerate() {
    for len in [0usize, 1, 2, 3, 4, 5, 7, 8, 12, 15, 16, 31, 63, 64, 127, 255, 256, 511, 1023, 2048]
    {
        run(&vec![0u8; len]);
        run(&vec![0xFFu8; len]);
        run(&vec![0x20u8; len]);
        run(&vec![0x49u8; len]);
        run(&vec![0x4du8; len]);
    }
    for s in seeds() {
        let mut b = s.clone();
        b.resize(2048, 0);
        run(&b);
    }
}

#[test]
fn fuzz_metadata_never_panics_huge_length_fields() {
    // Replace every 4-byte-aligned length slot with a huge value: container
    // parsers trust declared sizes and often subtract them from cursors.
    let bigs: [u8; 4] = [0xFF, 0xFF, 0xFF, 0xFF];
    let mut rng = XorShift(0xC0FFEE_DEAD_0001);
    let all_seeds = seeds();
    for s in &all_seeds {
        let mut base = s.clone();
        // Length-bomb: set a prefix of the seed's 4-byte words to huge values.
        for word in 0..4 {
            if base.len() >= word * 4 + 4 {
                let off = word * 4;
                base[off..off + 4].copy_from_slice(&bigs);
            }
        }
        for len in 0..=base.len() {
            run(&base[..len]);
        }
    }
    // Randomly inject huge 4-byte length words at arbitrary offsets.
    for _ in 0..8000 {
        let seed = &all_seeds[(rng.next_u32() as usize) % all_seeds.len()];
        let mut buf = seed.clone();
        for _ in 0..3 {
            if buf.len() >= 4 {
                let off = (rng.next_u32() as usize) % (buf.len() - 3);
                let val = match rng.next_u32() % 4 {
                    0 => [0xFF, 0xFF, 0xFF, 0xFF],
                    1 => [0x7F, 0xFF, 0xFF, 0xFF],
                    2 => [0x80, 0x00, 0x00, 0x00],
                    _ => [0xFF, 0xFF, 0xFF, 0xFE],
                };
                buf[off..off + 4].copy_from_slice(&val);
            }
        }
        let keep = (rng.next_u32() as usize) % buf.len().saturating_add(1);
        buf.truncate(keep);
        run(&buf);
    }
}
