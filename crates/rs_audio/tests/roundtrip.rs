use rs_audio::{Bnk, BnkSection, WemEntry, Wpk};
use rs_io::{Parse, Serialize};

#[test]
fn wpk_round_trips_single_wem() {
    let original = Wpk {
        version: 1,
        entries: vec![WemEntry::new("123.wem", vec![0x11, 0x22, 0x33, 0x44, 0x55])],
        dead_slots: Vec::new(),
    };

    let bytes = original.to_bytes().unwrap();
    assert_eq!(&bytes[..4], b"r3d2");

    let parsed = Wpk::from_bytes(&bytes).unwrap();
    assert_eq!(parsed, original);
    assert_eq!(parsed.to_bytes().unwrap(), bytes);

    let wems = parsed.wems();
    assert_eq!(wems.len(), 1);
    assert_eq!(wems[0].0, Some(123));
    assert_eq!(wems[0].1, "123.wem");
}

#[test]
fn wpk_round_trips_multiple_wems() {
    let original = Wpk {
        version: 1,
        entries: vec![
            WemEntry::new("a.wem", vec![1, 2, 3]),
            WemEntry::new("longer_name.wem", vec![9, 8, 7, 6, 5, 4]),
        ],
        dead_slots: Vec::new(),
    };

    let bytes = original.to_bytes().unwrap();
    let parsed = Wpk::from_bytes(&bytes).unwrap();
    assert_eq!(parsed, original);
    assert_eq!(parsed.to_bytes().unwrap(), bytes);
}

/** Exercises the awkward real-file layout the canonical writer would not produce on its own:
dead (zero) offset-table slots interleaved with live ones, and per-blob alignment padding. We
build the raw bytes by hand, parse them, assert the model captured the quirks, and assert the
re-serialized bytes are identical to the hand-built input. */
#[test]
fn wpk_round_trips_dead_slots_and_alignment() {
    fn u32b(v: u32) -> [u8; 4] {
        v.to_le_bytes()
    }
    fn name16(s: &str) -> Vec<u8> {
        s.encode_utf16().flat_map(|u| u.to_le_bytes()).collect()
    }

    let names = ["1.wem", "22.wem"];
    let datas: [&[u8]; 2] = [&[0xAA, 0xBB], &[0xCC, 0xDD, 0xEE]];
    let aligns = [4u32, 8u32];

    let total_slots = 4u32; // 2 live + 2 dead
    let header_len = 12 + total_slots as usize * 4;

    let entry_sizes: Vec<usize> = names.iter().map(|n| 12 + n.encode_utf16().count() * 2).collect();
    let mut entry_offsets = Vec::new();
    let mut cursor = header_len;
    for s in &entry_sizes {
        entry_offsets.push(cursor as u32);
        cursor += s;
    }
    let mut data_offsets = Vec::new();
    for (i, d) in datas.iter().enumerate() {
        cursor += aligns[i] as usize;
        data_offsets.push(cursor as u32);
        cursor += d.len();
    }

    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"r3d2");
    bytes.extend_from_slice(&u32b(1));
    bytes.extend_from_slice(&u32b(total_slots));
    // table: live, dead, live, dead
    bytes.extend_from_slice(&u32b(entry_offsets[0]));
    bytes.extend_from_slice(&u32b(0));
    bytes.extend_from_slice(&u32b(entry_offsets[1]));
    bytes.extend_from_slice(&u32b(0));
    for (i, n) in names.iter().enumerate() {
        bytes.extend_from_slice(&u32b(data_offsets[i]));
        bytes.extend_from_slice(&u32b(datas[i].len() as u32));
        bytes.extend_from_slice(&u32b(n.encode_utf16().count() as u32));
        bytes.extend_from_slice(&name16(n));
    }
    for (i, d) in datas.iter().enumerate() {
        bytes.extend(std::iter::repeat_n(0u8, aligns[i] as usize));
        bytes.extend_from_slice(d);
    }

    let parsed = Wpk::from_bytes(&bytes).unwrap();
    assert_eq!(parsed.entries.len(), 2);
    assert_eq!(parsed.dead_slots, vec![1, 3]);
    assert_eq!(parsed.entries[0].align, 4);
    assert_eq!(parsed.entries[1].align, 8);
    assert_eq!(parsed.entries[0].name, "1.wem");

    let written = parsed.to_bytes().unwrap();
    assert_eq!(written, bytes, "WPK with dead slots + alignment must round-trip byte-exact");
}

