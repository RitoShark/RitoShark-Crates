use rs_mapgeo::{ElementFormat, Geometry, MapGeometry, VertexDescription, VertexUsage};
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
