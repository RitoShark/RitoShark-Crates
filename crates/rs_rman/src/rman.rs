use std::collections::HashMap;

/// One chunk inside a bundle: compressed in the CDN bundle, decompressed into a file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chunk {
    pub id: u64,
    pub compressed_size: u32,
    pub uncompressed_size: u32,
}

/// Per-file chunk hash algorithm, selected by a file's `param_index` into the
/// manifest parameters table. Numeric values match Riot's RMAN encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChunkHashType {
    Sha512,
    Sha256,
    Hkdf,
    Blake3,
}

impl ChunkHashType {
    /// Map the on-disk `u8` tag (`1..=4`) to a variant; `None` for unknown tags.
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            1 => Some(Self::Sha512),
            2 => Some(Self::Sha256),
            3 => Some(Self::Hkdf),
            4 => Some(Self::Blake3),
            _ => None,
        }
    }
}

/// One entry of the manifest parameters table. A file's `param_index` selects the entry
/// whose `hash_type` governs that file's chunk validation. `raw_hash_type` preserves the
/// on-disk tag even when it maps to no known [`ChunkHashType`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Parameter {
    pub raw_hash_type: u8,
    pub hash_type: Option<ChunkHashType>,
    pub min_chunk_size: u32,
    pub max_chunk_size: u32,
    pub max_uncompressed_size: u32,
}

/// A bundle groups chunks that are downloaded together.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bundle {
    pub id: u64,
    pub chunks: Vec<Chunk>,
}

/// A file in the release: its name (basename), parent directory, size, and the ordered
/// chunk ids whose decompressed contents concatenate to the file body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileEntry {
    pub id: u64,
    pub name: String,
    pub size: u32,
    pub directory_id: Option<u64>,
    pub chunk_ids: Vec<u64>,
    pub link: Option<String>,
    pub permissions: u8,
    /// Bitmask selecting which entries of the manifest flags table apply to this file.
    /// Bit `n` set means the flag whose `id` is `n` (a locale or platform tag) is active.
    pub flags_mask: Option<u64>,
    /// Index into [`Rman::parameters`] selecting this file's chunk hash parameters.
    /// `None` for files Riot emits without parameters (most non-WAD entries).
    pub param_index: Option<u8>,
    /** Verbatim copies of the file-entry fields the reader does not interpret (FlatBuffer
    field indices 5, 6, 8, 10, 11). Riot's encoder emits some of these on real manifests —
    field 11 (a `u16`, observed as `1`/`2`) marks localized WADs — so they are captured and
    re-emitted to keep the read→write→read model identical and lose nothing. The remaining
    indices are unused by every shipped manifest seen but are modelled the same way for safety. */
    pub extra: FileExtra,
}

/// Preserved-but-uninterpreted file-entry fields, kept so writing loses no data (see
/// [`FileEntry::extra`]). Each is `None` when the corresponding FlatBuffer slot is absent.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileExtra {
    pub field5: Option<u32>,
    pub field6: Option<u32>,
    pub field8: Option<u32>,
    pub field10: Option<u32>,
    pub field11: Option<u16>,
}

/// One entry of the manifest flags table: a small numeric `id` paired with a locale
/// (e.g. `en_US`) or platform (e.g. `windows`, `macos`) tag. A file's `flags_mask`
/// references these by bit position `1 << id`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileFlag {
    pub id: u8,
    pub name: String,
}

/// One chunk of a file located within its CDN bundle. `offset_in_bundle` is the running
/// start of this chunk's compressed bytes inside the bundle (chunks are concatenated in
/// bundle order); `compressed_size` is its length there, and `uncompressed_size` its size
/// once inflated into the target file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChunkRange {
    pub bundle_id: u64,
    pub chunk_id: u64,
    pub offset_in_bundle: u32,
    pub compressed_size: u32,
    pub uncompressed_size: u32,
}

/// A directory node; full paths are reconstructed by walking `parent_id` up to the root.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Directory {
    pub id: u64,
    pub parent_id: Option<u64>,
    pub name: String,
}

/// A parsed release manifest.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rman {
    pub version: (u8, u8),
    pub flags: u16,
    pub manifest_id: u64,
    pub bundles: Vec<Bundle>,
    pub files: Vec<FileEntry>,
    pub directories: Vec<Directory>,
    /// Locale/platform flag table; each file's `flags_mask` selects entries from it.
    pub file_flags: Vec<FileFlag>,
    /// Chunk parameters table; each file's `param_index` selects the entry governing
    /// that file's chunk hash algorithm and chunk-size bounds.
    pub parameters: Vec<Parameter>,
}

