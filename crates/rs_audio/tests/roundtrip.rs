use rs_audio::{Bnk, BnkSection, WemEntry, Wpk};
use rs_io::{Parse, Serialize};

#[test]
fn wpk_round_trips_single_wem() {
    let original = Wpk {
        version: 1,
        entries: vec![WemEntry {
            name: String::from("sound.wem"),
            data: vec![0x11, 0x22, 0x33, 0x44, 0x55],
        }],
    };

    let bytes = original.to_bytes().unwrap();
    assert_eq!(&bytes[..4], b"r3d2");

    let parsed = Wpk::from_bytes(&bytes).unwrap();
    assert_eq!(parsed, original);
    assert_eq!(parsed.to_bytes().unwrap(), bytes);
}

#[test]
fn wpk_round_trips_multiple_wems() {
    let original = Wpk {
        version: 1,
        entries: vec![
            WemEntry {
                name: String::from("a.wem"),
                data: vec![1, 2, 3],
            },
            WemEntry {
                name: String::from("longer_name.wem"),
                data: vec![9, 8, 7, 6, 5, 4],
            },
        ],
    };

    let bytes = original.to_bytes().unwrap();
    let parsed = Wpk::from_bytes(&bytes).unwrap();
    assert_eq!(parsed, original);
    assert_eq!(parsed.to_bytes().unwrap(), bytes);
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
