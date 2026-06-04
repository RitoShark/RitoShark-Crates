# rs_tex — real-files report

Results of parsing, decoding, and round-tripping the real `.tex`/`.dds` sample files, plus a
cross-check of our format handling against the texture-reference repositories
(Paint.NET-Tex-Plugin, RitoTex-Photoshop, TexThumbnailProvider), pyritofile, and ltk_texture.

Run with:

```
cargo test -p rs_tex -- --nocapture --test-threads=1
```

Samples live in `sample-files/` (gitignored); the test skips any missing file.

## Per-file results

### `.tex`

| File                          | W×H     | Format byte | `TexFormat` | Mipmaps | Decode | Round-trip (`to_bytes` vs original) |
|-------------------------------|---------|------------:|-------------|:-------:|:------:|:-----------------------------------:|
| `aatrox_base_sword_tx_cm.tex` | 256×256 | `0x0C` (12) | `Bc3`       | yes     | OK     | **byte-identical**                  |
| `aatrox_circle.tex`           | 128×128 | `0x0C` (12) | `Bc3`       | no      | OK     | **byte-identical**                  |
| `aatrox_wings_tx_cm.tex`      | 256×256 | `0x0A` (10) | `Bc1`       | yes     | OK     | **byte-identical**                  |

All three decode to an `RgbaImage` whose dimensions match the header, and re-serialize to bytes
identical to the original file (header + full mip chain).

### `.dds`

| File                 | W×H   | Pixel format        | Decode | Notes                                   |
|----------------------|-------|---------------------|:------:|-----------------------------------------|
| `aatrox_cubemap.dds` | 64×64 | DXT5 / BC3          | OK     | cubemap (`caps2` cubemap bits) — all six faces decoded via `read_dds_faces` |
| `aatrox_q.dds`       | 64×64 | DXT1 / BC1          | OK     |                                         |
| `icons_ahri_e.dds`   | 64×64 | DXT1 / BC1          | OK     | has a non-standard `"NVT3"` reserved tag; `ddsfile` ignores it |
| `kayle_p.dds`        | 64×64 | DXT1 / BC1          | OK     |                                         |

All DDS samples decode to RGBA8 via the legacy D3D9 `FourCC` path.

## Cross-check vs references

### Header layout — agrees everywhere

The 12-byte header (`magic u32`, `width u16`, `height u16`, `unknown1 u8`, `format u8`,
`unknown2 u8`, `has_mipmaps bool`) matches Paint.NET `TexFile.Read`, RitoTex `TEX_HEADER`,
TexThumbnailProvider, and pyritofile `TEX.read`. Magic is `0x00584554` (`"TEX\0"`). The mip
chain is stored **smallest-first**, with mip count `floor(log2(max(w,h))) + 1`, and our reader
reproduces the reference ordering and per-mip block-size math exactly.

### Format-byte mapping

Authoritative byte → meaning, gathered from the references:

| Byte | Meaning         | Paint.NET | RitoTex | TexThumbnail | pyritofile | ltk_texture | rs_tex (now) |
|-----:|-----------------|:---------:|:-------:|:------------:|:----------:|:-----------:|:------------:|
| 1    | ETC1            | —         | yes     | yes          | yes        | yes         | yes          |
| 2    | **ETC2-EAC**    | —         | yes     | yes          | yes        | yes (2≡3)   | **fixed**    |
| 3    | **ETC2**        | —         | yes     | yes          | yes        | alt of 2    | **fixed**    |
| 10   | DXT1 / BC1      | yes       | yes     | yes          | yes        | yes         | yes          |
| 11   | BC1 (alt)       | —         | —       | —            | yes        | alt of 10   | yes          |
| 12   | DXT5 / BC3      | yes       | yes     | yes          | yes        | yes         | yes          |
| 13   | BC7             | yes       | yes     | yes          | —          | —           | yes          |
| 14   | BC5             | yes       | yes     | yes          | —          | —           | yes          |
| 20   | BGRA8           | yes       | yes     | yes          | yes        | yes         | yes          |
| 21   | RGBA16_SNORM    | yes       | —       | —            | —          | —           | **added**    |

The ETC tie-breaker: TexThumbnailProvider (`tex_format_etc2_eac = 0x2`, `tex_format_etc2 = 0x3`)
and RitoTex `TexFormat.h` (identical) are the two production C++ codecs and agree, so rs_tex now
matches them.

Discrepancies found:

1. **Missing `0x0D` BC7 and `0x0E` BC5.** The three texture-reference repos (the production
   `.tex` decoders) all list BC7=13 and BC5=14. Our `TexFormat` had neither — an unmapped byte
   would have failed parsing with `UnsupportedFormat`. **Fixed** (see below). None of the
   current samples use them, but they occur in shipped game `.tex` (BC7 color, BC5 normal maps).

