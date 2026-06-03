use std::path::{Path, PathBuf};

use rs_anim::{Animation, Error};
use rs_io::{Parse, Serialize};

fn sample_dir() -> Option<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../sample-files");
    dir.is_dir().then_some(dir)
}

const ANM_FILES: &[&str] = &[
    "aatrox__skin07_ult_attack1.anm",
    "aatrox_sheath_run_haste.anm",
    "dance_windup.anm",
];

fn magic_version(path: &Path) -> ([u8; 8], u32) {
    let bytes = std::fs::read(path).expect("read sample bytes");
    let mut magic = [0u8; 8];
    magic.copy_from_slice(&bytes[..8]);
    let version = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
    (magic, version)
}

#[test]
fn anm_real_files_parse() {
    let Some(dir) = sample_dir() else {
        eprintln!("sample-files directory missing; skipping real .anm tests");
        return;
    };

    for name in ANM_FILES {
        let path = dir.join(name);
        if !path.is_file() {
            eprintln!("missing sample {name}; skipping");
            continue;
        }

        let (magic, version) = magic_version(&path);
        eprintln!(
            "{name}: magic={:?} version={version}",
            std::str::from_utf8(&magic)
        );

        match Animation::from_path(&path) {
            Ok(anim) => {
                assert_eq!(
                    &magic, b"r3d2anmd",
                    "{name}: only uncompressed anm should parse successfully"
                );
                assert!(
                    !anim.tracks().is_empty(),
                    "{name}: expected at least one track"
                );
                assert!(
                    anim.frame_count() > 0,
                    "{name}: expected at least one frame"
                );
                for track in anim.tracks() {
                    assert!(
                        !track.frames.is_empty(),
                        "{name}: track {:#010x} has no frames",
                        track.joint_hash
                    );
                }
                eprintln!(
                    "{name}: parsed {} tracks x {} frames",
                    anim.tracks().len(),
                    anim.frame_count()
                );

                // Round-trip: writer emits v4 (full quaternions), so re-reading yields the same
                // poses. The frame palette is rebuilt, so we compare parsed structures, not bytes.
                let bytes = anim.to_bytes().expect("write parsed anm");
                let reparsed = Animation::from_bytes(&bytes).expect("re-read written anm");
                assert_eq!(
                    anim.tracks().len(),
                    reparsed.tracks().len(),
                    "{name}: track count changed across round-trip"
                );
                for original in anim.tracks() {
                    let got = reparsed
                        .tracks()
                        .iter()
                        .find(|t| t.joint_hash == original.joint_hash)
                        .unwrap_or_else(|| {
                            panic!(
                                "{name}: track {:#010x} missing after round-trip",
                                original.joint_hash
                            )
                        });
                    assert_eq!(
                        original.frames.len(),
                        got.frames.len(),
                        "{name}: frame count changed for track {:#010x}",
                        original.joint_hash
                    );
                }
            }
            Err(Error::Unsupported(msg)) => {
                assert_eq!(
                    &magic, b"r3d2canm",
                    "{name}: only compressed anm is expected to be Unsupported (got {msg})"
                );
                eprintln!("{name}: compressed (r3d2canm) — Unsupported as expected");
            }
            Err(e) => panic!("{name}: unexpected error: {e}"),
        }
    }
}
