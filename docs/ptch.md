# PTCH files

`rs_bin` already reads and writes PTCH files through the same `Bin`, `Parse`, and
`Serialize` APIs as PROP files. No separate crate or patch framework is needed.
Detection uses the `PTCH` magic, not the filename extension.

## What is supported

- The eight bytes after `PTCH` are retained in `Bin::patch_header`.
- The embedded PROP document retains its version, linked files, entries, and fields.
- The ordered trailing overrides are exposed as `Bin::patches: Vec<BinPatch>`.
  Each record has a `key_hash`, a dotted `path`, and a typed replacement `value`.
- Binary parsing and writing preserve these records. The text API reads and prints
  `#PTCH_text` and its `patches: map[hash,embed]` section.

The text representation does **not** carry the raw eight-byte header or arbitrary
binary trailing data. Text parsing reconstructs the canonical header
`[1, 0, 0, 0, 0, 0, 0, 0]`. Keep the binary model for lossless preservation of
noncanonical headers or appended data; text conversion is not universally byte-exact.

## Read, inspect, and save

```rust,no_run
use rs_bin::Bin;
use rs_io::{Parse, Serialize};

let patch = Bin::from_path("example.ptch.bin")?;
assert!(patch.is_patch);
for record in &patch.patches {
    println!("{:08x}: {} = {:?}", record.key_hash, record.path, record.value);
}
patch.to_path("copy.ptch.bin")?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

A new document can be built with the existing public model:

```rust
use rs_bin::{Bin, BinPatch, BinValue};
use rs_io::Serialize;

let mut patch = Bin::new(); // PROP version 3
patch.is_patch = true;
patch.patch_header = [1, 0, 0, 0, 0, 0, 0, 0];
patch.patches.push(BinPatch {
    key_hash: 0xCAFE_BABE, // illustrative; use the actual target key
    path: "a.b".into(),
    value: BinValue::U32(42),
});
let bytes = patch.to_bytes()?;
assert_eq!(&bytes[..4], b"PTCH");
# Ok::<(), rs_bin::Error>(())
```

## Limits and future use

These APIs inspect, edit, and serialize patch records. They do not resolve linked
files, locate targets in another BIN, apply dotted paths, validate against a Riot
schema, or prove the game accepts a newly constructed override. Unknown raw header
bytes are preserved without assigning them speculative meanings.

PTCH could support future override inspection or focused editing without rewriting
an entire base document. Add an apply operation only when a tool needs it and real
fixtures establish path resolution, missing-target behavior, and type compatibility.
Keep generic BIN visitor/path/mutation APIs lower priority; ordinary field edits
already work through the public model. Tool-specific schema and migration policy
can stay in the consuming tools until there is a shared use case.

## Validation

`crates/rs_bin/tests/roundtrip.rs` covers raw header preservation and binary/text
round-trips of a canonical PTCH override. `tests/hash_width.rs` exercises patch
records with widened value hashes. These are synthetic checks, not a claim of
coverage over a representative real PTCH corpus or in-game patch application.

Run `cargo test -p rs_bin --locked` to exercise the current suite.
