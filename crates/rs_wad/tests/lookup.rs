use std::io::Cursor;

use rs_io::Parse;
use rs_wad::{Wad, WadBuilder};

fn archive() -> Wad {
    let bytes = WadBuilder::new()
        .with_chunk("a")
        .with_chunk("b")
        .with_chunk("c")
        .build_to_bytes(|hash, out| {
            out.write_all(&hash.to_le_bytes())
                .map_err(rs_io::Error::from)?;
            Ok(())
        })
        .unwrap();
    let mut wad = Wad::from_bytes(&bytes).unwrap();
    wad.chunks.reverse();
    wad
}

#[test]
fn index_and_bulk_extraction_handle_unsorted_tables() {
    let wad = archive();
    let index = wad.chunk_index();
    let all = wad.extract_all().unwrap();
    assert_eq!(index.len(), 3);
    assert_eq!(all.len(), 3);
    for chunk in &wad.chunks {
        assert!(std::ptr::eq(index[&chunk.path_hash], chunk));
        assert_eq!(all[&chunk.path_hash], chunk.path_hash.to_le_bytes());
    }
}

#[test]
fn selection_skips_unknowns_and_collapses_repeated_requests() {
    let wad = archive();
    let a = wad.chunks[0].path_hash;
    let b = wad.chunks[1].path_hash;
    let selected = wad.extract_selected([b, 0, a, b, a]).unwrap();
    assert_eq!(selected.len(), 2);
    assert_eq!(selected[&a], a.to_le_bytes());
    assert_eq!(selected[&b], b.to_le_bytes());
    assert!(wad.extract_selected([]).unwrap().is_empty());
    assert!(wad.extract_selected([0]).unwrap().is_empty());
}

#[test]
fn duplicate_hashes_always_use_the_first_entry() {
    let mut wad = archive();
    let hash = wad.chunks[0].path_hash;
    wad.chunks[1].path_hash = hash;
    wad.chunks[1].data_offset = u32::MAX;
    assert!(std::ptr::eq(wad.chunk_index()[&hash], &wad.chunks[0]));
    assert_eq!(wad.extract_all().unwrap()[&hash], hash.to_le_bytes());
    assert_eq!(
        wad.extract_selected([hash, hash]).unwrap()[&hash],
        hash.to_le_bytes()
    );
}

#[test]
fn errors_in_selected_entries_are_not_hidden_by_duplicates() {
    let mut wad = archive();
    let hash = wad.chunks[0].path_hash;
    wad.chunks[1].path_hash = hash;
    wad.chunks[0].data_offset = u32::MAX;
    assert!(wad.extract_all().is_err());
    assert!(wad.extract_selected([hash]).is_err());
    let other = wad.chunks[2].path_hash;
    assert_eq!(
        wad.extract_selected([other]).unwrap()[&other],
        other.to_le_bytes()
    );
}

#[test]
fn index_works_with_toc_only_streaming_reads() {
    use rs_io::Serialize;
    let bytes = archive().to_bytes().unwrap();
    let mut reader = Cursor::new(bytes);
    let toc = Wad::from_reader_toc(&mut reader).unwrap();
    for (&hash, &chunk) in &toc.chunk_index() {
        assert_eq!(
            toc.chunk_data_from(&mut reader, chunk).unwrap(),
            hash.to_le_bytes()
        );
    }
}

#[test]
fn rebuilt_index_observes_public_table_edits() {
    let mut wad = archive();
    let old = wad.chunks[0].path_hash;
    assert!(wad.chunk_index().contains_key(&old));
    wad.chunks[0].path_hash = 0;
    let index = wad.chunk_index();
    assert!(!index.contains_key(&old));
    assert!(std::ptr::eq(index[&0], &wad.chunks[0]));
}
