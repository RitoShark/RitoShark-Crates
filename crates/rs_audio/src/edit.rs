use crate::bnk::{Bnk, BnkSection};
use crate::error::{Error, Result};
use crate::wem::{PcmAudio, Wem, WemCodec, encode_pcm, encode_vorbis_like};
use crate::wpk::{WemEntry, Wpk};

const DIDX: [u8; 4] = *b"DIDX";
const DATA: [u8; 4] = *b"DATA";
const BKHD: [u8; 4] = *b"BKHD";

/** Riot aligns each embedded blob to a sixteen-byte boundary within the DATA body, with no
padding after the last one. Verified against real banks of both header revisions. */
const DATA_ALIGN: usize = 16;

/** How much silence [`Bnk::silence_wem`] and [`Wpk::silence_wem`] write. A few milliseconds is
enough to mute a sound while leaving a real, decodable stream in place. */
const SILENCE_FRAMES: usize = 1024;

/** Builds a replacement stream that is silent but keeps the original's rate, channel count **and
codec**, so whatever the engine expects to mix still lines up.

Matching the codec matters: a Wwise Vorbis sound is muted with Wwise Vorbis, cloning the original's
header template, so the bank keeps using the one codec the game demonstrably plays. PCM is only
written when the original was already PCM. */
fn silent_like(original: &[u8]) -> Result<Vec<u8>> {
    let format = *Wem::new(original)?.format();
    let quiet = PcmAudio::silence(
        format.sample_rate.max(1),
        format.channels.max(1),
        SILENCE_FRAMES,
    );

    match format.codec {
        WemCodec::Vorbis => encode_vorbis_like(original, &quiet, 0.1),
        _ => encode_pcm(&quiet),
    }
}

impl Bnk {
    /// The embedded payload with this id, if the bank has one.
    pub fn wem(&self, id: u32) -> Option<&[u8]> {
        self.wems()
            .into_iter()
            .find(|(k, _)| *k == id)
            .map(|(_, b)| b)
    }

    pub fn wem_ids(&self) -> Vec<u32> {
        self.wems().into_iter().map(|(id, _)| id).collect()
    }

    /** Swaps one embedded payload, leaving everything else in the bank exactly as it was.

    This is a modification of the parsed container, not a rebuild from scratch: the header keeps
    its version and bank id, and the object hierarchy and every unknown section pass through
    untouched. Only DIDX and DATA are rewritten, because only they changed. */
    pub fn replace_wem(&mut self, id: u32, bytes: Vec<u8>) -> Result<()> {
        let mut entries = self.owned_wems();
        let slot = entries
            .iter_mut()
            .find(|(k, _)| *k == id)
            .ok_or(Error::NoSuchWem(id))?;
        slot.1 = bytes;
        self.set_wems(entries)
    }

    /** Replaces a sound with silence at its original sample rate and channel count.

    The entry keeps its id, so every event, action and container that referenced it still
    resolves — which is why muting is done this way rather than by removing the entry. */
    pub fn silence_wem(&mut self, id: u32) -> Result<()> {
        let original = self.wem(id).ok_or(Error::NoSuchWem(id))?;
        let silent = silent_like(original)?;
        self.replace_wem(id, silent)
    }

    /** Drops an embedded payload entirely.

    Any object in the hierarchy still pointing at this id becomes a dangling reference, so prefer
    [`Bnk::silence_wem`] when the intent is to mute rather than to delete. */
    pub fn remove_wem(&mut self, id: u32) -> Result<()> {
        let mut entries = self.owned_wems();
        let before = entries.len();
        entries.retain(|(k, _)| *k != id);
        if entries.len() == before {
            return Err(Error::NoSuchWem(id));
        }
        self.set_wems(entries)
    }

    /// Adds a payload, replacing any existing entry with the same id.
    pub fn insert_wem(&mut self, id: u32, bytes: Vec<u8>) -> Result<()> {
        let mut entries = self.owned_wems();
        match entries.iter_mut().find(|(k, _)| *k == id) {
            Some(slot) => slot.1 = bytes,
            None => entries.push((id, bytes)),
        }
        self.set_wems(entries)
    }

    fn owned_wems(&self) -> Vec<(u32, Vec<u8>)> {
        self.wems()
            .into_iter()
            .map(|(id, bytes)| (id, bytes.to_vec()))
            .collect()
    }

    fn section_index(&self, tag: [u8; 4]) -> Option<usize> {
        self.sections.iter().position(|s| s.tag == tag)
    }

