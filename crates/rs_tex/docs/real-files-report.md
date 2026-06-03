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
| `aatrox_cubemap.dds` | 64×64 | DXT5 / BC3          | OK     | cubemap (`caps2` cubemap bits) — only the first face is decoded |
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

| Byte | Meaning      | Paint.NET | RitoTex | TexThumbnail | pyritofile | ltk_texture | rs_tex (now) |
|-----:|--------------|:---------:|:-------:|:------------:|:----------:|:-----------:|:------------:|
| 1    | ETC1         | —         | yes     | yes          | yes        | yes         | yes          |
| 2    | ETC2 / ETC2-EAC | —      | yes     | yes          | yes        | yes (2≡3)   | yes          |
| 3    | ETC2-EAC / ETC2 | —      | yes     | yes          | yes        | alt of 2    | yes          |
| 10   | DXT1 / BC1   | yes       | yes     | yes          | yes        | yes         | yes          |
| 11   | BC1 (alt)    | —         | —       | —            | yes        | alt of 10   | yes          |
| 12   | DXT5 / BC3   | yes       | yes     | yes          | yes        | yes         | yes          |
| 13   | BC7          | yes       | yes     | yes          | —          | —           | **added**    |
| 14   | BC5          | yes       | yes     | yes          | —          | —           | **added**    |
| 20   | BGRA8        | yes       | yes     | yes          | yes        | yes         | yes          |
| 21   | RGBA16_SNORM | yes       | —       | —            | —          | —           | not yet      |

Discrepancies found:

1. **Missing `0x0D` BC7 and `0x0E` BC5.** The three texture-reference repos (the production
   `.tex` decoders) all list BC7=13 and BC5=14. Our `TexFormat` had neither — an unmapped byte
   would have failed parsing with `UnsupportedFormat`. **Fixed** (see below). None of the
   current samples use them, but they occur in shipped game `.tex` (BC7 color, BC5 normal maps).

2. **ETC2 / ETC2-EAC byte-2-vs-3 ambiguity.** pyritofile and RitoTex/Paint.NET headers call
   `2 = ETC2` and `3 = ETC2_EAC`; TexThumbnailProvider's enum calls `2 = etc2_eac` and
   `3 = etc2`; ltk_texture collapses both `2` and `3` to a single `Etc2Eac`. rs_tex keeps `2`
   and `3` as distinct variants (`Etc2` / `Etc2Eac`) and decodes `2` as ETC2-RGB and `3` as
   ETC2-EAC-RGBA. The sources disagree and no sample exercises ETC, so this is left as-is and
   flagged; it should be pinned down against a real mobile `.tex` before relying on it.

3. **`RGBA16_SNORM` (`0x15`/21).** Only the Paint.NET plugin lists it. Not present in samples;
   noted as a future addition.

4. **DDS cubemaps.** `aatrox_cubemap.dds` is a cubemap; `read_dds_bytes` decodes only the first
   face (the `ddsfile` main image). The reference TexThumbnailProvider similarly previews a
   single surface. Multi-face extraction is not implemented — noted.

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

## Improvements / TODO

1. **Encoding (RGBA → `.tex`).** No compressor or mip generator exists. Add BC1/BC3/BC7/BC5
   encoding (e.g. `intel_tex_2` / `image_dds`) plus Lanczos mip generation, mirroring the
   Paint.NET/RitoTex writers, to make the crate a full read/write codec.
2. **Resolve the ETC2 vs ETC2-EAC byte-2/3 ambiguity** against a real mobile `.tex`, and add
   `RGBA16_SNORM` (byte 21).
3. **Cubemap / multi-surface support** for both `.tex` (resource-type byte) and DDS (all six
   faces + array layers), instead of decoding only the first surface.
