# rs_tex

Reads, **writes, and encodes** the League of Legends extended `.tex` texture container, and
reads and writes `.dds` containers, decoding either to an `image::RgbaImage`. It can compress an
`image::RgbaImage` into a valid BC1/BC3 `.tex` (with a generated mip chain) and serialize any
decoded image to an uncompressed `.dds`.

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

| Byte | `TexFormat`   | Meaning                  | Decode | Encode |
|-----:|---------------|--------------------------|:------:|:------:|
| 1    | `Etc1`        | ETC1 (mobile)            | yes    | no     |
| 2    | `Etc2Eac`     | ETC2 + EAC alpha (mobile)| yes    | no     |
| 3    | `Etc2`        | ETC2 RGB (mobile)        | yes    | no     |
| 10   | `Bc1`         | DXT1 / BC1               | yes    | yes    |
| 11   | `Bc1Alt`      | DXT1 / BC1 (alt code)    | yes    | yes    |
| 12   | `Bc3`         | DXT5 / BC3               | yes    | yes    |
| 13   | `Bc7`         | BC7                      | yes    | no     |
| 14   | `Bc5`         | BC5 (two-channel/normal) | yes    | yes    |
| 20   | `Bgra8`       | uncompressed BGRA8       | yes    | yes    |
| 21   | `Rgba16Snorm` | 16-bit signed RGBA       | yes    | no     |

The ETC byte mapping (`2 = ETC2-EAC`, `3 = ETC2`) follows the two production C++ `.tex` codecs
(TexThumbnailProvider and RitoTex), which are the tie-breaker over the inconsistent docs.

### `.dds` (DirectDraw Surface)

Parsed via `ddsfile`. The pixel format is resolved from either the DXGI header (BC1/BC2/BC3/BC7,
R8G8B8A8, B8G8R8A8/X8) or the legacy D3D9 `FourCC`/mask header (DXT1–5, A8R8G8B8, A8B8G8R8).
The main image is decoded to RGBA8.

## API

All types follow the workspace's universal shape (`Parse` / `Serialize` from `rs_io`).

```rust
use rs_io::{Parse, Serialize};
use rs_tex::{Texture, TexFormat, read_dds, read_dds_bytes, read_dds_faces, write_dds_bytes};

// Parse a .tex
let tex = Texture::from_path("aatrox_circle.tex")?;   // also from_bytes / from_reader
let img = tex.decode_rgba()?;                          // -> image::RgbaImage (full-res mip)

// Serialize a .tex (byte-exact round-trip of the parsed mip chain)
let bytes = tex.to_bytes()?;                           // also to_path / to_writer

// Encode an image into a brand-new .tex
let tex = Texture::encode_bc1(&img, /*mipmaps=*/ true)?;  // BC1 (DXT1) + Lanczos-3 mip chain
let tex = Texture::encode_bc3(&img, true)?;               // BC3 (DXT5)
let tex = Texture::encode(&img, TexFormat::Bc5, false)?;  // BC5, no mips
let tex = Texture::from_rgba_bgra8(&img);                 // uncompressed BGRA8
tex.to_path("out.tex")?;

// DDS read
let img = read_dds("aatrox_q.dds")?;                   // path -> RgbaImage (first surface)
let img = read_dds_bytes(&buf)?;                       // bytes -> RgbaImage
let faces = read_dds_faces("aatrox_cubemap.dds")?;     // Vec<RgbaImage>: all 6 cubemap faces
let tex = Texture::from_dds_bytes(&buf)?;              // DDS -> Texture (BC1/BC3/BC7/BGRA8)

// DDS write
let dds = write_dds_bytes(&img)?;                      // RgbaImage -> uncompressed RGBA8 .dds
tex.save_dds("out.dds")?;                              // a Texture's decoded image -> .dds
```

Key items:

- `Texture` — `width`, `height`, `format`, `has_mipmaps`, `unknown1`, `unknown2`, `mips`
  (the mip chain kept exactly as on disk). `Texture::new(w, h, format, data)` builds a
  single-mip texture; `mip_count()`, `largest_mip()`, `decode_rgba()`.
- **Encoding** — `Texture::encode(&RgbaImage, TexFormat, mipmaps)` and the `encode_bc1` /
  `encode_bc3` shortcuts compress an image into a valid `.tex` (BC1/BC3/BC5), generating a
  Lanczos-3 mip chain when `mipmaps` is set. `from_rgba_bgra8` builds an uncompressed texture.
- `TexFormat` — the format byte enum with `from_u8` / `to_u8`, `block_size`,
  `bytes_per_block`, `mip_size`.
- `read_dds` / `read_dds_bytes` — decode the first DDS surface to RGBA8, including formats with
  no `.tex` equivalent (BC2, BC7). `read_dds_faces` / `read_dds_faces_bytes` decode **all**
  surfaces (six cubemap faces or every array layer). `dds_is_cubemap` reports the surface kind.
- `write_dds_bytes` / `save_dds` and `Texture::to_dds_bytes` / `Texture::save_dds` — write an
  uncompressed RGBA8 `.dds`.
- `Texture::from_dds_bytes` — adopt a DDS payload as a `Texture` when its format maps onto a
  `.tex` format.

## Encoding notes

BC compression is lossy, so encode→decode is *close*, not exact (the tests bound the mean
absolute per-channel difference). BC1 carries no alpha; BC3 carries a full alpha channel; BC5 is
two-channel (normal maps). BC7 and ETC **encoding** are not implemented — only decoding — so
those formats are read-only for now. The `.tex` writer still reproduces any *parsed* texture
byte-for-byte via `to_writer`.

## Fixtures

Real game textures are **gitignored**. The integration tests in `tests/real_files.rs` look for
sample files in `../../sample-files` relative to this crate and skip cleanly when they are
absent. Decoded PNGs are written to the OS temp dir (never into the repo) for eyeballing.
