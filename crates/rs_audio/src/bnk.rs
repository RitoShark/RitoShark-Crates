use std::io::{Read, Seek, Write};

use rs_io::{Parse, ReaderExt, Serialize, WriterExt};

use crate::error::{Error, Result};

const DIDX: [u8; 4] = *b"DIDX";
const DATA: [u8; 4] = *b"DATA";

/** One raw BNK chunk: its four-byte tag and the verbatim body. Unknown sections survive untouched
so the container round-trips byte for byte. */
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BnkSection {
    pub tag: [u8; 4],
    pub data: Vec<u8>,
}

/** A Wwise SoundBank as its flat sequence of chunked sections. Parsing reads each `tag + u32 size
+ body` triple in order; serializing writes them back unchanged. Embedded `.wem` audio is reached
through the DIDX index and the DATA blob via [`Bnk::wems`]. */
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Bnk {
    pub sections: Vec<BnkSection>,
}

impl Bnk {
    pub fn new() -> Self {
        Self::default()
    }

    fn section(&self, tag: [u8; 4]) -> Option<&BnkSection> {
        self.sections.iter().find(|s| s.tag == tag)
    }

    /** The embedded `.wem` blobs as `(id, bytes)`, resolved by slicing DATA with the offsets and
    sizes listed in DIDX. Returns an empty vector when either section is absent. An out-of-range
    DIDX entry is skipped rather than panicking. */
    pub fn wems(&self) -> Vec<(u32, &[u8])> {
        let (Some(didx), Some(data)) = (self.section(DIDX), self.section(DATA)) else {
            return Vec::new();
        };

        let mut out = Vec::with_capacity(didx.data.len() / 12);
        for entry in didx.data.chunks_exact(12) {
            let id = u32::from_le_bytes([entry[0], entry[1], entry[2], entry[3]]);
            let offset = u32::from_le_bytes([entry[4], entry[5], entry[6], entry[7]]) as usize;
            let size = u32::from_le_bytes([entry[8], entry[9], entry[10], entry[11]]) as usize;
            let Some(end) = offset.checked_add(size) else {
                continue;
            };
            if let Some(slice) = data.data.get(offset..end) {
                out.push((id, slice));
            }
        }
        out
    }
}

impl Parse for Bnk {
    type Error = Error;

    fn from_reader<R: Read + Seek>(reader: &mut R) -> Result<Self> {
        let mut sections = Vec::new();
        loop {
            let mut first = [0u8; 1];
            match reader.read(&mut first) {
                Ok(0) => break,
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(Error::Io(e.into())),
            }
            let rest = reader.read_array::<3>()?;
            let tag = [first[0], rest[0], rest[1], rest[2]];
            let size = reader.read_u32()? as usize;
            let data = reader.read_bytes(size)?;
            sections.push(BnkSection { tag, data });
        }
        Ok(Self { sections })
    }
}

impl Serialize for Bnk {
    type Error = Error;

    fn to_writer<W: Write>(&self, writer: &mut W) -> Result<()> {
        for section in &self.sections {
            writer.write_bytes(&section.tag)?;
            writer.write_u32(section.data.len() as u32)?;
            writer.write_bytes(&section.data)?;
        }
        Ok(())
    }
}
