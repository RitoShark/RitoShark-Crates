# Format reference

Detailed support for every format, including versions and known limits. "Byte-exact round-trip"
means reading a real file and writing it back reproduces the original bytes exactly, verified by
tests against real game files.

---

## `.bin` — property bin (PROP / PTCH) — `rs_bin`

- **Read / write:** binary, **byte-exact**.
- **Text:** the human-readable `#PROP_text` form is both **parsed** (`from_text`) and **printed**
  (`to_text`); `bin → text → bin` is byte-identical.
- **Covers:** all value types (scalars, vectors, matrix, colour, string, hash/link/file), both
  list kinds, ordered maps (duplicate keys preserved), pointer/embed structs, options, null
  pointers, linked-file lists, and the `PTCH` patches / data-override trailer.
- **Hashing:** field/class/entry names are FNV-1a; resolve to readable names with a `HashMapper`.

## `.wad`, `.wad.client` — archive — `rs_wad`

- **Read / write:** **byte-exact** for v2 and v3 (including v3.4); the header signature/checksum
  trailer is preserved verbatim.
- **Compression:** `None`, `Gzip`, `Zstd`, `Zstd-multi` decompress. `Satellite` chunks reference
  data outside the archive and return a clear error.
- **Lookup & extract:** `chunk_by_hash` / `chunk_by_path`, a bulk extractor (parallel behind the
  `parallel` feature), and `.subchunktoc` parsing for explicit multi-subchunk sizing.

## `.tex`, `.dds` — textures — `rs_tex`

- **`.tex` read / write:** **byte-exact**.
- **Decode:** BC1–BC7, ETC1 / ETC2 / ETC2-EAC, RGBA8 / BGRA8, RGBA16_SNORM → `image::RgbaImage`.
  Cubemaps and multi-surface DDS decode all faces.
- **Encode:** BC1 / BC3 / BC5 (pure-Rust) and BC7, with full mip-chain generation; writes both
  `.tex` and compressed or uncompressed `.dds`.
- **Limits:** no ETC encoder (none exists in pure Rust), no RGBA16_SNORM encode; the DDS writer
  re-compresses rather than copying original blocks.

## `.skn` — skinned mesh — `rs_mesh`

- **Read / write:** **byte-exact** for versions 1, 2, and 4 (Basic / Colour / Tangent vertex
  layouts). Submesh ranges, vertex and index buffers, and the trailing tab are all preserved.

## `.scb` — static mesh — `rs_mesh`

- **Read / write:** **byte-exact**, including the trailing per-face / pivot data that other
  readers discard.
- **`.sco`** (the old text static-object format) is read-only; the format was removed from the
  game, so no writer is provided.

## `.skl` — skeleton / rig — `rs_anim`

- **Read / write:** the modern rig format; joints are keyed and ordered by the lowercased ELF hash
  of their name. Legacy `r3d2sklt` skeletons report an unsupported-version error.

## `.anm` — animation — `rs_anim`

- **Read / write:** **byte-exact** for uncompressed v3 / v4 / v5 **and** compressed `r3d2canm`,
  via faithful preservation of the on-disk sections.
- **Editing:** mutating a compressed animation re-emits it as a lossless uncompressed v4 (the
  decoded keyframes are preserved; the compressed encoder is not reproduced).

## `.mapgeo` — environment geometry (OEGM) — `rs_mapgeo`

- **Read / write:** **byte-exact** for versions 5, 6, 7, 9, 11, 12, 13, 14, 15, 17, and 18,
  including the scene graph and planar reflectors.
- **Unsupported:** versions 8, 10, and 16 — these revisions were skipped by Riot and are absent
  from every reference, so rather than guess a layout they return an unsupported-version error.

## `.stringtable` — string table (RST) — `rs_rst`

- **Read / write:** **byte-exact** for v2–v5 (40-bit key hashes for v2/v3, 38-bit for v4/v5).
- **Keys:** hashed with xxh3-64 over the lowercased key. Legacy pre-v5 encrypted entries are
  preserved through a round-trip.

## `.manifest` — release manifest (RMAN) — `rs_rman`

- **Read only.** Release manifests are produced by Riot's servers and are never authored on the
  client/modding side, so the crate provides no writer.
- **Exposes:** bundles and their chunks, files (name, size, ordered chunk ids, directory,
  permissions, locale/platform flag mask), directories, the flags table, reconstructed full file
  paths, and each file's ordered chunk byte-ranges within bundles (the basis for extraction).

## `.wpk`, `.bnk` — audio containers — `rs_audio`

- **Read / write:** **byte-exact** at the container level. `.bnk` preserves every section verbatim
  (including the `HIRC` event hierarchy), and `.wem` audio is extracted from `DIDX` / `DATA`.
- **Scope:** container-level only — extracting and repacking `.wem` covers audio modding; the
  internal Wwise event/object format is intentionally out of scope.
