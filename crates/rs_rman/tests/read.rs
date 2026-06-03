use std::io::Cursor;

use rs_io::Parse;
use rs_rman::{Error, Rman};

const HEADER_LEN: u32 = 28;

/// Wrap a decompressed body in a valid v2.0 RMAN header (zstd-compressing the body).
fn wrap(body: &[u8]) -> Vec<u8> {
    let compressed = zstd::stream::encode_all(body, 3).unwrap();
    let mut out = Vec::new();
    out.extend_from_slice(b"RMAN");
    out.push(2); // major
    out.push(0); // minor
    out.extend_from_slice(&0u16.to_le_bytes()); // flags
    out.extend_from_slice(&HEADER_LEN.to_le_bytes()); // offset to body
    out.extend_from_slice(&(compressed.len() as u32).to_le_bytes()); // compressed size
    out.extend_from_slice(&0x1122_3344_5566_7788u64.to_le_bytes()); // manifest id
    out.extend_from_slice(&(body.len() as u32).to_le_bytes()); // uncompressed size
    out.extend_from_slice(&compressed);
    out
}

#[test]
fn rejects_bad_magic() {
    let mut bytes = wrap(&[0u8; 4]);
    bytes[0] = b'X';
    let err = Rman::from_bytes(&bytes).unwrap_err();
    assert!(matches!(err, Error::InvalidMagic(_)));
}

#[test]
fn rejects_unsupported_major_version() {
    let mut bytes = wrap(&[0u8; 4]);
    bytes[4] = 1; // major -> 1.0
    let err = Rman::from_bytes(&bytes).unwrap_err();
    assert!(matches!(err, Error::UnsupportedVersion(1, 0)));
}

#[test]
fn accepts_minor_version_two_one() {
    let mut bytes = wrap(&Body::empty_tables());
    bytes[5] = 1; // minor -> 2.1, the version real game manifests use
    let rman = Rman::from_bytes(&bytes).unwrap();
    assert_eq!(rman.version, (2, 1));
}

#[test]
fn parses_header_with_empty_tables() {
    let body = Body::empty_tables();
    let bytes = wrap(&body);
    let rman = Rman::from_bytes(&bytes).unwrap();
    assert_eq!(rman.version, (2, 0));
    assert_eq!(rman.manifest_id, 0x1122_3344_5566_7788);
    assert!(rman.bundles.is_empty());
    assert!(rman.files.is_empty());
    assert!(rman.directories.is_empty());
    assert!(rman.file_paths().is_empty());
}

#[test]
fn truncated_body_errors_not_panics() {
    let bytes = wrap(&[0u8; 2]);
    assert!(Rman::from_bytes(&bytes).is_err());
}

#[test]
fn parses_synthetic_body() {
    let bytes = wrap(&Body::full());
    let rman = Rman::from_bytes(&bytes).unwrap();

    assert_eq!(rman.bundles.len(), 1);
    let bundle = &rman.bundles[0];
    assert_eq!(bundle.id, 0xB001);
    assert_eq!(bundle.chunks.len(), 1);
    assert_eq!(bundle.chunks[0].id, 0xC001);
    assert_eq!(bundle.chunks[0].compressed_size, 100);
    assert_eq!(bundle.chunks[0].uncompressed_size, 250);

    assert_eq!(rman.directories.len(), 2);
    assert_eq!(rman.files.len(), 1);
    let file = &rman.files[0];
    assert_eq!(file.id, 0xF001);
    assert_eq!(file.name, "champion.bin");
    assert_eq!(file.size, 250);
    assert_eq!(file.directory_id, Some(2));
    assert_eq!(file.chunk_ids, vec![0xC001]);
    assert_eq!(file.permissions, 2);

    let paths = rman.file_paths();
    assert_eq!(paths, vec![("data/champions/champion.bin".to_string(), 250u64)]);
}

#[test]
fn parse_via_reader_seek() {
    let bytes = wrap(&Body::full());
    let mut cur = Cursor::new(bytes);
    let rman = Rman::from_reader(&mut cur).unwrap();
    assert_eq!(rman.files.len(), 1);
}

/// Two-pass FlatBuffer body builder. Every object is emitted at a known absolute position and
/// referenced through self-relative offsets, matching exactly what the reader walks: a body
/// header (i32 length + four table offsets), then bundle/flag/file/directory tables whose
/// entries each carry a vtable (two skipped `u16` headers followed by the field-offset array).
struct Body {
    buf: Vec<u8>,
}

impl Body {
    fn new() -> Self {
        Self { buf: Vec::new() }
    }

    fn pos(&self) -> i32 {
        self.buf.len() as i32
    }

