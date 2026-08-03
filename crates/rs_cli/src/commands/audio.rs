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
