# rs_cli — command reference

`rs_cli` is the RitoShark command-line tool. It detects, reads, and converts League of Legends
file formats entirely in-process via the RitoShark crates. It never invokes any external program.

Run with the installed binary or from source:

```
rs_cli <COMMAND> [OPTIONS]
cargo run -p rs_cli -- <COMMAND> [OPTIONS]
```

---

## Exit codes

| Code | Meaning |
|------|---------|
| `0` | Success |
| `1` | Error (I/O failure, parse error, invalid argument, etc.) |
| `2` | Unknown or undetectable format |

---

## Format detection

`rs_cli` identifies formats by inspecting the magic bytes at the start of each file. Detection is
ordered so that longer, higher-entropy tags are tested before their shorter prefixes (the `r3d2*`
family before the bare `r3d2` WPK tag; `RW` last of the multi-byte matches).

| Magic / sentinel | Format |
|-----------------|--------|
| `PROP` (4 bytes) | Property bin (binary) |
| `PTCH` (4 bytes) | Patch bin |
| `RW` (2 bytes) | WAD archive |
| `TEX\0` (bytes `54 45 58 00`) | Texture (`.tex`) |
| `DDS ` (4 bytes) | DirectDraw Surface |
| `OEGM` (4 bytes) | Map geometry |
| `RMAN` (4 bytes) | Release manifest |
| `r3d2anmd` (8 bytes) | Animation, uncompressed |
| `r3d2canm` (8 bytes) | Animation, compressed |
| `r3d2Mesh` (8 bytes) | Static mesh (binary) |
| `r3d2` (4 bytes, after ruling out longer tags) | Wwise package (`.wpk`) |
| `BKHD` (4 bytes) | Wwise bank (`.bnk`) |
| `RST` (3 bytes) | String table |
| `[ObjectBegin]` (13 bytes) | Static mesh (text `.sco`) |
| `0x22FD4FC3` at offset 4 | Skeleton (`.skl`) |
| _(none match)_ | Unknown (exit code 2) |

When detection succeeds but a format receives only read support (no per-command action), `rs_cli
read` will still print a brief summary. Format-specific subcommands (`bin`, `wad`, `tex`, `rst`,
`audio`) are only available for the formats listed in those sections.

---

## Hash resolution

Several commands accept a `--hashes` flag for resolving numeric hashes to human-readable names.
When the flag is absent, resolution follows this order:

1. `--hashes <path>` (explicit flag)
2. `RITOSHARK_HASHES` environment variable
3. A `hashes/` directory beside the running executable

A directory path loads the conventional CDTB file set (all files that exist are merged):

```
hashes.binentries.txt
hashes.binhashes.txt
hashes.bintypes.txt
hashes.binfields.txt
hashes.game.txt
hashes.lcu.txt
```

A path to a single file loads just that dictionary. Loading is best-effort — missing files leave
hashes as raw hex rather than failing.

---

## Commands

### `detect`

Identify a file's format from its magic bytes.

```
rs_cli detect [--json] <FILE>
```

| Argument / flag | Description |
|----------------|-------------|
| `<FILE>` | File to inspect |
| `--json` | Output `{"kind": "<FileKind>"}` as JSON |

Without `--json`, prints the `FileKind` debug name (e.g. `PropBin`, `Wad`, `Tex`).

**Examples**

```
rs_cli detect Aatrox.bin
rs_cli detect --json champion.wad.client
```

---

### `read`

Detect a file and print a per-format summary.

```
rs_cli read [--json] [--hashes <DIR>] <FILE>
```

| Argument / flag | Description |
|----------------|-------------|
| `<FILE>` | File to read |
| `--json` | Output summary as JSON (see JSON shapes below) |
| `--hashes <DIR>` | Hash dictionary directory or file for name resolution |

**JSON output shapes**

| Format | Fields |
|--------|--------|
| PropBin / PatchBin | `kind`, `patch` (bool), `version`, `linked` (count), `entries` (count), `patches` (count) |
| Wad | `kind`, `chunks` (count) |
| Tex | `kind`, `width`, `height`, `format` (e.g. `"Bc3"`), `mips` (count) |
| Rst | `kind`, `version`, `entries` (count) |
| Other detected formats | `kind` only |

**Examples**

```
rs_cli read Aatrox.bin
rs_cli read --json --hashes hashes/ champion.wad.client
```

---

### `transform`

