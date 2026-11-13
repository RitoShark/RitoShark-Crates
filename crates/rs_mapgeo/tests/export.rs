use rs_mapgeo::{
    ElementFormat, ElementName, Geometry, MapGeometry, SubmeshRange, VertexBuffer,
    VertexDescription, VertexElement, VertexUsage,
};
use rs_math::{Mat4, Quat, Vec3};

fn triangle() -> Geometry {
    Geometry::single(
        "stone",
        vec![0.0, 0.0, 0.0, 1.0, 0.0, -1.0, 0.0, 1.0, 0.0],
        vec![1.0, 0.0, 1.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0],
        vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0],
        vec![0, 1, 2],
    )
}

fn scene(transform: Mat4) -> MapGeometry {
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
    map.add_model("wall", &triangle(), transform, 0).unwrap();
    map
}

fn assert_normal_matches_face(geometry: &Geometry) {
    let [a, b, c] = geometry.submesh_triangles(0).unwrap().next().unwrap();
    let position = |i: u16| Vec3::from_slice(&geometry.positions[usize::from(i) * 3..]);
    let edge1 = position(b) - position(a);
    let edge2 = position(c) - position(a);
    let normal = Vec3::from_slice(&geometry.normals);
    assert!(normal.dot(edge1).abs() < 1e-5);
    assert!(normal.dot(edge2).abs() < 1e-5);
    assert!(normal.dot(edge1.cross(edge2).normalize()) > 0.99999);
    assert!((normal.length() - 1.0).abs() < 1e-5);
}

#[test]
fn world_positions_apply_rotation_and_translation_without_mutating_the_map() {
    let map = scene(Mat4::from_rotation_translation(
        Quat::from_rotation_z(std::f32::consts::FRAC_PI_2),
        Vec3::new(10.0, 20.0, 30.0),
    ));
    let before = map.clone();
    let geometry = map.geometry_world(0).unwrap();
    for (actual, expected) in geometry
        .positions
        .iter()
        .zip([10.0, 20.0, 30.0, 10.0, 21.0, 29.0, 9.0, 20.0, 30.0])
    {
        assert!((actual - expected).abs() < 1e-5);
    }
    assert_eq!(geometry.uvs, triangle().uvs);
    assert_eq!(geometry.submeshes, triangle().submeshes);
    assert_normal_matches_face(&geometry);
    assert_eq!(map, before);
}

#[test]
fn normals_stay_perpendicular_after_non_uniform_scale_and_shear() {
    let mut transform = Mat4::from_scale(Vec3::new(2.0, 3.0, 0.5));
    transform.y_axis.x = 0.7;
    let geometry = scene(transform).geometry_world(0).unwrap();
    assert_normal_matches_face(&geometry);
    assert_eq!(geometry.indices, [0, 1, 2]);
}

#[test]
fn reflection_reverses_winding_once_even_for_overlapping_material_ranges() {
    let mut map = scene(Mat4::from_scale(Vec3::new(-2.0, 3.0, 0.5)));
    let range = map.models[0].submeshes[0].clone();
    map.models[0].submeshes.push(range);
    let geometry = map.geometry_world(0).unwrap();
    assert_eq!(geometry.indices, [0, 2, 1]);
    assert_normal_matches_face(&geometry);
    assert_eq!(
        geometry.submesh_triangles(1).unwrap().collect::<Vec<_>>(),
        [[0, 2, 1]]
    );
}

#[test]
fn material_ranges_keep_order_names_and_indices() {
    let mut geometry = triangle();
    geometry.indices.extend([2, 1, 0]);
    geometry.submeshes.push(SubmeshRange {
        name: "water".into(),
        index_start: 3,
        index_count: 3,
    });
    assert_eq!(
        geometry.submesh_triangles(0).unwrap().collect::<Vec<_>>(),
        [[0, 1, 2]]
    );
    assert_eq!(
        geometry.submesh_triangles(1).unwrap().collect::<Vec<_>>(),
        [[2, 1, 0]]
    );
    assert_eq!(geometry.submeshes[1].name, "water");
}

#[test]
fn invalid_triangle_ranges_and_vertex_references_fail() {
    for (start, count) in [(0, 2), (1, 3), (3, 3), (u32::MAX, u32::MAX)] {
        let mut geometry = triangle();
        geometry.submeshes[0].index_start = start;
        geometry.submeshes[0].index_count = count;
        assert!(geometry.submesh_triangles(0).is_err());
    }
    let mut geometry = triangle();
    geometry.indices[0] = 100;
    assert!(geometry.submesh_triangles(0).is_err());
    assert!(geometry.submesh_triangles(1).is_err());
}

#[test]
fn singular_projective_and_non_finite_transforms_fail() {
    let mut non_finite = Mat4::IDENTITY;
    non_finite.x_axis.x = f32::NAN;
    for transform in [
        Mat4::from_scale(Vec3::new(1.0, 0.0, 1.0)),
        Mat4::perspective_rh(1.0, 1.0, 0.1, 100.0),
        non_finite,
    ] {
        assert!(scene(transform).geometry_world(0).is_err());
    }
    assert!(scene(Mat4::IDENTITY).geometry_world(1).is_err());
}

#[test]
fn absent_attributes_stay_absent_and_zero_normals_stay_zero() {
    let mut map = scene(Mat4::IDENTITY);
    map.vertex_descriptions[0]
        .elements
        .retain(|e| e.name == ElementName::Position);
    map.vertex_buffers[0].data = triangle()
        .positions
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect();
    let geometry = map.geometry_world(0).unwrap();
    assert!(geometry.normals.is_empty() && geometry.uvs.is_empty());
    let mut geometry = triangle();
    geometry.normals.fill(0.0);
    let mut map = scene(Mat4::IDENTITY);
    map.replace_geometry(0, &geometry).unwrap();
    assert_eq!(map.geometry_world(0).unwrap().normals, geometry.normals);
}

#[test]
fn split_half_float_streams_decode_before_transformation() {
    let mut map = scene(Mat4::from_translation(Vec3::new(2.0, 3.0, 4.0)));
    map.vertex_descriptions.clear();
    map.vertex_buffers.clear();
    for (name, format, values) in [
        (
            ElementName::Position,
            ElementFormat::XyzFloat32,
            triangle().positions,
        ),
        (
            ElementName::Normal,
            ElementFormat::XyzPacked161616,
            triangle().normals,
        ),
        (
            ElementName::Texcoord0,
            ElementFormat::XyPacked1616,
            triangle().uvs,
        ),
    ] {
        let description = VertexDescription {
            usage: VertexUsage::Static,
            elements: vec![VertexElement { name, format }],
        };
        let mut data = vec![0; 3 * format.byte_size()];
        description
            .write_attribute(&mut data, name, &values)
            .unwrap();
        map.vertex_descriptions.push(description);
        map.vertex_buffers.push(VertexBuffer { layer: 0, data });
    }
    map.models[0].vertex_buffer_ids = vec![0, 1, 2];
    let geometry = map.geometry_world(0).unwrap();
    assert_eq!(geometry.positions[..3], [2.0, 3.0, 4.0]);
    assert_eq!(geometry.uvs, triangle().uvs);
    assert_normal_matches_face(&geometry);
}

#[test]
fn malformed_geometry_and_non_finite_attributes_fail() {
    let mut map = scene(Mat4::IDENTITY);
    map.models[0].vertex_count = 4;
    assert!(map.geometry_world(0).is_err());
    map.models[0].vertex_count = 3;
    map.vertex_buffers[0].data[..4].copy_from_slice(&f32::INFINITY.to_le_bytes());
    assert!(map.geometry_world(0).is_err());
}
