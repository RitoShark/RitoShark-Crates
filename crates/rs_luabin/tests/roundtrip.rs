use std::path::PathBuf;

use rs_io::{Parse, Serialize};
use rs_luabin::{LuaBin, LuaConstant};

fn sample_dir() -> Option<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../sample-files");
    dir.is_dir().then_some(dir)
}

const LUABIN_FILES: &[&str] = &[
    "electrocute.luabin64",
    "perksglobalbuff.luabin64",
    "charscriptazirsundisc.luabin64",
];

#[test]
fn real_files_round_trip_byte_exact() {
    let Some(dir) = sample_dir() else {
        eprintln!("sample-files directory missing; skipping real .luabin64 tests");
        return;
    };

    for name in LUABIN_FILES {
        let path = dir.join(name);
        if !path.is_file() {
            eprintln!("missing sample {name}; skipping");
            continue;
        }

        let original = std::fs::read(&path).expect("read sample");
        let parsed = LuaBin::from_bytes(&original).expect("parse luabin");
        assert_eq!(parsed.version, 0x51, "{name}: expected Lua 5.1");
        let written = parsed.to_bytes().expect("write luabin");
        assert!(
            written == original,
            "{name}: round-trip is not byte-exact ({} vs {} bytes)",
            written.len(),
            original.len()
        );
        eprintln!(
            "{name}: {} constants, {} nested protos",
            parsed.main.constants.len(),
            parsed.main.protos.len()
        );
    }
}

/// Patch a single numeric constant in a real file, re-emit, and confirm the result re-parses, only
/// that constant changed, and the byte length is unchanged (a same-width number edit).
#[test]
fn patch_number_constant_changes_only_that_value() {
    let Some(dir) = sample_dir() else {
        return;
    };

    for name in LUABIN_FILES {
        let path = dir.join(name);
        if !path.is_file() {
            continue;
        }
        let original = std::fs::read(&path).expect("read sample");
        let mut bin = LuaBin::from_bytes(&original).expect("parse");

        let Some(idx) = bin
            .main
            .constants
            .iter()
            .position(|c| matches!(c, LuaConstant::Number(_)))
        else {
            continue;
        };

        let before = bin.main.constants[idx].as_f64().expect("number");
        let target = before + 123.5;
        assert!(
            bin.main.constants[idx].set_f64(target),
            "set_f64 on a number"
        );

        let written = bin.to_bytes().expect("write patched");
        assert_eq!(
            written.len(),
            original.len(),
            "{name}: same-width number patch must not change length"
        );

        let reparsed = LuaBin::from_bytes(&written).expect("re-parse patched");
        assert_eq!(
            reparsed.main.constants[idx].as_f64(),
            Some(target),
            "{name}: patched constant must read back as the new value"
        );

        // Every other constant is unchanged.
        for (i, (a, b)) in bin
            .main
            .constants
            .iter()
            .zip(reparsed.main.constants.iter())
            .enumerate()
        {
            if i != idx {
                assert_eq!(a, b, "{name}: constant {i} changed unexpectedly");
            }
        }
        return; // one real file is enough to prove the patch path
    }
}

#[test]
fn bad_signature_is_error() {
    assert!(matches!(
        LuaBin::from_bytes(b"\x00\x00\x00\x00\x51\x00\x01\x04\x08\x04\x08\x00"),
        Err(rs_luabin::Error::InvalidSignature)
    ));
}

#[test]
fn wrong_version_is_error() {
    let mut bytes = b"\x1bLua".to_vec();
    bytes.push(0x52);
    assert!(matches!(
        LuaBin::from_bytes(&bytes),
        Err(rs_luabin::Error::UnsupportedVersion(0x52))
    ));
}

#[test]
fn string_constant_edit_helpers() {
    let mut c = LuaConstant::Str(Some(b"hello\0".to_vec()));
    assert_eq!(c.as_string(), Some(&b"hello"[..]));
    c.set_string("world!");
    assert_eq!(c.as_string(), Some(&b"world!"[..]));
    assert_eq!(c, LuaConstant::Str(Some(b"world!\0".to_vec())));
}