Convert a file (or a directory tree with `-r`) between formats. When no output path is given,
the output path is derived by swapping to the opposite representation.

```
rs_cli transform [-r] [-k] [--hashes <DIR>] <INPUT> [OUTPUT]
```

| Argument / flag | Description |
|----------------|-------------|
| `<INPUT>` | Input file or directory (`-r` required for directory) |
| `[OUTPUT]` | Output path (derived automatically if omitted) |
| `-r`, `--recursive` | Walk `<INPUT>` as a directory and convert every matching file |
| `-k`, `--keep-hashed` | When writing text output from `.bin`, keep hashes as raw hex instead of resolving them |
| `--hashes <DIR>` | Hash dictionary directory or file |

**Conversion matrix**

| Input extension | Output extension | Conversion |
|----------------|-----------------|------------|
| `.bin` | `.ritobin` / `.txt` / `.py` | Binary PROP → `#PROP_text`; hashes resolved unless `--keep-hashed` |
| `.ritobin` / `.txt` / `.py` | `.bin` | `#PROP_text` → binary PROP |
| `.tex` | `.png` / `.jpg` / `.jpeg` / `.tga` / `.bmp` / `.webp` | Decode texture → image |
| `.tex` | `.dds` | Decode texture → uncompressed BGRA8 DDS |
| `.png` / `.jpg` / `.jpeg` / `.tga` / `.bmp` / `.webp` | `.tex` | Encode image → BC3 texture with mipmaps |
| `.dds` | `.tex` | Decode DDS → encode as BC3 texture with mipmaps |

When output is omitted, the default mapping is `.bin` → `.ritobin`, text → `.bin`, `.tex` → `.png`,
and image/`.dds` → `.tex`.

In recursive mode (`-r`), files whose extension does not match any source format in the matrix are
silently skipped. One or more individual conversion failures set the exit code to 1 but processing
continues for the remaining files.

**Examples**

```
rs_cli transform Aatrox.bin
rs_cli transform Aatrox.bin Aatrox.ritobin
rs_cli transform -r --keep-hashed data/
rs_cli transform skin.tex skin.png
```

---

### `bin convert`

Convert `.bin` ↔ text. Identical behavior to `transform` but scoped to bin/text conversions.

```
rs_cli bin convert [-r] [-k] [--hashes <DIR>] <INPUT> [OUTPUT]
```

| Argument / flag | Description |
|----------------|-------------|
| `<INPUT>` | Input `.bin` or text file (or directory with `-r`) |
| `[OUTPUT]` | Output path |
| `-r`, `--recursive` | Walk directory |
| `-k`, `--keep-hashed` | Keep hashes as raw hex in text output |
| `--hashes <DIR>` | Hash dictionary directory or file |

**Examples**

```
rs_cli bin convert Aatrox.bin
rs_cli bin convert -r --hashes hashes/ data/bin/ out/
```

---

### `bin diff`

Print a unified diff of two bin or text files, normalizing both to `#PROP_text` form first.

```
rs_cli bin diff [-C <N>] [--no-color] <A> <B>
```

| Argument / flag | Description |
|----------------|-------------|
| `<A>` | First file (`.bin` or text) |
| `<B>` | Second file (`.bin` or text) |
| `-C, --context <N>` | Lines of context (default: `3`) |
| `--no-color` | Accepted for compatibility; currently has no effect (diff output contains no ANSI color) |

Both inputs are converted to `#PROP_text` before diffing, so a binary `.bin` and its text
equivalent will produce an empty diff.

**Example**

```
rs_cli bin diff old/Aatrox.bin new/Aatrox.bin -C 5
```

---

### `wad list`

List the chunk table of one or more WAD archives.

```
rs_cli wad list [-F <FORMAT>] [--stats] [--hashes <DIR>] [ARCHIVES]...
```

| Argument / flag | Description |
|----------------|-------------|
| `[ARCHIVES]...` | One or more `.wad` / `.wad.client` files |
| `-F, --format <FORMAT>` | Output format: `table` (default), `json`, `csv`, `flat` |
| `--stats` | Print a summary line (chunk count and byte totals) to **stderr** after each archive (on by default) |
| `--hashes <DIR>` | Hash dictionary directory or file |

**Output formats**

| Format | Description |
|--------|-------------|
| `table` | `<hash>  <compressed> -> <uncompressed>  <compression>  <name>` per line |
| `json` | `{"chunks": [{hash, name, compressed, uncompressed, compression}, …]}` |
| `csv` | `hash,name,compressed,uncompressed,compression` with header |
| `flat` | One resolved name (or hex hash) per line |