#[test]
fn wpk_rejects_bad_magic() {
    let bytes = [0u8; 16];
    assert!(Wpk::from_bytes(&bytes).is_err());
}

fn build_bnk_bytes() -> Vec<u8> {
    let mut out = Vec::new();

    let bkhd_body: [u8; 8] = [0x88, 0, 0, 0, 0x01, 0, 0, 0];
    out.extend_from_slice(b"BKHD");
    out.extend_from_slice(&(bkhd_body.len() as u32).to_le_bytes());
    out.extend_from_slice(&bkhd_body);

    let wem_a: &[u8] = &[0xAA, 0xBB, 0xCC];
    let wem_b: &[u8] = &[0xDD, 0xEE];

    let mut didx_body = Vec::new();
    didx_body.extend_from_slice(&1u32.to_le_bytes());
    didx_body.extend_from_slice(&0u32.to_le_bytes());
    didx_body.extend_from_slice(&(wem_a.len() as u32).to_le_bytes());
    didx_body.extend_from_slice(&2u32.to_le_bytes());
    didx_body.extend_from_slice(&(wem_a.len() as u32).to_le_bytes());
    didx_body.extend_from_slice(&(wem_b.len() as u32).to_le_bytes());
    out.extend_from_slice(b"DIDX");
    out.extend_from_slice(&(didx_body.len() as u32).to_le_bytes());
    out.extend_from_slice(&didx_body);

    let mut data_body = Vec::new();
    data_body.extend_from_slice(wem_a);
    data_body.extend_from_slice(wem_b);
    out.extend_from_slice(b"DATA");
    out.extend_from_slice(&(data_body.len() as u32).to_le_bytes());
    out.extend_from_slice(&data_body);

    out
}

#[test]
fn bnk_round_trips_byte_exact() {
    let bytes = build_bnk_bytes();
    let parsed = Bnk::from_bytes(&bytes).unwrap();

    let tags: Vec<[u8; 4]> = parsed.sections.iter().map(|s| s.tag).collect();
    assert_eq!(tags, vec![*b"BKHD", *b"DIDX", *b"DATA"]);

    assert_eq!(parsed.to_bytes().unwrap(), bytes);
}

#[test]
fn bnk_extracts_embedded_wems() {
    let bytes = build_bnk_bytes();
    let parsed = Bnk::from_bytes(&bytes).unwrap();

    let wems = parsed.wems();
    assert_eq!(wems.len(), 2);
    assert_eq!(wems[0], (1u32, &[0xAA, 0xBB, 0xCC][..]));
    assert_eq!(wems[1], (2u32, &[0xDD, 0xEE][..]));
}

#[test]
fn bnk_preserves_unknown_sections() {
    let original = Bnk {
        sections: vec![
            BnkSection {
                tag: *b"BKHD",
                data: vec![1, 2, 3, 4],
            },
            BnkSection {
                tag: *b"XXXX",
                data: vec![9, 9, 9],
            },
        ],
    };

    let bytes = original.to_bytes().unwrap();
    let parsed = Bnk::from_bytes(&bytes).unwrap();
    assert_eq!(parsed, original);
    assert!(parsed.wems().is_empty());
}

// ---- robustness / fuzz-style: malformed input must Err cleanly, never panic ----

#[test]
fn bnk_truncated_section_size_errs() {
    // Section claims a 0x10000-byte body but only a few bytes follow.
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"BKHD");
    bytes.extend_from_slice(&0x0001_0000u32.to_le_bytes());
    bytes.extend_from_slice(&[1, 2, 3]);
    assert!(Bnk::from_bytes(&bytes).is_err());
}

#[test]
fn bnk_giant_section_size_does_not_oom_panic() {
    // Near-u32::MAX size: must Err on the bound check, not attempt a 4 GiB allocation.
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"DATA");
    bytes.extend_from_slice(&0xFFFF_FFF0u32.to_le_bytes());
    bytes.extend_from_slice(&[0; 8]);
    assert!(Bnk::from_bytes(&bytes).is_err());
}

#[test]
fn bnk_partial_header_errs() {
    // Only two bytes — not even a full tag.
    assert!(Bnk::from_bytes(b"BK").is_err());
}

#[test]
fn bnk_empty_input_is_empty_bank() {
    let parsed = Bnk::from_bytes(&[]).unwrap();
    assert!(parsed.sections.is_empty());
    assert!(parsed.wems().is_empty());
    assert!(parsed.to_bytes().unwrap().is_empty());
}

