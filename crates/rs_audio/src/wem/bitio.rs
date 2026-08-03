use crate::error::{Error, Result};

/** Ogg's CRC-32: polynomial 0x04c11db7, no reflection and no final inversion, which is why a
stock CRC-32 will not do. Generated at compile time rather than pasted as a 256-entry table. */
const CRC_LOOKUP: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut r = (i as u32) << 24;
        let mut bit = 0;
        while bit < 8 {
            r = if r & 0x8000_0000 != 0 {
                (r << 1) ^ 0x04c1_1db7
            } else {
                r << 1
            };
            bit += 1;
        }
        table[i] = r;
        i += 1;
    }
    table
};

fn ogg_checksum(data: &[u8]) -> u32 {
    let mut crc: u32 = 0;
    for &byte in data {
        crc = (crc << 8) ^ CRC_LOOKUP[((crc >> 24) as u8 ^ byte) as usize];
    }
    crc
}

/** Reads least-significant-bit-first from a byte slice, which is the order Vorbis packs its
bitstream in. Running past the end of the slice is an error rather than a panic, so a truncated
or hostile packet cannot take the process down. */
pub(crate) struct BitReader<'a> {
    data: &'a [u8],
    byte_offset: usize,
    bit_buffer: u8,
    bits_left: u8,
    total_bits_read: usize,
}

impl<'a> BitReader<'a> {
    pub(crate) fn new(data: &'a [u8], initial_offset: usize) -> Self {
        Self {
            data,
            byte_offset: initial_offset,
            bit_buffer: 0,
            bits_left: 0,
            total_bits_read: 0,
        }
    }

    fn bit(&mut self) -> Result<u32> {
        if self.bits_left == 0 {
            let position = self
                .byte_offset
                .checked_add(self.total_bits_read / 8)
                .ok_or(Error::Wem("bit reader offset overflow"))?;
            self.bit_buffer = *self
                .data
                .get(position)
                .ok_or(Error::Wem("bit reader ran past end of packet"))?;
            self.bits_left = 8;
        }
        self.total_bits_read += 1;
        self.bits_left -= 1;
        Ok(u32::from(self.bit_buffer & (0x80 >> self.bits_left) != 0))
    }

    pub(crate) fn read(&mut self, count: u32) -> Result<u32> {
        let mut value = 0u32;
        for i in 0..count {
            if self.bit()? != 0 {
                value |= 1 << i;
            }
        }
        Ok(value)
    }

    pub(crate) fn total_bits_read(&self) -> usize {
        self.total_bits_read
    }
}

const MAX_PAYLOAD: usize = 255 * 255;
const HEADER_LEN: usize = 27;

/** Accumulates bits into Ogg pages: bits pack into bytes, bytes into a page payload, and a page
is framed with its lacing table and CRC when flushed. The payload is staged past the maximum
lacing table so segments can be moved into place once the segment count is known. */
pub(crate) struct OggWriter {
    output: Vec<u8>,
    bit_buffer: u8,
    bits_stored: u8,
    payload_bytes: usize,
    first: bool,
    continued: bool,
    granule: u32,
    sequence: u32,
    page: Vec<u8>,
}

impl OggWriter {
    pub(crate) fn new() -> Self {
        Self {
            output: Vec::new(),
            bit_buffer: 0,
            bits_stored: 0,
            payload_bytes: 0,
            first: true,
            continued: false,
            granule: 0,
            sequence: 0,
            page: vec![0u8; HEADER_LEN + 255 + MAX_PAYLOAD],
        }
    }

    fn put_bit(&mut self, bit: bool) {
        if bit {
            self.bit_buffer |= 1 << self.bits_stored;
        }
        self.bits_stored += 1;
        if self.bits_stored == 8 {
            self.flush_bits();
        }
    }

    pub(crate) fn write(&mut self, value: u32, count: u32) {
        for i in 0..count {
            self.put_bit(value & (1 << i) != 0);
        }
    }

