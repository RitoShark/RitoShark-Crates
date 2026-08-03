#![forbid(unsafe_code)]
/*!
The `audio extract` command writes every `.wem` from a `.wpk` or `.bnk` container into a
directory, naming each by its entry name when present and by id or index otherwise.  Container
parsing is done in-process through `rs_audio`.

Path safety: every candidate filename is passed through [`crate::pathsafe::safe_relative`] so
that adversarial entry names (containing `..`, `/`, or absolute roots) cannot escape the output
directory.

Collision handling: a [`std::collections::HashSet`] tracks every relative path already written in
this invocation.  When a collision is detected the entry is renamed by inserting an incrementing
counter before the extension (`name (1).wem`, `name (2).wem`, …) until a free slot is found.
*/

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::error::{CliError, Result};

/// Return the extension of `p` as a `&str`, or `None`.
fn ext_str(p: &Path) -> Option<&str> {
    p.extension().and_then(|e| e.to_str())
}

/// Return the first path derived from `base` (a relative `PathBuf`) that is
/// not present in `seen`.  If `base` itself is free it is returned unchanged;
/// otherwise successive `(1)`, `(2)`, … suffixes are tried.  The result is
/// inserted into `seen` before returning.
fn dedup(base: PathBuf, seen: &mut HashSet<PathBuf>) -> PathBuf {
    if seen.insert(base.clone()) {
        return base;
    }
    let stem = base
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ext_part = ext_str(&base).map(|e| format!(".{e}")).unwrap_or_default();
    let parent = base.parent().map(Path::to_path_buf).unwrap_or_default();
    let mut n: u32 = 1;
    loop {
        let candidate = parent.join(format!("{stem} ({n}){ext_part}"));
        if seen.insert(candidate.clone()) {
            return candidate;
        }
        n += 1;
    }
}

pub fn extract(input: &Path, out: &Path) -> Result<()> {
    use ritoshark::prelude::*;
    std::fs::create_dir_all(out)?;
    let kind = ritoshark::file::detect_path(input)?;
    let mut seen: HashSet<PathBuf> = HashSet::new();
    match kind {
        ritoshark::file::FileKind::Wpk => {
            let wpk = ritoshark::audio::Wpk::from_path(input)?;
            for (idx, (id, name, data)) in wpk.wems().into_iter().enumerate() {
                // Naming precedence: non-empty name → id-based → index-based.
                let raw = if !name.is_empty() {
                    name.to_string()
                } else if let Some(id) = id {
                    format!("{id}.wem")
                } else {
                    format!("{idx}.wem")
                };
                let safe = match crate::pathsafe::safe_relative(&raw) {
                    Some(p) => p,
                    None => {
                        eprintln!("audio extract: skipping unsafe path: {raw}");
                        continue;
                    }
                };
                let rel = dedup(safe, &mut seen);
                if rel.to_string_lossy() != raw {
                    eprintln!(
                        "audio extract: collision — renamed '{}' → '{}'",
                        raw,
                        rel.display()
                    );
                }
                std::fs::write(out.join(&rel), data)?;
            }
        }
        ritoshark::file::FileKind::Bnk => {
            let bnk = ritoshark::audio::Bnk::from_path(input)?;
            for (id, data) in bnk.wems() {
                let raw = format!("{id}.wem");
                // Bnk ids are numeric so safe_relative always succeeds, but
                // route through the guard for uniformity.
                let safe = match crate::pathsafe::safe_relative(&raw) {
                    Some(p) => p,
                    None => {
                        eprintln!("audio extract: skipping unsafe path: {raw}");
                        continue;
                    }
                };
                let rel = dedup(safe, &mut seen);
                if rel.to_string_lossy() != raw {
                    eprintln!(
                        "audio extract: collision — renamed '{}' → '{}'",
                        raw,
                        rel.display()
                    );
                }
                std::fs::write(out.join(&rel), data)?;
            }
        }
        other => return Err(CliError::msg(format!("not an audio container: {other:?}"))),
    }
    Ok(())
}

/** Every `(id, bytes)` pair an input holds: the embedded payloads of a container, or the single
payload of a bare `.wem` file named after its stem. */
fn payloads(input: &Path) -> Result<Vec<(String, Vec<u8>)>> {
    use ritoshark::prelude::*;

    let bytes = std::fs::read(input)?;

    if bytes.starts_with(b"RIFF") || bytes.starts_with(b"RIFX") {
        let name = input
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "audio".into());
        return Ok(vec![(name, bytes)]);
    }

    match ritoshark::file::detect_path(input)? {
        ritoshark::file::FileKind::Wpk => {
            let wpk = ritoshark::audio::Wpk::from_path(input)?;
            Ok(wpk
                .wems()
                .into_iter()
                .enumerate()
                .map(|(idx, (id, name, data))| {
                    let stem = id.map(|i| i.to_string()).unwrap_or_else(|| {
                        Path::new(name)
                            .file_stem()
                            .map(|s| s.to_string_lossy().into_owned())
                            .unwrap_or_else(|| idx.to_string())
                    });
                    (stem, data.to_vec())
                })
                .collect())
        }
        ritoshark::file::FileKind::Bnk => {
            let bnk = ritoshark::audio::Bnk::from_path(input)?;
            Ok(bnk
                .wems()
                .into_iter()
                .map(|(id, data)| (id.to_string(), data.to_vec()))
                .collect())
        }
        other => Err(CliError::msg(format!("not an audio container: {other:?}"))),
    }
}

