/*!
Editing tests over the real map files in `Sample-Files/`. They are skipped when a file is absent so
the suite still passes on a clean checkout, since game assets are never committed.
*/

use rs_io::{Parse, Serialize};
use rs_mapgeo::{ElementName, Geometry, MapGeometry, SubmeshRange};
use rs_math::Mat4;

fn sample(name: &str) -> Option<MapGeometry> {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../Sample-Files/").to_string() + name;
    let data = std::fs::read(path).ok()?;
    Some(MapGeometry::from_bytes(&data).expect("sample file should parse"))
}

const SAMPLES: [&str; 3] = [
    "bloom.mapgeo",
    "ultbook.mapgeo",
    "spectator_only_banners.mapgeo",
];

fn triangle() -> Geometry {
    Geometry::single(
        "test_material",
        vec![0.0, 0.0, 0.0, 100.0, 0.0, 0.0, 0.0, 100.0, 0.0],
        vec![0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0],
        vec![0.0, 0.0, 1.0, 0.0, 0.0, 1.0],
        vec![0, 1, 2],
    )
}

#[test]
fn untouched_file_still_round_trips() {
    for name in SAMPLES {
        let Some(map) = sample(name) else { continue };
        let original = std::fs::read(
            concat!(env!("CARGO_MANIFEST_DIR"), "/../../Sample-Files/").to_string() + name,
        )
        .unwrap();
        assert_eq!(map.to_bytes().unwrap(), original, "{name} round-trip");
    }
}

#[test]
fn edited_file_still_parses_and_keeps_everything_else() {
    for name in SAMPLES {
        let Some(mut map) = sample(name) else {
            continue;
        };
        let scene_graphs = map.scene_graphs.clone();
        let reflectors = map.planar_reflectors.clone();
        let model_count = map.models.len();
        let untouched = map.models[model_count - 1].clone();

        map.replace_geometry(0, &triangle()).unwrap();

        let bytes = map.to_bytes().unwrap();
        let reparsed = MapGeometry::from_bytes(&bytes).unwrap();

        assert_eq!(reparsed.version, map.version, "{name} version");
        assert_eq!(reparsed.models.len(), model_count, "{name} model count");
        assert_eq!(reparsed.scene_graphs, scene_graphs, "{name} scene graphs");
        assert_eq!(reparsed.planar_reflectors, reflectors, "{name} reflectors");
        assert_eq!(
            reparsed.models[model_count - 1],
            untouched,
            "{name} untouched model"
        );

        let edited = &reparsed.models[0];
        assert_eq!(edited.vertex_count, 3, "{name} edited vertex count");
        assert_eq!(edited.index_count, 3, "{name} edited index count");
        assert_eq!(edited.submeshes.len(), 1, "{name} edited submesh count");
        assert_eq!(
            edited.submeshes[0].name, "test_material",
            "{name} submesh name"
        );
    }
}

#[test]
fn replaced_geometry_reads_back_with_the_positions_written() {
    for name in SAMPLES {
        let Some(mut map) = sample(name) else {
            continue;
        };
        let source = triangle();
        map.replace_geometry(0, &source).unwrap();

        let bytes = map.to_bytes().unwrap();
        let reparsed = MapGeometry::from_bytes(&bytes).unwrap();
        let read_back = reparsed.geometry(0).unwrap();

        assert_eq!(read_back.positions, source.positions, "{name} positions");
        assert_eq!(read_back.indices, source.indices, "{name} indices");
    }
}

#[test]
fn replacing_geometry_leaves_a_model_sharing_the_old_buffer_intact() {
    for name in SAMPLES {
        let Some(mut map) = sample(name) else {
            continue;
        };

        let Some((a, b)) = find_shared_buffer_pair(&map) else {
            continue;
        };
        let before = map.geometry(b).unwrap();
        map.replace_geometry(a, &triangle()).unwrap();
        let after = map.geometry(b).unwrap();

        assert_eq!(before, after, "{name} model {b} sharing a buffer with {a}");
    }
}

fn find_shared_buffer_pair(map: &MapGeometry) -> Option<(usize, usize)> {
    for (i, model) in map.models.iter().enumerate() {
        for (j, other) in map.models.iter().enumerate().skip(i + 1) {
            let shares_vertices = model
                .vertex_buffer_ids
                .iter()
                .any(|id| other.vertex_buffer_ids.contains(id));
            if shares_vertices && model.vertex_count > 0 && other.vertex_count > 0 {
                return Some((i, j));
            }
        }
    }
    None
}

