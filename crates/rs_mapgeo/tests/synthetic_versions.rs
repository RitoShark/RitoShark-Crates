//! Synthetic per-version round-trip tests. No real `.mapgeo` samples exist for the older OEGM
//! versions, so for each newly supported version we hand-build the smallest valid file straight
//! from the C# oracle's layout (one vertex declaration, one vertex buffer, one index buffer, one
//! model, a minimal scene graph, no planar reflectors) and assert byte-exact
//! `read -> to_bytes -> ==`. Each builder honors exactly the version gates the reader/writer use:
//! the leading `separate_point_lights` byte (< 7), implicit sampler strings (9/11, < 17), embedded
//! mesh names (< 12), per-buffer layer bytes (>= 13), the post-transform layer byte (7..=12), the
//! render-flag word (u8 11..=13 / transition + u8 14..15 / transition + u16 >= 16), the per-mesh
//! point light (< 7), the nine spherical-harmonics coefficients (< 9), the single baked-paint
//! channel (12..=16), the counted overrides + scale/offset (>= 17), the visibility-controller hash
//! and counted scene-graph list (>= 15), and the planar reflectors (>= 13).

use rs_io::Parse;
use rs_mapgeo::MapGeometry;

fn push_u32(b: &mut Vec<u8>, v: u32) {
    b.extend_from_slice(&v.to_le_bytes());
}
fn push_i32(b: &mut Vec<u8>, v: i32) {
    b.extend_from_slice(&v.to_le_bytes());
}
fn push_u16(b: &mut Vec<u8>, v: u16) {
    b.extend_from_slice(&v.to_le_bytes());
}
fn push_f32(b: &mut Vec<u8>, v: f32) {
    b.extend_from_slice(&v.to_le_bytes());
}
fn push_str(b: &mut Vec<u8>, s: &str) {
    push_u32(b, s.len() as u32);
    b.extend_from_slice(s.as_bytes());
}
fn push_channel(b: &mut Vec<u8>, path: &str) {
    push_str(b, path);
    for v in [1.0f32, 1.0, 0.0, 0.0] {
        push_f32(b, v);
    }
}