2. **ETC2 / ETC2-EAC byte-2-vs-3.** The two production C++ codecs that ship the format —
   TexThumbnailProvider and RitoTex `TexFormat.h` — both define `0x2 = etc2_eac` and
   `0x3 = etc2`; ltk_texture collapses both to a single `Etc2Eac`. rs_tex previously had this
   inverted (`2 = Etc2`). **Fixed** to match the C++ codecs: `2 = Etc2Eac` (decodes as ETC2-EAC
   RGBA), `3 = Etc2` (decodes as ETC2 RGB). No ETC sample exercises the decode path yet, so it
   should still be confirmed against a real mobile `.tex`.

3. **`RGBA16_SNORM` (`0x15`/21).** Only the Paint.NET plugin lists it. Not present in samples;
   noted as a future addition.

4. **DDS cubemaps.** `aatrox_cubemap.dds` is a cubemap. `read_dds_bytes` returns the first face,
   but `read_dds_faces` / `read_dds_faces_bytes` now decode **all six faces** (and every array
   layer for array textures). **Fixed.**

## What I changed

- **Added `TexFormat::Bc7 = 13` and `TexFormat::Bc5 = 14`** to match the authoritative
  texture-reference format codes (`texture.rs`: enum, `from_u8`, `bytes_per_block` — both are
  16 bytes/block).
- **Wired decoding** for the new formats: `decode.rs` routes `Bc7 → decode_bc7` and
  `Bc5 → decode_bc5` (texture2ddecoder); `read.rs` includes them in the block-layout mip set so
  mipmapped BC7/BC5 `.tex` parse their chains correctly; `dds.rs` `from_dds_bytes` now maps a
  BC7 DDS onto `TexFormat::Bc7`.
- **Added `tests/real_files.rs`**: skip-if-missing harness that, per `.tex`, asserts
  `width/height > 0` and a recognized format, decodes to RGBA and checks dimensions, and asserts
  `to_bytes()` is byte-identical to the original; per `.dds`, parses and decodes to RGBA.
  Decoded PNGs are saved to the OS temp dir for manual inspection.

All `rs_tex` tests pass; `cargo clippy -p rs_tex --all-targets` is clean.

## Gap analysis vs C#/texture-reference

Comparing the read-only crate against the C# `LeagueToolkit` oracle and the production
texture-reference codecs (Paint.NET `TexFile`, RitoTex `TexFormat.h` + Intel ISPC plugin,
TexThumbnailProvider) surfaced these gaps:

1. **No encoder.** The crate could parse and byte-exactly re-serialize a `.tex`, but could not
   build one from raw pixels. Paint.NET's `TexFile.Write` and the RitoTex/Photoshop plugin both
   compress an RGBA image and emit a full mip chain; rs_tex had neither a BC compressor nor a mip
   generator.
2. **No DDS writer.** `ddsfile` was a dependency but only used for reading; there was no path
   from an `RgbaImage`/`Texture` back out to a `.dds`.
3. **Single-surface DDS only.** `read_dds_bytes` decoded just the `ddsfile` main image, so
   `aatrox_cubemap.dds` exposed one of its six faces. DirectXTex (used by RitoTex) and the C#
   loaders surface all faces/array layers.
4. **Inverted ETC2 byte mapping.** rs_tex had `2 = Etc2`, `3 = Etc2Eac`. Both production C++
   codecs — TexThumbnailProvider and RitoTex `TexFormat.h` — define `0x2 = etc2_eac` and
   `0x3 = etc2`. The references agree, so this was a genuine bug.
5. **Missing `RGBA16_SNORM` (byte 21).** Listed by the Paint.NET plugin (`TexFile.RGBA16_SNORM`,
   8 bytes/pixel, signed-normalised) and unmapped in rs_tex.

## What I implemented

