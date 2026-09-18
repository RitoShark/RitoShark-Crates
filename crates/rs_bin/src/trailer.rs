/*! Legacy `CELMAP` footer, read-only.

The first version of the hash-to-path record lived after the declared bin body:

```text
[ payload (JSON: {"<hex hash>": "<path>", ...}) ][ u32 len (LE) ][ b"CELMAP\0\0" ]
```

Nothing writes it any more. ritobin ends its read with `bin_assert(reader.cur_ == reader.cap_)`,
so a single byte past the body fails the file outright — the record moved into the bin proper as
[`crate::PathMap`]. These two stay so a bin authored with the old footer can still be read and
migrated: hand the names to [`crate::capture`], which sorts them into the categories the JSON
blob merged, then [`crate::write_path_map`], which drops the footer.
*/

use std::collections::BTreeMap;

const MAGIC: [u8; 8] = *b"CELMAP\0\0";
const FOOTER: usize = 4 + MAGIC.len();

/// Hash-to-path pairs recovered from a legacy footer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Trailer {
    pub names: BTreeMap<u32, String>,
    pub files: BTreeMap<u64, String>,
}

impl Trailer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.names.is_empty() && self.files.is_empty()
    }

    pub fn len(&self) -> usize {
        self.names.len() + self.files.len()
    }

    /// Every recorded name, unkeyed, ready for [`crate::capture`] to re-file by category.
    pub fn all_names(&self) -> impl Iterator<Item = &str> {
        self.names
            .values()
            .chain(self.files.values())
            .map(String::as_str)
    }
}

/// Decodes the footer at the end of `trailing`. A missing, truncated or unparseable record
/// reads as an empty [`Trailer`] — it was optional data appended behind the format's back,
/// never a reason to fail a bin.
pub fn read_trailer(trailing: &[u8]) -> Trailer {
    let Some(payload) = payload_slice(trailing) else {
        return Trailer::new();
    };
    let Ok(entries) = serde_json::from_slice::<BTreeMap<String, String>>(payload) else {
        return Trailer::new();
    };
    let mut trailer = Trailer::new();
    for (hex, path) in entries {
        match hex.len() {
            8 => {
                if let Ok(hash) = u32::from_str_radix(&hex, 16) {
                    trailer.names.insert(hash, path);
                }
            }
            16 => {
                if let Ok(hash) = u64::from_str_radix(&hex, 16) {
                    trailer.files.insert(hash, path);
                }
            }
            _ => {}
        }
    }
    trailer
}

/// `trailing` without its footer, keeping any unrelated bytes that preceded it.
pub fn strip_trailer(trailing: &[u8]) -> &[u8] {
    match payload_slice(trailing) {
        Some(payload) => &trailing[..trailing.len() - FOOTER - payload.len()],
        None => trailing,
    }
}

fn payload_slice(trailing: &[u8]) -> Option<&[u8]> {
    let end = trailing.len().checked_sub(FOOTER)?;
    if trailing[end + 4..] != MAGIC {
        return None;
    }
    let len = u32::from_le_bytes([
        trailing[end],
        trailing[end + 1],
        trailing[end + 2],
        trailing[end + 3],
    ]) as usize;
    let start = end.checked_sub(len)?;
    Some(&trailing[start..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Trailer {
        let mut trailer = Trailer::new();
        trailer
            .names
            .insert(0x1234abcd, "MyCustomEmitterName".to_string());
        trailer.files.insert(
            0x0123456789abcdef,
            "ASSETS/Characters/Ahri/Skins/Skin99/Custom \"quoted\".dds".to_string(),
        );
        trailer
    }

    fn legacy_bytes(before: &[u8], trailer: &Trailer) -> Vec<u8> {
        let mut entries: BTreeMap<String, &str> = BTreeMap::new();
        for (hash, path) in &trailer.names {
            entries.insert(format!("{hash:08x}"), path);
        }
        for (hash, path) in &trailer.files {
            entries.insert(format!("{hash:016x}"), path);
        }
        let payload = serde_json::to_vec(&entries).expect("encode");
        let mut out = before.to_vec();
        out.extend_from_slice(&payload);
        out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        out.extend_from_slice(&MAGIC);
        out
    }

    #[test]
    fn reads_a_footer_written_by_the_old_writer() {
        assert_eq!(read_trailer(&legacy_bytes(&[], &sample())), sample());
    }

    #[test]
    fn key_width_decides_the_hash_kind() {
        let bytes = legacy_bytes(&[], &sample());
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("\"1234abcd\""));
        assert!(text.contains("\"0123456789abcdef\""));
    }

    #[test]
    fn every_name_comes_back_unkeyed_for_recapture() {
        let trailer = sample();
        let names: Vec<&str> = trailer.all_names().collect();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"MyCustomEmitterName"));
    }

    #[test]
    fn unrelated_trailing_bytes_survive() {
        let foreign = b"someone else's footer".to_vec();
        let bytes = legacy_bytes(&foreign, &sample());
        assert_eq!(read_trailer(&bytes), sample());
        assert_eq!(strip_trailer(&bytes), &foreign[..]);
    }

    #[test]
    fn a_bin_without_a_footer_reads_empty() {
        assert!(read_trailer(&[]).is_empty());
        assert!(read_trailer(b"PROP not a footer").is_empty());
    }

    #[test]
    fn a_truncated_record_is_ignored_rather_than_panicking() {
        let bytes = legacy_bytes(&[], &sample());
        for cut in 1..bytes.len() {
            let damaged = bytes[cut..].to_vec();
            let _ = read_trailer(&damaged);
            let _ = strip_trailer(&damaged);
        }
        let mut lying = bytes.clone();
        let len = lying.len();
        lying[len - FOOTER..len - MAGIC.len()].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(read_trailer(&lying).is_empty());
        assert_eq!(strip_trailer(&lying), &lying[..]);
    }
}
