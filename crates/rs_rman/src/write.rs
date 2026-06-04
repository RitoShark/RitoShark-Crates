use std::io::Write;

use rs_io::Serialize;

use crate::error::{Error, Result};
use crate::rman::{Bundle, Directory, FileEntry, FileFlag, Rman};

const MAGIC: &[u8; 4] = b"RMAN";
const HEADER_LEN: u32 = 4 + 1 + 1 + 2 + 4 + 4 + 8 + 4;

/** Emits the RMAN container: the fixed 28-byte little-endian header followed by the
zstd-compressed FlatBuffer body. The body is rebuilt from the owned model with
[`BodyBuilder`], re-creating every table and field index the reader walks, then zstd-compressed.

Byte-exact reproduction of Riot's own output is intentionally *not* the contract for RMAN:
both zstd re-compression and FlatBuffer layout (field packing order, vtable sharing, alignment
padding) are encoder choices with many valid encodings, and we do not reproduce Riot's exact
ones. The guarantee here is a *semantic* round-trip — `read → write → read` yields an identical
logical [`Rman`] (bundles, files including the preserved uninterpreted fields, directories,
flags) — which the tests assert on the real manifests. */
impl Serialize for Rman {
    type Error = Error;

    fn to_writer<W: Write>(&self, writer: &mut W) -> Result<()> {
        let body = BodyBuilder::build(self);
        let compressed = zstd::stream::encode_all(body.as_slice(), 3)
            .map_err(|e| Error::Decompress(e.to_string()))?;

        let compressed_len = u32::try_from(compressed.len())
            .map_err(|_| Error::Malformed("compressed body exceeds u32"))?;
        let uncompressed_len = u32::try_from(body.len())
            .map_err(|_| Error::Malformed("body exceeds u32"))?;

        writer.write_all(MAGIC)?;
        writer.write_all(&[self.version.0, self.version.1])?;
        writer.write_all(&self.flags.to_le_bytes())?;
        writer.write_all(&HEADER_LEN.to_le_bytes())?;
        writer.write_all(&compressed_len.to_le_bytes())?;
        writer.write_all(&self.manifest_id.to_le_bytes())?;
        writer.write_all(&uncompressed_len.to_le_bytes())?;
        writer.write_all(&compressed)?;
        Ok(())
    }
}

/** Two-pass FlatBuffer body builder. Each object is appended at a known absolute position and
referenced through self-relative `i32` offsets, exactly matching what the reader resolves: a
body header (`i32` length set to `0` + four self-relative table offsets in the order bundles,
flags, files, directories), then bundle / flag / file / directory tables whose entries each
carry a self-relative vtable pointer ahead of an indexed field-offset array. Offsets that point
forward are reserved as zeroed slots and patched once their target position is known. */
struct BodyBuilder {
    buf: Vec<u8>,
}

impl BodyBuilder {
    fn build(rman: &Rman) -> Vec<u8> {
        let mut b = Self { buf: Vec::new() };
        b.i32(0); // body header length (no extra header bytes)
        let bundles_slot = b.reserve();
        let flags_slot = b.reserve();
        let files_slot = b.reserve();
        let dirs_slot = b.reserve();

        let target = b.pos();
        b.patch(bundles_slot, target);
        b.write_bundles(&rman.bundles);

        let target = b.pos();
        b.patch(flags_slot, target);
        b.write_flags(&rman.file_flags);

        let target = b.pos();
        b.patch(files_slot, target);
        b.write_files(&rman.files);

        let target = b.pos();
        b.patch(dirs_slot, target);
        b.write_directories(&rman.directories);

        b.buf
    }

