# Compressed ANM test fixtures

`tests/real_files.rs` validates the compressed `r3d2canm` evaluator against real game data when
these files are present in `../../Sample-Files/` (gitignored):

- `compressed_507c1f34b053b389.anm`
- `compressed_e890878834c561be.anm`
- `compressed_e63f4f2e8c074937.anm`

They were extracted from `Azir.wad.client` (any chunk whose payload begins with `r3d2canm`). To
regenerate, mount a WAD with `rs_wad`, find chunks starting with the `r3d2canm` magic, and write
them out. The test skips gracefully when the files are absent.
