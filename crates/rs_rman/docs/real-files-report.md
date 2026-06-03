# rs_rman — real-files report

Results of running `Rman::from_path` over the real `.manifest` samples in
`sample-files/`, plus a cross-check of our body walk against the cdragon-rman
reference (Rust). Run with:

```
cargo test -p rs_rman --test real_files -- --nocapture
```

## Per-file results

All three manifests are RMAN **major 2, minor 1**, flags `0x0200` (bit 9 set), body
offset 28, body zstd-compressed. In every case the parsed `manifest_id` equals the
filename hash — a strong correctness signal that the header was decoded correctly.

### 7D6C65378829C6AA.manifest (~16 MB)

- version `(2, 1)`, flags `0x0200`, manifest_id `0x7d6c65378829c6aa`
- bundles **2062**, chunks **578285**, files **4706**, directories **8**
- parse result: **OK**
- sample paths:
  - `DATA/FINAL/Champions/Aatrox.ar_AE.wad.client` (125182236 bytes)
  - `DATA/FINAL/Champions/Aatrox.cs_CZ.wad.client` (112072055 bytes)
  - `DATA/FINAL/Champions/Aatrox.de_DE.wad.client` (111680672 bytes)

### DAFB5FDD5647079F.manifest (~16 MB)

- version `(2, 1)`, flags `0x0200`, manifest_id `0xdafb5fdd5647079f`
- bundles **2046**, chunks **576913**, files **4714**, directories **8**
- parse result: **OK**
- sample paths:
  - `D3DCompiler_47.dll` (4481992 bytes)  — file with no directory, bare basename
  - `DATA/FINAL/Bootstrap.windows.wad.client` (51919439 bytes)
  - `DATA/FINAL/Champions/Aatrox.ar_AE.wad.client` (125182236 bytes)

### F8FBA48750270222.manifest (~2 MB)

- version `(2, 1)`, flags `0x0200`, manifest_id `0xf8fba48750270222`
- bundles **332**, chunks **100213**, files **190**, directories **46**
- parse result: **OK**
- sample paths:
  - `LeagueClient.exe` (30532544 bytes)
  - `LeagueClientUx.exe` (4597696 bytes)
  - `LeagueClientUxRender.exe` (3645368 bytes)

`file_paths()` returns exactly one entry per file in every manifest, with no empty
paths, and the directory chains reconstruct sensible release layouts (`DATA/FINAL/...`
for game data; bare basenames and a deeper tree for the client install).

## Comparison vs cdragon-rman (Rust reference)

Our body walk was diffed field-by-field against cdragon-rman's parser. They agree on
every load-bearing detail:

- **Header.** Same 28-byte layout (magic, major, minor, flags, offset, compressed
  size, manifest id, uncompressed size) and the same "skip bytes between header and
  body when offset > header length" rule.
- **Body header.** Both read an `i32` length, skip it, then read four self-relative
  table offsets in order bundles / flags / files / directories.
- **Offset tables.** Both read a `u32` count followed by that many self-relative entry
  offsets, then resolve each entry's vtable.
- **vtable resolution.** cdragon computes `fields = entry - i32 + 4` (skipping two
  `u16` headers); ours computes `vtable = entry - i32 + 4`. Field slot = `u16` at
  `vtable + 2*field`, `0` meaning absent. Identical.
- **String / offset fields.** cdragon's `get_offset_cursor` resolves to
  `entry + vtable_slot + stored_i32`; ours resolves `field_at = entry + vtable_slot`
  then adds the stored `i32` via `read_offset`. Algebraically identical — verified to
  produce the same absolute target.
- **Field indices.** Bundle `{0:id, 1:chunks}`, chunk `{0:id, 1:compressed,
  2:uncompressed}`, file `{0:id, 1:dir, 2:size, 3:name, 7:chunks, 9:link, 12:type}`,
  directory `{0:id, 1:parent, 2:name}` — all match cdragon's documented offsets.
- **Path reconstruction.** Both join basename onto the parent chain with `/`; files
  with no directory id keep the bare name. Verified against real output
  (`D3DCompiler_47.dll` has no directory and stays a bare name, exactly as cdragon
  would emit).

**Key divergences (intentional, in our favour):**

1. **Version gate.** cdragon-rman hard-rejects anything that is not exactly `(2, 0)`,
   so it would **fail to open all three real samples** (they are `2.1`). We accept any
   major-2 manifest because the body format is unchanged across minor versions, and we
   record the true minor. This is the single most important real-world finding.
2. **No panics.** cdragon's parser panics on malformed offsets/data (it says so in its
   own docs). Ours is fully bounds-checked: every slice is range-validated and every
   offset arithmetic is checked, so malformed input is an `Err`, never a panic — as
   required by the workspace golden rules.
3. **Eager + owned.** cdragon parses lazily on each `iter_*` call and borrows from the
   body. We parse once into owned `Vec`s, which is friendlier for tooling that holds
   the data; the price is up-front allocation, paid with pre-sized vectors.

**Not yet surfaced (parity gap, low priority):** cdragon exposes the **flags table**
(locale/platform codes such as `en_US`, `macos`) and each file's flag bitmask, plus
per-chunk **bundle offsets** and the bundle/target byte ranges needed to actually
download and reassemble a file. We parse bundles, chunks, files and directories but do
not yet expose the flags table or compute download ranges.

## Improvements / TODO

1. **Expose the flags table and per-file flag mask** (file field 4). Needed to filter
   which files belong to a given locale/platform — a real use case for release tooling.
   cdragon already models this (`FileFlagEntry`, `FileFlagSet`).
2. **Compute chunk download/target ranges.** Track each chunk's running offset within
   its bundle and expose file → (bundle ranges, target ranges), mirroring cdragon's
   `bundle_chunks` / `FileEntry::bundle_chunks`. This is the gateway to extraction.
3. **Writing support.** Emit a byte-faithful FlatBuffer body + header so manifests can
   be round-tripped. Large; deferred. Until then `to_writer` returns `Unsupported`.

## What I changed

- **read.rs:** widened the version gate from `(major, minor) == (2, 0)` to
  `major == 2` (any minor). The real game manifests are `2.1` and share the identical
  body layout; the old gate rejected every real file. The exact minor is still stored
  in `Rman::version`.
- **tests/real_files.rs (new):** skip-if-missing harness that parses each real
  manifest, asserts major version 2 and non-empty bundles/files/directories, checks
  `file_paths()` returns one non-empty entry per file, and prints counts plus three
  sample paths per file.
- **tests/read.rs:** replaced the now-obsolete `rejects_unsupported_version` (which
  expected `2.1` to be rejected) with `rejects_unsupported_major_version` (rejects
  major 1) and added `accepts_minor_version_two_one` to lock in the new behaviour.

No source outside `rs_rman` was touched. `cargo test -p rs_rman` and
`cargo clippy -p rs_rman --all-targets -- -D warnings` are both green.
