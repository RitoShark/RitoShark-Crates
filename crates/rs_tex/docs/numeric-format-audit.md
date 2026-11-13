# TEX numeric formats and signed DDS BC5 — 2026-11-13

DDS explicitly identifies signed BC5, so its decoder can be corrected independently
of the disputed TEX mappings. `BC5_SNorm` now uses signed endpoints and interpolation
through the safe Rust `bcdec_rs` 0.2.1 decoder. Previously it was dispatched to the
unsigned decoder alongside `BC5_UNorm`. TEX codes 14 and 21 retain their existing
interpretations pending stronger evidence.

## Independent numeric contract

Microsoft specifies BC5 UNORM channels in `[0, 1]` and SNORM channels in `[-1, 1]`.
The signed format includes signed endpoint ordering, the two interpolation modes,
and saturation of endpoint -128 to -127. The implementation uses the maintained
codec's floating-point output rather than adding another block codec to this library.
See [Microsoft's BC5 specification](https://learn.microsoft.com/en-us/windows/win32/direct3d10/d3d10-graphics-programming-guide-resources-block-compression)
and [bcdec_rs source](https://github.com/ScanMountGoat/image_dds/tree/main/bcdec_rs).

The public result remains an RGBA8 normal preview: signed R/G map to display bytes,
positive Z is reconstructed from `sqrt(max(1 - x*x - y*y, 0))`, and alpha is opaque.
Signed output rounds to nearest; the existing unsigned path retains its rounding.
This does not preserve raw signed channel values or negative normal Z. Typeless DDS
retains the unsigned fallback. DDS-to-TEX adoption and DDS export restrictions are
unchanged; this is a decoding fix, not signed TEX encoding support.

`tests/numeric_decode.rs` uses synthetic blocks with independently calculated expected
values: zero, signed extrema, -128 saturation, both endpoint orderings and all eight
palette indices, channel order, partial edge blocks, small mips, and truncated input.
It exercises the public DDS readers and preserves unsigned TEX behavior. A unit test
checks zero/oversized dimensions before allocation. The format-21 regression locks
the existing byte preservation and SNORM conversion; it does **not** establish the
game's numeric format. No private assets are needed for these tests.

## Live search

Four additional installed map archives were scanned read-only. Every readable chunk
up to 64 MiB was examined for a TEX header; counts include duplicate assets and only
establish header-level coverage, not successful decoding of every format.

| Archive | Code 10 | Code 12 | Code 14 | Code 20 | Code 21 | Code 23 | Chunks skipped >64 MiB |
|---|---:|---:|---:|---:|---:|---:|---:|
| Map22 | 14,319 | 21,718 | 4 | 13 | 0 | 161 | 1 |
| Map12 | 7,092 | 13,188 | 3 | 0 | 0 | 5 | 1 |
| Map30 | 3,063 | 6,146 | 3 | 0 | 0 | 10 | 0 |
| Map453 | 3,774 | 6,352 | 0 | 0 | 0 | 1 | 0 |

Total: 75,852 TEX headers, two oversized chunks skipped, no chunk-read errors.
Code 21 was also absent from the earlier champion/Map11 audit. This is not a search
of every installed archive; format 21 may still exist elsewhere. Additional code-23
headers were counted only; the earlier [format-23 payload audit](format23-audit.md)
still applies specifically to Map11's 26 byte-identical files.

Combining these results with the known Ahri/Map11 entries produced six distinct
code-14 files. Each passed exact parse/write round-trip and full-resolution decoding
with opaque output. Local extracts are ignored under `Sample-Files/tex-numeric`.

| WAD path hash | Size | File SHA-256 |
|---|---|---|
| `5e461a0f9ec5baad` | 512×512 | `bb131da9d9e8cea981a6fa1a2361f09cfdb8ad632fc6bfad35e40ce91211c79f` |
| `5f0f788eb3afb007` | 256×256 | `a654d4c032149a4b37adaa88b6731b6f37e9b1ac1a176b7d4c8ee3178f376608` |
| `707997069055ddae` | 1024×1024 | `77cb51f630708721b7b55ce669b4a8c3cfd46c53023e7d1ec76128aecb98ee06` |
| `7f70d42ae90bdfe7` | 512×512 | `64120a419b91f4d3899308a2e26a961abace103d1ee4aaddac5ec712163fa37f` |
| `a69a219197b6d0e8` | 1024×1024 | `16ac434c049c17ec5244809bd932be5ced3704b57c1f4cd28574c00ea5da8455` |
| `b8e724287ad40d01` | 1024×1024 | `a0f4b3a4f55523741e953a71fa050bd5a6a1219912f6e2c83f2de59274995238` |

All six have depth byte 1, resource byte 0, flags 1, and complete mip chains.
Their endpoint distributions cluster around unsigned midrange. For example,
`a69a219197b6d0e8` has 70,571 red endpoint-0 bytes equal to 127 across its mip chain
(70,547 green). Interpreted as unsigned this is near normal-map center; interpreted
as signed it is near the positive extreme. This supports retaining the current
unsigned behavior, but is an inference, not an independent renderer comparison.

## Conflicting TEX references and remaining evidence

The inspected Rust LTK revision `065af3508767ecd1ec906a0cf2e9bdc6fc03dc2b` labels code 14
`Bc5Snorm` and code 21 `Rgba16Float`; its tests for these formats are synthetic.
See its [format definitions](https://github.com/LeagueToolkit/league-toolkit/blob/065af3508767ecd1ec906a0cf2e9bdc6fc03dc2b/crates/ltk_texture/src/tex/format.rs).
The local Photoshop TEX codec maps both DXGI BC5 UNORM and SNORM to code 14, while
the thumbnail codec uses unsigned BC5. The existing RitoShark format-21 interpretation
came from an experimental SNORM mapping. These references do not agree on a verified
game contract.

Changing code 14 globally would also change our unsigned encoder's decode behavior.
Changing code 21 to half-float without a verified sample would replace one assumption
with another. Both are deferred. Useful next evidence is a nontrivial format-21 file
with independently known channel values, or a verified client format mapping.
For code 14, compare the same bytes with an independent game/reference rendering
before introducing a signed TEX contract. No new surface framework or public numeric
format options were added.
