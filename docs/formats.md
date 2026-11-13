# Format reference

Detailed support for every format, including versions and known limits. "Byte-exact round-trip" means an unchanged binary read/write reproduces the source bytes for the covered fixtures. Coverage combines synthetic and locally available real files; it is not proof for every version or future game patch. See [testing and CI coverage](testing.md).

## `.bin` — property bin (PROP / PTCH) — `rs_bin`

- **Read / write:** binary, **byte-exact**.
- **Text:** the human-readable `#PROP_text` form is both **parsed** (`from_text`) and **printed** (`to_text`). Text round-trips preserve supported values, but not arbitrary appended binary data or noncanonical PTCH header bytes.
- **Covers:** all value types (scalars, vectors, matrix, colour, string, hash/link/file), both list kinds, ordered maps (duplicate keys preserved), pointer/embed structs, options, null pointers, linked-file lists, and the `PTCH` patches / data-override trailer.
- **Hashing:** field/class/entry names are FNV-1a; resolve to readable names with a `HashMapper`.
- **PTCH:** binary and text override records are supported; applying overrides to a base BIN is not implemented. See [PTCH support and examples](ptch.md).
- **CLI:** `rs_cli read`, `rs_cli transform`, `rs_cli bin convert`, `rs_cli bin diff`.

## `.wad`, `.wad.client` — archive — `rs_wad`

- **Read / write:** **byte-exact** for v2 and v3 (including v3.4); the header signature/checksum trailer is preserved verbatim.
- **Compression:** `None`, `Gzip`, `Zstd`, `Zstd-multi` decompress. `Satellite` chunks reference data outside the archive and return a clear error.
- **Lookup & extract:** `chunk_by_hash` / `chunk_by_path`, a borrowed `chunk_index` for repeated lookups, a bulk extractor with one table scan (parallel decoding behind the `parallel` feature), and `.subchunktoc` parsing for explicit multi-subchunk sizing. Duplicate hashes use the first table entry.
- **Streaming and writes:** `from_reader_toc` supports extraction from a separate reader, but serialization rejects absent payloads. `WadBuilder` builds v3 archives and checks that repeated provider calls return the same content. Decoding rejects incorrect output lengths and incomplete subchunk tables.
- **CLI:** `rs_cli read`, `rs_cli wad list`, `rs_cli wad extract`.

## `.tex`, `.dds` — textures — `rs_tex`

