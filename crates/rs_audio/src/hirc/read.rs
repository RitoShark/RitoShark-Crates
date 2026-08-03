use crate::error::{Error, Result};

/** A bounds-checked cursor over one HIRC object body.

Every read is checked against the end of the slice, so a version mismatch that walks the cursor
off the end produces an error rather than silently reading zeroes. That distinction is what lets
the caller demote a single object to opaque instead of accepting fabricated values. */
pub(super) struct Reader<'a> {
    data: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    pub(super) fn new(data: &'a [u8]) -> Self {
        Self { data, at: 0 }
    }

    pub(super) fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.at)
    }

    pub(super) fn u8(&mut self) -> Result<u8> {
        let value = *self
            .data
            .get(self.at)
            .ok_or(Error::Hirc("object body ended early"))?;
        self.at += 1;
        Ok(value)
    }

    pub(super) fn u16(&mut self) -> Result<u16> {
        let bytes = self
            .data
            .get(self.at..self.at + 2)
            .ok_or(Error::Hirc("object body ended early"))?;
        self.at += 2;
        Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
    }

    pub(super) fn u32(&mut self) -> Result<u32> {
        let bytes = self
            .data
            .get(self.at..self.at + 4)
            .ok_or(Error::Hirc("object body ended early"))?;
        self.at += 4;
        Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    pub(super) fn skip(&mut self, count: usize) -> Result<()> {
        let next = self
            .at
            .checked_add(count)
            .ok_or(Error::Hirc("skip overflows"))?;
        if next > self.data.len() {
            return Err(Error::Hirc("skip runs past the object body"));
        }
        self.at = next;
        Ok(())
    }

    /** Reads a list of object ids after checking the declared count against the bytes actually
    left. A count that cannot fit is the signature of a misparse — the cursor has drifted and is
    reading a length out of unrelated data — so it is rejected rather than allocated for. */
    pub(super) fn ids(&mut self, count: u32) -> Result<Vec<u32>> {
        let count = count as usize;
        if count.saturating_mul(4) > self.remaining() {
            return Err(Error::Hirc("declared child count exceeds the object body"));
        }
        let mut ids = Vec::with_capacity(count);
        for _ in 0..count {
            ids.push(self.u32()?);
        }
        Ok(ids)
    }

    pub(super) fn skip_scaled(&mut self, count: u32, size: usize) -> Result<()> {
        let total = (count as usize)
            .checked_mul(size)
            .ok_or(Error::Hirc("skip overflows"))?;
        self.skip(total)
    }
}

/** Wwise object bodies open with a block of mixing, positioning and automation parameters whose
size depends on the bank version. Nothing in the block is needed to answer which sound an event
plays, but the child list sits after it, so it has to be stepped over exactly.

The layout below covers the revisions League ships. Getting it wrong does not corrupt anything —
the cursor overruns the object body and the object is kept opaque instead. */
pub(super) fn skip_base_params(reader: &mut Reader, version: u32) -> Result<u32> {
    skip_initial_fx(reader, version)?;

    if version > 0x88 {
        reader.skip(1)?;
        let count = reader.u8()?;
        reader.skip_scaled(u32::from(count), 6)?;
    }
    if version > 0x59 && version <= 0x91 {
        reader.skip(1)?;
    }

    let _bus_id = reader.u32()?;
    let parent_id = reader.u32()?;
    reader.skip(if version <= 0x59 { 2 } else { 1 })?;

    skip_initial_params(reader)?;
    skip_positioning(reader, version)?;
    skip_aux(reader, version)?;
    reader.skip(6)?;
    skip_state_chunk(reader)?;
    skip_rtpc(reader, version)?;

    Ok(parent_id)
}

fn skip_initial_fx(reader: &mut Reader, version: u32) -> Result<()> {
    reader.skip(1)?;
    let count = reader.u8()?;
    if count > 0 {
        reader.skip(1)?;
    }
    reader.skip_scaled(u32::from(count), if version <= 0x91 { 7 } else { 6 })
}

pub(super) fn skip_initial_params(reader: &mut Reader) -> Result<()> {
    let count = reader.u8()?;
    reader.skip_scaled(u32::from(count), 5)?;
    let ranged = reader.u8()?;
    reader.skip_scaled(u32::from(ranged), 9)
}

fn skip_positioning(reader: &mut Reader, version: u32) -> Result<()> {
    let bits = reader.u8()?;
    let has_positioning = bits & 1 != 0;
    let mut has_3d = false;
    let mut has_automation = false;

    if has_positioning {
        if version <= 0x59 {
            let has_2d = reader.u8()?;
            has_3d = reader.u8()? != 0;
            if has_2d != 0 {
                reader.skip(1)?;
            }
        } else {
            has_3d = bits & 0x2 != 0;
        }
    }

    if has_positioning && has_3d {
        if version <= 0x59 {
            has_automation = reader.u8()? & 3 != 1;
            reader.skip(8)?;
        } else {
            has_automation = (bits >> 5) & 3 != 0;
            reader.skip(1)?;
        }
    }

    if has_automation {
        reader.skip(if version <= 0x59 { 9 } else { 5 })?;
        let vertices = reader.u32()?;
        reader.skip_scaled(vertices, 16)?;
        let playlist = reader.u32()?;
        reader.skip_scaled(playlist, if version <= 0x59 { 16 } else { 20 })?;
    } else if version <= 0x59 {
        reader.skip(1)?;
    }

    Ok(())
}

fn skip_aux(reader: &mut Reader, version: u32) -> Result<()> {
    let bits = reader.u8()?;
    if (bits >> 3) & 1 != 0 {
        reader.skip(16)?;
    }
    if version > 0x87 {
        reader.skip(4)?;
    }
    Ok(())
}

fn skip_state_chunk(reader: &mut Reader) -> Result<()> {
    let props = reader.u8()?;
    reader.skip_scaled(u32::from(props), 3)?;
    let groups = reader.u8()?;
    for _ in 0..groups {
        reader.skip(5)?;
        let states = reader.u8()?;
        reader.skip_scaled(u32::from(states), 8)?;
    }
    Ok(())
}

fn skip_rtpc(reader: &mut Reader, version: u32) -> Result<()> {
    let count = reader.u16()?;
    for _ in 0..count {
        reader.skip(if version <= 0x59 { 13 } else { 12 })?;
        let points = reader.u16()?;
        reader.skip_scaled(u32::from(points), 12)?;
    }
    Ok(())
}