/** Decodes embedded audio to playable files. Wwise Vorbis is remuxed to Ogg without re-encoding
and PCM is wrapped as WAV, so nothing here is lossy. */
pub fn decode(input: &Path, out: &Path) -> Result<()> {
    use ritoshark::audio::{AudioFormat, Wem};

    std::fs::create_dir_all(out)?;
    let entries = payloads(input)?;
    if entries.is_empty() {
        eprintln!("audio decode: no embedded audio found");
        return Ok(());
    }

    let mut seen: HashSet<PathBuf> = HashSet::new();
    let (mut decoded, mut failed) = (0usize, 0usize);

    for (stem, bytes) in entries {
        let audio = match Wem::new(&bytes).and_then(|w| w.decode()) {
            Ok(audio) => audio,
            Err(e) => {
                eprintln!("audio decode: skipping {stem}: {e}");
                failed += 1;
                continue;
            }
        };

        let extension = match audio.format {
            AudioFormat::Ogg => "ogg",
            AudioFormat::Wav => "wav",
        };
        let raw = format!("{stem}.{extension}");
        let Some(safe) = crate::pathsafe::safe_relative(&raw) else {
            eprintln!("audio decode: skipping unsafe path: {raw}");
            continue;
        };

        std::fs::write(out.join(dedup(safe, &mut seen)), &audio.data)?;
        decoded += 1;
    }

    eprintln!("audio decode: {decoded} decoded, {failed} skipped");
    Ok(())
}

/** Summarises a container: header revision, sections, embedded audio and its codecs, and — when
the bank carries an object hierarchy — how many events resolve to how many sounds. */
pub fn info(input: &Path) -> Result<()> {
    use ritoshark::audio::{Bnk, Wem, Wpk};
    use ritoshark::prelude::*;

    match ritoshark::file::detect_path(input)? {
        ritoshark::file::FileKind::Bnk => {
            let bnk = Bnk::from_path(input)?;
            println!("format:     bnk");
            println!(
                "version:    {}",
                bnk.version()
                    .map_or_else(|| "unknown".into(), |v| v.to_string())
            );
            println!(
                "bank id:    {}",
                bnk.bank_id()
                    .map_or_else(|| "unknown".into(), |v| v.to_string())
            );
            let tags: Vec<String> = bnk
                .sections
                .iter()
                .map(|s| String::from_utf8_lossy(&s.tag).into_owned())
                .collect();
            println!("sections:   {}", tags.join(", "));
            report_wems(bnk.wems().into_iter().map(|(_, d)| d));

            if let Some(hirc) = bnk.hirc()? {
                let events = hirc.event_wem_map();
                let resolved = events.iter().filter(|(_, w)| !w.is_empty()).count();
                println!(
                    "hierarchy:  {} objects ({} opaque), {} sounds",
                    hirc.objects.len(),
                    hirc.opaque_count(),
                    hirc.sounds().count()
                );
                println!("events:     {} ({resolved} resolve to audio)", events.len());
            }
        }
        ritoshark::file::FileKind::Wpk => {
            let wpk = Wpk::from_path(input)?;
            println!("format:     wpk");
            println!("version:    {}", wpk.version);
            println!(
                "entries:    {} ({} dead slots)",
                wpk.entries.len(),
                wpk.dead_slots.len()
            );
            report_wems(wpk.wems().into_iter().map(|(_, _, d)| d));
        }
        other => {
            let bytes = std::fs::read(input)?;
            let wem = Wem::new(&bytes)
                .map_err(|_| CliError::msg(format!("not an audio file: {other:?}")))?;
            let format = wem.format();
            println!("format:     wem");
            println!("codec:      {:?}", format.codec);
            println!("channels:   {}", format.channels);
            println!("rate:       {} Hz", format.sample_rate);
        }
    }

    Ok(())
}