    /** Rewrites DIDX and DATA from an entry list and splices them back into the section sequence
    at their existing positions, or directly after BKHD when the bank had none. Every other
    section keeps its identity, contents and order. */
    fn set_wems(&mut self, entries: Vec<(u32, Vec<u8>)>) -> Result<()> {
        if entries.is_empty() {
            self.sections.retain(|s| s.tag != DIDX && s.tag != DATA);
            return Ok(());
        }

        let mut index = Vec::with_capacity(entries.len() * 12);
        let mut blob = Vec::new();

        for (id, bytes) in &entries {
            let offset = blob.len().next_multiple_of(DATA_ALIGN);
            blob.resize(offset, 0);

            let length: u32 = bytes
                .len()
                .try_into()
                .map_err(|_| Error::Wem("embedded wem is larger than 4 GiB"))?;
            let offset32: u32 = offset
                .try_into()
                .map_err(|_| Error::Wem("DATA section is larger than 4 GiB"))?;

            index.extend_from_slice(&id.to_le_bytes());
            index.extend_from_slice(&offset32.to_le_bytes());
            index.extend_from_slice(&length.to_le_bytes());
            blob.extend_from_slice(bytes);
        }

        self.write_section(DIDX, index);
        self.write_section(DATA, blob);
        Ok(())
    }

    fn write_section(&mut self, tag: [u8; 4], data: Vec<u8>) {
        match self.section_index(tag) {
            Some(at) => self.sections[at].data = data,
            None => {
                let at = match tag {
                    DIDX => self.section_index(BKHD).map_or(0, |i| i + 1),
                    _ => self.section_index(DIDX).map_or(0, |i| i + 1),
                };
                self.sections.insert(at, BnkSection { tag, data });
            }
        }
    }
}

impl Wpk {
    /** Swaps one entry's payload, keeping its name, its position in the offset table and every
    other entry untouched. */
    pub fn replace_wem(&mut self, id: u32, bytes: Vec<u8>) -> Result<()> {
        let entry = self
            .entries
            .iter_mut()
            .find(|e| e.id() == Some(id))
            .ok_or(Error::NoSuchWem(id))?;
        entry.data = bytes;
        Ok(())
    }

    /// Replaces a sound with silence at its original sample rate and channel count.
    pub fn silence_wem(&mut self, id: u32) -> Result<()> {
        let original = self.wem(id).ok_or(Error::NoSuchWem(id))?;
        let silent = silent_like(original)?;
        self.replace_wem(id, silent)
    }

    /** Removes an entry. Its offset-table slot becomes a dead slot rather than disappearing, so
    the table keeps its length — which is how real packages express a removed entry. */
    pub fn remove_wem(&mut self, id: u32) -> Result<()> {
        let at = self
            .entries
            .iter()
            .position(|e| e.id() == Some(id))
            .ok_or(Error::NoSuchWem(id))?;
        self.entries.remove(at);

        let slot = self.slot_of_live_entry(at);
        self.dead_slots.push(slot);
        self.dead_slots.sort_unstable();
        Ok(())
    }

    /// Adds an entry named `"<id>.wem"`, replacing any existing entry with the same id.
    pub fn insert_wem(&mut self, id: u32, bytes: Vec<u8>) -> Result<()> {
        match self.entries.iter_mut().find(|e| e.id() == Some(id)) {
            Some(entry) => entry.data = bytes,
            None => self.entries.push(WemEntry::new(format!("{id}.wem"), bytes)),
        }
        Ok(())
    }