/// Builds the smallest valid OEGM file for `version` following the oracle's version gates.
fn build(version: u32) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend_from_slice(b"OEGM");
    push_u32(&mut b, version);

    // separate_point_lights byte (< 7)
    if version < 7 {
        b.push(1u8);
    }

    // shader/sampler overrides
    if version >= 17 {
        push_u32(&mut b, 0); // counted [index, name] list, empty
    } else {
        // implicit bare strings: one from v9, a second from v11
        if version >= 9 {
            push_str(&mut b, "BAKED_DIFFUSE_TEXTURE");
        }
        if version >= 11 {
            push_str(&mut b, "BAKED_DIFFUSE_TEXTURE_ALPHA");
        }
    }

    // vertex declarations: 1 decl, Static, 1 element (Position, XYZ_Float32)
    push_u32(&mut b, 1);
    push_u32(&mut b, 0); // usage Static
    push_u32(&mut b, 1); // element count
    push_u32(&mut b, 0); // name Position
    push_u32(&mut b, 2); // format XYZ_Float32
    for _ in 0..14 {
        push_u32(&mut b, 0); // Position
        push_u32(&mut b, 3); // XYZW_Float32
    }

    // vertex buffers: 1 buffer, optional layer (>= 13), 12 bytes (one XYZ vertex)
    push_u32(&mut b, 1);
    if version >= 13 {
        b.push(0u8);
    }
    push_u32(&mut b, 12);
    for v in [1.0f32, 2.0, 3.0] {
        push_f32(&mut b, v);
    }

    // index buffers: 1 buffer, optional layer (>= 13), 6 bytes (three u16)
    push_u32(&mut b, 1);
    if version >= 13 {
        b.push(0u8);
    }
    push_u32(&mut b, 6);
    for i in 0u16..3 {
        push_u16(&mut b, i);
    }

    // models: 1 model
    push_u32(&mut b, 1);

    // embedded name (< 12)
    if version < 12 {
        push_str(&mut b, "MapGeo_Instance_0");
    }

    push_u32(&mut b, 1); // vertex count
    push_u32(&mut b, 1); // vertex buffer count
    push_u32(&mut b, 0); // vertex description id
    push_i32(&mut b, 0); // vertex buffer id
    push_u32(&mut b, 3); // index count
    push_i32(&mut b, 0); // index buffer id

    if version >= 13 {
        b.push(0u8); // layer
    }
    if version >= 18 {
        push_u32(&mut b, 0xAABBCCDD); // unknown_v18
    }
    if version >= 15 {
        push_u32(&mut b, 0); // bucket grid hash
    }

    push_u32(&mut b, 0); // submesh count

    if version != 5 {
        b.push(0u8); // disable backface culling
    }

    // bounds min/max
    for v in [0.0f32, 0.0, 0.0, 1.0, 1.0, 1.0] {
        push_f32(&mut b, v);
    }
    // identity transform
    let identity: [f32; 16] = [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ];
    for v in identity {
        push_f32(&mut b, v);
    }

    b.push(31u8); // quality

    if (7..=12).contains(&version) {
        b.push(0u8); // post-transform layer
    }

    // render-flag block
    if (11..14).contains(&version) {
        b.push(0u8); // u8 render flags, no transition byte
    } else if version >= 14 {
        b.push(0u8); // layer transition
        if version >= 16 {
            push_u16(&mut b, 0); // u16 render flags
        } else {
            b.push(0u8); // u8 render flags
        }
    }

    // per-mesh point light (< 7), gated by separate_point_lights (set above)
    if version < 7 {
        for v in [10.0f32, 20.0, 30.0] {
            push_f32(&mut b, v);
        }
    }

    if version < 9 {
        // nine spherical-harmonics coefficients
        for i in 0..9u32 {
            for axis in 0..3u32 {
                push_f32(&mut b, (i * 3 + axis) as f32);
            }
        }
        push_channel(&mut b, "baked"); // baked light only; no stationary, no paint
    } else {
        push_channel(&mut b, "baked");
        push_channel(&mut b, "stationary");
        if version >= 17 {
            push_u32(&mut b, 0); // texture override count
            for v in [0.5f32, 0.5, 0.1, 0.1] {
                push_f32(&mut b, v); // baked paint scale/offset
            }
        } else if version >= 12 {
            push_channel(&mut b, "paint"); // single baked-paint channel
        }
    }

    // scene graphs: one disabled bucketed-geometry graph
    if version >= 15 {
        push_u32(&mut b, 1); // scene graph count
        push_u32(&mut b, 0); // controller hash
    }
    if version >= 18 {
        push_f32(&mut b, 0.0); // leading unknown f32
    }
    for v in [0.0f32, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 1.0] {
        push_f32(&mut b, v); // bounds + bucket size
    }
    push_u16(&mut b, 0); // buckets per side
    b.push(1u8); // is_disabled
    b.push(0u8); // flags
    push_u32(&mut b, 0); // vertex count
    push_u32(&mut b, 0); // index count

    // planar reflectors (>= 13): empty
    if version >= 13 {
        push_u32(&mut b, 0);
    }

    b
}

fn round_trip(version: u32) {
    let bytes = build(version);
    let geo = MapGeometry::from_bytes(&bytes)
        .unwrap_or_else(|e| panic!("v{version}: parse failed: {e:?}"));
    assert_eq!(geo.version, version, "v{version}: version mismatch");
    assert_eq!(geo.models.len(), 1, "v{version}: expected one model");

    let mut out = Vec::new();
    geo.to_writer(&mut out)
        .unwrap_or_else(|e| panic!("v{version}: write failed: {e:?}"));

    assert_eq!(
        out.len(),
        bytes.len(),
        "v{version}: length {} != source {}",
        out.len(),
        bytes.len()
    );
    if let Some(offset) = out.iter().zip(&bytes).position(|(a, b)| a != b) {
        panic!("v{version}: re-serialized output diverges at byte {offset}");
    }
}

macro_rules! version_test {
    ($name:ident, $version:literal) => {
        #[test]
        fn $name() {
            round_trip($version);
        }
    };
}

version_test!(round_trip_v5, 5);
version_test!(round_trip_v6, 6);
version_test!(round_trip_v7, 7);
version_test!(round_trip_v9, 9);
version_test!(round_trip_v11, 11);
version_test!(round_trip_v12, 12);
version_test!(round_trip_v13, 13);
version_test!(round_trip_v15, 15);

/// Versions outside the oracle's matrix must be rejected, never mis-parsed.
#[test]
fn rejects_undocumented_versions() {
    for version in [8u32, 10, 16] {
        // Build a v15-shaped body then stamp the unsupported version into the header; the reader
        // must reject on the version gate before consuming the body.
        let mut bytes = build(15);
        bytes[4..8].copy_from_slice(&version.to_le_bytes());
        match MapGeometry::from_bytes(&bytes) {
            Err(rs_mapgeo::Error::UnsupportedVersion(reported)) => {
                assert_eq!(reported, version);
            }
            other => panic!("v{version}: expected UnsupportedVersion, got {other:?}"),
        }
    }
}