#[test]
fn bnk_didx_not_multiple_of_12_skips_remainder() {
    // DIDX body of 13 bytes: one valid 12-byte triple + 1 stray byte; wems() must not panic.
    let wem: &[u8] = &[7, 7, 7, 7];
    let mut didx = Vec::new();
    didx.extend_from_slice(&5u32.to_le_bytes()); // id
    didx.extend_from_slice(&0u32.to_le_bytes()); // offset
    didx.extend_from_slice(&(wem.len() as u32).to_le_bytes());
    didx.push(0xAB); // stray 13th byte

    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"DIDX");
    bytes.extend_from_slice(&(didx.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&didx);
    bytes.extend_from_slice(b"DATA");
    bytes.extend_from_slice(&(wem.len() as u32).to_le_bytes());
    bytes.extend_from_slice(wem);

    let parsed = Bnk::from_bytes(&bytes).unwrap();
    let wems = parsed.wems();
    assert_eq!(wems, vec![(5u32, wem)]);
    assert_eq!(parsed.to_bytes().unwrap(), bytes);
}

#[test]
fn bnk_didx_offset_past_data_is_skipped() {
    // DIDX entry whose offset+size exceed the DATA body: skipped, not panicked.
    let mut didx = Vec::new();
    didx.extend_from_slice(&1u32.to_le_bytes());
    didx.extend_from_slice(&100u32.to_le_bytes()); // offset well past DATA
    didx.extend_from_slice(&50u32.to_le_bytes());

    let data: &[u8] = &[1, 2, 3, 4];
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"DIDX");
    bytes.extend_from_slice(&(didx.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&didx);
    bytes.extend_from_slice(b"DATA");
    bytes.extend_from_slice(&(data.len() as u32).to_le_bytes());
    bytes.extend_from_slice(data);

    let parsed = Bnk::from_bytes(&bytes).unwrap();
    assert!(parsed.wems().is_empty());
}

#[test]
fn bnk_zero_length_data_yields_no_wems() {
    let mut didx = Vec::new();
    didx.extend_from_slice(&1u32.to_le_bytes());
    didx.extend_from_slice(&0u32.to_le_bytes());
    didx.extend_from_slice(&4u32.to_le_bytes());

    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"DIDX");
    bytes.extend_from_slice(&(didx.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&didx);
    bytes.extend_from_slice(b"DATA");
    bytes.extend_from_slice(&0u32.to_le_bytes()); // empty DATA

    let parsed = Bnk::from_bytes(&bytes).unwrap();
    assert!(parsed.wems().is_empty());
    assert_eq!(parsed.to_bytes().unwrap(), bytes);
}

#[test]
fn wpk_truncated_header_errs() {
    assert!(Wpk::from_bytes(b"r3d2").is_err());
    assert!(Wpk::from_bytes(&[]).is_err());
}

#[test]
fn wpk_bad_version_errs() {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"r3d2");
    bytes.extend_from_slice(&2u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    assert!(Wpk::from_bytes(&bytes).is_err());
}

#[test]
fn wpk_table_offset_past_eof_errs() {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"r3d2");
    bytes.extend_from_slice(&1u32.to_le_bytes());
    bytes.extend_from_slice(&1u32.to_le_bytes()); // 1 slot
    bytes.extend_from_slice(&0x00FF_FFFFu32.to_le_bytes()); // offset past EOF
    assert!(Wpk::from_bytes(&bytes).is_err());
}

#[test]
fn wpk_data_offset_size_past_eof_errs() {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"r3d2");
    bytes.extend_from_slice(&1u32.to_le_bytes());
    bytes.extend_from_slice(&1u32.to_le_bytes());
    bytes.extend_from_slice(&16u32.to_le_bytes()); // entry record at offset 16
    // entry record: data_offset=16, huge size, name_len=0
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    assert!(Wpk::from_bytes(&bytes).is_err());
}

#[test]
fn wpk_huge_slot_count_errs_not_panics() {
    // Claims a vast slot count but provides no offsets: must Err while reading the table.
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"r3d2");
    bytes.extend_from_slice(&1u32.to_le_bytes());
    bytes.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
    assert!(Wpk::from_bytes(&bytes).is_err());
}

#[test]
fn wpk_all_dead_slots_round_trips_empty() {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"r3d2");
    bytes.extend_from_slice(&1u32.to_le_bytes());
    bytes.extend_from_slice(&2u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());

    let parsed = Wpk::from_bytes(&bytes).unwrap();
    assert!(parsed.entries.is_empty());
    assert_eq!(parsed.dead_slots, vec![0, 1]);
    assert_eq!(parsed.to_bytes().unwrap(), bytes);
}
