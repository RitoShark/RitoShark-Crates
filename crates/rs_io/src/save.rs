use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

/// Writes to a temporary file in the destination directory, flushes and syncs its
/// contents, then atomically replaces the destination. Errors before replacement
/// leave the destination untouched and remove the temporary file.
///
/// The parent directory must exist. Replacement creates a new file: destination
/// permissions/metadata are not retained, and a symlink at `path` is replaced,
/// not followed. This does not sync the parent directory for power-loss durability.
pub fn write_atomic<E: From<crate::Error>>(
    path: impl AsRef<Path>,
    write: impl FnOnce(&mut BufWriter<File>) -> Result<(), E>,
) -> Result<(), E> {
    let path = path.as_ref();
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let temporary = tempfile::NamedTempFile::new_in(parent).map_err(crate::Error::from)?;
    let (file, temporary_path) = temporary.into_parts();
    let mut writer = BufWriter::new(file);
    write(&mut writer)?;
    writer.flush().map_err(crate::Error::from)?;
    writer.get_ref().sync_all().map_err(crate::Error::from)?;
    drop(writer);
    temporary_path
        .persist(path)
        .map_err(|e| crate::Error::from(e.error))?;
    Ok(())
}