    fn write_bundles(&mut self, bundles: &[Bundle]) {
        self.u32(bundles.len() as u32);
        let slots: Vec<i32> = (0..bundles.len()).map(|_| self.reserve()).collect();
        for (bundle, slot) in bundles.iter().zip(slots) {
            let entry = self.pos();
            self.patch(slot, entry);

            let vtable_slot = self.reserve();
            self.u64(bundle.id); // field 0 @ +4
            let chunks_slot = self.reserve(); // field 1 @ +12
            let field_array = self.vtable(&[4, 12]);
            self.patch_vtable(vtable_slot, field_array);

            let chunks_at = self.pos();
            self.patch(chunks_slot, chunks_at);
            self.u32(bundle.chunks.len() as u32);
            let chunk_slots: Vec<i32> =
                (0..bundle.chunks.len()).map(|_| self.reserve()).collect();
            for (chunk, cslot) in bundle.chunks.iter().zip(chunk_slots) {
                let centry = self.pos();
                self.patch(cslot, centry);
                let cvtable_slot = self.reserve();
                self.u64(chunk.id); // field 0 @ +4
                self.u32(chunk.compressed_size); // field 1 @ +12
                self.u32(chunk.uncompressed_size); // field 2 @ +16
                let cfield_array = self.vtable(&[4, 12, 16]);
                self.patch_vtable(cvtable_slot, cfield_array);
            }
        }
    }

    /** Flag entries use a fixed body layout rather than indexed lookups: a self-relative vtable
    pointer (`i32`), three reserved bytes, the `id` (`u8`) at entry offset 7, then a self-relative
    offset (`i32`) at offset 8 to the length-prefixed name. The vtable carries field slots `[4, 7]`
    so that any indexed read of fields 0/1 still resolves the same id/name positions. */
    fn write_flags(&mut self, flags: &[FileFlag]) {
        self.u32(flags.len() as u32);
        let slots: Vec<i32> = (0..flags.len()).map(|_| self.reserve()).collect();
        for (flag, slot) in flags.iter().zip(slots) {
            let entry = self.pos();
            self.patch(slot, entry);
            let vtable_slot = self.reserve(); // +0 vtable ptr
            self.u8(0); // +4 reserved
            self.u8(0); // +5 reserved
            self.u8(0); // +6 reserved
            self.u8(flag.id); // +7 id
            let name_slot = self.reserve(); // +8 name offset
            let field_array = self.vtable(&[4, 7]);
            self.patch_vtable(vtable_slot, field_array);
            let name_at = self.pos();
            self.patch(name_slot, name_at);
            self.string(&flag.name);
        }
    }

    fn write_files(&mut self, files: &[FileEntry]) {
        self.u32(files.len() as u32);
        let slots: Vec<i32> = (0..files.len()).map(|_| self.reserve()).collect();
        for (file, slot) in files.iter().zip(slots) {
            self.write_file(file, slot);
        }
    }

    /** Emit one file entry, populating only the fields the model carries so absent fields stay
    absent (a `0` vtable slot). Field indices match the reader: 0 id, 1 directory, 2 size, 3 name,
    4 flags mask, 5/6/8/10/11 the preserved uninterpreted fields, 7 chunk-id list, 9 link, 12
    permissions. Scalars are inlined into the entry body; name, link and the chunk-id list are
    self-relative offsets patched after the entry's vtable. */
    fn write_file(&mut self, file: &FileEntry, ptr_slot: i32) {
        let entry = self.pos();
        self.patch(ptr_slot, entry);
        let vtable_slot = self.reserve();

        let mut slots = [0u16; 13];

        // Field 0: id (u64)
        slots[0] = (self.pos() - entry) as u16;
        self.u64(file.id);

        // Field 1: directory id (u64, optional)
        if let Some(dir) = file.directory_id {
            slots[1] = (self.pos() - entry) as u16;
            self.u64(dir);
        }

        // Field 2: size (u32)
        slots[2] = (self.pos() - entry) as u16;
        self.u32(file.size);

        // Field 4: flags mask (u64, optional)
        if let Some(mask) = file.flags_mask {
            slots[4] = (self.pos() - entry) as u16;
            self.u64(mask);
        }

        // Preserved uninterpreted scalar fields.
        if let Some(v) = file.extra.field5 {
            slots[5] = (self.pos() - entry) as u16;
            self.u32(v);
        }
        if let Some(v) = file.extra.field6 {
            slots[6] = (self.pos() - entry) as u16;
            self.u32(v);
        }
        if let Some(v) = file.extra.field8 {
            slots[8] = (self.pos() - entry) as u16;
            self.u32(v);
        }
        if let Some(v) = file.extra.field10 {
            slots[10] = (self.pos() - entry) as u16;
            self.u32(v);
        }
        if let Some(v) = file.extra.field11 {
            slots[11] = (self.pos() - entry) as u16;
            self.u16(v);
        }

        // Field 12: permissions (u8). Stored as a 4-byte slot (reader reads its first byte).
        slots[12] = (self.pos() - entry) as u16;
        self.u32(file.permissions as u32);

        // Offset fields: reserve in-entry slots, then write targets after the vtable.
        slots[3] = (self.pos() - entry) as u16;
        let name_slot = self.reserve();

        slots[7] = (self.pos() - entry) as u16;
        let chunks_slot = self.reserve();

        let link_slot = file.link.as_ref().map(|_| {
            slots[9] = (self.pos() - entry) as u16;
            self.reserve()
        });

        let field_array = self.vtable(&slots);
        self.patch_vtable(vtable_slot, field_array);

        let name_at = self.pos();
        self.patch(name_slot, name_at);
        self.string(&file.name);

        let chunks_at = self.pos();
        self.patch(chunks_slot, chunks_at);
        self.u32(file.chunk_ids.len() as u32);
        for &id in &file.chunk_ids {
            self.u64(id);
        }

        if let (Some(slot), Some(link)) = (link_slot, file.link.as_ref()) {
            let link_at = self.pos();
            self.patch(slot, link_at);
            self.string(link);
        }
    }

