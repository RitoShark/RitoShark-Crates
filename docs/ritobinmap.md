# `ritobinmap` — the hash→path record inside a `.bin`

Status: implemented in `rs_bin` (`PathMap`, `capture`, `read_path_map`, `write_path_map`,
`strip_path_map`). Supersedes the `CELMAP` trailing footer, which is now read-only.

---

## 1. The problem

Riot is migrating asset references in `.bin` from readable strings to hashes:

```
mTexturePath: string = "assets/characters/ahri/skins/skin99/ahri_tx_cm.dds"   # before
mTexturePath: file   = 0x8f2c1d4b7a05e961                                     # after
```

A hash is one-way. It can only be read back if the pair is in a dictionary:

| Path | Recoverable? |
|---|---|
| A Riot path | Yes — the community hashtables list it. |
| A path a **mod invented** (a repath, a custom emitter name) | **No.** Only that one mod ever used the string, so no dictionary anywhere has it. |

The moment a mod's own path becomes a bare hash, it is gone — for every tool, and for the
mod's own author on a different machine. There is exactly one instant at which both halves
exist together: when the tool hashes a readable path. **That is the instant to record it.**

## 2. Where the record lives, and why there

It is a **normal top-level bin entry**. Not a footer, not a sidecar-only file.

**Why not after the body.** The bin format declares an entry count and no whole-file length, so
bytes appended past the last entry are invisible to the game — which is why the first version of
this record went there. But ritobin ends its read with

```cpp
bin_assert(reader.cur_ == reader.cap_);   // bin_io_binary_read.cpp, read_sections()
```

so **one byte past the declared body fails the entire file**. Anything written that way is a bin
that ritobin-cli, and anything built on it, refuses to open. An entry, by contrast, is data every
`.bin` parser already reads, rewrites and preserves — including through a third-party
`bin → py → bin` round-trip, which the footer did not survive at all.

**Why not sidecar-only.** A `hashes.txt` in `/META` (fantome) or a `_meta_` chunk (modpkg) is
strictly better *inside an archive*: written once, no per-file duplication. It is also gone the
moment someone copies one loose `.bin` out of the mod, which is most of what actually happens.
The two are complementary — a tool that ships archives should write both — but the in-bin record
is what makes a single file self-describing.

## 3. The record

```
ritobinmap = RitoBinMap {
    binEntries: list[string]
    binTypes:   list[string]
    binFields:  list[string]
    binHashes:  list[string]
    game:       list[string]
}
```

Hashes, all FNV1a-32 of the lowercased name (`fnv1a` lowercases, so `ritobinmap` and
`RitoBinMap` collide by design — the entry path hash and the class hash are the same value):

| Name | Hash | Role |
|---|---|---|
| `ritobinmap` | `0xad28fe20` | entry path hash |
| `RitoBinMap` | `0xad28fe20` | entry class hash |
| `binEntries` | `0x6c14cf6e` | field |
| `binTypes` | `0xe4087ef7` | field |
| `binFields` | `0x90b72a9b` | field |
| `binHashes` | `0xaa10b402` | field |
| `game` | `0x4249d707` | field |

Add those two names to `hashes.binentries.txt` / `hashes.bintypes.txt` and the field names to
`hashes.binfields.txt` and the record prints readably in ritobin text.

### Categories

The five lists are ritobin's five hashtables, one for one. **A name is only ever resolved from
the category it was recorded in.**

| List | Holds the names behind | Hash |
|---|---|---|
| `binEntries` | entry path hashes, `link` values, `PTCH` patch keys | FNV1a-32 |
| `binTypes` | class hashes (entry, `pointer`, `embed`) | FNV1a-32 |
| `binFields` | field-name hashes | FNV1a-32 |
| `binHashes` | `hash` values, and both halves of a packed `mBlendDataTable` key | FNV1a-32 |
| `game` | `file` values (and any WAD path) | XXH64, seed 0 |

Why the split matters: four different kinds of name share one 32-bit hash space. Merged into a
single table, a `hash` value can be named by a *field* name that collided with it, and the tool
displays a confident lie. Split, a lookup can only ever return a name recorded for that position.

### Rules

1. **Paths only. Never store the hash.** `hash = fnv1a(name)` / `xxh64(path)` — the key is a pure
   function of the value, so writing it as well is redundant bytes that also happen to be
   high-entropy hex, which is exactly what a WAD's zstd cannot compress.
2. **Deduplicated and sorted.** Each list is a set, written in sorted order, so the record is
   deterministic and two builds of the same mod produce the same bytes.
3. **Empty lists are omitted**, and a record with no lists is not written at all.
4. **Record only what nothing else can resolve.** A name the shared community hashtable already
   knows does not belong here — it is already recoverable everywhere. In practice the record
   holds repaths and invented names only, and is a few hundred bytes.
