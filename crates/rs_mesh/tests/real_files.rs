use std::path::PathBuf;

use rs_io::{Parse, Serialize};
use rs_mesh::{SkinnedMesh, SkinnedMeshVertexType, StaticMesh};

fn sample_dir() -> Option<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../Sample-Files");
    dir.is_dir().then_some(dir)
}

const SCB_FILES: &[&str] = &[
    "aatrox_base_q_cone_blast.scb",
    "aatrox_skin11_swipemesh02.scb",
    "blitzcrank_skin47_lighting_cyl_02.scb",
    "floorslash.scb",
];

const SKN_FILES: &[&str] = &["aatrox.skn", "aatrox_skin01.skn", "aatrox_skin02.skn"];

fn check_static(mesh: &StaticMesh, label: &str) {
    assert!(
        !mesh.positions().is_empty(),
        "{label}: expected at least one vertex"
    );
    assert!(
        !mesh.faces().is_empty(),
        "{label}: expected at least one face"
    );

    let vertex_count = mesh.positions().len() as u32;
    for (i, face) in mesh.faces().iter().enumerate() {
        for &index in &face.indices {
            assert!(
                index < vertex_count,
                "{label}: face {i} index {index} out of range (vertices = {vertex_count})"
            );
        }
    }

    if let Some(colors) = mesh.colors() {
        assert_eq!(
            colors.len(),
            mesh.positions().len(),
            "{label}: vertex color count must match vertex count"
        );
    }
}

#[test]
fn scb_real_files_roundtrip() {
    let Some(dir) = sample_dir() else {
        eprintln!("Sample-Files directory missing; skipping real .scb tests");
        return;
    };

    for name in SCB_FILES {
        let path = dir.join(name);
        if !path.is_file() {
            eprintln!("missing sample {name}; skipping");
            continue;
        }
        let bytes = std::fs::read(&path).unwrap();
        let mesh = StaticMesh::from_bytes(&bytes)
            .unwrap_or_else(|e| panic!("failed to parse {name}: {e}"));
        check_static(&mesh, name);

        // Every observed sample is r3d2Mesh 3.2.
        assert_eq!(mesh.version, (3, 2), "{name}: expected version 3.2");

        // Byte-exact round-trip.
        let out = mesh
            .to_bytes()
            .unwrap_or_else(|e| panic!("failed to write {name}: {e}"));
        assert_eq!(out, bytes, "{name}: .scb round-trip is not byte-exact");

        // Reparse equality.
        let mesh2 = StaticMesh::from_bytes(&out).unwrap();
        assert_eq!(mesh, mesh2, "{name}: struct round-trip mismatch");
    }
}

#[test]
fn scb_blitzcrank_trailing_preserved() {
    let Some(dir) = sample_dir() else {
        eprintln!("Sample-Files directory missing; skipping");
        return;
    };
    let path = dir.join("blitzcrank_skin47_lighting_cyl_02.scb");
    if !path.is_file() {
        eprintln!("missing blitzcrank sample; skipping");
        return;
    }
    let mesh = StaticMesh::from_path(&path).unwrap();
    // flags == 5 (HasVcp | HasLocalOriginLocatorAndPivot); the post-face block is captured raw.
    assert_eq!(mesh.flags(), 5, "blitzcrank: expected flags == 5");
    assert_eq!(
        mesh.trailing.len(),
        1104,
        "blitzcrank: expected 1104 trailing bytes to be preserved"
    );
}

#[test]
fn skn_real_files_roundtrip() {
    let Some(dir) = sample_dir() else {
        eprintln!("Sample-Files directory missing; skipping real .skn tests");
        return;
    };

    for name in SKN_FILES {
        let path = dir.join(name);
        if !path.is_file() {
            eprintln!("missing sample {name}; skipping");
            continue;
        }
        let bytes = std::fs::read(&path).unwrap();
        let mesh = SkinnedMesh::from_bytes(&bytes)
            .unwrap_or_else(|e| panic!("failed to parse {name}: {e}"));

        // All three real samples are version 4.1, Basic vertex layout.
        assert_eq!(mesh.version(), (4, 1), "{name}: expected version 4.1");
        assert_eq!(
            mesh.vertex_type,
            SkinnedMeshVertexType::Basic,
            "{name}: expected Basic vertex layout"
        );
        assert!(!mesh.ranges().is_empty(), "{name}: expected ranges");
        assert_eq!(
            mesh.indices().len() % 3,
            0,
            "{name}: index count must be a multiple of 3"
        );
        // The game appends a 12-byte zero end-tab after the vertex buffer.
        assert_eq!(
            mesh.trailing.len(),
            12,
            "{name}: expected 12 trailing bytes preserved"
        );

        // Submesh ranges must stay within the shared buffers.
        let vcount = mesh.vertices().len() as u32;
        let icount = mesh.indices().len() as u32;
        for r in mesh.ranges() {
            assert!(
                r.vertex_start + r.vertex_count <= vcount,
                "{name}: range '{}' vertex span out of bounds",
                r.name
            );
            assert!(
                r.index_start + r.index_count <= icount,
                "{name}: range '{}' index span out of bounds",
                r.name
            );
        }

        // Byte-exact round-trip.
        let out = mesh
            .to_bytes()
            .unwrap_or_else(|e| panic!("failed to write {name}: {e}"));
        assert_eq!(out, bytes, "{name}: .skn round-trip is not byte-exact");

        let mesh2 = SkinnedMesh::from_bytes(&out).unwrap();
        assert_eq!(mesh, mesh2, "{name}: struct round-trip mismatch");
    }
}