    /** The offset-table slot a live entry occupies, accounting for the dead slots interleaved
    among them. Live entries fill the non-zero slots in order. */
    fn slot_of_live_entry(&self, live_index: usize) -> u32 {
        let mut seen = 0usize;
        let mut slot = 0u32;
        loop {
            if !self.dead_slots.contains(&slot) {
                if seen == live_index {
                    return slot;
                }
                seen += 1;
            }
            slot += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wem::silence;
    use rs_io::{Parse, Serialize};

    fn body(bnk: &Bnk, tag: &[u8; 4]) -> Vec<u8> {
        bnk.sections
            .iter()
            .find(|s| s.tag == *tag)
            .map(|s| s.data.clone())
            .unwrap_or_default()
    }

    fn bank_with(sections: Vec<(&[u8; 4], Vec<u8>)>) -> Bnk {
        Bnk {
            sections: sections
                .into_iter()
                .map(|(tag, data)| BnkSection { tag: *tag, data })
                .collect(),
        }
    }

    fn audio_bank() -> Bnk {
        let mut bnk = bank_with(vec![
            (b"BKHD", vec![0x91, 0, 0, 0, 0xAB, 0xCD, 0xEF, 0x01]),
            (b"HIRC", vec![9, 9, 9, 9]),
            (b"STMG", vec![7, 7]),
        ]);
        bnk.insert_wem(100, silence(44100, 1, 8).unwrap()).unwrap();
        bnk.insert_wem(200, silence(22050, 2, 8).unwrap()).unwrap();
        bnk
    }

    #[test]
    fn replacing_a_wem_preserves_the_header_and_every_other_section() {
        let mut bnk = audio_bank();
        let before: Vec<[u8; 4]> = bnk.sections.iter().map(|s| s.tag).collect();
        let bkhd = body(&bnk, b"BKHD");
        let hirc = body(&bnk, b"HIRC");
        let stmg = body(&bnk, b"STMG");

        bnk.replace_wem(100, silence(48000, 2, 64).unwrap())
            .unwrap();

        assert_eq!(
            bnk.sections.iter().map(|s| s.tag).collect::<Vec<_>>(),
            before,
            "section list must not change"
        );
        assert_eq!(body(&bnk, b"BKHD"), bkhd, "BKHD must be verbatim");
        assert_eq!(body(&bnk, b"HIRC"), hirc, "HIRC must survive");
        assert_eq!(body(&bnk, b"STMG"), stmg, "STMG must survive");
        assert_eq!(bnk.wem_ids(), vec![100, 200], "ids and order preserved");
    }

    #[test]
    fn a_no_op_edit_is_byte_identical() {
        let bnk = audio_bank();
        let before = bnk.to_bytes().unwrap();

        let mut again = Bnk::from_bytes(&before).unwrap();
        let payload = again.wem(200).unwrap().to_vec();
        again.replace_wem(200, payload).unwrap();

        assert_eq!(
            again.to_bytes().unwrap(),
            before,
            "replacing a payload with itself must change nothing"
        );
    }

    #[test]
    fn silencing_keeps_the_id_and_the_stream_parameters() {
        let mut bnk = audio_bank();
        bnk.silence_wem(200).unwrap();

        assert!(bnk.wem_ids().contains(&200), "entry must still exist");
        let decoded = Wem::new(bnk.wem(200).unwrap()).unwrap().to_pcm().unwrap();
        assert_eq!(decoded.sample_rate, 22050);
        assert_eq!(decoded.channels, 2);
        assert!(decoded.samples.iter().all(|&s| s == 0));
    }

    #[test]
    fn embedded_blobs_are_sixteen_byte_aligned() {
        let mut bnk = audio_bank();
        bnk.insert_wem(300, vec![0xAB; 3]).unwrap();
        let bytes = bnk.to_bytes().unwrap();
        let parsed = Bnk::from_bytes(&bytes).unwrap();

        let didx = parsed
            .sections
            .iter()
            .find(|s| s.tag == DIDX)
            .expect("DIDX present");
        for entry in didx.data.chunks_exact(12) {
            let offset = u32::from_le_bytes([entry[4], entry[5], entry[6], entry[7]]) as usize;
            assert_eq!(offset % DATA_ALIGN, 0, "blob offset must be 16-aligned");
        }
    }

    #[test]
    fn removing_the_last_wem_drops_the_audio_sections() {
        let mut bnk = audio_bank();
        bnk.remove_wem(100).unwrap();
        bnk.remove_wem(200).unwrap();

        let tags: Vec<[u8; 4]> = bnk.sections.iter().map(|s| s.tag).collect();
        assert!(!tags.contains(&DIDX) && !tags.contains(&DATA));
        assert!(tags.contains(b"HIRC"), "hierarchy must still survive");
    }

    #[test]
    fn editing_a_missing_id_is_an_error_not_a_silent_no_op() {
        let mut bnk = audio_bank();
        assert!(matches!(
            bnk.replace_wem(999, vec![1]),
            Err(Error::NoSuchWem(999))
        ));
        assert!(matches!(bnk.remove_wem(999), Err(Error::NoSuchWem(999))));
        assert!(matches!(bnk.silence_wem(999), Err(Error::NoSuchWem(999))));
    }

    #[test]
    fn package_removal_leaves_a_dead_slot_so_the_table_keeps_its_length() {
        let mut wpk = Wpk::new(1);
        wpk.push(WemEntry::new("1.wem", vec![1, 2, 3]));
        wpk.push(WemEntry::new("2.wem", vec![4, 5, 6]));
        wpk.push(WemEntry::new("3.wem", vec![7, 8, 9]));

        wpk.remove_wem(2).unwrap();

        assert_eq!(wpk.entries.len(), 2);
        assert_eq!(
            wpk.dead_slots,
            vec![1],
            "the vacated slot stays in the table"
        );

        let bytes = wpk.to_bytes().unwrap();
        let parsed = Wpk::from_bytes(&bytes).unwrap();
        assert_eq!(parsed, wpk);
        assert_eq!(parsed.wem(1), Some(&[1u8, 2, 3][..]));
        assert_eq!(parsed.wem(3), Some(&[7u8, 8, 9][..]));
        assert_eq!(parsed.wem(2), None);
    }

    #[test]
    fn package_silencing_keeps_the_entry_name() {
        let mut wpk = Wpk::new(1);
        wpk.push(WemEntry::new("42.wem", silence(32000, 1, 16).unwrap()));
        wpk.silence_wem(42).unwrap();

        assert_eq!(wpk.entries[0].name, "42.wem");
        let decoded = Wem::new(&wpk.entries[0].data).unwrap().to_pcm().unwrap();
        assert_eq!(decoded.sample_rate, 32000);
        assert!(decoded.samples.iter().all(|&s| s == 0));
    }
}
