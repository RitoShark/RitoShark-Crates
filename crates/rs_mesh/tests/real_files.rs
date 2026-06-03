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
