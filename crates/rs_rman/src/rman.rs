use std::collections::HashMap;

/// One chunk inside a bundle: compressed in the CDN bundle, decompressed into a file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chunk {
    pub id: u64,
    pub compressed_size: u32,
    pub uncompressed_size: u32,
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
        let by_id: HashMap<u64, &Directory> =
            self.directories.iter().map(|d| (d.id, d)).collect();

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
}
