# rs_audio

Container-level reader/writer for the two Wwise audio containers League ships: `.wpk`
and `.bnk`. The crate operates strictly at the **container** level — it extracts and
repacks the embedded `.wem` audio blobs and preserves every other byte verbatim. It does
**not** interpret the Wwise event graph, object hierarchy, or decode the `.wem` codecs.

## Scope

| Concern | In scope | Out of scope |
|---|---|---|
| Locate / extract embedded `.wem` blobs | yes | — |
| Byte-exact round-trip of the whole container | yes | — |
| Preserve unknown / unparsed sections verbatim | yes | — |
| Decode the Wwise HIRC object graph (events, sounds, actions) | no | yes |
| Decode / transcode `.wem` audio (Vorbis/Opus/etc.) | no | yes |

Decoding the HIRC hierarchy is deliberately avoided: it is version-fragile (the Python
reference reader crashes on the BKHD-version-145 banks in our sample set), and decoding it
is unnecessary for lossless extract/repack. Keeping HIRC and every other unknown section as
opaque bytes is what guarantees the round-trip contract.

## Container models

### BNK — Wwise SoundBank

A flat sequence of chunked sections, each:

```
[ 4-byte ASCII tag ][ u32 size (LE) ][ size bytes of body ]
```

Sections seen in League banks: `BKHD` (bank header: version + id), `DIDX` (data index:
one `(id: u32, offset: u32, size: u32)` triple per embedded wem), `DATA` (the concatenated
wem bytes, addressed by DIDX offsets/sizes), `HIRC` (object hierarchy), and others such as
`STID`/`STMG`/`INIT`/`ENVS`/`PLAT` which this crate treats as opaque. The reader keeps every
section in order as a raw `(tag, body)` pair, so unknown sections survive untouched.

`_audio.bnk` banks carry the audio via `DIDX`/`DATA`; `_events.bnk` banks carry `BKHD`/`HIRC`
and contain no embedded wems. (Note: the `_audio`/`_events` naming is a convention, not a
guarantee — one sample `_audio.bnk` in our set contains only `BKHD`.)

### WPK — Wwise audio package

```
[ "r3d2" magic ][ u32 version ][ u32 count ][ count * u32 entry-offset ]
   then per entry: [ u32 data_offset ][ u32 size ][ u32 name_len ][ name_len * u16 UTF-16-LE ]
   then the audio blobs.
```

Each entry names a `.wem` and points at its bytes. The writer rebuilds a canonical layout:
header, the entry-offset array, packed entries, then the audio blobs in order.

## API

Both types follow the workspace-standard `Parse` / `Serialize` traits:

```rust
use rs_audio::Bnk;
use rs_io::{Parse, Serialize};

let bnk = Bnk::from_path("aatrox_base_sfx_audio.bnk")?;
for (id, bytes) in bnk.wems() {        // (u32 id, &[u8] wem body)
    // write bytes to <id>.wem ...
}
let out = bnk.to_bytes()?;             // byte-identical to the input
```

| Method | Meaning |
|---|---|
| `Bnk::from_path` / `from_bytes` / `from_reader` | parse a SoundBank |
| `Bnk::wems() -> Vec<(u32, &[u8])>` | embedded wems via DIDX/DATA; empty if absent |
| `Bnk::to_bytes` / `to_path` / `to_writer` | byte-exact serialize |
| `Wpk::from_path` / … | parse a `.wpk` package |
| `Wpk::to_bytes` / … | serialize a `.wpk` package |

`wems()` borrows from the parsed `DATA` body; an out-of-range DIDX entry is skipped rather
than panicking. There are no panics in library code; malformed input returns `Err`.

## Test fixtures

Real game audio is **gitignored**. Drop sample `.bnk` / `.wpk` files in the workspace
`sample-files/` directory; `tests/real_files.rs` skips gracefully when they are absent.
We currently have `.bnk` samples only — see the report for the `.wpk` coverage gap.
