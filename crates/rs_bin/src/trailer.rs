use std::collections::BTreeMap;

/* The side table lives after the declared bin body, in `Bin::trailing`:

  [ payload (JSON: {"<hex hash>": "<path>", ...}) ][ u32 len (LE) ][ MAGIC ]

Keys are hex-encoded, and their WIDTH carries the hash kind: 8 for the FNV1a-32
of a `hash`/`link` value, 16 for the XXH64 of a `file` value. The record sits at
the very end, so anything else already in `trailing` is kept ahead of it. */

const MAGIC: [u8; 8] = *b"CELMAP\0\0";
const FOOTER: usize = 4 + MAGIC.len();

/// Hash-to-path pairs a tool captured while it still knew both, so paths that no
/// shared dictionary can resolve (repaths a mod invented) survive inside the bin
/// once the value is only a hash.
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
}

/// Decodes the side table at the end of `trailing`. A missing, truncated or
/// unparseable record reads as an empty `Trailer` — it is optional data appended
/// behind the format's back, never a reason to fail a bin.
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

/// `trailing` without its side table, keeping any unrelated bytes that preceded it.
pub fn strip_trailer(trailing: &[u8]) -> &[u8] {
    match payload_slice(trailing) {
        Some(payload) => &trailing[..trailing.len() - FOOTER - payload.len()],
        None => trailing,
    }
}

/// `trailing` with its side table replaced by `trailer`. Idempotent: appending
/// twice leaves one record. An empty `trailer` just strips.
pub fn append_trailer(trailing: &[u8], trailer: &Trailer) -> Vec<u8> {
    let base = strip_trailer(trailing);
    if trailer.is_empty() {
        return base.to_vec();
    }
    let payload = encode_payload(trailer);
    let mut out = Vec::with_capacity(base.len() + payload.len() + FOOTER);
    out.extend_from_slice(base);
    out.extend_from_slice(&payload);
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(&MAGIC);
    out
}

fn encode_payload(trailer: &Trailer) -> Vec<u8> {
    let mut entries: BTreeMap<String, &str> = BTreeMap::new();
    for (hash, path) in &trailer.names {
        entries.insert(format!("{hash:08x}"), path);
    }
    for (hash, path) in &trailer.files {
        entries.insert(format!("{hash:016x}"), path);
    }
    serde_json::to_vec(&entries).unwrap_or_default()
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

    #[test]
    fn round_trips_through_the_footer() {
        let bytes = append_trailer(&[], &sample());
        assert_eq!(read_trailer(&bytes), sample());
    }

    #[test]
    fn key_width_decides_the_hash_kind() {
        let bytes = append_trailer(&[], &sample());
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("\"1234abcd\""));
        assert!(text.contains("\"0123456789abcdef\""));
    }

    #[test]
    fn appending_twice_leaves_one_record() {
        let once = append_trailer(&[], &sample());
        let twice = append_trailer(&once, &sample());
        assert_eq!(once, twice);
    }

    #[test]
    fn an_empty_trailer_writes_nothing() {
        assert!(append_trailer(&[], &Trailer::new()).is_empty());
        assert!(append_trailer(&append_trailer(&[], &sample()), &Trailer::new()).is_empty());
    }

    #[test]
    fn unrelated_trailing_bytes_survive() {
        let foreign = b"someone else's footer".to_vec();
        let bytes = append_trailer(&foreign, &sample());
        assert_eq!(read_trailer(&bytes), sample());
        assert_eq!(strip_trailer(&bytes), &foreign[..]);
    }

    #[test]
    fn a_bin_without_a_trailer_reads_empty() {
        assert!(read_trailer(&[]).is_empty());
        assert!(read_trailer(b"PROP not a footer").is_empty());
    }

    #[test]
    fn a_truncated_record_is_ignored_rather_than_panicking() {
        let bytes = append_trailer(&[], &sample());
        for cut in 1..bytes.len() {
            let damaged = [&bytes[cut..], &[][..]].concat();
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
