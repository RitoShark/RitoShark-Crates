use std::io::Cursor;

use rs_io::Parse;
use rs_mapgeo::{Error, MapGeometry};

/// Builds the smallest valid OEGM v17 file: one vertex declaration, one vertex buffer, one
/// index buffer, and one model with no submeshes or texture overrides.
fn minimal_v17() -> Vec<u8> {
    let mut b = Vec::new();

    b.extend_from_slice(b"OEGM");
    b.extend_from_slice(&17u32.to_le_bytes());

    // texture overrides
    b.extend_from_slice(&0u32.to_le_bytes());

    // vertex descriptions: 1 decl, Static usage, 1 element (Position, XYZ_Float32)
    b.extend_from_slice(&1u32.to_le_bytes());
    b.extend_from_slice(&0u32.to_le_bytes()); // usage = Static
    b.extend_from_slice(&1u32.to_le_bytes()); // element count
    b.extend_from_slice(&0u32.to_le_bytes()); // name = Position
    b.extend_from_slice(&2u32.to_le_bytes()); // format = XYZ_Float32
    // padding: the 14 unused element slots default to (Position=0, XYZW_Float32=3)
    for _ in 0..14 {
        b.extend_from_slice(&0u32.to_le_bytes());
        b.extend_from_slice(&3u32.to_le_bytes());
    }

    // vertex buffers: 1 buffer, layer 0, 12 bytes (one XYZ vertex)
    b.extend_from_slice(&1u32.to_le_bytes());
    b.push(0u8); // layer
    b.extend_from_slice(&12u32.to_le_bytes());
    b.extend_from_slice(&1.0f32.to_le_bytes());
    b.extend_from_slice(&2.0f32.to_le_bytes());
    b.extend_from_slice(&3.0f32.to_le_bytes());

    // index buffers: 1 buffer, layer 0, 6 bytes (three u16 indices)
    b.extend_from_slice(&1u32.to_le_bytes());
    b.push(0u8); // layer
    b.extend_from_slice(&6u32.to_le_bytes());
    for i in 0u16..3 {
        b.extend_from_slice(&i.to_le_bytes());
    }

    // models: 1 model
    b.extend_from_slice(&1u32.to_le_bytes());
    b.extend_from_slice(&1u32.to_le_bytes()); // vertex count
    b.extend_from_slice(&1u32.to_le_bytes()); // vertex buffer count
    b.extend_from_slice(&0u32.to_le_bytes()); // vertex description id
    b.extend_from_slice(&0i32.to_le_bytes()); // vertex buffer id
    b.extend_from_slice(&3u32.to_le_bytes()); // index count
    b.extend_from_slice(&0i32.to_le_bytes()); // index buffer id
    b.push(0u8); // layer
    b.extend_from_slice(&0u32.to_le_bytes()); // bucket grid hash
    b.extend_from_slice(&0u32.to_le_bytes()); // submesh count
    b.push(0u8); // disable backface culling
    // bounds min/max
    for v in [0.0f32, 0.0, 0.0, 1.0, 1.0, 1.0] {
        b.extend_from_slice(&v.to_le_bytes());
    }
    // identity transform
    let identity: [f32; 16] = [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ];
    for v in identity {
        b.extend_from_slice(&v.to_le_bytes());
    }
    b.push(31u8); // quality
    b.push(0u8); // layer transition behavior
    b.extend_from_slice(&0u16.to_le_bytes()); // render flags
    // baked light channel: empty path + scale + offset
    b.extend_from_slice(&0u32.to_le_bytes());
    for v in [1.0f32, 1.0, 0.0, 0.0] {
        b.extend_from_slice(&v.to_le_bytes());
    }
    // stationary light channel
    b.extend_from_slice(&0u32.to_le_bytes());
    for v in [0.0f32, 0.0, 0.0, 0.0] {
        b.extend_from_slice(&v.to_le_bytes());
    }
    // model texture overrides + baked paint scale/offset
    b.extend_from_slice(&0u32.to_le_bytes());
    for v in [0.0f32, 0.0, 0.0, 0.0] {
        b.extend_from_slice(&v.to_le_bytes());
    }

    // scene graphs: 1 disabled bucketed-geometry graph
    b.extend_from_slice(&1u32.to_le_bytes()); // scene graph count
    b.extend_from_slice(&0u32.to_le_bytes()); // controller hash
    for v in [0.0f32, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0] {
        b.extend_from_slice(&v.to_le_bytes()); // bounds + bucket size
    }
    b.extend_from_slice(&0u16.to_le_bytes()); // buckets per side
    b.push(1u8); // is_disabled
    b.push(0u8); // flags
    b.extend_from_slice(&0u32.to_le_bytes()); // vertex count
    b.extend_from_slice(&0u32.to_le_bytes()); // index count

    // planar reflectors: none
    b.extend_from_slice(&0u32.to_le_bytes());

    b
}

#[test]
fn parses_magic_and_version() {
    let bytes = minimal_v17();
    let geo = MapGeometry::from_bytes(&bytes).expect("parse v17");

    assert_eq!(geo.version, 17);
    assert_eq!(geo.models.len(), 1);
    assert_eq!(geo.vertex_descriptions.len(), 1);
    assert_eq!(geo.vertex_buffers.len(), 1);
    assert_eq!(geo.index_buffers.len(), 1);

    let model = &geo.models[0];
    assert_eq!(model.name, "MapGeo_Instance_0");
    assert_eq!(model.vertex_count, 1);
    assert_eq!(model.index_count, 3);
    assert_eq!(model.quality, 31);
    assert_eq!(geo.index_buffers[0].indices, vec![0, 1, 2]);
}

#[test]
fn rejects_bad_magic() {
    let mut bytes = minimal_v17();
    bytes[0] = b'X';
    match MapGeometry::from_bytes(&bytes) {
        Err(Error::InvalidMagic) => {}
        other => panic!("expected InvalidMagic, got {other:?}"),
    }
}

#[test]
fn rejects_unsupported_version() {
    let mut bytes = minimal_v17();
    // overwrite the version field (bytes 4..8) with 13
    bytes[4..8].copy_from_slice(&13u32.to_le_bytes());
    match MapGeometry::from_bytes(&bytes) {
        Err(Error::UnsupportedVersion(13)) => {}
        other => panic!("expected UnsupportedVersion(13), got {other:?}"),
    }
}

#[test]
fn round_trips_byte_exact() {
    let bytes = minimal_v17();
    let geo = MapGeometry::from_bytes(&bytes).expect("parse v17");

    let mut out = Cursor::new(Vec::new());
    geo.to_writer(&mut out).expect("write v17");
    assert_eq!(out.into_inner(), bytes);
}

#[test]
fn truncated_input_errors_not_panics() {
    let bytes = minimal_v17();
    // Feed only the first 20 bytes; parsing must return Err, never panic.
    let result = MapGeometry::from_bytes(&bytes[..20]);
    assert!(result.is_err());
}
