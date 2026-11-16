use rs_audio::{Bnk, BnkSection, PcmAudio, Wem, encode_pcm};

fn append_chunk(bytes: &mut Vec<u8>, tag: &[u8; 4], data: &[u8]) {
    bytes.extend_from_slice(tag);
    bytes.extend_from_slice(&(data.len() as u32).to_le_bytes());
    bytes.extend_from_slice(data);
    let length = (bytes.len() - 8) as u32;
    bytes[4..8].copy_from_slice(&length.to_le_bytes());
}

#[test]
fn identifies_metadata_and_partial_containers_without_changing_decode() {
    let plain = encode_pcm(&PcmAudio::silence(44100, 1, 128)).unwrap();
    assert!(!Wem::new(&plain).unwrap().has_extra_chunks());
    for tag in [b"smpl", b"cue ", b"JUNK"] {
        let mut bytes = plain.clone();
        append_chunk(&mut bytes, tag, &[0; 36]);
        let wem = Wem::new(&bytes).unwrap();
        assert!(wem.has_extra_chunks());
        assert_eq!(wem.to_pcm().unwrap().frames(), 128);
    }
    let mut trailing = plain.clone();
    trailing.push(1);
    assert!(Wem::new(&trailing).unwrap().has_extra_chunks());
    let mut truncated = plain.clone();
    append_chunk(&mut truncated, b"JUNK", &[0; 10]);
    truncated.pop();
    assert!(Wem::new(&truncated).unwrap().has_extra_chunks());
    let mut duplicate = plain;
    append_chunk(&mut duplicate, b"data", &[0; 4]);
    assert!(Wem::new(&duplicate).unwrap().has_extra_chunks());
}

#[test]
fn validates_media_bounds_duplicates_and_sections() {
    let mut bank = Bnk::new();
    bank.insert_wem(7, vec![1, 2, 3]).unwrap();
    bank.validate_media().unwrap();
    let mut out_of_bounds = bank.clone();
    let index = out_of_bounds
        .sections
        .iter_mut()
        .find(|s| s.tag == *b"DIDX")
        .unwrap();
    index.data[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(out_of_bounds.validate_media().is_err());
    let mut duplicate = bank.clone();
    let index = duplicate
        .sections
        .iter_mut()
        .find(|s| s.tag == *b"DIDX")
        .unwrap();
    index.data.extend_from_within(..);
    assert!(duplicate.validate_media().is_err());
    let mut malformed = bank.clone();
    malformed
        .sections
        .iter_mut()
        .find(|s| s.tag == *b"DIDX")
        .unwrap()
        .data
        .push(0);
    assert!(malformed.validate_media().is_err());
    bank.sections.push(BnkSection {
        tag: *b"DATA",
        data: vec![],
    });
    assert!(bank.validate_media().is_err());
    Bnk::new().validate_media().unwrap();
}
