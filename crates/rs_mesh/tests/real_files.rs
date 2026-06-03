use std::path::PathBuf;

use rs_io::Parse;
use rs_mesh::StaticMesh;

fn sample_dir() -> Option<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../sample-files");
    dir.is_dir().then_some(dir)
}

const SCB_FILES: &[&str] = &[
    "aatrox_base_q_cone_blast.scb",
    "aatrox_skin11_swipemesh02.scb",
    "blitzcrank_skin47_lighting_cyl_02.scb",
    "floorslash.scb",
];

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
fn scb_real_files_parse() {
    let Some(dir) = sample_dir() else {
        eprintln!("sample-files directory missing; skipping real .scb tests");
        return;
    };

    for name in SCB_FILES {
        let path = dir.join(name);
        if !path.is_file() {
            eprintln!("missing sample {name}; skipping");
            continue;
        }
        let mesh = StaticMesh::from_path(&path)
            .unwrap_or_else(|e| panic!("failed to parse {name}: {e}"));
        check_static(&mesh, name);
    }
}
