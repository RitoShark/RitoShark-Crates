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
    assert_eq!(file.flags_mask, Some(0b1000));

    let paths = rman.file_paths();
    assert_eq!(
        paths,
        vec![("data/champions/champion.bin".to_string(), 250u64)]
    );

    assert_eq!(rman.file_flags.len(), 1);
    assert_eq!(rman.file_flags[0].id, 3);
    assert_eq!(rman.file_flags[0].name, "en_US");
    assert_eq!(rman.file_flag_names(file), vec!["en_US"]);
    assert_eq!(rman.files_with_flag("en_US").len(), 1);
    assert!(rman.files_with_flag("ko_KR").is_empty());

    let ranges = rman.file_chunks(file);
    assert_eq!(ranges.len(), 1);
    assert_eq!(ranges[0].bundle_id, 0xB001);
    assert_eq!(ranges[0].chunk_id, 0xC001);
    assert_eq!(ranges[0].offset_in_bundle, 0);
    assert_eq!(ranges[0].compressed_size, 100);
    assert_eq!(ranges[0].uncompressed_size, 250);
}

/// The reader captures file fields it does not interpret (FlatBuffer indices 5/6/8/10/11) so the
/// full manifest model is available even though the format is never written back.
#[test]
fn preserves_uninterpreted_file_fields() {
    let original = Rman::from_bytes(&wrap(&Body::with_extras())).unwrap();
    assert_eq!(original.files[0].extra.field11, Some(2));
    assert_eq!(original.files[0].extra.field5, Some(0xABCD));
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

    fn u8(&mut self, v: u8) {
        self.buf.push(v);
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
        let slots = [
            b.reserve(),
            b.reserve(),
            b.reserve(),
            b.reserve(),
            b.reserve(),
            b.reserve(),
        ];
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
        let _slot4 = b.reserve();
        let params_slot = b.reserve();

        // Slot 4 (unused) and slot 5 (parameters) — both empty tables.
        b.patch(_slot4, b.pos());
        b.u32(0);
        b.patch(params_slot, b.pos());
        b.u32(0);

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

        // ---- Flags table: one flag {id: 3, name: "en_US"} ----
        // Flag entries use a fixed layout: vtable ptr (i32) + 3 reserved bytes + id (u8 @ +7)
        // + self-relative name offset (i32 @ +8).
        b.patch(flags_slot, b.pos());
        b.u32(1);
        let flag_ptr = b.reserve();

        let flag_entry = b.pos();
        b.patch(flag_ptr, flag_entry);
        let flag_vtable_slot = b.reserve(); // +0 vtable ptr
        b.u8(0); // +4 reserved
        b.u8(0); // +5 reserved
        b.u8(0); // +6 reserved
        b.u8(3); // +7 id
        let flag_name_slot = b.reserve(); // +8 name (self-relative offset)
        let flag_field_array = b.vtable(&[4, 7]);
        b.patch_vtable(flag_vtable_slot, flag_field_array);
        b.patch(flag_name_slot, b.pos());
        b.string("en_US");

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
        b.u64(0b1000); // +28 flags mask (bit 3 -> flag id 3 "en_US")
        let file_chunks_slot = b.reserve(); // +36 chunks
        b.u32(2); // +40 permissions (u8 from first byte)
        let file_field_array = b.vtable(&[4, 12, 20, 24, 28, 0, 0, 36, 0, 0, 0, 0, 40]);
        b.patch_vtable(file_vtable_slot, file_field_array);

        b.patch(file_name_slot, b.pos());
        b.string("champion.bin");

        b.patch(file_chunks_slot, b.pos());
        b.u32(1); // chunk id count
        b.u64(0xC001);

        b.finish()
    }

    /// A minimal body whose single file carries the normally-uninterpreted fields 5 (`u32`) and
    /// 11 (`u16`), used to prove the writer preserves them across a round-trip.
    fn with_extras() -> Vec<u8> {
        let mut b = Self::new();
        b.i32(0);
        let bundles_slot = b.reserve();
        let flags_slot = b.reserve();
        let files_slot = b.reserve();
        let dirs_slot = b.reserve();
        let _slot4 = b.reserve();
        let params_slot = b.reserve();

        // Empty bundles / flags / directories tables.
        b.patch(bundles_slot, b.pos());
        b.u32(0);
        b.patch(flags_slot, b.pos());
        b.u32(0);

        // Files table: one file with field 5 (u32) and field 11 (u16) present.
        b.patch(files_slot, b.pos());
        b.u32(1);
        let file_ptr = b.reserve();
        let file_entry = b.pos();
        b.patch(file_ptr, file_entry);
        let file_vtable_slot = b.reserve();
        b.u64(0xF002); // +4  field 0 id
        b.u32(64); // +12 field 2 size
        b.u32(0xABCD); // +16 field 5 (u32)
        let file_name_slot = b.reserve(); // +20 field 3 name
        let file_chunks_slot = b.reserve(); // +24 field 7 chunks
        b.u16(2); // +28 field 11 (u16)
        b.u32(1); // +30 field 12 permissions
        let file_field_array = b.vtable(&[4, 0, 12, 20, 0, 16, 0, 24, 0, 0, 0, 28, 30]);
        b.patch_vtable(file_vtable_slot, file_field_array);
        b.patch(file_name_slot, b.pos());
        b.string("loose.txt");
        b.patch(file_chunks_slot, b.pos());
        b.u32(0); // no chunk ids

        b.patch(dirs_slot, b.pos());
        b.u32(0);
        b.patch(_slot4, b.pos());
        b.u32(0);
        b.patch(params_slot, b.pos());
        b.u32(0);

        b.finish()
    }
}