    pub(crate) fn write_bytes(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.write(u32::from(byte), 8);
        }
    }

    pub(crate) fn set_granule(&mut self, granule: u32) {
        self.granule = granule;
    }

    fn flush_bits(&mut self) {
        if self.bits_stored == 0 {
            return;
        }
        if self.payload_bytes == MAX_PAYLOAD {
            self.flush_page(true, false);
        }
        self.page[HEADER_LEN + 255 + self.payload_bytes] = self.bit_buffer;
        self.payload_bytes += 1;
        self.bits_stored = 0;
        self.bit_buffer = 0;
    }

    pub(crate) fn flush_page(&mut self, next_continued: bool, last: bool) {
        if self.payload_bytes != MAX_PAYLOAD {
            self.flush_bits();
        }
        if self.payload_bytes == 0 {
            return;
        }

        /* Lacing values are a base-255 length: full 255s followed by a terminator below 255. A
        payload that is an exact multiple of 255 still needs that terminator, or the packet reads
        as continuing into the next page and the two merge. The one case with no terminator is a
        payload filling the page completely, which genuinely does continue. */
        let mut segments = self.payload_bytes / 255 + 1;
        if segments == 256 {
            segments = 255;
        }

        self.page.copy_within(
            HEADER_LEN + 255..HEADER_LEN + 255 + self.payload_bytes,
            HEADER_LEN + segments,
        );

        self.page[..4].copy_from_slice(b"OggS");
        self.page[4] = 0;
        self.page[5] =
            u8::from(self.continued) | (u8::from(self.first) << 1) | (u8::from(last) << 2);

        self.page[6..10].copy_from_slice(&self.granule.to_le_bytes());
        let granule_high: u32 = if self.granule == u32::MAX {
            u32::MAX
        } else {
            0
        };
        self.page[10..14].copy_from_slice(&granule_high.to_le_bytes());
        self.page[14..18].copy_from_slice(&1u32.to_le_bytes());
        self.page[18..22].copy_from_slice(&self.sequence.to_le_bytes());
        self.page[22..26].copy_from_slice(&0u32.to_le_bytes());
        self.page[26] = segments as u8;

        let mut remaining = self.payload_bytes;
        for i in 0..segments {
            let lacing = remaining.min(255);
            self.page[HEADER_LEN + i] = lacing as u8;
            remaining -= lacing;
        }

        let page_size = HEADER_LEN + segments + self.payload_bytes;
        let crc = ogg_checksum(&self.page[..page_size]);
        self.page[22..26].copy_from_slice(&crc.to_le_bytes());

        self.output.extend_from_slice(&self.page[..page_size]);

        self.sequence += 1;
        self.first = false;
        self.continued = next_continued;
        self.payload_bytes = 0;
    }

    pub(crate) fn finish(mut self) -> Vec<u8> {
        self.flush_page(false, false);
        self.output
    }
}

/** Vorbis' `ilog`: the position of the highest set bit, one-based, zero for zero. */
pub(crate) fn ilog(value: u32) -> u32 {
    32 - value.leading_zeros()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc_table_matches_the_ogg_polynomial() {
        assert_eq!(CRC_LOOKUP[0], 0x0000_0000);
        assert_eq!(CRC_LOOKUP[1], 0x04c1_1db7);
        assert_eq!(CRC_LOOKUP[2], 0x0982_3b6e);
        assert_eq!(CRC_LOOKUP[255], 0xb1f7_40b4);
    }

    #[test]
    fn ilog_matches_the_spec() {
        assert_eq!(ilog(0), 0);
        assert_eq!(ilog(1), 1);
        assert_eq!(ilog(2), 2);
        assert_eq!(ilog(7), 3);
        assert_eq!(ilog(8), 4);
    }

    #[test]
    fn bit_reader_is_lsb_first_within_each_byte() {
        let mut reader = BitReader::new(&[0b1010_0101], 0);
        assert_eq!(reader.read(4).unwrap(), 0b0101);
        assert_eq!(reader.read(4).unwrap(), 0b1010);
        assert_eq!(reader.total_bits_read(), 8);
    }

    #[test]
    fn bit_reader_errs_past_the_end_instead_of_panicking() {
        let mut reader = BitReader::new(&[0xFF], 0);
        assert!(reader.read(8).is_ok());
        assert!(reader.read(1).is_err());
    }

    /** A payload that is an exact multiple of 255 needs a terminating zero lacing value. Without
    it a decoder treats the packet as continuing into the next page and merges the two, silently
    dropping audio roughly once every 255 pages. */
    #[test]
    fn payload_on_a_lacing_boundary_gets_a_zero_terminator() {
        for (payload, expected) in [
            (100usize, vec![100u8]),
            (255, vec![255, 0]),
            (256, vec![255, 1]),
            (510, vec![255, 255, 0]),
        ] {
            let mut writer = OggWriter::new();
            writer.write_bytes(&vec![0xAB; payload]);
            let page = writer.finish();

            let segments = page[26] as usize;
            assert_eq!(
                segments,
                expected.len(),
                "payload {payload}: wrong segment count"
            );
            assert_eq!(
                &page[HEADER_LEN..HEADER_LEN + segments],
                expected.as_slice(),
                "payload {payload}: wrong lacing values"
            );
            assert_eq!(
                page.len(),
                HEADER_LEN + segments + payload,
                "payload {payload}: wrong page length"
            );
        }
    }

    #[test]
    fn writer_emits_a_capture_pattern_and_stable_crc() {
        let mut writer = OggWriter::new();
        writer.write_bytes(b"hello");
        let page = writer.finish();
        assert_eq!(&page[..4], b"OggS");
        assert_eq!(page[26], 1, "five bytes fit in one lacing segment");

        let mut again = OggWriter::new();
        again.write_bytes(b"hello");
        assert_eq!(page, again.finish(), "page framing must be deterministic");
    }
}
