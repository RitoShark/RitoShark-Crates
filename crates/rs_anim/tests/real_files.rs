use std::path::{Path, PathBuf};

use rs_anim::Animation;
use rs_io::{Parse, Serialize};

fn sample_dir() -> Option<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../sample-files");
    dir.is_dir().then_some(dir)
}

const UNCOMPRESSED_ANM_FILES: &[&str] = &[
    "aatrox__skin07_ult_attack1.anm",
    "aatrox_sheath_run_haste.anm",
    "dance_windup.anm",
];

const COMPRESSED_ANM_FILES: &[&str] = &[
    "compressed_507c1f34b053b389.anm",
    "compressed_e890878834c561be.anm",
    "compressed_e63f4f2e8c074937.anm",
];

fn magic(path: &Path) -> [u8; 8] {
    let bytes = std::fs::read(path).expect("read sample bytes");
    let mut magic = [0u8; 8];
    magic.copy_from_slice(&bytes[..8]);
    magic
}

/// Parses, round-trips byte-for-byte, and sanity-checks every decoded pose: rotations must be
/// unit-length and every translation/scale channel finite. Applies to both containers.
fn check_anm(name: &str, path: &Path, expected_magic: &[u8; 8]) {
    assert_eq!(
        &magic(path),
        expected_magic,
        "{name}: unexpected container magic"
    );

    let anim = Animation::from_path(path).expect("parse anm");
    assert!(anim.is_byte_exact(), "{name}: source bytes not preserved");
    assert!(
        !anim.tracks().is_empty(),
        "{name}: expected at least one track"
    );
    for track in anim.tracks() {
        assert!(
            !track.frames.is_empty(),
            "{name}: track {:#010x} has no frames",
            track.joint_hash
        );
        for frame in &track.frames {
            let q = frame.rotation;
            let len = (q.x * q.x + q.y * q.y + q.z * q.z + q.w * q.w).sqrt();
            assert!(
                len.is_finite() && (len - 1.0).abs() < 1e-3,
                "{name}: non-unit rotation {q:?} in track {:#010x}",
                track.joint_hash
            );
            for c in [
                frame.translation.x,
                frame.translation.y,
                frame.translation.z,
                frame.scale.x,
                frame.scale.y,
                frame.scale.z,
            ] {
                assert!(c.is_finite(), "{name}: non-finite vector channel {c}");
            }
        }
    }

    let original = std::fs::read(path).expect("read sample bytes");
    let written = anim.to_bytes().expect("write parsed anm");
    assert!(written == original, "{name}: round-trip is not byte-exact");

    let reparsed = Animation::from_bytes(&written).expect("re-read written anm");
    assert_eq!(
        anim.tracks().len(),
        reparsed.tracks().len(),
        "{name}: track count changed across round-trip"
    );

    eprintln!(
        "{name}: parsed {} tracks, fps {:.3}",
        anim.tracks().len(),
        anim.fps
    );
}

#[test]
fn anm_real_files_parse() {
    let Some(dir) = sample_dir() else {
        eprintln!("sample-files directory missing; skipping real .anm tests");
        return;
    };

    for name in UNCOMPRESSED_ANM_FILES {
        let path = dir.join(name);
        if !path.is_file() {
            eprintln!("missing sample {name}; skipping");
            continue;
        }
        check_anm(name, &path, b"r3d2anmd");
    }

    for name in COMPRESSED_ANM_FILES {
        let path = dir.join(name);
        if !path.is_file() {
            eprintln!("missing sample {name}; skipping");
            continue;
        }
        check_anm(name, &path, b"r3d2canm");
    }
}