In `json` output, `name` is `null` when the hash has no dictionary entry.

**Examples**

```
rs_cli wad list champion.wad.client
rs_cli wad list -F json --hashes hashes/ champion.wad.client > chunks.json
rs_cli wad list -F flat --stats champion.wad.client
```

---

### `wad extract`

Extract chunks from one or more WAD archives to a directory.

```
rs_cli wad extract -o <DIR> [-f <TYPE>...] [-x <REGEX>] [--overwrite] [--hashes <DIR>] [ARCHIVES]...
```

| Argument / flag | Description |
|----------------|-------------|
| `[ARCHIVES]...` | One or more `.wad` / `.wad.client` files |
| `-o, --output <DIR>` | Output directory (created if absent) |
| `-f, --filter-type [<TYPE>...]` | Only extract chunks whose resolved name ends with this extension (case-insensitive). Repeat for multiple types. |
| `-x, --pattern <REGEX>` | Only extract chunks whose resolved name (or hex hash) matches this regex |
| `--overwrite` | Overwrite existing files; by default existing files are skipped |
| `--hashes <DIR>` | Hash dictionary directory or file |

**Naming and path safety**

Chunks with a resolved name are written to `<output>/<resolved-name>`, preserving any directory
hierarchy embedded in the name (e.g. `assets/characters/aatrox/aatrox.tex`). Unresolved chunks
are written as `<output>/<16-hex-hash>.<ext>`, where `<ext>` is inferred from the resolved name
or defaults to `bin`.

Every output path is validated before writing: `..` components, absolute roots, and drive prefixes
are rejected and the chunk is skipped with a warning. This confines all output to the specified
directory.

**Examples**

```
rs_cli wad extract -o out/ champion.wad.client
rs_cli wad extract -o out/ -f tex champion.wad.client
rs_cli wad extract -o out/ -x "characters/aatrox" --hashes hashes/ champion.wad.client
```

---

### `tex info`

Print texture metadata.

```
rs_cli tex info [--json] <INPUT>
```

| Argument / flag | Description |
|----------------|-------------|
| `<INPUT>` | `.tex` file to inspect |
| `--json` | Output `{"width", "height", "format", "mips"}` as JSON |

Without `--json`, prints one `key: value` line per field. `format` is the internal format name
(e.g. `Bc3`, `Bc1`, `Rgba8`).

**Example**

```
rs_cli tex info aatrox_skin01_tx_cm.tex --json
```

---

### `tex decode`

Decode a texture to a standard image file or DDS.

```
rs_cli tex decode [-o <OUT>] [--mip <N>] <INPUT>
```

| Argument / flag | Description |
|----------------|-------------|
| `<INPUT>` | `.tex` file to decode |
| `-o, --output <OUT>` | Output path (default: input with `.png` extension) |
| `--mip <N>` | Accepted for compatibility; mip selection is not yet implemented (always decodes the full-resolution surface) |

The output format is chosen by the output file extension:

| Extension | Output |
|-----------|--------|
| `.png`, `.jpg`, `.jpeg`, `.tga`, `.bmp`, `.webp` | Decoded RGBA image |
| `.dds` | Uncompressed BGRA8 DDS |

When writing `.dds`, the original block-compressed data is decoded to BGRA8 first; no
recompression is performed.

**Examples**

```
rs_cli tex decode aatrox_skin01_tx_cm.tex
rs_cli tex decode aatrox_skin01_tx_cm.tex -o out/aatrox.png
rs_cli tex decode aatrox_skin01_tx_cm.tex -o raw.dds
```

---

### `tex encode`

Encode a standard image into a `.tex` texture.

```
rs_cli tex encode -f <FORMAT> [-m] [-o <OUT>] <INPUT>
```

| Argument / flag | Description |
|----------------|-------------|
| `<INPUT>` | Image file to encode (`.png`, `.jpg`, `.jpeg`, `.tga`, `.bmp`, `.webp`) |
| `-f, --format <FORMAT>` | Block format: `bc1`, `bc3`, `bc5`, `bc7`, or `bgra8` |
| `-m, --mipmaps` | Generate a full mip chain (default: `true`; ignored for `bgra8`) |
| `-o, --output <OUT>` | Output `.tex` path (default: input with `.tex` extension) |

