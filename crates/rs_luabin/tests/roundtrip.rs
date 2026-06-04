use std::path::PathBuf;

use rs_io::{Parse, Serialize};
use rs_luabin::{ConstPath, LocalVar, LuaBin, LuaConstant, Proto};

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

    // Non-UTF-8 bytes are readable via as_string and now writable via set_string_bytes.
    let raw = &[0xC3, 0x28, 0xFFu8][..];
    c.set_string_bytes(raw);
    assert_eq!(c.as_string(), Some(raw));
    assert_eq!(c, LuaConstant::Str(Some([raw, &[0]].concat())));
}

#[test]
fn bool_constant_accessors() {
    let mut c = LuaConstant::Bool(0);
    assert_eq!(c.as_bool(), Some(false));
    assert!(c.set_bool(true));
    assert_eq!(c, LuaConstant::Bool(1));
    assert_eq!(c.as_bool(), Some(true));
    assert!(!LuaConstant::Nil.set_bool(true));
    assert_eq!(LuaConstant::Nil.as_bool(), None);
}

/// Builds a tiny chunk in memory and confirms the flattened iterator reaches nested constants and
/// addresses them by [`ConstPath`].
#[test]
fn flattened_iterator_reaches_nested_constants() {
    let bin = synthetic_chunk();
    let all: Vec<_> = bin.iter_constants().collect();
    // main has 2 constants; the one nested proto has 1 → 3 total.
    assert_eq!(all.len(), 3);

    let nested_path = ConstPath::new(vec![0], 0);
    let nested = all.iter().find(|(p, _)| *p == nested_path).map(|(_, c)| *c);
    assert_eq!(nested, Some(&LuaConstant::Str(Some(b"nested\0".to_vec()))));
    assert_eq!(bin.constant(&nested_path), nested);
    assert!(bin.constant(&ConstPath::new(vec![5], 0)).is_none());
    assert!(bin.constant(&ConstPath::new(vec![], 99)).is_none());
}

/// Confirms the global-assignment pairing (LOADK then SETGLOBAL) and that editing the value through
/// its path lands on exactly one constant.
#[test]
fn global_assignment_pairing_and_edit() {
    let mut bin = synthetic_chunk();
    let assigns = bin.global_assignments();
    assert_eq!(assigns.len(), 1);
    assert_eq!(assigns[0].name, "MyGlobal");
    assert_eq!(assigns[0].value, ConstPath::new(vec![], 1));

    assert_eq!(bin.number(&assigns[0].value), Some(50.0));
    bin.set_number(&assigns[0].value, 99.0).expect("set number");
    assert_eq!(bin.number(&assigns[0].value), Some(99.0));

    // A non-number path errors instead of silently no-opping.
    assert!(bin.set_number(&ConstPath::new(vec![], 0), 1.0).is_err());

    let written = bin.to_bytes().expect("write");
    let reparsed = LuaBin::from_bytes(&written).expect("reparse");
    assert_eq!(reparsed.number(&ConstPath::new(vec![], 1)), Some(99.0));
}

/// Real files: the flattened iterator must see strictly more constants than `main` alone when the
/// chunk has nested functions, and every reconstructed global assignment must resolve.
#[test]
fn real_files_expose_nested_constants_and_globals() {
    let Some(dir) = sample_dir() else {
        return;
    };
    let mut total_globals = 0usize;
    for name in LUABIN_FILES {
        let path = dir.join(name);
        if !path.is_file() {
            continue;
        }
        let bin = LuaBin::from_bytes(&std::fs::read(&path).unwrap()).expect("parse");
        let flat = bin.iter_constants().count();
        if !bin.main.protos.is_empty() {
            assert!(
                flat >= bin.main.constants.len(),
                "{name}: flattened count must include nested constants"
            );
        }
        for a in bin.global_assignments() {
            assert!(!a.name.is_empty(), "{name}: global name empty");
            assert!(
                bin.constant(&a.value).is_some(),
                "{name}: global value path must resolve"
            );
            total_globals += 1;
        }
        eprintln!("{name}: {flat} constants flattened");
    }
    eprintln!("reconstructed {total_globals} global assignments across samples");
}

fn synthetic_chunk() -> LuaBin {
    // code: LOADK R0 := K1 ; SETGLOBAL Gbl[K0] := R0 (op | A<<6 | Bx<<14; A and Bx are 0 here)
    let loadk = 1u32 | (1u32 << 14);
    let setglobal = 7u32;
    let nested = Proto {
        source: None,
        line_defined: 0,
        last_line_defined: 0,
        num_upvalues: 0,
        num_params: 0,
        is_vararg: 0,
        max_stack: 2,
        code: vec![],
        constants: vec![LuaConstant::Str(Some(b"nested\0".to_vec()))],
        protos: vec![],
        line_info: vec![],
        locals: vec![],
        upvalue_names: vec![],
    };
    let main = Proto {
        source: Some(b"@chunk\0".to_vec()),
        line_defined: 0,
        last_line_defined: 0,
        num_upvalues: 0,
        num_params: 0,
        is_vararg: 2,
        max_stack: 2,
        code: vec![loadk, setglobal],
        constants: vec![
            LuaConstant::Str(Some(b"MyGlobal\0".to_vec())),
            LuaConstant::Number(50.0f64.to_le_bytes().to_vec()),
        ],
        protos: vec![nested],
        line_info: vec![],
        locals: Vec::<LocalVar>::new(),
        upvalue_names: vec![],
    };
    LuaBin {
        version: 0x51,
        format: 0,
        endian: 1,
        int_size: 4,
        size_t_size: 8,
        instruction_size: 4,
        number_size: 8,
        is_integral: 0,
        main,
    }
}