/// The `.scb` TAIL: per-corner VCP colors + the local-origin/pivot pair, and the two consumer
/// helpers that depend on them (`centred_positions`, `effective_bounds`).
///
/// Built in memory rather than from Sample-Files so it runs everywhere. The shape mirrors a real
/// file that motivated these accessors - Draven Skin68's `draven_skin68_weapontrail.scb`:
///   * `flags = 5` (bit0 HasVcp + bit2), with bit1 CLEAR yet the origin/pivot pair present,
///   * an all-zero header AABB while the vertices sit far from the origin,
///   * `central` equal to the vertex AABB centre.
#[test]
fn scb_tail_accessors_and_degenerate_bounds() {
    fn put_vec3(out: &mut Vec<u8>, v: [f32; 3]) {
        for c in v {
            out.extend_from_slice(&c.to_le_bytes());
        }
    }

    const VERTS: [[f32; 3]; 3] = [
        [100.0, 200.0, 10.0],
        [120.0, 220.0, 30.0],
        [80.0, 240.0, 20.0],
    ];
    const CENTRAL: [f32; 3] = [100.0, 220.0, 20.0];
    const PIVOT: [f32; 3] = [1.0, 2.0, 3.0];

    let mut b: Vec<u8> = Vec::new();
    b.extend_from_slice(b"r3d2Mesh");
    b.extend_from_slice(&3u16.to_le_bytes()); // major
    b.extend_from_slice(&2u16.to_le_bytes()); // minor -> vertex_type present
    b.extend_from_slice(&[0u8; 128]); // name
    b.extend_from_slice(&3u32.to_le_bytes()); // vertex count
    b.extend_from_slice(&1u32.to_le_bytes()); // face count
    b.extend_from_slice(&5u32.to_le_bytes()); // flags: bit0 | bit2
    put_vec3(&mut b, [0.0, 0.0, 0.0]); // DEGENERATE bbox min
    put_vec3(&mut b, [0.0, 0.0, 0.0]); // DEGENERATE bbox max
    b.extend_from_slice(&0u32.to_le_bytes()); // vertex_type 0 -> no per-vertex color block
    for v in VERTS {
        put_vec3(&mut b, v);
    }
    put_vec3(&mut b, CENTRAL);
    // one face: indices, 64-byte material, then 3 u + 3 v
    for i in 0u32..3 {
        b.extend_from_slice(&i.to_le_bytes());
    }
    b.extend_from_slice(&[0u8; 64]);
    for f in [0.0f32, 1.0, 0.0, 0.0, 0.0, 1.0] {
        b.extend_from_slice(&f.to_le_bytes());
    }
    // TAIL: per-corner RGB (1 face * 3 corners * 3 bytes) then local origin + pivot
    b.extend_from_slice(&[10, 20, 30, 40, 50, 60, 70, 80, 90]);
    put_vec3(&mut b, CENTRAL); // local origin repeats `central`
    put_vec3(&mut b, PIVOT);

    let mesh = StaticMesh::from_scb_reader(&mut b.as_slice()).expect("parse synthetic scb");

    let vcp = mesh.vcp_colors().expect("bit0 set -> vcp colors");
    assert_eq!(vcp.len(), 3, "one face = three corners");
    assert_eq!(vcp[0], [10, 20, 30]);
    assert_eq!(vcp[2], [70, 80, 90]);

    // Present despite bit1 being CLEAR - the real file behaves this way too.
    let (origin, pivot) = mesh
        .local_origin_and_pivot()
        .expect("bit2 set -> origin/pivot pair");
    assert_eq!([origin.x, origin.y, origin.z], CENTRAL);
    assert_eq!([pivot.x, pivot.y, pivot.z], PIVOT);

    // Re-centred geometry straddles the origin instead of sitting at `central`.
    let centred = mesh.centred_positions();
    let cx: f32 = centred.iter().map(|p| p.x).sum::<f32>() / 3.0;
    let cy: f32 = centred.iter().map(|p| p.y).sum::<f32>() / 3.0;
    assert!(
        cx.abs() < 1e-3 && cy.abs() < 1e-3,
        "expected re-centred, got ({cx}, {cy})"
    );

    // Degenerate header box recovered from the vertices.
    let bounds = mesh.effective_bounds();
    assert_eq!([bounds.min.x, bounds.min.y], [80.0, 200.0]);
    assert_eq!([bounds.max.x, bounds.max.y], [120.0, 240.0]);

    // The raw tail is still intact, so `.scb` -> `.scb` stays byte-exact.
    assert_eq!(mesh.trailing.len(), 9 + 24);
}