- **`.tex` read / write:** **byte-exact**.
- **Header inspection:** `TextureHeader` reads TEX/DDS dimensions and declared format without reading pixel data, including unknown formats. A mipmapped TEX has no header-level count; `mip_count` is `None`. Metadata success does not validate the payload. See [header inspection](../crates/rs_tex/README.md#header-inspection).
- **Mip validation and extraction:** full parsing checks the exact known mip-chain byte length. `Texture::from_reader_largest_mip` explicitly extracts a single full-resolution image from the end of a 2D TEX; it does not repair or validate lower levels. See [the non-square audit](../crates/rs_tex/docs/mip-layout-audit.md).
- **Unknown TEX inspection:** raw format, depth/reserved, resource, and flag bytes remain available through `TextureHeader`. [Format 23](../crates/rs_tex/docs/format23-audit.md) was observed in 26 identical zero-filled live entries; its pixel meaning is unverified and full parsing/decoding remains unsupported.
- **Decode:** BC1 / BC2 (DDS) / BC3 / BC5 / BC7, ETC1 / ETC2 / ETC2-EAC, RGBA8 / BGRA8 → `image::RgbaImage`. Cubemaps and multi-surface DDS decode all faces. DDS BC5 distinguishes signed and unsigned channels; both produce an opaque normal preview with reconstructed blue.
- **Unverified numeric mapping:** TEX code 21 retains the provisional RGBA16_SNORM interpretation; no independently verified game sample establishes SNORM versus half-float. TEX code 14 retains unsigned BC5. See the [numeric-format audit](../crates/rs_tex/docs/numeric-format-audit.md).
- **Encode:** TEX supports BC1 / BC3 / BC5 (pure-Rust) and BC7, with full mip-chain generation. DDS writing supports BC1 / BC3 or uncompressed BGRA8; BC5/BC7 export is rejected for game compatibility.
- **Limits:** no ETC or RGBA16_SNORM encoder; the DDS writer re-compresses rather than copying original blocks.
- **CLI:** `rs_cli read`, `rs_cli transform`, `rs_cli tex info`, `rs_cli tex decode`, `rs_cli tex encode`.

## `.skn` — skinned mesh — `rs_mesh`

- **Read / write:** majors 0 through 4 (Basic / Colour / Tangent vertex layouts where supported). Submesh ranges, vertex and index buffers, and trailing bytes are preserved; minor versions are retained rather than rejected.
- **CLI:** `rs_cli read` only.

## `.scb` — static mesh — `rs_mesh`

- **Read / write:** **byte-exact**, including the trailing per-face / pivot data that other readers discard.
- **`.sco`** (the old text static-object format) is read-only; the format was removed from the game, so no writer is provided.
- **CLI:** `rs_cli read` only.

## `.skl` — skeleton / rig — `rs_anim`

- **Read:** modern v0 rigs and legacy `r3d2sklt` v1/v2 skeletons.
- **Write:** unchanged files retain their source bytes; edited/new skeletons emit modern v0. There is no legacy encoder. See [skeleton support](../crates/rs_anim/README.md).
- **CLI:** `rs_cli read` only.

## `.anm` — animation — `rs_anim`

- **Read / write:** **byte-exact** for uncompressed v3 / v4 / v5 **and** compressed `r3d2canm`, via faithful preservation of the on-disk sections.
- **Default editing:** after `make_editable()`, normal saves emit uncompressed v4 and preserve the decoded samples.
- **Format selection:** `Animation::export_bytes` / `export_to_path` use the adaptive CANM writer with explicit CANM -> v5 -> v4 fallback, decoded-byte validation, and a report of the actual format and fallback reasons. See [the export API guide](../crates/rs_anim/docs/animation-export.md).
- **V5 export:** explicit `to_v5_bytes`, `write_v5` and `to_v5_path` retain every frame and float32 vectors with 48-bit rotations.
- **Strict compressed export:** explicit `Animation::to_compressed_bytes`, `write_compressed` and `to_compressed_path` methods encode current tracks as `r3d2canm` v1. `CompressionOptions` controls subframe-grid error bounds, including quantization, and optional skeleton model-space checks. See [compressed export](../crates/rs_anim/README.md#compressed-export) for constraints and Python support.
- **CLI:** `rs_cli read` only.

## `.mapgeo` — environment geometry (OEGM) — `rs_mapgeo`

- **Read / write:** **byte-exact** for versions 5, 6, 7, 9, 11, 12, 13, 14, 15, 17, and 18, including the scene graph and planar reflectors.
- **Editing:** `geometry`, `replace_geometry`, `add_model`, and `remove_model` operate on an existing map, including attributes split across streams. Edits append geometry buffers to avoid corrupting shared buffers. Scene graphs and baked data are preserved, not regenerated from edited render geometry.
- **Export geometry:** `geometry_world` applies placement, transforms normals with the inverse transpose, and reverses triangle winding for reflected models. `Geometry::submesh_triangles` provides validated material-range triangles. See [export behavior and limits](../crates/rs_mapgeo/README.md#geometry-for-export). Scene-graph generation and FBX serialization are not provided.
- **Vertex codec:** 16-bit packed components use half-floats; 8-bit packed colors use normalized channels. Half-float reads/writes preserve signed and out-of-unit-range values instead of clamping them to `0..=1`.
- **Edit validation:** missing vertex/index buffer references return errors. `replace_geometry` prepares all vertex streams before appending them, so an encoding failure in a later stream leaves the map unchanged.
- **Geometry preflight:** public `Geometry::validate` checks finite attributes, editable counts, vertex references, and complete triangle ranges. Add/replace/world export share it. Finite values that overflow half-float encoding are rejected before attribute writes, preserving the map on failed replacement.
- **Unsupported:** versions 8, 10, and 16 — these revisions were skipped by Riot and are absent from every reference, so rather than guess a layout they return an unsupported-version error.
- **CLI:** `rs_cli read` only.

## `.stringtable` — string table (RST) — `rs_rst`

- **Read / write:** **byte-exact** for v2–v5 (40-bit key hashes for v2/v3, 38-bit for v4/v5).
- **Keys:** hashed with xxh3-64 over the lowercased key. Legacy pre-v5 encrypted entries are preserved through a round-trip.
- **CLI:** `rs_cli read`, `rs_cli rst list`.

## `.manifest` — release manifest (RMAN) — `rs_rman`

- **Read only.** Release manifests are produced by Riot's servers and are never authored on the client/modding side, so the crate provides no writer.
- **Exposes:** bundles and their chunks, files (name, size, ordered chunk ids, directory, permissions, locale/platform flag mask), directories, the flags table, reconstructed full file paths, and each file's ordered chunk byte-ranges within bundles (the basis for extraction).
- **CLI:** `rs_cli read` only.

## `.wpk`, `.bnk` — audio containers — `rs_audio`

- **Read / write:** **byte-exact** at the container level. `.bnk` preserves every section verbatim (including the `HIRC` event hierarchy), and `.wem` audio is extracted from `DIDX` / `DATA`.
- **Audio:** WEM decoding to playable Ogg/WAV, PCM/Vorbis encoding, replacement and silence helpers are available.
- **Batch replacement:** `Bnk::replace_wems` and `Wpk::replace_wems` validate all requested existing IDs before changing anything. BNK media is rebuilt once; unrelated sections and WPK names/slots survive. This Rust-only operation replaces payloads without adding/removing IDs or validating their codecs.
- **Hierarchy:** supported HIRC objects expose event-to-WEM relationships; unknown/unsupported data stays opaque. This is not a complete Wwise authoring model. See [audio APIs and limits](../crates/rs_audio/README.md).
- **CLI:** `rs_cli read`, `rs_cli audio extract`.

## `.troybin`, `.inibin`, `.cfgbin` — legacy parameters — `rs_troybin`

- **Read / write:** v1 and v2; preserve raw hashes, bucket ordering and stored values. `Inibin` aliases the same model.
- **Editing:** typed v2 lookup/set/insert/remove and name resolution. V1's untyped string body is preserved but not edited through the typed API. Display scaling does not alter raw values.

## `.luabin64` — compiled Lua — `rs_luabin`

- **Read / write:** the supported Lua 5.1 bytecode layout with 64-bit `size_t`, including prototypes, constants, instructions and debug tables.
- **Editing:** constant traversal by `ConstPath`, typed constant edits, and recovered global assignments. No Lua source decompiler or compiler.
