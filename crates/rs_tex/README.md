# rs_tex

Reads, **writes, and encodes** the League of Legends extended `.tex` texture container, and
reads and writes `.dds` containers, decoding either to an `image::RgbaImage`. It can compress an
`image::RgbaImage` into a valid BC1/BC3/BC5/BC7 `.tex` (with a generated mip chain) and serialize
any decoded image to either an uncompressed or a block-compressed (BC1/BC3/BC5/BC7) `.dds`.

## Header inspection

```rust,no_run
use rs_io::Parse;
use rs_tex::TextureHeader;

let header = TextureHeader::from_path("texture.dds")?;
println!("{} x {}: {}", header.width, header.height, header.format);
# Ok::<(), rs_tex::Error>(())
```

`TextureHeader::from_reader`, `from_bytes`, and `from_path` detect TEX or DDS by
magic. They read only 12 bytes for TEX, 128 for legacy DDS, or 148 for DX10 DDS.
The path method opens a file directly; it does not map or load the pixel payload.
This API accepts a complete header with no payload, so success does not establish
that the texture can be decoded or that its mip data is complete.

| Field | Meaning |
|---|---|
| `container` | `TextureContainer::Tex` or `Dds` |
| `width`, `height` | Declared dimensions |
| `format` | Known TEX/DXGI name, DDS FourCC, or a label retaining an unknown format code |
| `unknown_format` | This build cannot name/classify the declared format; not a decoder-support flag |
| `block_compressed` | Recognized as block compressed; false is inconclusive when the format is unknown |
| `has_mipmaps` | TEX header flag, or DDS count greater than one |
| `mip_count` | `Some(1)` for non-mipmapped TEX; `None` for mipmapped TEX; declared DDS count, using one when absent or zero |
| `dx10`, `dxgi_format` | Whether a DDS DX10 extension is present, and its raw format code |
| `tex_format_code` | Raw TEX format byte, including unknown codes; `None` for DDS |
| `tex_depth` | Raw TEX byte at offset 8, conventionally depth/reserved; no zero-to-one normalization; `None` for DDS |
| `tex_resource_type` | Raw TEX resource byte at offset 10; `None` for DDS |
| `tex_flags` | Raw TEX flag byte at offset 11, including unrecognized bits; `None` for DDS |