/** Reads a 16-bit PCM `.wav` into samples.

Deliberately minimal and CLI-local: `rs_audio` is a Wwise format library and does not carry a
general audio importer. Anything more exotic than 16-bit PCM should be converted first. */
fn read_wav(path: &Path) -> Result<ritoshark::audio::PcmAudio> {
    let bytes = std::fs::read(path)?;
    if bytes.len() < 12 || &bytes[..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(CliError::msg("not a RIFF/WAVE file"));
    }

    let le16 = |at: usize| u16::from_le_bytes([bytes[at], bytes[at + 1]]);
    let le32 =
        |at: usize| u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]);

    let (mut format, mut data) = (None, None);
    let mut at = 12usize;
    while at + 8 <= bytes.len() {
        let tag = &bytes[at..at + 4];
        let len = le32(at + 4) as usize;
        let body = at + 8;
        if body + len > bytes.len() {
            break;
        }
        match tag {
            b"fmt " => format = Some(body),
            b"data" => data = Some((body, len)),
            _ => {}
        }
        at = body + len + (len & 1);
    }

    let fmt = format.ok_or_else(|| CliError::msg("wav has no fmt chunk"))?;
    let (data_at, data_len) = data.ok_or_else(|| CliError::msg("wav has no data chunk"))?;

    let bits = le16(fmt + 14);
    if bits != 16 {
        return Err(CliError::msg(format!(
            "only 16-bit PCM wav is supported, got {bits}-bit"
        )));
    }

    let channels = le16(fmt + 2);
    let sample_rate = le32(fmt + 4);
    let samples = bytes[data_at..data_at + data_len]
        .chunks_exact(2)
        .map(|p| i16::from_le_bytes([p[0], p[1]]))
        .collect();

    Ok(ritoshark::audio::PcmAudio::new(
        sample_rate,
        channels,
        samples,
    ))
}

fn load_container(input: &Path) -> Result<Container> {
    use ritoshark::prelude::*;
    match ritoshark::file::detect_path(input)? {
        ritoshark::file::FileKind::Bnk => {
            Ok(Container::Bank(ritoshark::audio::Bnk::from_path(input)?))
        }
        ritoshark::file::FileKind::Wpk => {
            Ok(Container::Package(ritoshark::audio::Wpk::from_path(input)?))
        }
        other => Err(CliError::msg(format!("not an audio container: {other:?}"))),
    }
}

enum Container {
    Bank(ritoshark::audio::Bnk),
    Package(ritoshark::audio::Wpk),
}

impl Container {
    fn payload(&self, id: u32) -> Option<&[u8]> {
        match self {
            Self::Bank(b) => b.wem(id),
            Self::Package(p) => p.wem(id),
        }
    }

    fn replace(&mut self, id: u32, bytes: Vec<u8>) -> Result<()> {
        match self {
            Self::Bank(b) => b.replace_wem(id, bytes)?,
            Self::Package(p) => p.replace_wem(id, bytes)?,
        }
        Ok(())
    }

    fn silence(&mut self, id: u32) -> Result<()> {
        match self {
            Self::Bank(b) => b.silence_wem(id)?,
            Self::Package(p) => p.silence_wem(id)?,
        }
        Ok(())
    }

    fn write(&self, out: &Path) -> Result<()> {
        use ritoshark::prelude::*;
        match self {
            Self::Bank(b) => b.to_path(out)?,
            Self::Package(p) => p.to_path(out)?,
        }
        Ok(())
    }
}

/** Replaces one sound with audio from a `.wav`, encoded as Wwise Vorbis.

The header template is cloned from the payload being replaced, so every field whose meaning is not
established carries a value the engine already accepts. */
pub fn replace(input: &Path, id: u32, wav: &Path, quality: f32, out: &Path) -> Result<()> {
    let mut container = load_container(input)?;
    let original = container
        .payload(id)
        .ok_or_else(|| CliError::msg(format!("no embedded wem with id {id}")))?
        .to_vec();

    let audio = read_wav(wav)?;
    let encoded = ritoshark::audio::encode_vorbis_like(&original, &audio, quality)?;

    eprintln!(
        "audio replace: wem {id}: {} -> {} bytes ({} frames, {} Hz, {}ch, quality {quality})",
        original.len(),
        encoded.len(),
        audio.frames(),
        audio.sample_rate,
        audio.channels
    );

    container.replace(id, encoded)?;
    container.write(out)?;
    Ok(())
}

/// Replaces one sound with silence at its original sample rate and channel count.
pub fn silence(input: &Path, id: u32, out: &Path) -> Result<()> {
    let mut container = load_container(input)?;
    container.silence(id)?;
    container.write(out)?;
    eprintln!("audio silence: wem {id} muted");
    Ok(())
}

fn report_wems<'a>(payloads: impl Iterator<Item = &'a [u8]>) {
    use ritoshark::audio::Wem;
    use std::collections::BTreeMap;

    let mut codecs: BTreeMap<String, usize> = BTreeMap::new();
    let mut rates: BTreeMap<u32, usize> = BTreeMap::new();
    let mut count = 0usize;

    for bytes in payloads {
        count += 1;
        match Wem::new(bytes) {
            Ok(wem) => {
                *codecs
                    .entry(format!("{:?}", wem.format().codec))
                    .or_default() += 1;
                *rates.entry(wem.format().sample_rate).or_default() += 1;
            }
            Err(_) => *codecs.entry("unreadable".into()).or_default() += 1,
        }
    }

    println!("wems:       {count}");
    if count > 0 {
        let codecs: Vec<String> = codecs.iter().map(|(k, n)| format!("{k} x{n}")).collect();
        println!("codecs:     {}", codecs.join(", "));
        let rates: Vec<String> = rates.iter().map(|(k, n)| format!("{k} Hz x{n}")).collect();
        println!("rates:      {}", rates.join(", "));
    }
}
