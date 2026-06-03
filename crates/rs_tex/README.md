# rs_tex

Reads and writes the League of Legends extended `.tex` texture container, and reads
`.dds` containers, decoding either to an `image::RgbaImage`.

## Formats

### `.tex` (League extended texture)

12-byte little-endian header followed by the pixel payload:

| Offset | Size | Field         | Notes                                            |
|-------:|-----:|---------------|--------------------------------------------------|
| 0      | 4    | magic         | `"TEX\0"` = `0x00584554`                          |
| 4      | 2    | width         | `u16`                                            |
| 6      | 2    | height        | `u16`                                            |
| 8      | 1    | unknown1      | reserved; Riot/tooling write `1`                 |
| 9      | 1    | format        | format byte (see table below)                    |
| 10     | 1    | unknown2      | reserved; written `0`                            |
| 11     | 1    | has_mipmaps   | bool                                             |
| 12     | …    | payload       | mip chain, **smallest mip first**, full-res last |

When `has_mipmaps` is set and the format has a known block layout, the payload is a full
mip chain stored smallest-to-largest; mip count is `floor(log2(max(w, h))) + 1`. Otherwise
the payload is a single full-resolution image and is read to end-of-file.

Supported `.tex` format bytes:

| Byte | `TexFormat` | Meaning                  | Decode |
|-----:|-------------|--------------------------|:------:|
| 1    | `Etc1`      | ETC1 (mobile)            | yes    |
| 2    | `Etc2`      | ETC2 RGB (mobile)        | yes    |
| 3    | `Etc2Eac`   | ETC2 + EAC alpha         | yes    |
| 10   | `Bc1`       | DXT1 / BC1               | yes    |
| 11   | `Bc1Alt`    | DXT1 / BC1 (alt code)    | yes    |
| 12   | `Bc3`       | DXT5 / BC3               | yes    |
| 13   | `Bc7`       | BC7                      | yes    |
| 14   | `Bc5`       | BC5 (two-channel/normal) | yes    |
| 20   | `Bgra8`     | uncompressed BGRA8       | yes    |

### `.dds` (DirectDraw Surface)

Parsed via `ddsfile`. The pixel format is resolved from either the DXGI header (BC1/BC2/BC3/BC7,
R8G8B8A8, B8G8R8A8/X8) or the legacy D3D9 `FourCC`/mask header (DXT1–5, A8R8G8B8, A8B8G8R8).
The main image is decoded to RGBA8.

## API

All types follow the workspace's universal shape (`Parse` / `Serialize` from `rs_io`).

```rust
use rs_io::{Parse, Serialize};
use rs_tex::{Texture, TexFormat, read_dds, read_dds_bytes};

// Parse a .tex
let tex = Texture::from_path("aatrox_circle.tex")?;   // also from_bytes / from_reader
let img = tex.decode_rgba()?;                          // -> image::RgbaImage (full-res mip)

// Serialize a .tex (byte-exact round-trip of the parsed mip chain)
let bytes = tex.to_bytes()?;                           // also to_path / to_writer

// DDS helpers
let img = read_dds("aatrox_q.dds")?;                   // path -> RgbaImage
let img = read_dds_bytes(&buf)?;                       // bytes -> RgbaImage
let tex = Texture::from_dds_bytes(&buf)?;              // DDS -> Texture (BC1/BC3/BC7/BGRA8)
```

Key items:

- `Texture` — `width`, `height`, `format`, `has_mipmaps`, `unknown1`, `unknown2`, `mips`
  (the mip chain kept exactly as on disk). `Texture::new(w, h, format, data)` builds a
  single-mip texture; `mip_count()`, `largest_mip()`, `decode_rgba()`.
- `TexFormat` — the format byte enum with `from_u8` / `to_u8`, `block_size`,
  `bytes_per_block`, `mip_size`.
- `read_dds` / `read_dds_bytes` — decode a DDS to RGBA8, including formats with no `.tex`
  equivalent (BC2, BC7).
- `Texture::from_dds_bytes` — adopt a DDS payload as a `Texture` when its format maps onto a
  `.tex` format.

## No encoding (RGBA → `.tex`)

This crate **does not encode** an `RgbaImage` (or DDS) into block-compressed `.tex` payloads,
and does not generate mip chains. It reproduces a parsed `.tex` byte-for-byte via `to_writer`,
but constructing a brand-new compressed texture from raw pixels is out of scope for now (see
`docs/real-files-report.md` → "Improvements / TODO").

## Fixtures

Real game textures are **gitignored**. The integration tests in `tests/real_files.rs` look for
sample files in `../../sample-files` relative to this crate and skip cleanly when they are
absent. Decoded PNGs are written to the OS temp dir (never into the repo) for eyeballing.