DDS structure sizes and the full DX10 extension length are checked. Format flags
determine whether FourCC bytes are meaningful. Unknown format codes remain
inspectable. Dimensions, surface topology, and payload validity are not validated
by this metadata API. DDS layout references: [pixel format](https://learn.microsoft.com/en-us/windows/win32/direct3ddds/dds-pixelformat)
and [DX10 extension](https://learn.microsoft.com/en-us/windows/win32/direct3ddds/dds-header-dxt10).

For example, the [format-23 investigation](docs/format23-audit.md) found live files
with `tex_depth = Some(1)` and `tex_resource_type = Some(3)`. These fields retain the
declared bytes even when the format is unsupported. They do not promise volume or
array decoding. Adding these fields means downstream struct literals must supply
them; constructing metadata through the parsing API requires no changes.

`rs_cli tex info` uses this API for both containers. Its JSON `mips` value is `null`
for a mipmapped TEX because the header alone does not establish the count.

## Formats

### `.tex` (League extended texture)

12-byte little-endian header followed by the pixel payload:

| Offset | Size | Field         | Notes                                            |
|-------:|-----:|---------------|--------------------------------------------------|
| 0      | 4    | magic         | `"TEX\0"` = `0x00584554`                          |
| 4      | 2    | width         | `u16`                                            |
| 6      | 2    | height        | `u16`                                            |
| 8      | 1    | unknown1      | raw depth/reserved byte; retained as read        |
| 9      | 1    | format        | format byte (see table below)                    |
| 10     | 1    | unknown2      | raw resource byte; `0` for 2D, `3` observed for volume |
| 11     | 1    | has_mipmaps   | bool                                             |
| 12     | …    | payload       | mip chain, **smallest mip first**, full-res last |

When `has_mipmaps` is set and the format has a known block layout, the full reader
expects a smallest-to-largest chain with `floor(log2(max(w, h))) + 1` levels. Each
compressed mip rounds each dimension up to a whole block independently. The payload
must match that total exactly; short or surplus data produces `InvalidMipPayload`
with expected/actual byte counts before allocating mip buffers. The header itself
stores no mip count. Otherwise the full reader retains the remaining payload as one block.
The reader's EOF must mark the end of this texture; use a bounded stream for an
embedded TEX.

The [non-square mip audit](docs/mip-layout-audit.md) found correct full chains in the
checked live files and undersized exported chains in a local project. A payload can
match the length of a shorter chain while still containing incorrectly sized levels,
so the parser does not guess a replacement count from its length.

### Explicit largest-mip extraction

For callers that deliberately want just the full-resolution image:

```rust,no_run
use std::fs::File;
use rs_io::Serialize;
use rs_tex::Texture;

let texture = Texture::from_reader_largest_mip(&mut File::open("source.tex")?)?;
let image = texture.decode_rgba()?;
texture.to_path("single-mip.tex")?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

This reads the TEX header and the final full-resolution block, skipping lower mips.
The returned texture has one stored mip and `has_mipmaps = false`; writing it drops
the original lower-mip bytes. It is an extraction operation, not a lossless parse or
mip-chain repair. It assumes a complete full-resolution mip ends at the stream's EOF;
it cannot detect a damaged top mip or an appended trailer. Bound a reader to one
texture when working within a larger stream. Use the normal `Parse` API when validating
or preserving the complete file.

Extraction supports the known BC and uncompressed layouts for 2D resources (header
resource byte zero and depth/reserved byte zero or one). Zero dimensions, insufficient
base-image bytes, other resource kinds, and ETC layouts are rejected. Metadata-only
inspection remains available through `TextureHeader` even when full parsing fails.

Supported `.tex` format bytes:

| Byte | `TexFormat`   | Meaning                  | Decode | Encode |
|-----:|---------------|--------------------------|:------:|:------:|
| 1    | `Etc1`        | ETC1 (mobile)            | yes    | no     |
| 2    | `Etc2Eac`     | ETC2 + EAC alpha (mobile)| yes    | no     |
| 3    | `Etc2`        | ETC2 RGB (mobile)        | yes    | no     |
| 10   | `Bc1`         | DXT1 / BC1               | yes    | yes    |
| 11   | `Bc1Alt`      | DXT1 / BC1 (alt code)    | yes    | yes    |
| 12   | `Bc3`         | DXT5 / BC3               | yes    | yes    |
| 13   | `Bc7`         | BC7                      | yes    | yes    |
| 14   | `Bc5`         | BC5 unsigned normal preview | yes | yes    |
| 20   | `Bgra8`       | uncompressed BGRA8       | yes    | yes    |
| 21   | `Rgba16Snorm` | provisional RGBA16_SNORM | provisional | no |

Code 21's numeric interpretation remains unverified: the current decoder treats it as
signed normalized integers, while Rust LTK labels it half-float. No code-21 sample was
found in the checked corpus. Code 14 keeps unsigned decoding and encoding; the live
samples do not justify changing it globally to signed. See the
[numeric-format audit](docs/numeric-format-audit.md) for evidence and limitations.

The ETC byte mapping (`2 = ETC2-EAC`, `3 = ETC2`) follows the two production C++ `.tex` codecs
(TexThumbnailProvider and RitoTex), which are the tie-breaker over the inconsistent docs.

### `.dds` (DirectDraw Surface)

Parsed via `ddsfile`. The pixel format is resolved from either the DXGI header (BC1/BC2/BC3/BC5/BC7,
R8G8B8A8, B8G8R8A8/X8) or the legacy D3D9 `FourCC`/mask header (DXT1–5, A8R8G8B8, A8B8G8R8).
The main image is decoded to RGBA8.

Decoders validate nonzero dimensions, checked byte counts, and sufficient encoded
payload before allocating image buffers. DDS layer offsets are calculated from each
mip's dimensions and block rounding, including rectangular and partial-block images;
they are not inferred by dividing the file length. All declared mip/layer bytes must
be present, even when reading only the first surface. Extra trailing bytes are ignored
for decoding. Impossible mip counts, empty/overflowing arrays, and volume textures
return errors. Supported uncompressed DDS layouts are tightly packed 32-bit pixels;
header pitch/linear-size hints do not override the calculated layout.

`BC5_SNorm` uses signed endpoints and interpolation through `bcdec_rs`; `BC5_UNorm`
and `BC5_Typeless` retain unsigned decoding. Both produce an opaque normal-map preview:
R/G contain the two channels mapped to display bytes, and blue reconstructs positive Z.
This is not a raw two-channel export. Signed values map `[-1, 1]` to `[0, 255]` with
nearest-integer rounding (zero becomes 128). Typeless DDS has no declared numeric
interpretation; unsigned is the existing fallback. DDS writing supports BC1/BC3/BGRA8;
BC5/BC7 writing remains rejected for game compatibility.

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
let tex = Texture::encode_bc7(&img, true)?;               // BC7 (high-quality RGBA)
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
let dds = write_dds_bytes_bc(&img, TexFormat::Bc3)?;   // RgbaImage -> compressed BC3 .dds
tex.save_dds("out.dds")?;                              // a Texture's decoded image -> .dds (RGBA8)
tex.save_dds_bc("out.dds", TexFormat::Bc3)?;           // a Texture's decoded image -> BC3 .dds
```

Key items:

- `Texture` — `width`, `height`, `format`, `has_mipmaps`, `unknown1`, `unknown2`, `mips`
  (the mip chain kept exactly as on disk). `Texture::new(w, h, format, data)` builds a
  single-mip texture; `mip_count()`, `largest_mip()`, `decode_rgba()`.
- **Encoding** — `Texture::encode(&RgbaImage, TexFormat, mipmaps)` and the `encode_bc1` /
  `encode_bc3` / `encode_bc7` shortcuts compress an image into a valid `.tex`
  (BC1/BC3/BC5/BC7), generating a Lanczos-3 mip chain when `mipmaps` is set. `from_rgba_bgra8`
  builds an uncompressed texture.
- `TexFormat` — the format byte enum with `from_u8` / `to_u8`, `block_size`,
  `bytes_per_block`, `mip_size`.
- `read_dds` / `read_dds_bytes` — decode the first DDS surface to RGBA8, including formats with
  no supported DDS-to-TEX adoption (BC2, BC5). `read_dds_faces` / `read_dds_faces_bytes` decode **all**
  surfaces (six cubemap faces or every array layer). `dds_is_cubemap` reports the surface kind.
- `write_dds_bytes` / `save_dds` and `Texture::to_dds_bytes` / `Texture::save_dds` — write an
  uncompressed RGBA8 `.dds`. `write_dds_bytes_bc` / `save_dds_bc` and
  `Texture::to_dds_bytes_bc` / `Texture::save_dds_bc` — write a block-compressed
  (BC1/BC3) `.dds`.
- `Texture::from_dds_bytes` — adopt a DDS payload as a `Texture` when its format maps onto a
  `.tex` format. This requires one surface, validates the declared mip chain, and retains
  only the full-resolution mip. Arrays, cubemaps, and volumes are rejected.

## Encoding notes

BC compression is lossy, so encode→decode is *close*, not exact (the tests bound the mean
absolute per-channel difference). BC1 carries no alpha; BC3 carries a full alpha channel; BC5 is
two-channel (normal maps); BC7 keeps a high-quality RGBA payload. BC7 encoding runs on a
prebuilt SIMD block kernel (no external toolchain needed) that operates on whole 4x4 tiles, so
non-block-aligned mips are padded to the tile grid before encoding. **ETC encoding** is still not
implemented — ETC formats remain decode-only. The `.tex` writer reproduces any *parsed* texture
byte-for-byte via `to_writer`.

## Fixtures

Real game textures are **gitignored**. The integration tests in `tests/real_files.rs` look for
sample files in `../../sample-files` relative to this crate and skip cleanly when they are
absent. Decoded PNGs are written to the OS temp dir (never into the repo) for eyeballing.