#[test]
fn added_model_survives_a_round_trip() {
    for name in SAMPLES {
        let Some(mut map) = sample(name) else {
            continue;
        };
        let before = map.models.len();

        let index = map
            .add_model("RitoShark_Test", &triangle(), Mat4::IDENTITY, 255)
            .unwrap();
        assert_eq!(index, before, "{name} new model index");

        let bytes = map.to_bytes().unwrap();
        let reparsed = MapGeometry::from_bytes(&bytes).unwrap();
        assert_eq!(reparsed.models.len(), before + 1, "{name} model count");

        let added = &reparsed.models[index];
        assert_eq!(added.vertex_count, 3, "{name} added vertex count");
        assert_eq!(added.layer, 255, "{name} added layer");
        assert_eq!(added.bounds.max.x, 100.0, "{name} added bounds");

        let geometry = reparsed.geometry(index).unwrap();
        assert_eq!(geometry.positions, triangle().positions, "{name} positions");
        assert_eq!(geometry.normals, triangle().normals, "{name} normals");
        assert_eq!(geometry.uvs, triangle().uvs, "{name} uvs");
    }
}

#[test]
fn removed_model_is_gone_after_a_round_trip() {
    for name in SAMPLES {
        let Some(mut map) = sample(name) else {
            continue;
        };
        let before = map.models.len();
        let second = map.models[1].clone();

        map.remove_model(0).unwrap();

        let bytes = map.to_bytes().unwrap();
        let reparsed = MapGeometry::from_bytes(&bytes).unwrap();
        assert_eq!(reparsed.models.len(), before - 1, "{name} model count");

        /* Versions >= 12 do not store the name: it is generated from the model's position, so the
        model that shifted into slot 0 is read back as MapGeo_Instance_0 rather than under the name
        it had before. Every other field must survive untouched. */
        let mut shifted = reparsed.models[0].clone();
        if map.version >= 12 {
            shifted.name = second.name.clone();
        }
        assert_eq!(shifted, second, "{name} first model shifted up");
    }
}

#[test]
fn every_model_reads_back_the_positions_the_raw_buffer_holds() {
    for name in SAMPLES {
        let Some(map) = sample(name) else { continue };
        let mut checked = 0;
        for (i, model) in map.models.iter().enumerate() {
            let geometry = map.geometry(i).unwrap();
            if geometry.positions.is_empty() {
                continue;
            }
            assert_eq!(
                geometry.positions.len(),
                model.vertex_count as usize * 3,
                "{name} model {i} position count"
            );
            assert!(
                geometry.positions.iter().all(|v| v.is_finite()),
                "{name} model {i} has non-finite positions"
            );
            checked += 1;
        }
        assert!(checked > 0, "{name} decoded no model geometry at all");
    }
}

/** A model stores one vertex_description_id but may reference several buffers, and the descriptions
are consumed consecutively from it rather than the stored one describing them all. Reading every
buffer through the first description silently drops whichever attributes live in a later stream, and
nothing else in this suite notices because positions are always in the first one. */
#[test]
fn multi_buffer_models_decode_the_attributes_in_their_later_streams() {
    for name in SAMPLES {
        let Some(map) = sample(name) else { continue };

        let mut multi = 0;
        let mut complete = 0;
        for (i, model) in map.models.iter().enumerate() {
            if model.vertex_buffer_ids.len() < 2 || model.vertex_count == 0 {
                continue;
            }
            multi += 1;

            let declared: Vec<_> = (0..model.vertex_buffer_ids.len())
                .filter_map(|offset| {
                    map.vertex_descriptions
                        .get(model.vertex_description_id as usize + offset)
                })
                .flat_map(|description| description.elements.iter().map(|e| e.name))
                .collect();
            let geometry = map.geometry(i).unwrap();
            let vertex_count = model.vertex_count as usize;

            for (name_of, values, per_vertex) in [
                (ElementName::Position, &geometry.positions, 3),
                (ElementName::Normal, &geometry.normals, 3),
                (ElementName::Texcoord0, &geometry.uvs, 2),
            ] {
                if declared.contains(&name_of) {
                    assert_eq!(
                        values.len(),
                        vertex_count * per_vertex,
                        "{name} model {i} declares {name_of:?} across its streams but decoded {} \
                         floats for {vertex_count} vertices",
                        values.len()
                    );
                }
            }
            complete += 1;
        }
        assert!(
            multi == 0 || complete == multi,
            "{name}: {complete} of {multi} multi-buffer models decoded completely"
        );
    }
}

#[test]
fn geometry_with_a_bad_index_is_rejected() {
    let Some(mut map) = sample("bloom.mapgeo") else {
        return;
    };
    let mut bad = triangle();
    bad.indices = vec![0, 1, 9];
    assert!(map.replace_geometry(0, &bad).is_err());
}

#[test]
fn geometry_with_a_submesh_past_the_indices_is_rejected() {
    let Some(mut map) = sample("bloom.mapgeo") else {
        return;
    };
    let mut bad = triangle();
    bad.submeshes = vec![SubmeshRange {
        name: "too_long".to_string(),
        index_start: 0,
        index_count: 12,
    }];
    assert!(map.replace_geometry(0, &bad).is_err());
}

#[test]
fn geometry_with_mismatched_normals_is_rejected() {
    let Some(mut map) = sample("bloom.mapgeo") else {
        return;
    };
    let mut bad = triangle();
    bad.normals = vec![0.0, 1.0, 0.0];
    assert!(map.replace_geometry(0, &bad).is_err());
}
