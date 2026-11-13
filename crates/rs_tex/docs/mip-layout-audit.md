# TEX mip-layout audit — 2026-11-13

The checked live game files support the existing full-chain formula. The reproduced
non-square failures were in exported project textures with undersized mip payloads.
Replacing `max(width, height)` with `min(width, height)` is not a justified parser fix.

## Evidence

| Source | Supported-format TEX entries parsed and byte-round-tripped |
|---|---:|
| Local Azir reference WAD | 1,392 |
| Live Aatrox, Ahri, Akali, Brand, Ashe, and Lux WADs | 22,569 |
| Live Common map WAD | 1 |
| Live Map11 WAD | 17,077 |
| Total | 41,039 |

An additional 26 Map11 TEX entries use unsupported format code 23 and are excluded
from that success count. Seventeen Map11 chunks exceed the 64 MiB size cap and were
not inspected; all other chunks were readable. Counts are archive entries, not unique
textures; archives can share content. This audit checks parsing and binary fidelity, not pixel accuracy or
in-game rendering. The scanning example reports unsupported formats, unreadable
chunks, and chunks over its 64 MiB decompressed-size cap separately.

Header/length inspection of 4,595 local project texture paths found 51 anomalous
BC1/BC3 payloads in one project (45 BC3, 6 BC1). All 51 lengths match this incorrect
per-mip byte-count calculation:

```text
max(bytes_per_block, mip_width * mip_height * bytes_per_block / 16)
```

BC storage instead requires:

```text
ceil(mip_width / 4) * ceil(mip_height / 4) * bytes_per_block
```

The first expression loses storage when a dimension becomes smaller than four.
For example, a mipmapped 4x128 BC3 texture needs 1,040 payload bytes; the project
file has 752, exactly the first formula's total. A 256x64 BC3 file has 21,888 bytes
instead of 21,904. That shorter size also matches eight correctly sized levels,
even though the full chain has nine. In total, **27 of the 51** anomalous lengths
also match a shorter standard chain. Length alone therefore cannot establish
intact mip boundaries.

This establishes a consistent size defect, not the identity of the original writer.
Comparing the final base-image bytes with the current Jhin WAD found no exact matches,
so the audit does not claim the project files are byte-truncated copies of that build.
Twenty-two distinct dimension/format/length cases were copied into the ignored local
`Sample-Files/tex-audit` directory for verification. No game or project assets are committed.

The [C# reference reader](https://github.com/LeagueToolkit/LeagueToolkit/blob/main/src/LeagueToolkit/Core/Renderer/Texture.cs)
also counts levels from the larger dimension, rounds each dimension to blocks, and
reads mips backwards from the end of the stream. The library retains that full-chain
layout and provides explicit top-mip extraction for callers accepting that narrower
contract.

## Library changes

- Full parsing validates the exact known mip-chain length before allocating buffers.
  `InvalidMipPayload` reports expected bytes, actual bytes, and expected level count.
- Extra payload bytes are rejected instead of silently being omitted on serialization.
- `Texture::from_reader_largest_mip` extracts only the final full-resolution image and
  returns a single-mip texture. It does not validate or repair the lower mip chain,
  and assumes the full-resolution image is complete and ends at EOF.
- Full-chain counting, valid rectangular encoding, and the header-only API are unchanged.

## Reproduce

```powershell
cargo test -p rs_tex --test mip_layout --locked -- --nocapture
cargo run -p ritoshark --example audit_tex_mips --no-default-features --features tex,wad --locked -- "path/to/archive.wad.client"
```

The first command always runs synthetic regressions; its optional local fixture test
prints a skip if the private audit directory is absent. The second opens supplied WADs
read-only, checks supported TEX binary round-trips, and reports coverage limits per
archive. A parse/round-trip or chunk-read failure makes it exit unsuccessfully;
unsupported formats and the explicit size cap are reported separately. It does not
replace a maintained required-corpus lane.

Remaining texture questions include unsupported formats and numerical decoding
semantics; those require separate fixtures and checks rather than changes to mip
counting based on this project corpus.
