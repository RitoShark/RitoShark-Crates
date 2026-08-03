# rs_audio — Wwise BNK, WPK and WEM audio in Rust

**Read, decode and edit Audiokinetic Wwise soundbanks in pure Rust. Convert `.wem` to `.ogg`
losslessly, extract and repack `.bnk` and `.wpk` containers, resolve Wwise events to the sounds
they play, and mute or replace audio without an external Wwise toolchain.**

Built for League of Legends modding, where every audio file the game ships is a Wwise container,
but nothing here is League-specific except the fixtures it is tested against.

```rust
use rs_audio::{Bnk, Wem};
use rs_io::Parse;

let bank = Bnk::from_path("aatrox_base_sfx_audio.bnk")?;

for (id, payload) in bank.wems() {
    let ogg = Wem::new(payload)?.to_ogg()?;   // lossless — no re-encode
    std::fs::write(format!("{id}.ogg"), ogg)?;
}
```

---

## What it does

- **Converts WEM to OGG losslessly.** Wwise Vorbis strips the Vorbis headers and replaces the
  codebooks with indices into an external library. `rs_audio` rebuilds the headers, expands the
  codebooks and repages the audio packets. The compressed audio is copied bit for bit — this is a
  remux, not a transcode, so it cannot degrade quality.
- **Decodes to PCM samples** for waveform display, trimming or any other sample-level work.
- **Encodes PCM back to WEM**, so replacing a sound needs no Wwise installation.
- **Extracts and repacks `.bnk` and `.wpk`** byte-exactly, preserving unknown sections verbatim.
- **Edits banks without destroying them** — the header, bank id and object hierarchy survive.
- **Resolves Wwise events to the WEM ids they play**, which is what "mute this voice line"
  actually requires.

## Install

```toml
[dependencies]
rs_audio = { git = "https://github.com/RitoShark/RitoShark-Crates" }
```

Or through the umbrella crate, which re-exports it as `ritoshark::audio`:

```toml
ritoshark = { git = "https://github.com/RitoShark/RitoShark-Crates", features = ["audio"] }
```

---

## Common tasks

### Convert a WEM file to OGG Vorbis

```rust
use rs_audio::Wem;

let bytes = std::fs::read("225608650.wem")?;
let ogg = Wem::new(&bytes)?.to_ogg()?;
std::fs::write("225608650.ogg", ogg)?;
```

`decode()` picks the right container automatically — Ogg for Vorbis, WAV for PCM — and reports
which one it produced:

```rust
use rs_audio::{AudioFormat, Wem};

let audio = Wem::new(&bytes)?.decode()?;
let extension = match audio.format {
    AudioFormat::Ogg => "ogg",
    AudioFormat::Wav => "wav",
};
println!("{} Hz, {} channels", audio.sample_rate, audio.channels);
```

### Extract every sound from a soundbank or package

```rust
use rs_audio::{Bnk, Wpk};
use rs_io::Parse;

let bank = Bnk::from_path("champion_sfx_audio.bnk")?;
for (id, payload) in bank.wems() { /* … */ }

let package = Wpk::from_path("champion_vo.wpk")?;
for (id, name, payload) in package.wems() { /* … */ }
```

### Mute a sound without breaking the bank

```rust
use rs_audio::Bnk;
use rs_io::{Parse, Serialize};

let mut bank = Bnk::from_path("champion_sfx_audio.bnk")?;
bank.silence_wem(225608650)?;      // same rate and channel count, silent
bank.to_path("champion_sfx_audio.bnk")?;
```

The entry keeps its id, so every event, action and container that referenced it still resolves.
Removing the entry instead would leave dangling references in the hierarchy — use `remove_wem`
only when you mean it.

### Replace a sound with your own audio

```rust
use rs_audio::{Bnk, PcmAudio, encode_pcm};
use rs_io::{Parse, Serialize};

let audio = PcmAudio::new(44100, 1, my_samples);   // interleaved i16
let wem = encode_pcm(&audio)?;

let mut bank = Bnk::from_path("champion_vo_audio.bnk")?;
bank.replace_wem(225608650, wem)?;
bank.to_path("champion_vo_audio.bnk")?;
```

### Find which sounds an event plays

```rust
use rs_audio::Bnk;
use rs_io::Parse;

let events_bank = Bnk::from_path("champion_sfx_events.bnk")?;

if let Some(hierarchy) = events_bank.hirc()? {
    for (event_id, wem_ids) in hierarchy.event_wem_map() {
        println!("event {event_id} plays {wem_ids:?}");
    }
}
```

Events usually live in a `_events.bnk` while the audio lives in the companion `_audio.bnk` or a
`.wpk`, so the ids resolved here are looked up in the other file.

---

## Command line

The workspace binary `rs_cli` exposes the same functionality:

```sh
rs_cli audio info     champion_sfx_events.bnk    # header, sections, codecs, events
rs_cli audio extract  champion_sfx_audio.bnk -o out/   # raw .wem files
rs_cli audio decode   champion_sfx_audio.bnk -o out/   # playable .ogg / .wav
```

