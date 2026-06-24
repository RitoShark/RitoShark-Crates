#![forbid(unsafe_code)]
/*!
Shared path-safety helper for `rs_cli` subcommands that write files into a
caller-supplied output directory. Centralising the guard here ensures that any
command writing to user-controlled filenames goes through the same rejection
logic.
*/

use std::path::PathBuf;

/// Reject path components that would escape the output directory.
///
/// Returns a relative, normalised [`PathBuf`] containing only
/// [`std::path::Component::Normal`] and (silently skipped)
/// [`std::path::Component::CurDir`] segments.  Returns `None` when the
/// candidate is unsafe — i.e. absolute, empty after normalisation, or contains
/// `..` / root / prefix components.
pub(crate) fn safe_relative(rel: &str) -> Option<PathBuf> {
    use std::path::Component;
    let mut out = PathBuf::new();
    for comp in std::path::Path::new(rel).components() {
        match comp {
            Component::Normal(c) => out.push(c),
            Component::CurDir => {}
            // Reject ParentDir, RootDir, Prefix outright.
            _ => return None,
        }
    }
    if out.as_os_str().is_empty() {
        None
    } else {
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::safe_relative;
    use std::path::PathBuf;

    #[test]
    fn safe_relative_normal_path() {
        let result = safe_relative("a/b.bin");
        assert_eq!(result, Some(PathBuf::from("a/b.bin")));
    }

    #[test]
    fn safe_relative_parent_dir_is_rejected() {
        assert_eq!(safe_relative("../escape"), None);
    }

    #[test]
    fn safe_relative_absolute_path_is_rejected() {
        assert_eq!(safe_relative("/abs/path"), None);
    }

    #[test]
    fn safe_relative_traversal_through_normal_is_rejected() {
        assert_eq!(safe_relative("a/../../b"), None);
    }
}
