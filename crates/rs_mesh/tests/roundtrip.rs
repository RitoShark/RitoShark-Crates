use rs_io::{Parse, Serialize, WriterExt};
use rs_math::{Vec2, Vec3};
use rs_mesh::{SkinnedMesh, SkinnedMeshVertexType, StaticMesh};

fn write_padded(buf: &mut Vec<u8>, s: &str, n: usize) {
    let bytes = s.as_bytes();
    let len = bytes.len().min(n);
    buf.extend_from_slice(&bytes[..len]);
    buf.extend(std::iter::repeat_n(0u8, n - len));
}

fn basic_vertex(buf: &mut Vec<u8>, pos: [f32; 3], n: [f32; 3], uv: [f32; 2]) {
    for c in pos {
        buf.write_f32(c).unwrap();
    }
    buf.extend_from_slice(&[0, 1, 2, 3]); // blend indices
    for w in [1.0f32, 0.0, 0.0, 0.0] {
        buf.write_f32(w).unwrap();
    }
    for c in n {
        buf.write_f32(c).unwrap();
    }
    for c in uv {
        buf.write_f32(c).unwrap();
    }
}

fn build_skn(major: u16, vtype: u32) -> Vec<u8> {
    let mut buf = Vec::new();
    buf.write_u32(0x0011_2233).unwrap();
    buf.write_u16(major).unwrap();
    buf.write_u16(1).unwrap();

    // one submesh
    buf.write_u32(1).unwrap();
    write_padded(&mut buf, "Body", 64);
    buf.write_u32(0).unwrap(); // vertex_start
    buf.write_u32(3).unwrap(); // vertex_count
    buf.write_u32(0).unwrap(); // index_start
    buf.write_u32(3).unwrap(); // index_count

    if major == 4 {
        buf.write_u32(0).unwrap(); // flags
    }

    buf.write_u32(3).unwrap(); // index_count
    buf.write_u32(3).unwrap(); // vertex_count

    if major == 4 {
        let size = match vtype {
            0 => 52u32,
            1 => 56,
            2 => 72,
            _ => unreachable!(),
        };
        buf.write_u32(size).unwrap();
        buf.write_u32(vtype).unwrap();
        // bbox min, max
        for c in [-1.0f32, -1.0, -1.0, 1.0, 1.0, 1.0] {
            buf.write_f32(c).unwrap();
        }
        // sphere center, radius
        for c in [0.0f32, 0.0, 0.0, 1.732] {
            buf.write_f32(c).unwrap();
        }
    }

    // indices
    for i in [0u16, 1, 2] {
        buf.write_u16(i).unwrap();
    }

    // vertices
    for k in 0..3 {
        let f = k as f32;
        basic_vertex(&mut buf, [f, f + 1.0, f + 2.0], [0.0, 1.0, 0.0], [f, 1.0 - f]);
        if vtype >= 1 {
            buf.extend_from_slice(&[255, 128, 64, 255]); // color
            if vtype == 2 {
                for c in [0.0f32, 0.0, 1.0, 1.0] {
                    buf.write_f32(c).unwrap(); // tangent
                }
            }
        }
    }

    buf
}

fn roundtrip_skn(major: u16, vtype: u32) {
    let bytes = build_skn(major, vtype);
    let mesh = SkinnedMesh::from_bytes(&bytes).expect("parse");
    let out = mesh.to_bytes().expect("write");
    assert_eq!(out, bytes, "byte-exact round-trip for v{major} type{vtype}");

    let mesh2 = SkinnedMesh::from_bytes(&out).expect("reparse");
    assert_eq!(mesh, mesh2, "struct round-trip for v{major} type{vtype}");
    assert_eq!(mesh.vertices().len(), 3);
    assert_eq!(mesh.indices(), &[0, 1, 2]);
}

#[test]
fn skn_v1_basic_roundtrip() {
    roundtrip_skn(1, 0);
}

#[test]
fn skn_v2_basic_roundtrip() {
    roundtrip_skn(2, 0);
}

#[test]
fn skn_v4_basic_roundtrip() {
    roundtrip_skn(4, 0);
}

#[test]
fn skn_v4_color_roundtrip() {
    roundtrip_skn(4, 1);
}

#[test]
fn skn_v4_tangent_roundtrip() {
    roundtrip_skn(4, 2);
}