    fn write_directories(&mut self, dirs: &[Directory]) {
        self.u32(dirs.len() as u32);
        let slots: Vec<i32> = (0..dirs.len()).map(|_| self.reserve()).collect();
        for (dir, slot) in dirs.iter().zip(slots) {
            let entry = self.pos();
            self.patch(slot, entry);
            let vtable_slot = self.reserve();

            let mut slots = [0u16; 3];
            slots[0] = (self.pos() - entry) as u16;
            self.u64(dir.id); // field 0
            if let Some(parent) = dir.parent_id {
                slots[1] = (self.pos() - entry) as u16;
                self.u64(parent); // field 1
            }
            slots[2] = (self.pos() - entry) as u16;
            let name_slot = self.reserve(); // field 2 (offset)

            let field_array = self.vtable(&slots);
            self.patch_vtable(vtable_slot, field_array);

            let name_at = self.pos();
            self.patch(name_slot, name_at);
            self.string(&dir.name);
        }
    }

    fn pos(&self) -> i32 {
        self.buf.len() as i32
    }

    fn u8(&mut self, v: u8) {
        self.buf.push(v);
    }

    fn u16(&mut self, v: u16) {
        self.buf.extend_from_slice(&v.to_le_bytes());
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

    /// Reserve a 4-byte offset slot, returning its absolute position so it can be patched later.
    fn reserve(&mut self) -> i32 {
        let at = self.pos();
        self.i32(0);
        at
    }

    /// Patch a reserved slot to a self-relative offset pointing at `target`.
    fn patch(&mut self, slot: i32, target: i32) {
        self.patch_raw(slot, target - slot);
    }

    fn patch_raw(&mut self, slot: i32, value: i32) {
        let s = slot as usize;
        self.buf[s..s + 4].copy_from_slice(&value.to_le_bytes());
    }

    /// Backfill an entry's vtable pointer: the reader computes `entry - stored + 4`, so store
    /// `entry - field_array + 4` to make it resolve to the field-offset array.
    fn patch_vtable(&mut self, entry_slot: i32, field_array: i32) {
        self.patch_raw(entry_slot, entry_slot - field_array + 4);
    }

    /// Emit a vtable (two zero `u16` headers + the field-offset array) and return the position of
    /// the field array, which is what the reader resolves the vtable pointer to.
    fn vtable(&mut self, field_offsets: &[u16]) -> i32 {
        self.u16(0);
        self.u16(0);
        let field_array = self.pos();
        for &o in field_offsets {
            self.u16(o);
        }
        field_array
    }

    /// Emit a length-prefixed (`i32`) UTF-8 string at the current position.
    fn string(&mut self, s: &str) {
        self.i32(s.len() as i32);
        self.buf.extend_from_slice(s.as_bytes());
    }
}
