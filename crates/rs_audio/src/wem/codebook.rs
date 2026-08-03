use super::bitio::{BitReader, OggWriter, ilog};
use crate::error::{Error, Result};

/** The aoTuV 6.03 codebook set every Wwise Vorbis stream in League references by index. Wwise
strips these from the file to save space, so decoding is impossible without them. */
const PACKED_CODEBOOKS: &[u8] = include_bytes!("../../resources/packed_codebooks_aoTuV_603.bin");

/** Vorbis' `BCV` codebook sync pattern, written into every rebuilt codebook. */
const CODEBOOK_SYNC: u32 = 0x0056_4342;

/** The packed library: codebook bodies concatenated, followed by an offset table whose position
is stored in the last four bytes. Entry `i` spans `offsets[i]..offsets[i + 1]`, so the table has
one more entry than there are codebooks. */
pub(crate) struct CodebookLibrary {
    data: Vec<u8>,
    offsets: Vec<u32>,
}

impl CodebookLibrary {
    pub(crate) fn packed() -> Result<Self> {
        Self::load(PACKED_CODEBOOKS)
    }

    fn load(data: &[u8]) -> Result<Self> {
        if data.len() < 8 {
            return Err(Error::Wem("codebook library too small"));
        }

        let table_start = u32::from_le_bytes([
            data[data.len() - 4],
            data[data.len() - 3],
            data[data.len() - 2],
            data[data.len() - 1],
        ]) as usize;

        if table_start >= data.len() || (data.len() - table_start) % 4 != 0 {
            return Err(Error::Wem("codebook offset table is out of range"));
        }

        let count = (data.len() - table_start) / 4;
        if count < 2 {
            return Err(Error::Wem("codebook library has no entries"));
        }

        let mut offsets = Vec::with_capacity(count);
        for i in 0..count {
            let at = table_start + i * 4;
            offsets.push(u32::from_le_bytes([
                data[at],
                data[at + 1],
                data[at + 2],
                data[at + 3],
            ]));
        }

        Ok(Self {
            data: data[..table_start].to_vec(),
            offsets,
        })
    }

    fn count(&self) -> usize {
        self.offsets.len() - 1
    }

    fn codebook(&self, id: usize) -> Result<&[u8]> {
        if id >= self.count() {
            return Err(Error::UnknownCodebook(id as u32));
        }
        let start = self.offsets[id] as usize;
        let end = self.offsets[id + 1] as usize;
        self.data
            .get(start..end)
            .ok_or(Error::Wem("codebook entry runs past the library"))
    }

    /** Expands one packed codebook into the full Vorbis setup-header form. The packed layout
    omits the sync pattern and narrows several fields, so every value is re-read at its packed
    width and re-emitted at its spec width. */
    pub(crate) fn rebuild(&self, id: usize, out: &mut OggWriter) -> Result<()> {
        let packed = self.codebook(id)?;
        let mut input = BitReader::new(packed, 0);

        let dimensions = input.read(4)?;
        let entries = input.read(14)?;

        out.write(CODEBOOK_SYNC, 24);
        out.write(dimensions, 16);
        out.write(entries, 24);

        let ordered = input.read(1)?;
        out.write(ordered, 1);

        if ordered != 0 {
            out.write(input.read(5)?, 5);

            let mut current = 0u32;
            while current < entries {
                let bits = ilog(entries - current);
                let number = input.read(bits)?;
                out.write(number, bits);
                current = current
                    .checked_add(number)
                    .ok_or(Error::Wem("ordered codebook entry count overflow"))?;
            }
            if current > entries {
                return Err(Error::Wem("ordered codebook overran its entry count"));
            }
        } else {
            let length_bits = input.read(3)?;
            let sparse = input.read(1)?;

            if length_bits == 0 || length_bits > 5 {
                return Err(Error::Wem("nonsensical packed codeword length"));
            }

            out.write(sparse, 1);

            for _ in 0..entries {
                let present = if sparse != 0 {
                    let flag = input.read(1)?;
                    out.write(flag, 1);
                    flag != 0
                } else {
                    true
                };
                if present {
                    out.write(input.read(length_bits)?, 5);
                }
            }
        }

        /* The packed form spends one bit on the lookup type; the spec spends four. */
        let lookup_type = input.read(1)?;
        out.write(lookup_type, 4);

        match lookup_type {
            0 => {}
            1 => self.copy_lookup_values(&mut input, out, entries, dimensions)?,
            _ => return Err(Error::Wem("invalid codebook lookup type")),
        }

        Ok(())
    }