5. **A name may appear in more than one list** if the mod genuinely uses the same string as, say,
   an entry path and a `hash` value. That is not duplication; the two lookups are separate.

### On-disk bytes

Nothing special — this is the standard entry encoding, listed here so another tool can write it
without reading `rs_bin`:

```
header:   ... u32 entryCount, then u32 classHash × entryCount   (0xad28fe20 is one of them)
entry:    u32 length            bytes after this field
          u32 pathHash          0xad28fe20
          u16 fieldCount
          per field:
            u32 nameHash        e.g. 0x4249d707 for `game`
            u8  type            0x80  LIST
            u8  itemType        0x10  STRING
            u32 size            bytes after this field
            u32 count
            per item:
              u16 length, UTF-8 bytes (no NUL)
```

## 4. API (`rs_bin`)

```rust
let map  = rs_bin::read_path_map(&bin);          // the record, empty if there is none
let map  = rs_bin::strip_path_map(&mut bin);     // remove it and hand it back
rs_bin::write_path_map(&mut bin, &map);          // replace it (empty map = remove)

let found = rs_bin::capture(&bin, names);        // the capture step
let table = map.tables();                        // hash -> name lookups, per category
```

`capture(bin, names)` takes readable names a tool still has — the text it is about to hash, the
values of a legacy footer — hashes each one and files it under **every category whose hashes the
bin actually uses**, dropping the rest. It skips the record's own entry, so capturing twice never
starts naming `binEntries`, `game` and friends.

Writing is idempotent, and `write_path_map` also drops a legacy `CELMAP` footer from
`Bin::trailing`, so a rewritten bin stops tripping ritobin.

Typical save path:

```rust
let mut map = rs_bin::capture(&bin, names_the_author_typed);
map.merge(&existing_map);            // never lose what an earlier tool recorded
rs_bin::write_path_map(&mut bin, &map);
```

## 5. Migrating a bin that carries the old `CELMAP` footer

The old layout was `[JSON {hex: path}][u32 len][b"CELMAP\0\0"]` appended after the body: one
merged 32-bit table (key width told FNV1a from XXH64 and nothing else), the redundant hashes
written out in hex, and a file ritobin refuses. `read_trailer`/`strip_trailer` stay so it can be
read; nothing writes it.

```rust
let legacy = rs_bin::read_trailer(&bin.trailing);
let map = rs_bin::capture(&bin, legacy.all_names());   // re-files them by category
rs_bin::write_path_map(&mut bin, &map);                // writes the entry, drops the footer
```

`capture` re-derives the category from where the bin actually uses each hash, which is
information the footer threw away. A name whose hash the bin no longer references anywhere is
dropped — it was dead weight.

## 6. Size

60 recorded names (40 repathed `.dds` paths, 20 invented emitter names):

| | Raw | Compressed |
|---|---|---|
| `CELMAP` JSON footer | 4573 B | 803 B |
| `ritobinmap` entry | 3560 B | 278 B |
| | **−22%** | **−65%** |

Raw, the saving is the hex keys and JSON punctuation. Compressed — which is what actually ships,
since WAD chunks are zstd — it is far larger: paths share long prefixes and compress several
times over, while a hash is by construction incompressible noise. Dropping the hashes is the
single biggest win in the format.

## 7. Compatibility

| Reader | `CELMAP` footer | `ritobinmap` entry |
|---|---|---|
| League client | ignored (past the declared body) | entry with an unknown class — **verify in-game before shipping** |
| ritobin (cli/gui) | **read fails** (`cur_ == cap_`) | reads, prints, converts back |
| C# LeagueToolkit | reads (ignores the tail) | reads and rewrites |
| `rs_bin` | read-only, migrates | full support |
| Flint | reads/writes today, needs the migration | pending the rev bump |
| Quartz | never implemented | pending |

**What still destroys the record:** a tool that reserializes the bin from a tree it built without
the entry, or a text round-trip through a tool that drops unknown entries. Both are the same
failure the footer had, and both are avoided by carrying the entry through — which every parser
now does for free, because it is just an entry.

**Open item.** An entry whose class hash the client does not know is expected to be skipped
(entries are length-prefixed and the class table is read up front), and orphaned entries of known
classes are routinely harmless. That specific case has not yet been confirmed in a running
client — do it before a tool writes this into a shipped mod.

## 8. Implementing it in another tool

The minimum, in any language:

1. Read the entry whose class hash is `0xad28fe20`.
2. For each of the five field hashes, read a `list[string]`.
3. To resolve a `file` value, hash each string in `game` with XXH64 (lowercased, seed 0) and
   compare. For a `hash`/`link`/class/field, hash the strings of the matching list with FNV1a-32
   (lowercased). Build the lookup once per bin.
4. When writing a bin, carry the entry through unchanged unless you are re-capturing.
5. Never resolve a hash from a list other than the one for its position.
