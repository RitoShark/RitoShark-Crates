# rs_mapgeo — real-file report

Results of running `rs_mapgeo` against the real `.mapgeo` samples in the workspace `Sample-Files/`
directory. The crate now supports **OEGM versions 14, 17 and 18**; everything else is reported as
`Error::UnsupportedVersion`.

## Per-file results

| File | OEGM version | Parsed? | Round-trip |
|---|---|---|---|
| `ultbook.mapgeo` | **14** | yes | byte-exact, full file (20,045,042 bytes) |
| `bloom.mapgeo` | **17** | yes | byte-exact, full file (42,881,463 bytes) |
| `spectator_only_banners.mapgeo` | **18** | yes | byte-exact, full file (92,592,365 bytes) |

All three real samples now `read → to_bytes → ==` **byte-for-byte over the entire file**, including
the trailing scene graphs and planar reflectors. (`spectator_only_banners.mapgeo` has 28 scene
graphs and 0 reflectors; `bloom.mapgeo` previously dropped ~4.5 MB of tail — that tail is now fully
parsed and re-emitted.)

## Gap analysis vs C#

The C# `LeagueToolkit` `EnvironmentAsset` is the oracle. Its reader handles versions
`5, 6, 7, 9, 11, 12, 13, 14, 15, 17, 18`; its **writer is hardcoded to version 17**, so for v14 and
v18 the oracle is *not* a byte-exact round-trip reference — it would re-emit those files as v17. To
guarantee lossless round-trips on every version we read, this crate preserves every byte the oracle
discards and re-emits it version-correctly:

- **v14 baked-paint channel.** The oracle reads the single `version >= 12 && < 17` baked-paint
  channel (path + scale + bias) and keeps only its path as a sampler-0 override, discarding the
  scale/bias. We keep the whole `AssetChannel` (`MapModel::baked_paint`) and re-emit it verbatim.
- **v18 mesh `unknown_v18` `u32`** (after the mesh layer byte) — preserved per mesh.
- **v18 scene-graph leading `f32`** (one per `BucketedGeometry`, before the bounds) — the oracle
  reads it into a throwaway local; we store it on `SceneGraph::unknown_v18` and re-emit it. This was
  the last 112-byte (28 graphs × 4) discrepancy on `spectator_only_banners.mapgeo`.

## What I implemented

1. **OEGM v18 reader + writer.** Mesh body adds the `unknown_v18` `u32` after the layer byte
   (gated `version >= 18`); the scene graph adds its leading `f32`. Everything else equals v17.
   → unlocks `spectator_only_banners.mapgeo`.
2. **OEGM v14 reader + writer.** Implicit sampler strings (bare `BAKED_DIFFUSE_TEXTURE` from v9 and
   `..._ALPHA` from v11, no count/index); per-mesh `u8` render flags (the `u16` form starts at
   v16); no visibility-controller hash (that is `>= 15`); a single baked-paint channel instead of
   the counted override list + scale/bias; and a single implicit scene graph (the counted list is
   `>= 15`). → unlocks `ultbook.mapgeo`.
3. **v17 (and shared) trailing sections.** The bucketed scene graphs (bounds, bucket grid,
   vertex/index arrays, per-bucket records, optional per-face visibility flags) and the planar
   reflectors (`transform, plane, normal`) are now fully parsed and round-tripped, so `bloom.mapgeo`
   round-trips in full rather than just up to the model list.

## Field-by-field version matrix (mirrors the C# oracle)

| Section | v14 | v17 | v18 |
|---|---|---|---|
| shader/sampler overrides | implicit bare strings (v9 + v11) | counted `[i32 index, str]` | counted `[i32 index, str]` |
| vertex decl / buffers / index buffers | identical (15-slot padded decls, `u8` layer ≥ 13) | identical | identical |
| mesh: layer byte (≥ 13) | yes | yes | yes |
| mesh: `unknown_v18` (≥ 18) | — | — | `u32` |
| mesh: controller hash (≥ 15) | — | `u32` | `u32` |
| mesh: transition byte (≥ 14) + render flags | byte + `u8` | byte + `u16` (≥ 16) | byte + `u16` |
| mesh: paint | one baked-paint channel | counted overrides + scale/bias | counted overrides + scale/bias |
| scene graphs | single implicit graph | counted list | counted, each with leading `f32` |
| planar reflectors (≥ 13) | counted list | counted list | counted list |

## Remaining gaps

- **Versions 5, 6, 7, 9, 11, 12, 13, 15** are still `UnsupportedVersion` — no real samples to
  validate against, and several carry features this crate does not yet model (separate point
  lights `< 7`, spherical-harmonics light probes `< 9`, embedded mesh names `<= 11`, the
  `version == 5` special-case that omits `DisableBackfaceCulling`). The reader/writer are already
  structured by version gates, so adding them is mostly filling these branches once a fixture
  exists.
- **Light grid** (older embedded per-mesh grid) is not modeled; none of the three samples use it.

## Foundation needs

None. `rs_io`'s `ReaderExt`/`WriterExt` already cover every primitive used here (`u8/u16/u32/i32/
f32`, sized strings, `mtx44`). No changes to other crates were required.
