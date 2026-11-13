use rs_mapgeo::{
    ElementFormat, ElementName, Geometry, MapGeometry, VertexDescription, VertexElement,
    VertexUsage,
};
use rs_math::Mat4;

fn triangle() -> Geometry {
    Geometry::single(
        "material",
        vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0],
        vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0],
        vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0],
        vec![0, 1, 2],
    )
}

fn map() -> MapGeometry {
    let mut map = MapGeometry {
        version: 18,
        separate_point_lights: false,
        texture_overrides: vec![],
        vertex_descriptions: vec![],
        vertex_buffers: vec![],
        index_buffers: vec![],
        models: vec![],
        scene_graphs: vec![],
        planar_reflectors: vec![],
    };
    map.add_model("triangle", &triangle(), Mat4::IDENTITY, 0)
        .unwrap();
    map
}

#[test]
fn invalid_later_stream_does_not_leave_partially_appended_buffers() {
    let mut map = map();
    map.vertex_descriptions.push(VertexDescription {
        usage: VertexUsage::Static,
        elements: vec![],
    });
    map.models[0].vertex_buffer_ids.push(0);
    let before = map.clone();
    assert!(map.replace_geometry(0, &triangle()).is_err());
    assert_eq!(map, before);
}

#[test]
fn attribute_encoding_failure_preserves_every_stream() {
    let mut map = map();
    let mut invalid = map.vertex_descriptions[0].clone();
    invalid.elements[0].format = ElementFormat::XyzwFloat32;
    map.vertex_descriptions.push(invalid);
    map.models[0].vertex_buffer_ids.push(0);
    let before = map.clone();
    assert!(map.replace_geometry(0, &triangle()).is_err());
    assert_eq!(map, before);
}

#[test]
fn missing_vertex_buffers_fail_reads_and_edits_without_mutation() {
    for id in [-1, 100] {
        let mut map = map();
        map.models[0].vertex_buffer_ids[0] = id;
        let before = map.clone();
        assert!(map.geometry(0).is_err());
        assert!(map.replace_geometry(0, &triangle()).is_err());
        assert_eq!(map, before);
    }
}

#[test]
fn missing_index_buffers_fail_reads_and_edits_without_mutation() {
    for id in [-1, 100] {
        let mut map = map();
        map.models[0].index_buffer_id = id;
        let before = map.clone();
        assert!(map.geometry(0).is_err());
        assert!(map.replace_geometry(0, &triangle()).is_err());
        assert_eq!(map, before);
    }
}

#[test]
fn valid_replacement_retains_shared_buffers_and_other_models() {
    let mut map = map();
    map.models.push(map.models[0].clone());
    let before = map.clone();
    let mut geometry = triangle();
    geometry.positions[0] = 5.0;
    map.replace_geometry(0, &geometry).unwrap();
    assert_eq!(map.geometry(0).unwrap(), geometry);
    assert_eq!(map.geometry(1).unwrap(), triangle());
    assert_eq!(map.models[1], before.models[1]);
    assert_eq!(map.vertex_buffers[0], before.vertex_buffers[0]);
    assert_eq!(map.index_buffers[0], before.index_buffers[0]);
}

#[test]
fn malformed_geometry_is_rejected_before_add_or_replace_mutates_the_map() {
    for case in 0..7 {
        let mut geometry = triangle();
        match case {
            0 => geometry.positions[0] = f32::NAN,
            1 => geometry.normals[1] = f32::INFINITY,
            2 => geometry.uvs[0] = f32::NEG_INFINITY,
            3 => geometry.indices.push(0),
            4 => geometry.submeshes[0].index_count = 2,
            5 => {
                geometry.indices.extend([0, 1, 2]);
                geometry.submeshes[0].index_start = 1;
            }
            6 => geometry.positions[4] = f32::INFINITY,
            _ => unreachable!(),
        }
        let mut map = map();
        let before = map.clone();
        assert!(
            map.replace_geometry(0, &geometry).is_err(),
            "replace case {case}"
        );
        assert_eq!(map, before);
        assert!(
            map.add_model("bad", &geometry, Mat4::IDENTITY, 0).is_err(),
            "add case {case}"
        );
        assert_eq!(map, before);
    }
}

#[test]
fn half_float_overflow_in_a_later_stream_preserves_the_entire_map() {
    let mut map = map();
    map.vertex_descriptions.push(VertexDescription {
        usage: VertexUsage::Static,
        elements: vec![VertexElement {
            name: ElementName::Texcoord0,
            format: ElementFormat::XyPacked1616,
        }],
    });
    map.models[0].vertex_buffer_ids.push(0);
    let before = map.clone();
    let mut geometry = triangle();
    *geometry.uvs.last_mut().unwrap() = 100_000.0;
    assert!(
        geometry.validate().is_ok(),
        "generic geometry is valid before target encoding"
    );
    assert!(map.replace_geometry(0, &geometry).is_err());
    assert_eq!(map, before);
}

#[test]
fn validation_retains_supported_draw_ranges_and_attribute_values() {
    let mut geometry = triangle();
    geometry.normals.fill(0.0);
    geometry.uvs[0] = -2.0;
    geometry.uvs[1] = 3.0;
    geometry.indices.extend([0, 0, 0]);
    geometry.submeshes.push(geometry.submeshes[0].clone());
    geometry.submeshes.push(rs_mapgeo::SubmeshRange {
        name: "unused".into(),
        index_start: 6,
        index_count: 0,
    });
    geometry.validate().unwrap();
    let mut map = map();
    map.replace_geometry(0, &geometry).unwrap();
    assert_eq!(map.geometry(0).unwrap(), geometry);
}