```
$ rs_cli audio info champion_sfx_events.bnk
format:     bnk
version:    145
bank id:    1765265800
sections:   BKHD, HIRC
wems:       0
hierarchy:  564 objects (13 opaque), 239 sounds
events:     69 (64 resolve to audio)
```

---

## Format reference

Original measurements, not folklore. Every number below was produced by extracting a live League
of Legends installation and classifying what came out.

### Which codecs League actually uses

Across **13,420 embedded WEMs** in **1,398 banks and packages**, sampled from champions old and
new, maps, the global bank and companions:

| Codec id | Meaning | Share |
|---|---|---|
| `0xFFFF` | Wwise Vorbis | **100%** |

No Opus, no PCM, no ADPCM anywhere in the game. Channel counts are 1 or 2 only. Sample rates are
**not** uniformly 44.1 kHz — 44100 dominates, but 48000, 36000, 32000, 24000, 16000, 12000, 8000
and 6000 all occur, so code must never assume a rate.

Bank header revisions in circulation: **145** and **134**.

### BNK — Wwise SoundBank

A flat sequence of sections, each `4-byte tag + u32 size + body`, walked to end of file:

| Tag | Contents |
|---|---|
| `BKHD` | Bank header: version, bank id |
| `DIDX` | Data index: one `(id, offset, size)` `u32` triple per embedded WEM |
| `DATA` | The concatenated WEM payloads, addressed by DIDX |
| `HIRC` | The object hierarchy: events, actions, sounds, containers |
| `STID` `STMG` `INIT` `ENVS` `PLAT` | Settings and platform data, in the init bank |

Payloads inside `DATA` are aligned to **16 bytes**, with no padding after the last one.

The `_audio` / `_events` filename split is a convention, not a guarantee — one shipped
`_audio.bnk` contains only `BKHD` and no audio at all. Always key off the sections present.

### WPK — Wwise audio package

```
"r3d2" magic │ u32 version │ u32 slot count │ slot_count × u32 entry offset
  per live entry: u32 data offset │ u32 size │ u32 name length │ UTF-16-LE name
  then the audio payloads
```

Riot aligns **three** regions to an 8-byte boundary: the offset table, each entry record, and
each audio payload. Reproducing that single rule is what makes a real package re-serialize byte
for byte.

Offset-table slots may be `0` — **dead slots**, pointing at nothing. Their positions are preserved
so the table keeps its length.

### WEM — the audio file itself

A RIFF/WAVE container. `fmt ` declares the codec; Wwise Vorbis uses codec `0xFFFF` with a 0x42-byte
extended `fmt ` (or a separate `vorb` chunk on older files) carrying the sample count, the setup
packet offset and the block sizes.

---

## How the Vorbis decoding works

Wwise ships Vorbis with everything removable removed. Restoring a playable stream means:

1. **Synthesising the identification and comment headers** from the `fmt ` parameters — Wwise
   stores none of the three standard Vorbis header packets.
2. **Expanding the codebooks.** The file references codebooks by index into the aoTuV 6.03
   library, which is bundled here. The packed form also narrows several fields, so each is re-read
   at its packed width and re-emitted at its spec width.
3. **Re-emitting the setup header**, validating every index into the codebook, floor and residue
   tables along the way.
4. **Repaging the audio packets.** Wwise uses short length-prefixed packet headers instead of Ogg
   pages. The payloads are copied verbatim.
5. **Reconstructing granule positions.** The compact 2-byte packet headers carry no granule at
   all, so the position is recomputed the way an encoder does — each packet advances the stream by
   a quarter of its own window plus a quarter of the previous one — and the final page is clamped
   to the declared sample count.

Steps 1–4 are the well-known part. Step 5 is easy to get wrong and fails silently: the stream
still decodes, just with the tail of every file missing.

---

## Correctness

The workspace's whole reason to exist is lossless, correct round-trips, and this crate is held to
the same standard.

- **Byte-exact round-trip** on real game files, per container and per bank revision. 29 real
  `.wpk` packages and banks of both header versions, including the init bank whose
  `INIT`/`STMG`/`ENVS`/`PLAT` sections appear nowhere else.