    /** Copies a codebook that is already in full spec form, used by the older header-triad
    layout where Wwise embedded the setup header verbatim. */
    pub(crate) fn copy(input: &mut BitReader, out: &mut OggWriter) -> Result<()> {
        let sync = input.read(24)?;
        let dimensions = input.read(16)?;
        let entries = input.read(24)?;

        if sync != CODEBOOK_SYNC {
            return Err(Error::Wem("codebook is missing its sync pattern"));
        }

        out.write(sync, 24);
        out.write(dimensions, 16);
        out.write(entries, 24);

        let ordered = input.read(1)?;
        out.write(ordered, 1);

        if ordered != 0 {
            out.write(input.read(5)?, 5);

            let mut current = 0u32;
            while current < entries {
                let bits = ilog(entries - current);
                let number = input.read(bits)?;
                out.write(number, bits);
                current = current
                    .checked_add(number)
                    .ok_or(Error::Wem("ordered codebook entry count overflow"))?;
            }
        } else {
            let sparse = input.read(1)?;
            out.write(sparse, 1);

            for _ in 0..entries {
                let present = if sparse != 0 {
                    let flag = input.read(1)?;
                    out.write(flag, 1);
                    flag != 0
                } else {
                    true
                };
                if present {
                    out.write(input.read(5)?, 5);
                }
            }
        }

        let lookup_type = input.read(4)?;
        out.write(lookup_type, 4);

        match lookup_type {
            0 => {}
            1 => Self::copy_lookup_values_inner(input, out, entries, dimensions)?,
            2 => return Err(Error::Wem("unexpected codebook lookup type 2")),
            _ => return Err(Error::Wem("invalid codebook lookup type")),
        }

        Ok(())
    }

    fn copy_lookup_values(
        &self,
        input: &mut BitReader,
        out: &mut OggWriter,
        entries: u32,
        dimensions: u32,
    ) -> Result<()> {
        Self::copy_lookup_values_inner(input, out, entries, dimensions)
    }

    fn copy_lookup_values_inner(
        input: &mut BitReader,
        out: &mut OggWriter,
        entries: u32,
        dimensions: u32,
    ) -> Result<()> {
        let min = input.read(32)?;
        let max = input.read(32)?;
        let value_length = input.read(4)?;
        let sequence_flag = input.read(1)?;

        out.write(min, 32);
        out.write(max, 32);
        out.write(value_length, 4);
        out.write(sequence_flag, 1);

        for _ in 0..maptype1_quantvals(entries, dimensions)? {
            out.write(input.read(value_length + 1)?, value_length + 1);
        }

        Ok(())
    }
}

/** The number of lookup values a type-1 codebook stores: the largest `v` where `v^dimensions`
still fits within `entries`. Searching for it iteratively is what the Vorbis reference does; the
arithmetic is saturating and the search is bounded so malformed input cannot hang or overflow. */
fn maptype1_quantvals(entries: u32, dimensions: u32) -> Result<u32> {
    if entries == 0 || dimensions == 0 {
        return Ok(0);
    }

    let bits = ilog(entries);
    let mut vals = entries >> ((bits - 1) * (dimensions - 1) / dimensions);

    for _ in 0..64 {
        let mut acc = 1u64;
        let mut acc_next = 1u64;
        for _ in 0..dimensions {
            acc = acc.saturating_mul(u64::from(vals));
            acc_next = acc_next.saturating_mul(u64::from(vals) + 1);
        }

        if acc <= u64::from(entries) && acc_next > u64::from(entries) {
            return Ok(vals);
        }
        if acc > u64::from(entries) {
            vals = vals
                .checked_sub(1)
                .ok_or(Error::Wem("codebook lookup value search underflowed"))?;
        } else {
            vals += 1;
        }
    }

    Err(Error::Wem("codebook lookup value search did not converge"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packed_library_loads() {
        let library = CodebookLibrary::packed().expect("bundled codebooks should load");
        assert!(
            library.count() > 500,
            "aoTuV 6.03 ships several hundred codebooks, found {}",
            library.count()
        );
        assert!(library.codebook(0).is_ok());
        assert!(library.codebook(library.count() - 1).is_ok());
    }

    #[test]
    fn out_of_range_codebook_is_an_error() {
        let library = CodebookLibrary::packed().unwrap();
        let last = library.count();
        assert!(matches!(
            library.codebook(last),
            Err(Error::UnknownCodebook(_))
        ));
    }

    #[test]
    fn truncated_library_errs_instead_of_panicking() {
        assert!(CodebookLibrary::load(&[]).is_err());
        assert!(CodebookLibrary::load(&[0; 4]).is_err());
        assert!(CodebookLibrary::load(&[0xFF; 16]).is_err());
    }

    #[test]
    fn quantvals_matches_reference_values() {
        assert_eq!(maptype1_quantvals(0, 4).unwrap(), 0);
        assert_eq!(maptype1_quantvals(16, 2).unwrap(), 4);
        assert_eq!(maptype1_quantvals(81, 4).unwrap(), 3);
        assert_eq!(maptype1_quantvals(1, 1).unwrap(), 1);
    }
}