    fn u32(&mut self, v: u32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    fn i32(&mut self, v: i32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    fn u64(&mut self, v: u64) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    fn u16(&mut self, v: u16) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    /// Reserve a 4-byte offset slot; returns its absolute position so it can be patched later.
    fn reserve(&mut self) -> i32 {
        let at = self.pos();
        self.i32(0);
        at
    }

    /// Patch a reserved slot to a self-relative offset pointing at `target`.
    fn patch(&mut self, slot: i32, target: i32) {
        let rel = target - slot;
        self.patch_raw(slot, rel);
    }

    /// Write a raw i32 into a reserved slot (used for the backward-subtracted vtable pointer).
    fn patch_raw(&mut self, slot: i32, value: i32) {
        let s = slot as usize;
        self.buf[s..s + 4].copy_from_slice(&value.to_le_bytes());
    }

    /// Backfill an entry's vtable pointer: the reader computes `entry - stored + 4`, so store
    /// `entry - field_array + 4` to make it resolve to the field-offset array.
    fn patch_vtable(&mut self, entry_slot: i32, field_array: i32) {
        self.patch_raw(entry_slot, entry_slot - field_array + 4);
    }

    /// Emit a vtable (two zero `u16` headers + the field-offset array) and return the position
    /// of the field array, which is what the reader resolves the vtable pointer to.
    fn vtable(&mut self, field_offsets: &[u16]) -> i32 {
        self.u16(0);
        self.u16(0);
        let field_array = self.pos();
        for &o in field_offsets {
            self.u16(o);
        }
        field_array
    }

    /// Emit a length-prefixed UTF-8 string at the current position; returns its start.
    fn string(&mut self, s: &str) -> i32 {
        let at = self.pos();
        self.i32(s.len() as i32);
        self.buf.extend_from_slice(s.as_bytes());
        at
    }

    fn finish(self) -> Vec<u8> {
        self.buf
    }

    fn empty_tables() -> Vec<u8> {
        let mut b = Self::new();
        b.i32(0); // body header length
        let slots = [b.reserve(), b.reserve(), b.reserve(), b.reserve()];
        for slot in slots {
            let target = b.pos();
            b.patch(slot, target);
            b.u32(0); // empty table
        }
        b.finish()
    }

    fn full() -> Vec<u8> {
        let mut b = Self::new();
        b.i32(0); // body header length
        let bundles_slot = b.reserve();
        let flags_slot = b.reserve();
        let files_slot = b.reserve();
        let dirs_slot = b.reserve();

        // ---- Bundles table: one bundle with one chunk ----
        b.patch(bundles_slot, b.pos());
        b.u32(1);
        let bundle_ptr = b.reserve();

        let bundle_entry = b.pos();
        b.patch(bundle_ptr, bundle_entry);
        let bundle_vtable_slot = b.reserve(); // self-relative pointer to field array
        b.u64(0xB001); // +4 id
        let bundle_chunks_slot = b.reserve(); // +12 chunks offset
        let bundle_field_array = b.vtable(&[4, 12]);
        b.patch_vtable(bundle_vtable_slot, bundle_field_array);

        b.patch(bundle_chunks_slot, b.pos());
        b.u32(1); // chunk count
        let chunk_ptr = b.reserve();

        let chunk_entry = b.pos();
        b.patch(chunk_ptr, chunk_entry);
        let chunk_vtable_slot = b.reserve();
        b.u64(0xC001); // +4 id
        b.u32(100); // +12 compressed size
        b.u32(250); // +16 uncompressed size
        let chunk_field_array = b.vtable(&[4, 12, 16]);
        b.patch_vtable(chunk_vtable_slot, chunk_field_array);

        // ---- Flags table: empty ----
        b.patch(flags_slot, b.pos());
        b.u32(0);

        // ---- Directories table: "data" (root) and "champions" (parent = data) ----
        b.patch(dirs_slot, b.pos());
        b.u32(2);
        let dir0_ptr = b.reserve();
        let dir1_ptr = b.reserve();

        let dir0 = b.pos();
        b.patch(dir0_ptr, dir0);
        let dir0_vtable_slot = b.reserve();
        b.u64(1); // +4 id
        b.u64(1); // +12 parent (self -> treated as root)
        let dir0_name_slot = b.reserve(); // +20 name
        let dir0_field_array = b.vtable(&[4, 12, 20]);
        b.patch_vtable(dir0_vtable_slot, dir0_field_array);
        b.patch(dir0_name_slot, b.pos());
        b.string("data");

        let dir1 = b.pos();
        b.patch(dir1_ptr, dir1);
        let dir1_vtable_slot = b.reserve();
        b.u64(2); // +4 id
        b.u64(1); // +12 parent = data
        let dir1_name_slot = b.reserve(); // +20 name
        let dir1_field_array = b.vtable(&[4, 12, 20]);
        b.patch_vtable(dir1_vtable_slot, dir1_field_array);
        b.patch(dir1_name_slot, b.pos());
        b.string("champions");

        // ---- Files table: one file in directory 2 ----
        b.patch(files_slot, b.pos());
        b.u32(1);
        let file_ptr = b.reserve();

        let file_entry = b.pos();
        b.patch(file_ptr, file_entry);
        let file_vtable_slot = b.reserve();
        b.u64(0xF001); // +4 id
        b.u64(2); // +12 directory id
        b.u32(250); // +20 size
        let file_name_slot = b.reserve(); // +24 name
        let file_chunks_slot = b.reserve(); // +28 chunks
        b.u32(2); // +32 permissions (u8 from first byte)
        let file_field_array = b.vtable(&[4, 12, 20, 24, 0, 0, 0, 28, 0, 0, 0, 0, 32]);
        b.patch_vtable(file_vtable_slot, file_field_array);

        b.patch(file_name_slot, b.pos());
        b.string("champion.bin");

        b.patch(file_chunks_slot, b.pos());
        b.u32(1); // chunk id count
        b.u64(0xC001);

        b.finish()
    }
}