#[test]
fn skn_vertex_fields_parsed() {
    let bytes = build_skn(4, 2);
    let mesh = SkinnedMesh::from_bytes(&bytes).unwrap();
    assert_eq!(mesh.vertex_type, SkinnedMeshVertexType::Tangent);
    let v0 = &mesh.vertices()[0];
    assert_eq!(v0.position, Vec3::new(0.0, 1.0, 2.0));
    assert_eq!(v0.blend_indices, [0, 1, 2, 3]);
    assert_eq!(v0.blend_weights, [1.0, 0.0, 0.0, 0.0]);
    assert_eq!(v0.color, Some([255, 128, 64, 255]));
    assert!(v0.tangent.is_some());
}

#[test]
fn skn_bad_magic_errs() {
    let mut bytes = build_skn(1, 0);
    bytes[0] = 0xFF;
    assert!(SkinnedMesh::from_bytes(&bytes).is_err());
}

#[test]
fn skn_bad_version_errs() {
    let mut bytes = build_skn(1, 0);
    bytes[4] = 9; // major = 9
    assert!(SkinnedMesh::from_bytes(&bytes).is_err());
}

fn build_scb(vertex_type: Option<u32>) -> Vec<u8> {
    let (major, minor) = if vertex_type.is_some() { (3u16, 2u16) } else { (3, 1) };
    let mut buf = Vec::new();
    buf.extend_from_slice(b"r3d2Mesh");
    buf.write_u16(major).unwrap();
    buf.write_u16(minor).unwrap();
    write_padded(&mut buf, "mesh", 128);
    buf.write_u32(3).unwrap(); // vertex_count
    buf.write_u32(1).unwrap(); // face_count
    buf.write_u32(0).unwrap(); // flags
    for c in [-1.0f32, -1.0, -1.0, 1.0, 1.0, 1.0] {
        buf.write_f32(c).unwrap(); // bbox
    }
    if let Some(t) = vertex_type {
        buf.write_u32(t).unwrap();
    }
    // positions
    for k in 0..3 {
        let f = k as f32;
        for c in [f, f + 1.0, f + 2.0] {
            buf.write_f32(c).unwrap();
        }
    }
    // colors
    if matches!(vertex_type, Some(t) if t >= 1) {
        for _ in 0..3 {
            buf.extend_from_slice(&[1, 2, 3, 4]);
        }
    }
    // central
    for c in [0.0f32, 0.0, 0.0] {
        buf.write_f32(c).unwrap();
    }
    // one face
    for i in [0u32, 1, 2] {
        buf.write_u32(i).unwrap();
    }
    write_padded(&mut buf, "mat", 64);
    for c in [0.0f32, 0.5, 1.0, 0.0, 0.5, 1.0] {
        buf.write_f32(c).unwrap(); // uuu vvv
    }
    buf
}

#[test]
fn scb_v31_parses() {
    let bytes = build_scb(None);
    let mesh = StaticMesh::from_bytes(&bytes).expect("parse scb");
    assert_eq!(mesh.name(), "mesh");
    assert_eq!(mesh.positions().len(), 3);
    assert_eq!(mesh.faces().len(), 1);
    assert!(mesh.colors().is_none());
    let f = &mesh.faces()[0];
    assert_eq!(f.material, "mat");
    assert_eq!(f.indices, [0, 1, 2]);
    assert_eq!(f.uvs[1], Vec2::new(0.5, 0.5));
}

#[test]
fn scb_v32_color_parses() {
    let bytes = build_scb(Some(1));
    let mesh = StaticMesh::from_bytes(&bytes).expect("parse scb color");
    assert_eq!(mesh.colors().unwrap().len(), 3);
    assert_eq!(mesh.colors().unwrap()[0], [1, 2, 3, 4]);
}

#[test]
fn sco_text_parses() {
    let text = "[ObjectBegin]\n\
        Name= thing\n\
        CentralPoint= 0.0 0.0 0.0\n\
        Verts= 3\n\
        0.0 0.0 0.0\n\
        1.0 0.0 0.0\n\
        0.0 1.0 0.0\n\
        Faces= 1\n\
        3 0 1 2 mat 0.0 0.0 1.0 0.0 0.0 1.0\n\
        [ObjectEnd]\n";
    let mesh = StaticMesh::from_bytes(text.as_bytes()).expect("parse sco");
    assert_eq!(mesh.name(), "thing");
    assert_eq!(mesh.positions().len(), 3);
    assert_eq!(mesh.faces().len(), 1);
    assert_eq!(mesh.faces()[0].material, "mat");
}
