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

## Gap analysis vs cdragon-rman / C#

Comparing the prior rs_rman against cdragon-rman (the Rust oracle) left two real parity
gaps, both now closed:

- **Flags table.** cdragon reads the manifest's second body table — a list of
  `FileFlagEntry { id: u8, flag: &str }` locale/platform tags — and models each file's
  optional `u64` flag mask (file field 4), with `FileFlagSet::iter` filtering tags by
  `mask & (1 << id)`. rs_rman discarded that offset (`_flags_off`) and the file mask.
- **Per-chunk bundle ranges.** cdragon assigns each chunk a running `bundle_offset`
  (cumulative compressed size within its bundle) and exposes `bundle_chunks()` plus
  `FileEntry::bundle_chunks`, the byte ranges needed to download and rebuild a file.
  rs_rman parsed chunk ids and sizes but computed no offsets.

(The C# LeagueToolkit has no RMAN/manifest reader, so cdragon-rman is the sole oracle.)

A subtle layout detail confirmed against cdragon during this pass: **flag-table entries do
not use the indexed vtable** that bundle/file/directory entries use. Their body is fixed —
`i32` vtable pointer, three reserved bytes, the `id` (`u8`) at entry offset 7, then a
self-relative `i32` to the length-prefixed name at offset 8. A first vtable-based attempt
parsed the synthetic fixture but failed on every real manifest with a negative-offset
error; switching to the fixed layout (as cdragon does) parses all three.

## What I implemented

1. **Flags table** (`FileFlag { id, name }`, `Rman::file_flags`). Parsed from the second
   body offset with the fixed entry layout above. On the real samples this yields 30 / 29 /
   27 tags — the full locale set (`ar_AE` … `zh_TW`) plus `windows` / `macos`.
2. **Per-file flag mask** (`FileEntry::flags_mask: Option<u64>`, file field 4) and helpers
   `Rman::file_flag_names(file)` and `Rman::files_with_flag(tag)` to resolve a file's tags
   and filter files by a locale/platform tag.
3. **Per-chunk byte ranges** (`ChunkRange { bundle_id, chunk_id, offset_in_bundle,
   compressed_size, uncompressed_size }`). `Rman::chunk_index()` builds the chunk→range
   lookup once (running compressed offset per bundle); `Rman::file_chunks(file)` and
   `Rman::file_chunks_for(file, &index)` return a file's ordered chunk ranges.
4. **Validation** (`tests/real_files.rs`). On all three manifests: the flags table parses
   with sane non-empty names and in-range ids; `files_with_flag` matches a brute-force mask
   count; and `file_chunks` is verified on **every** file (4706 / 4714 / 190) — order
   preserved, each range contiguous within its bundle, and the uncompressed sizes summing
   exactly to the declared file size.

No body-walk bug was found in the existing bundle/file/directory path; it agrees with
cdragon field-for-field. The one correctness fix this pass was the flag-entry layout
(fixed-offset, not vtable), caught precisely because the new test runs against real files.

## Remaining gaps

1. **Writing support.** Emit a byte-faithful FlatBuffer body + header so manifests can
   be round-tripped. The reader is eager and owned, so a writer would re-emit the vtable
   tables (bundles / flags / files / directories) and re-zstd the body. Large; deferred.
   Until then `to_writer` returns `Unsupported`. This is the headline missing piece versus
   the workspace's lossless round-trip contract.
2. **Actual extraction / download.** `file_chunks` gives the byte ranges, but fetching
   bundle bytes from the CDN and zstd-decompressing each chunk into the target file is a
   separate concern (network + a per-chunk frame decode) that belongs above this crate.
3. **Unmodelled file fields.** Fields 5, 6, 8, 10, 11 are still unread (cdragon documents
   only 11 as "set to 1 for localized WADs"). None are needed for paths, flags, or chunk
   ranges, but a future writer must preserve them for a lossless round-trip.

## What I changed (this pass)

- **rman.rs:** added `FileFlag { id, name }`, `ChunkRange { … }`,
  `FileEntry::flags_mask`, `Rman::file_flags`, and methods `file_flag_names`,
  `files_with_flag`, `chunk_index`, `file_chunks`, `file_chunks_for`.
- **read.rs:** parse the flags table from the second body offset (fixed entry layout,
  not vtable) and read the file's flag mask (field 4). Added a `read_u8` cursor primitive.
- **tests/read.rs:** extended the synthetic body with a flag entry and a file flag mask;
  assert the flag table, `file_flag_names`, `files_with_flag`, and `file_chunks`.
- **tests/real_files.rs:** added `verify_flags` and `verify_file_chunks`, validating the
  flags table and the chunk byte-ranges (contiguity + size-sum) on every file.

### Earlier pass (context)

- **read.rs:** widened the version gate from `(2, 0)` to any `major == 2`; the real
  manifests are `2.1` with an identical body layout, and the exact minor is preserved.
- **tests/read.rs:** swapped `rejects_unsupported_version` for
  `rejects_unsupported_major_version` and added `accepts_minor_version_two_one`.

No source outside `rs_rman` was touched. `cargo test -p rs_rman` and
`cargo clippy -p rs_rman --all-targets -- -D warnings` are both green.
