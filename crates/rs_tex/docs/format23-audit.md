# TEX format 23 investigation — 2026-11-13

Format code 23 remains unsupported for pixel decoding. The live corpus establishes
one concrete file layout but does not establish its channel or numeric meaning.
The library exposes its raw header fields through `TextureHeader` without assigning
an unverified pixel format.

## Observed files

The live `Maps/Shipping/Map11.wad.client` contains 26 entries with this exact layout:

| Field | Observed value |
|---|---|
| Magic | `TEX\0` |
| Width × height | 64 × 64 |
| Byte 8 (depth/reserved) | 1 |
| Byte 9 (format) | 23 (`0x17`) |
| Byte 10 (resource type) | 3 |
| Byte 11 (flags) | 0 |
| Payload | 4,096 zero bytes |
| Total file size | 4,108 bytes |

All 26 payloads were inspected in full and are zero-filled. Their headers are also
identical: this provides one distinct byte layout, not 26 independent decoding
examples. The payload has exactly one byte per declared texel. This is consistent
with an 8-bit single-channel layout, but cannot distinguish red, alpha, luminance,
signed/unsigned interpretation, or a special-purpose representation.

The complete file SHA-256 shared by all 26 entries is
`dfa34a40c4fa0d5b7fb318f7caf84c6b21fdea441f816d10c27ce95fdb8dff27`.

The [C# reference](https://github.com/LeagueToolkit/LeagueToolkit/blob/main/src/LeagueToolkit/Core/Renderer/Texture.cs)
identifies resource byte 3 as a volume texture and does not map format 23. The
[Rust reference format enum](https://github.com/LeagueToolkit/league-toolkit/blob/065af3508767ecd1ec906a0cf2e9bdc6fc03dc2b/crates/ltk_texture/src/tex/format.rs)
also has no code 23. No verified format mapping was recovered from read-only static
inspection of the installed executable. Neither payload size nor an all-black
preview is enough evidence to implement a decoder.

The 26 entries were extracted into the ignored `Sample-Files/tex-format23` directory.
Game files were only read; no assets or executable bytes are committed. The original
archive scan excluded 17 chunks above 64 MiB, so this is not an exhaustive inventory
of every TEX in that archive or the installation.

## Library behavior

### Expanded live check

A follow-up payload scan inspected all 177 format-23 entries found in Map22 (161),
Map12 (5), Map30 (10), and Map453 (1). Every file has the same header, zero-filled
payload, and SHA-256 listed above. These are additional path entries, not additional
distinct layouts. Two chunks larger than 64 MiB were skipped across those archives;
there were no chunk-read errors. Extracts remain ignored under `Sample-Files/tex-remaining`.
This closes the earlier header-only limitation for these 177 entries but supplies
no new evidence for pixel interpretation. The combined five-map sample contains 203
identical entries; it is not an exhaustive search of the installed game.

### Current API

`TextureHeader` exposes `tex_format_code`, `tex_depth`, `tex_resource_type`, and
`tex_flags` as raw `Option<u8>` fields. All four are present for TEX and absent for
DDS. Inspection still reads only the header, preserves unknown values, and reports
`unknown_format = true` for code 23. A false `block_compressed` value for an unknown
format is inconclusive.

Full `Texture` parsing continues to return `UnsupportedFormat` for code 23. There
is no guessed grayscale conversion, TEX encoding, or volume decoding support for
this format. Archive extraction/repacking can retain its original bytes without
decoding them. No new dependency or general texture framework was added.

The WAD audit example now prints each unsupported entry's hash, dimensions, raw
depth/resource/flags, and payload byte count as well as the aggregate counts:

```powershell
cargo run -p ritoshark --example audit_tex_mips --no-default-features --features tex,wad --locked -- "path/to/Map11.wad.client"
cargo test -p rs_tex --test header --locked
```

To justify pixel support, obtain a verified mapping from code 23 to its actual
pixel format, or nonzero samples with independently known decoded values. A future
decoder must also address the resource layout explicitly; the observed volume
resource has only one slice and does not establish multi-slice or mip ordering.
