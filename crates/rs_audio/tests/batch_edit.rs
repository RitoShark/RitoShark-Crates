use rs_audio::{Bnk, BnkSection, Error, WemEntry, Wpk};
use rs_io::{Parse, Serialize};

fn bank() -> Bnk {
    let mut index = Vec::new();
    let mut data = vec![0xa5; 36];
    for (id, offset, payload) in [
        (1u32, 0u32, b"one".as_slice()),
        (2, 16, b"two"),
        (3, 32, b"last"),
    ] {
        index.extend_from_slice(&id.to_le_bytes());
        index.extend_from_slice(&offset.to_le_bytes());
        index.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        data[offset as usize..offset as usize + payload.len()].copy_from_slice(payload);
    }
    Bnk {
        sections: [
            (*b"BKHD", vec![145, 0, 0, 0, 3, 2, 1, 0]),
            (*b"DATA", data),
            (*b"HIRC", vec![0, 0, 0, 0]),
            (*b"DIDX", index),
            (*b"ZZZZ", vec![5, 6, 7]),
        ]
        .into_iter()
        .map(|(tag, data)| BnkSection { tag, data })
        .collect(),
    }
}

fn package() -> Wpk {
    Wpk {
        version: 1,
        dead_slots: vec![1, 4],
        entries: vec![
            WemEntry::new("0001.wem", b"one".to_vec()),
            WemEntry::new("2.wem", b"two".to_vec()),
            WemEntry::new("notes-音", b"opaque".to_vec()),
            WemEntry::new("3.wem", b"last".to_vec()),
        ],
    }
}

#[test]
fn batch_matches_sequential_replacement_and_preserves_unrelated_bank_data() {
    let original = bank();
    let updates = vec![(2, vec![8; 19]), (1, vec![]), (2, vec![9; 25])];
    let mut sequential = original.clone();
    for (id, data) in updates.clone() {
        sequential.replace_wem(id, data).unwrap();
    }
    let mut batch = original.clone();
    batch.replace_wems(updates).unwrap();
    assert_eq!(batch.to_bytes().unwrap(), sequential.to_bytes().unwrap());
    assert_eq!(batch.wem_ids(), vec![1, 2, 3]);
    assert_eq!(batch.wem(1), Some([].as_slice()));
    assert_eq!(batch.wem(2), Some([9; 25].as_slice()));
    assert_eq!(batch.wem(3), original.wem(3));
    for (before, after) in original.sections.iter().zip(&batch.sections) {
        assert_eq!(before.tag, after.tag);
        if !matches!(&before.tag, b"DIDX" | b"DATA") {
            assert_eq!(before.data, after.data);
        }
    }
    let parsed = Bnk::from_bytes(&batch.to_bytes().unwrap()).unwrap();
    assert_eq!(parsed.wems(), batch.wems());
    let index = &batch
        .sections
        .iter()
        .find(|s| s.tag == *b"DIDX")
        .unwrap()
        .data;
    assert!(
        index
            .chunks_exact(12)
            .all(|record| u32::from_le_bytes(record[4..8].try_into().unwrap()) % 16 == 0)
    );
}

#[test]
fn missing_id_after_valid_updates_rolls_back_both_containers() {
    let mut bank = bank();
    let before = bank.to_bytes().unwrap();
    assert!(matches!(
        bank.replace_wems([(1, vec![5]), (99, vec![6])]),
        Err(Error::NoSuchWem(99))
    ));
    assert_eq!(bank.to_bytes().unwrap(), before);
    let mut package = package();
    let before = package.clone();
    assert!(matches!(
        package.replace_wems([(1, vec![5]), (99, vec![6])]),
        Err(Error::NoSuchWem(99))
    ));
    assert_eq!(package, before);
}

#[test]
fn empty_and_unchanged_batches_preserve_noncanonical_padding() {
    let mut bank = bank();
    let before = bank.to_bytes().unwrap();
    bank.replace_wems([]).unwrap();
    bank.replace_wems([
        (1, b"discarded".to_vec()),
        (1, b"one".to_vec()),
        (3, b"last".to_vec()),
    ])
    .unwrap();
    assert_eq!(bank.to_bytes().unwrap(), before);
    bank.replace_wem(1, b"one".to_vec()).unwrap();
    assert_eq!(bank.to_bytes().unwrap(), before);
    let mut package = package();
    let before = package.to_bytes().unwrap();
    package.replace_wems([]).unwrap();
    package
        .replace_wems([(1, b"one".to_vec()), (2, b"two".to_vec())])
        .unwrap();
    assert_eq!(package.to_bytes().unwrap(), before);
}

#[test]
fn package_batch_keeps_names_slots_and_opaque_entries() {
    let mut package = package();
    let original = package.clone();
    package
        .replace_wems([(3, vec![7; 9]), (1, vec![8]), (1, vec![6; 3])])
        .unwrap();
    assert_eq!(package.version, original.version);
    assert_eq!(package.dead_slots, original.dead_slots);
    for (before, after) in original.entries.iter().zip(&package.entries) {
        assert_eq!(before.name, after.name);
    }
    assert_eq!(package.entries[1], original.entries[1]);
    assert_eq!(package.entries[2], original.entries[2]);
    assert_eq!(package.wem(1), Some([6; 3].as_slice()));
    assert_eq!(package.wem(3), Some([7; 9].as_slice()));
    assert_eq!(
        Wpk::from_bytes(&package.to_bytes().unwrap()).unwrap(),
        package
    );
}

#[test]
fn duplicate_container_ids_target_only_the_first_entry() {
    let mut bank = bank();
    let index = &mut bank
        .sections
        .iter_mut()
        .find(|s| s.tag == *b"DIDX")
        .unwrap()
        .data;
    index[24..28].copy_from_slice(&1u32.to_le_bytes());
    bank.replace_wems([(1, b"changed".to_vec())]).unwrap();
    assert_eq!(bank.wems()[0], (1, b"changed".as_slice()));
    assert_eq!(bank.wems()[2], (1, b"last".as_slice()));
    let mut package = package();
    package.entries[3].name = "1.wem".into();
    package.replace_wems([(1, b"changed".to_vec())]).unwrap();
    assert_eq!(package.entries[0].data, b"changed");
    assert_eq!(package.entries[3].data, b"last");
}

#[test]
fn malformed_bank_media_is_rejected_without_discarding_entries() {
    for case in 0..5 {
        let mut bank = bank();
        match case {
            0 => bank.sections[3].data.push(0),
            1 => bank.sections[3].data[4..8].copy_from_slice(&u32::MAX.to_le_bytes()),
            2 => {
                bank.sections.remove(1);
            }
            3 => bank.sections.push(bank.sections[3].clone()),
            4 => bank.sections.push(bank.sections[1].clone()),
            _ => unreachable!(),
        }
        let before = bank.to_bytes().unwrap();
        bank.replace_wems([]).unwrap();
        assert!(bank.replace_wems([(1, vec![4])]).is_err(), "case {case}");
        assert_eq!(bank.to_bytes().unwrap(), before, "case {case}");
    }
}