- **Encoder (`encode.rs`).** `Texture::encode(&RgbaImage, TexFormat, mipmaps)` plus `encode_bc1`
  / `encode_bc3` / `encode_bc7` shortcuts compress an image into a valid `.tex`, supporting
  **BC1, BC3, BC5, and BC7**. BC1/BC3/BC5 use the pure-Rust `texpresso` compressor (pinned
  `2.0.2`). When `mipmaps` is set it generates the full chain down to 1×1 with a separable
  **Lanczos-3** resample (matching Paint.NET's `DownsampleRgba`) and stores it smallest-first
  exactly as the reader/`TexFile.Read` expect. `from_rgba_bgra8` builds an uncompressed texture.
- **BC7 encoder.** Added `intel_tex_2` (pinned `=0.4.0`) — Intel's ISPC block compressor. It
  **builds clean on Windows/MSVC**: the crate ships prebuilt ISPC kernels via `ispc_rt`, so no
  ISPC/clang/`cl` toolchain is required (verified: compiles in ~8 s, links and runs). The kernel
  operates on whole 4×4 tiles, so non-aligned mips (3×3, 2×2, 1×1…) are padded up to the block
  grid by edge replication before encoding; the partial edge blocks the decoder discards are
  unaffected. `intel_tex_2` uses `unsafe` internally, but it is an external dependency —
  `rs_tex` itself keeps `#![forbid(unsafe_code)]`. BC7 encode uses the balanced
  `alpha_basic_settings` preset.
- **DDS writer (`dds.rs`).** `write_dds_bytes` / `save_dds` and `Texture::to_dds_bytes` /
  `save_dds` emit an uncompressed `R8G8B8A8_UNorm` `.dds` via `ddsfile`. **Compressed DDS:**
  `write_dds_bytes_bc` / `save_dds_bc` and `Texture::to_dds_bytes_bc` / `save_dds_bc` emit a
  block-compressed **BC1/BC3/BC5/BC7** `.dds` (mapped onto `BC{1,3,5,7}_UNorm` DXGI formats),
  re-compressing the decoded RGBA8 surface. The DDS reader gained BC5 decode so written BC5 DDS
  reads back.
- **Multi-surface DDS decode.** `read_dds_faces` / `read_dds_faces_bytes` walk every array layer
  (`get_num_array_layers`, which returns 6 for cubemaps) and decode each layer's full-resolution
  mip; `dds_is_cubemap` reports the surface kind. `read_dds_bytes` still returns the first
  surface for the common case.
- **Fixed the ETC2 mapping** to `2 = Etc2Eac`, `3 = Etc2` per the production codecs, and routed
  decoding accordingly (`2` → ETC2-EAC RGBA, `3` → ETC2 RGB). Lossless re-serialisation of real
  files is unaffected (the bytes are preserved regardless).
- **Added `TexFormat::Rgba16Snorm = 21`**: block math (8 bytes/pixel), block-layout mip parsing,
  and a decoder mapping the four signed-16 channels from `[-1, 1]` to `[0, 255]`.

### Validation (`tests/real_files.rs`)

- Each real BC1/BC3 `.tex` is decoded → **re-encoded with the same BC format** → decoded again;
  the mean absolute per-channel diff is asserted `< 12` (BC is lossy). The re-encoded `.tex` is
  also re-parsed and its header/mip-count checked, proving the encoder emits a structurally valid
  file our own reader accepts.
- Each real BC1/BC3 `.tex` is decoded → **encoded to BC7** (with the same mip flag) → re-parsed →
  decoded; header/mip-count checked and mean-abs-diff asserted `< 8`. Measured drift on the real
  samples is well under 8 (BC7 is high quality). A synthetic 64×64 BC7 gradient round-trips at
  drift `< 6` with a 7-level mip chain.
- A synthetic 64×64 gradient encodes to BC1 (opaque) and BC3 (alpha) with mip chains (7 levels),
  round-trips under threshold, and re-parses.
- **Compressed DDS:** a synthetic 32×32 image is written to BC1/BC3/BC5/BC7 `.dds` via
  `write_dds_bytes_bc`, read back through `read_dds_bytes`, and compared; dimensions match and
  drift is under threshold (BC5 compared on its R/G channels only).
- `aatrox_cubemap.dds` is detected as a cubemap and decodes to **six faces**.
- The DDS writer round-trips an uncompressed RGBA8 image byte-exactly.

All `rs_tex` tests pass and `cargo clippy -p rs_tex --all-targets -- -D warnings` is clean.

## Remaining gaps / TODO

1. **ETC encoding.** ETC1/ETC2/ETC2-EAC remain decode-only — no buildable pure-Rust ETC encoder
   is wired in. (`intel_tex_2` exposes an ETC1 path but not the ETC2 variants League uses.) This
   is the main remaining encode gap.
2. **`RGBA16_SNORM` encode.** Decode added; no encoder path yet.
3. **Compressed DDS re-compresses rather than preserves.** `write_dds_bytes_bc` re-encodes the
   decoded RGBA8 surface into BC, so exporting an already-BC source is a lossy re-compress. A
   block-preserving path (copy the original mip bytes straight into the DDS) would be lossless for
   already-compressed sources and is still TODO.
4. **Confirm the ETC2/EAC mapping against a real mobile `.tex`.** It now matches the two C++
   codecs, but no ETC sample exercises the decode path end to end.
5. **`.tex` has no multi-surface header**, so cubemap/array support is DDS-only; if a `.tex`
   resource-type/array variant exists in newer clients it is not yet handled.
