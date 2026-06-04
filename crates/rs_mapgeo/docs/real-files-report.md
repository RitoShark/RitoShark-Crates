# rs_mapgeo — real-file report

Results of running `rs_mapgeo` against the real `.mapgeo` samples in the workspace `Sample-Files/`
directory. The crate now supports the **full oracle matrix — OEGM versions 5, 6, 7, 9, 11, 12, 13,
14, 15, 17 and 18**. Versions 8, 10 and 16 are not defined by the oracle and are reported as
`Error::UnsupportedVersion`. Only v14/v17/v18 have real samples; the remaining versions are covered
by synthetic byte-exact round-trip tests (`tests/synthetic_versions.rs`).

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
4. **OEGM v5, v6, v7, v9, v11, v12, v13, v15 reader + writer.** Added the remaining oracle versions
   by filling the existing version-gated branches (no parallel reader). Per-version notes:
   - **v5 / v6.** Leading file-level `separate_point_lights` `bool` (`MapGeometry::separate_point_lights`);
     embedded per-mesh names; no per-buffer layer byte; no mesh layer byte; no render flags
     (< 11); a per-mesh point light `Vec3` when the flag is set (`MapModel::point_light`); nine
     spherical-harmonics coefficients (`MapModel::spherical_harmonics`) followed by only the
     baked-light channel. **v5 additionally omits the backface-culling byte** (`version != 5` gate).
   - **v7.** Same as v6 minus the `separate_point_lights` byte and point light (gate is `< 7`); the
     mesh layer byte moves to the post-transform slot (`7..=12`); still spherical-harmonics lit
     (< 9) with no stationary light or paint.
   - **v9.** Gains the first implicit sampler string (`BAKED_DIFFUSE_TEXTURE`), the stationary-light
     channel, and drops the spherical-harmonics block (>= 9). No render flags yet (< 11). Layer
     still post-transform (7..=12).
   - **v11.** Adds the second implicit sampler string (`..._ALPHA`) and the bare `u8` render-flag
     word (`11..=13`, no transition byte). Still embeds per-mesh names (< 12) and post-transform
     layer (7..=12).
   - **v12.** Drops embedded names (>= 12) and adds the single baked-paint channel
     (`MapModel::baked_paint`, `12..=16`). Layer still post-transform.
   - **v13.** Per-buffer + per-mesh layer bytes move to the >= 13 positions; adds the planar
     reflectors section. Render flags still bare `u8` (`11..=13`).
   - **v15.** Adds the mesh visibility-controller hash (`bucket_grid_hash`, >= 15), the counted
     scene-graph list with per-graph hashes, and the transition byte + `u8` render flags (`14..15`).

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

- **Versions 8, 10 and 16 are undocumented / skipped.** They are absent from the C# oracle's
  version gate (`5,6,7,9,11,12,13,14,15,17,18`), absent from pyritofile's matrix, and have no known
  real files, so there is no authoritative layout to mirror. They report `UnsupportedVersion` rather
  than being guessed at. (pyritofile additionally omits v18 from *its* reader, but the C# oracle and
  our existing real sample cover it, so we keep it.)
- **No real fixtures for 5/6/7/9/11/12/13/15.** These versions are implemented straight from the two
  oracles and validated by synthetic byte-exact round-trips. They should be re-checked against a
  real file if one ever surfaces, but the layout is a faithful mirror of the oracle gates.
- **Light grid** (older embedded per-mesh grid) is not modeled; none of the three real samples use
  it, and the oracle does not read it for any supported version.

## Foundation needs

None. `rs_io`'s `ReaderExt`/`WriterExt` already cover every primitive used here (`u8/u16/u32/i32/
f32`, sized strings, `mtx44`). No changes to other crates were required.