- **Cross-validated against an independent implementation.** Decoded audio is compared
  sample-for-sample against the [`ww2ogg`](https://crates.io/crates/ww2ogg) crate across every
  fixture. This is the check that catches a mis-rebuilt codebook or a misread setup field, which a
  self-consistent round-trip never could.
- **Cross-file validation of the event graph.** Events resolved by walking one file's object
  hierarchy are checked against a completely separate file's index — currently **478 of 478**
  resolved ids match.
- **No-op edit identity.** Replacing a payload with itself produces a byte-identical file.
- **No panics.** Every reader bounds each declared size and offset against the real input before
  allocating or slicing. Truncated, over-declared and adversarial input returns `Err`.

## Design notes

**Unknown sections are kept verbatim.** A bank round-trips byte for byte because sections we do
not model are stored as opaque bytes rather than reconstructed. This is also why editing is safe:
an edit modifies the parsed container instead of rebuilding it, so the header revision, the bank id
and the object hierarchy pass through untouched.

**The hierarchy is decoded only where it matters.** `HIRC` is version-fragile — parsers that decode
it fully tend to break on revisions they predate. Here only the object types on the path from an
event to a WEM are decoded; everything else, and anything that fails to parse, is kept opaque. The
per-object length prefix means one unfamiliar object costs one object, not the whole section. Since
the bank stores the section's bytes verbatim, this can never threaten the round-trip contract.

---

## FAQ

### How do I convert a .wem file to .ogg or .mp3?

`Wem::new(&bytes)?.to_ogg()?` gives you Ogg Vorbis with no quality loss, because the compressed
audio is copied rather than re-encoded. There is no MP3 output — going to MP3 would mean decoding
and re-encoding, which loses quality and needs an encoder this crate deliberately does not carry.
Decode to Ogg or to PCM samples and hand those to an encoder of your choice.

### Do I need ww2ogg, vgmstream or the Wwise SDK?

No. Decoding, encoding and editing are all in-process. The `ww2ogg` crate is used only as a
test-time correctness oracle and is not a runtime dependency.

### Can it edit soundbanks without corrupting them?

Yes, and that is the point. Editing modifies the parsed container: only `DIDX` and `DATA` are
rewritten, because only they changed. The bank header keeps its version and id, and `HIRC` and
every unknown section pass through byte for byte. A tool that rebuilds the bank from scratch on
each edit will reset the header and drop the hierarchy, producing a file the engine cannot load
even though the audio inside it is fine.

### How do I mute a single voice line?

Parse the `_events.bnk`, call `event_wem_map()` to find which WEM ids the event plays, then call
`silence_wem(id)` on whichever bank or package holds those payloads. Silencing keeps the entry, so
nothing in the hierarchy is left dangling.

### Why is a replaced sound so much larger than the original?

Encoding writes PCM, which is several times larger than the Vorbis the game ships. That is the
trade for not requiring a Wwise installation. Wwise Vorbis encoding is planned; see below.

### Does it support Wwise Opus, ADPCM, XMA or AT9?

Not currently. Every WEM in League is Wwise Vorbis, so those codecs have never appeared in a file
this crate needs to read. An unrecognised codec parses and reports itself rather than failing
anonymously, so adding one is a contained change if a real file ever calls for it.

### Does it work outside League of Legends?

The container and codec handling is ordinary Wwise, so other games using the same Wwise revisions
should work. Only the test fixtures and the codec census are League-specific. Bank revisions far
from 134–145 are untested, and the hierarchy parser degrades to opaque rather than guessing.

---

## Scope and limitations

| Concern | Supported |
|---|---|
| Extract, repack, byte-exact round-trip of `.bnk` and `.wpk` | yes |
| Preserve unknown sections verbatim | yes |
| Wwise Vorbis → Ogg, lossless | yes |
| WEM → PCM samples | yes |
| PCM → WEM | yes |
| Event → WEM resolution | yes |
| Wwise Vorbis **encoding** | not yet — planned |
| Wwise ADPCM encoding | no — see below |
| Opus, XMA, AT9, HEVAG | no |
| Sample-level editing: trim, gain, fade, resample | out of scope, by design |
| Reading user `.mp3` / `.flac` files | out of scope, by design |
| Authoring a new soundbank from nothing | no |

Sample-level editing and user-file import are deliberately left to applications; this crate is a
format library, and PCM samples are the handoff boundary in both directions.

**Two honest caveats:**

*PCM encoding is not yet verified in-game.* League ships no PCM WEM, so the layout follows the
documented Wwise extended-format structure rather than a shipped example. PCM is a core sound-engine
source rather than a codec plugin, so it should play — but "should" is not "does", and this is
worth testing before relying on it.

*ADPCM encoding is not implemented.* Implementing it would mean guessing at Wwise's specific ADPCM
variant with no reference file and no oracle to check against, since the game contains none. PCM
works today; Wwise Vorbis encoding is the more valuable next step and is the one planned.

---

## Test fixtures

Real game audio is copyrighted and is **never** committed. Drop `.bnk` and `.wpk` samples into the
workspace `Sample-Files/` directory; every test skips gracefully when its fixture is absent.

## License

MIT OR Apache-2.0, matching the rest of the workspace.

The bundled aoTuV 6.03 codebook library is required to decode Wwise Vorbis, which references it by
index rather than embedding it.

## Related

Part of [RitoShark](https://github.com/RitoShark/RitoShark-Crates), a Rust workspace for reading and
writing League of Legends file formats: `.bin`/PROP, `.wad`, `.tex`, `.skn`, `.skl`, `.anm`,
`.mapgeo`, `.stringtable`, `.manifest` and more.
