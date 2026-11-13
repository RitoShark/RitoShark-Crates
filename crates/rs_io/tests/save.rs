use std::io::Write;

use rs_io::Serialize;

struct Document {
    bytes: Vec<u8>,
    fail: bool,
}

impl Serialize for Document {
    type Error = rs_io::Error;

    fn to_writer<W: Write>(&self, writer: &mut W) -> Result<(), Self::Error> {
        writer.write_all(&self.bytes)?;
        if self.fail {
            return Err(std::io::Error::other("serialization failed").into());
        }
        Ok(())
    }
}

#[test]
fn failed_save_preserves_destination_and_cleans_temporary_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("document.bin");
    std::fs::write(&path, b"original").unwrap();
    let document = Document {
        bytes: vec![42; 32 * 1024],
        fail: true,
    };
    assert!(document.to_path(&path).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"original");
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn failed_save_does_not_create_destination() {
    let dir = tempfile::tempdir().unwrap();
    let document = Document {
        bytes: b"partial".to_vec(),
        fail: true,
    };
    assert!(document.to_path(dir.path().join("new.bin")).is_err());
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[test]
fn successful_save_creates_and_replaces_complete_files() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("document.bin");
    for bytes in [vec![17; 32 * 1024], b"buffered tail".to_vec(), Vec::new()] {
        Document {
            bytes: bytes.clone(),
            fail: false,
        }
        .to_path(&path)
        .unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }
}

#[test]
fn failed_replacement_preserves_destination_and_cleans_temporary_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("directory");
    std::fs::create_dir(&path).unwrap();
    std::fs::write(path.join("original"), b"original").unwrap();
    let document = Document {
        bytes: b"replacement".to_vec(),
        fail: false,
    };
    assert!(document.to_path(&path).is_err());
    assert_eq!(std::fs::read(path.join("original")).unwrap(), b"original");
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
}