impl Rman {
    /// Resolve full path for one directory id by walking parents to the root.
    fn dir_path(&self, id: u64, by_id: &HashMap<u64, &Directory>) -> String {
        let mut parts: Vec<&str> = Vec::new();
        let mut current = by_id.get(&id).copied();
        while let Some(dir) = current {
            if !dir.name.is_empty() {
                parts.push(&dir.name);
            }
            current = match dir.parent_id {
                Some(pid) if pid != dir.id => by_id.get(&pid).copied(),
                _ => None,
            };
        }
        parts.reverse();
        parts.join("/")
    }

    /// Full file paths paired with their extracted size.
    ///
    /// Paths join each file's directory chain with `/`; files without a directory use the
    /// bare name. Cycles in the directory chain are broken so this never loops.
    pub fn file_paths(&self) -> Vec<(String, u64)> {
        let by_id: HashMap<u64, &Directory> = self.directories.iter().map(|d| (d.id, d)).collect();

        self.files
            .iter()
            .map(|f| {
                let path = match f.directory_id {
                    Some(id) => {
                        let dir = self.dir_path(id, &by_id);
                        if dir.is_empty() {
                            f.name.clone()
                        } else {
                            format!("{dir}/{}", f.name)
                        }
                    }
                    None => f.name.clone(),
                };
                (path, f.size as u64)
            })
            .collect()
    }

    /// Names of the flags (locale/platform tags) active for `file`, in flag-table order.
    /// Empty when the file carries no mask or references no known flag.
    pub fn file_flag_names(&self, file: &FileEntry) -> Vec<&str> {
        let mask = match file.flags_mask {
            Some(m) => m,
            None => return Vec::new(),
        };
        self.file_flags
            .iter()
            .filter(|f| mask & (1u64 << f.id) != 0)
            .map(|f| f.name.as_str())
            .collect()
    }

    /// References to every file whose flag mask includes the flag named `tag`
    /// (a locale such as `en_US` or a platform such as `windows`). Files with no mask
    /// are excluded; an unknown `tag` yields an empty result.
    pub fn files_with_flag(&self, tag: &str) -> Vec<&FileEntry> {
        let bit = match self.file_flags.iter().find(|f| f.name == tag) {
            Some(f) => 1u64 << f.id,
            None => return Vec::new(),
        };
        self.files
            .iter()
            .filter(|f| f.flags_mask.is_some_and(|m| m & bit != 0))
            .collect()
    }

    /// Index every chunk by id, recording the bundle it belongs to and its running byte
    /// offset within that bundle (chunks concatenate in bundle order). Built once and
    /// reused by [`Rman::file_chunks`] / [`Rman::file_chunks_for`].
    pub fn chunk_index(&self) -> HashMap<u64, ChunkRange> {
        let mut index = HashMap::new();
        for bundle in &self.bundles {
            let mut offset: u32 = 0;
            for chunk in &bundle.chunks {
                index.entry(chunk.id).or_insert(ChunkRange {
                    bundle_id: bundle.id,
                    chunk_id: chunk.id,
                    offset_in_bundle: offset,
                    compressed_size: chunk.compressed_size,
                    uncompressed_size: chunk.uncompressed_size,
                });
                offset = offset.saturating_add(chunk.compressed_size);
            }
        }
        index
    }

    /// Ordered chunk byte-ranges that reconstruct `file`: for each of the file's chunk ids
    /// (in order), the bundle it lives in and its compressed range there plus uncompressed
    /// size. Concatenating each chunk's decompressed bytes yields the file. Chunk ids with
    /// no matching bundle chunk are skipped. Builds a fresh index each call; for many files
    /// build [`Rman::chunk_index`] once and use [`Rman::file_chunks_for`].
    pub fn file_chunks(&self, file: &FileEntry) -> Vec<ChunkRange> {
        let index = self.chunk_index();
        Self::file_chunks_for(file, &index)
    }

    /// Like [`Rman::file_chunks`] but against a prebuilt [`Rman::chunk_index`].
    pub fn file_chunks_for(file: &FileEntry, index: &HashMap<u64, ChunkRange>) -> Vec<ChunkRange> {
        file.chunk_ids
            .iter()
            .filter_map(|id| index.get(id).cloned())
            .collect()
    }
}