**Accepted formats**

| Value | Description |
|-------|-------------|
| `bc1` | BC1 block compression (no alpha) |
| `bc3` | BC3 block compression (full alpha) |
| `bc5` | BC5 block compression (two-channel, tangent normals) |
| `bc7` | BC7 block compression (high quality) |
| `bgra8` | Uncompressed BGRA8; always a single surface (mip chain ignored) |

Format names are case-insensitive (`BC1` and `bc1` are equivalent).

**Examples**

```
rs_cli tex encode -f bc3 aatrox.png
rs_cli tex encode -f bc7 -o out/skin.tex aatrox.png
rs_cli tex encode -f bgra8 ui_icon.png
```

---

### `rst list`

List every entry in a string table (`.stringtable`) file.

```
rs_cli rst list [--json] <INPUT>
```

| Argument / flag | Description |
|----------------|-------------|
| `<INPUT>` | `.stringtable` file |
| `--json` | Output `{"version", "entries": [{hash, value}, …]}` as JSON |

Without `--json`, each entry is printed as `<hex-hash>  <value>`. Encrypted entries are printed
as `<hex-hash>  <encrypted>`. Hashes are printed as 10-digit zero-padded hex.

**Example**

```
rs_cli rst list --json main_en_us.stringtable > strings.json
```

---

### `audio extract`

Extract `.wem` audio files from a Wwise package (`.wpk`) or bank (`.bnk`).

```
rs_cli audio extract -o <DIR> <INPUT>
```

| Argument / flag | Description |
|----------------|-------------|
| `<INPUT>` | `.wpk` or `.bnk` container |
| `-o, --output <DIR>` | Output directory (created if absent) |

**Naming**

For `.wpk`: entries are named by their embedded name when present, by their numeric id when the
name is absent, or by their index as a last resort. For `.bnk`: entries are always named
`<id>.wem`.

Collisions within a single invocation are resolved by appending ` (1)`, ` (2)`, etc. before the
extension. Every candidate path passes through the same path-safety check used by `wad extract`,
so adversarial entry names cannot escape the output directory.

**Example**

```
rs_cli audio extract -o wems/ Aatrox_Base_Sounds.wpk
rs_cli audio extract -o wems/ Aatrox_Base_Sounds.bnk
```

---

## Worked examples

### Inspect and convert a property bin

```sh
# Identify the format
rs_cli detect Aatrox.bin

# Print a summary
rs_cli read Aatrox.bin

# Convert to human-readable text (resolves hashes if hashes/ exists)
rs_cli transform Aatrox.bin

# Convert with hash resolution from an explicit directory
rs_cli bin convert --hashes /data/hashes Aatrox.bin Aatrox.ritobin

# Diff two versions
rs_cli bin diff old/Aatrox.bin new/Aatrox.bin

# Batch-convert an entire directory tree (bin -> text)
rs_cli bin convert -r --hashes hashes/ data/bin/
```

### List and extract a WAD archive

```sh
# List all chunks as a table
rs_cli wad list --hashes hashes/ champion.wad.client

# Export chunk names to a flat list
rs_cli wad list -F flat --hashes hashes/ champion.wad.client > paths.txt

# Extract everything
rs_cli wad extract -o out/ --hashes hashes/ champion.wad.client

# Extract only textures, overwriting existing files
rs_cli wad extract -o out/ -f tex --overwrite --hashes hashes/ champion.wad.client

# Extract files matching a regex
rs_cli wad extract -o out/ -x "characters/aatrox" --hashes hashes/ champion.wad.client
```

### Decode and re-encode a texture

```sh
# Print metadata
rs_cli tex info aatrox_skin01_tx_cm.tex

# Decode to PNG (default)
rs_cli tex decode aatrox_skin01_tx_cm.tex

# Decode to BGRA8 DDS
rs_cli tex decode aatrox_skin01_tx_cm.tex -o raw.dds

# Re-encode as BC7
rs_cli tex encode -f bc7 -o aatrox_skin01_tx_cm.tex aatrox.png
```

### Read a string table

```sh
rs_cli rst list main_en_us.stringtable
rs_cli rst list --json main_en_us.stringtable > strings.json
```

### Extract audio

```sh
rs_cli audio extract -o wems/ Aatrox_Base_Sounds.wpk
rs_cli audio extract -o wems/ Aatrox_Base_Sounds.bnk
```
