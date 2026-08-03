use super::bitio::{BitReader, OggWriter, ilog};
use super::codebook::CodebookLibrary;
use super::{VorbLocation, Wem, le_u16, le_u32};
use crate::error::{Error, Result};

/** Identifies the stream as produced here rather than by the encoder Wwise used. It lands in the
Vorbis comment header, which carries no audio, so its length does not affect decoded samples. */
const VENDOR: &[u8] = b"rs_audio";

/** Wwise strips the three Vorbis header packets and rewrites packet framing to save space. What
survives in the file is: a setup packet holding codebook indices plus the floor, residue, mapping
and mode configuration; and audio packets with short length-prefixed headers instead of Ogg pages.

Rebuilding a playable Ogg means synthesising the identification and comment headers from the
`fmt ` parameters, expanding the referenced codebooks out of the aoTuV library, re-emitting the
setup configuration at spec bit widths, and repaging the audio packets untouched. */
struct Stream {
    sample_count: u32,
    setup_packet_offset: usize,
    first_audio_packet_offset: usize,
    blocksize_0_pow: u32,
    blocksize_1_pow: u32,
    header_triad_present: bool,
    old_packet_headers: bool,
    no_granule: bool,
    mod_packets: bool,
    loop_count: u32,
    loop_start: u32,
    loop_end: u32,
}

/** A Wwise audio packet header: a length, and on most layouts a granule position. Three widths
exist across Wwise versions — 8 bytes on the oldest, 6 normally, and 2 when the layout omits
granules entirely. */
struct PacketHeader {
    header_size: usize,
    size: usize,
    granule: u32,
}

impl PacketHeader {
    fn read(data: &[u8], at: usize, no_granule: bool) -> Result<Self> {
        let size = le_u16(data, at)? as usize;
        if no_granule {
            Ok(Self {
                header_size: 2,
                size,
                granule: 0,
            })
        } else {
            Ok(Self {
                header_size: 6,
                size,
                granule: le_u32(data, at + 2)?,
            })
        }
    }

    fn read_old(data: &[u8], at: usize) -> Result<Self> {
        Ok(Self {
            header_size: 8,
            size: le_u32(data, at)? as usize,
            granule: le_u32(data, at + 4)?,
        })
    }

    fn payload(&self, base: usize) -> usize {
        base + self.header_size
    }

    fn next(&self, base: usize) -> usize {
        base + self.header_size + self.size
    }
}

impl Stream {
    fn parse(wem: &Wem<'_>, vorb: VorbLocation) -> Result<Self> {
        let data = wem.bytes();
        let at = vorb.offset;

        /* -1 in the reference implementation: the parameters are folded into an extended fmt. */
        let size = vorb.size.map_or(-1i32, |s| s as i32);
        if ![-1, 0x28, 0x2A, 0x2C, 0x32, 0x34].contains(&size) {
            return Err(Error::Wem("unrecognised vorb chunk size"));
        }

        let sample_count = le_u32(data, at)?;

        let folded = size == -1 || size == 0x2A;
        let mut no_granule = false;
        let mut mod_packets = false;

        let parameters_at = if folded {
            no_granule = true;
            let signal = le_u32(data, at + 0x04)?;
            mod_packets = !matches!(signal, 0x4A | 0x4B | 0x69 | 0x70);
            at + 0x10
        } else {
            at + 0x18
        };

        let setup_packet_offset = le_u32(data, parameters_at)? as usize;
        let first_audio_packet_offset = le_u32(data, parameters_at + 4)? as usize;

        /* The layouts that embed all three Vorbis headers are also the ones using 8-byte packet
        headers, so the two travel together. */
        let header_triad_present = size == 0x28 || size == 0x2C;
        let old_packet_headers = header_triad_present;
        let mut blocksize_0_pow = 0;
        let mut blocksize_1_pow = 0;

        if !header_triad_present {
            let blocksize_at = if folded { at + 0x24 } else { at + 0x2C } + 4;
            blocksize_0_pow = u32::from(
                *data
                    .get(blocksize_at)
                    .ok_or(Error::Wem("vorb chunk is truncated before blocksizes"))?,
            );
            blocksize_1_pow = u32::from(
                *data
                    .get(blocksize_at + 1)
                    .ok_or(Error::Wem("vorb chunk is truncated before blocksizes"))?,
            );
        }

        let mut loop_count = 0;
        let mut loop_start = 0;
        let mut loop_end = 0;
        if let Some(smpl) = wem.smpl_offset {
            loop_count = le_u32(data, smpl + 0x1C)?;
            if loop_count == 1 {
                loop_start = le_u32(data, smpl + 0x2C)?;
                loop_end = le_u32(data, smpl + 0x30)?;
            }
        }
        if loop_count != 0 {
            loop_end = if loop_end == 0 {
                sample_count
            } else {
                loop_end + 1
            };
        }

        Ok(Self {
            sample_count,
            setup_packet_offset,
            first_audio_packet_offset,
            blocksize_0_pow,
            blocksize_1_pow,
            header_triad_present,
            old_packet_headers,
            no_granule,
            mod_packets,
            loop_count,
            loop_start,
            loop_end,
        })
    }
}

pub(super) fn to_ogg(wem: &Wem<'_>, vorb: VorbLocation) -> Result<Vec<u8>> {
    let stream = Stream::parse(wem, vorb)?;
    let library = CodebookLibrary::packed()?;
    let data = wem.bytes();

    let mut out = OggWriter::new();
    let mut modes: Option<Vec<bool>> = None;
    let mut mode_bits = 0u32;

    if stream.header_triad_present {
        write_embedded_headers(wem, &stream, &mut out)?;
    } else {
        let (blockflags, bits) = write_rebuilt_headers(wem, &stream, &library, &mut out)?;
        modes = Some(blockflags);
        mode_bits = bits;
    }

    let data_end = wem
        .data_offset
        .checked_add(wem.data_size)
        .ok_or(Error::Wem("data chunk length overflows"))?;
    let mut at = wem
        .data_offset
        .checked_add(stream.first_audio_packet_offset)
        .ok_or(Error::Wem("first audio packet offset overflows"))?;
    let mut previous_blockflag = false;
    let mut granule = 0u32;
    let mut previous_blocksize: Option<u32> = None;

    while at < data_end {
        let packet = if stream.old_packet_headers {
            PacketHeader::read_old(data, at)?
        } else {
            PacketHeader::read(data, at, stream.no_granule)?
        };

        let payload_at = packet.payload(at);
        let next_at = packet.next(at);
        if next_at > data_end {
            return Err(Error::Wem("audio packet runs past the data chunk"));
        }

        let payload = data
            .get(payload_at..payload_at + packet.size)
            .ok_or(Error::Wem("audio packet runs past end of wem"))?;

        /* The mode number selects the window size, which the granule arithmetic below needs and
        the mod-packet branch re-emits. Wwise strips the leading packet-type bit when it packs
        mod packets, so only the untouched layout has one to skip. */
        let mut bits = BitReader::new(data, payload_at);
        let mode_number = if modes.is_some() && !payload.is_empty() {
            if !stream.mod_packets {
                bits.read(1)?;
            }
            bits.read(mode_bits)?
        } else {
            0
        };

        out.set_granule(next_granule(
            &stream,
            modes.as_deref(),
            mode_number,
            next_at >= data_end,
            &mut granule,
            &mut previous_blocksize,
            packet.granule,
        ));

        if stream.mod_packets {
            let blockflags = modes
                .as_ref()
                .ok_or(Error::Wem("mod packets need a rebuilt mode list"))?;

            /* Wwise drops the leading packet-type bit; audio packets are always type 0. */
            out.write(0, 1);
            out.write(mode_number, mode_bits);
            let remainder = bits.read(8 - mode_bits)?;

            if blockflags
                .get(mode_number as usize)
                .copied()
                .unwrap_or(false)
            {
                /* A long window needs the neighbouring window flags the stripped header held,
                so the next packet's mode is peeked at to recover the following one. */
                let mut next_blockflag = false;
                if next_at + packet.header_size <= data_end {
                    let next = PacketHeader::read(data, next_at, stream.no_granule)?;
                    if next.size > 0 {
                        let mut next_bits = BitReader::new(data, next.payload(next_at));
                        let next_mode = next_bits.read(mode_bits)?;
                        next_blockflag =
                            blockflags.get(next_mode as usize).copied().unwrap_or(false);
                    }
                }
                out.write(u32::from(previous_blockflag), 1);
                out.write(u32::from(next_blockflag), 1);
            }

            previous_blockflag = blockflags
                .get(mode_number as usize)
                .copied()
                .unwrap_or(false);
            out.write(remainder, 8 - mode_bits);
            out.write_bytes(payload.get(1..).unwrap_or_default());
        } else {
            out.write_bytes(payload);
        }

        at = next_at;
        out.flush_page(false, at >= data_end);
    }

    Ok(out.finish())
}

fn write_packet_header(out: &mut OggWriter, packet_type: u32) {
    out.write(packet_type, 8);
    out.write_bytes(b"vorbis");
}

/** The granule position to stamp on the page this packet lands in.

Wwise's compact packet headers are two bytes and carry no granule at all, so for those layouts
the position has to be reconstructed the way an encoder computes it: a packet advances the stream
by a quarter of its own window plus a quarter of the previous one, and the first packet advances
it by nothing. The final page is clamped to the sample count the `vorb` chunk declares, which is
what tells a decoder how much of the last window is real audio rather than overlap padding.

Getting this wrong is silent — the stream still decodes, just with the tail of the file missing. */
#[allow(clippy::too_many_arguments)]
fn next_granule(
    stream: &Stream,
    modes: Option<&[bool]>,
    mode_number: u32,
    is_last: bool,
    granule: &mut u32,
    previous_blocksize: &mut Option<u32>,
    stored: u32,
) -> u32 {
    if !stream.no_granule {
        return if stored == u32::MAX { 1 } else { stored };
    }

    let long_window = modes
        .and_then(|flags| flags.get(mode_number as usize).copied())
        .unwrap_or(false);
    let blocksize = 1u32
        << if long_window {
            stream.blocksize_1_pow
        } else {
            stream.blocksize_0_pow
        };

    if let Some(previous) = *previous_blocksize {
        *granule = granule.saturating_add((previous + blocksize) / 4);
    }
    *previous_blocksize = Some(blocksize);

    if is_last && stream.sample_count > 0 {
        (*granule).min(stream.sample_count)
    } else {
        *granule
    }
}

/** Synthesises the identification and comment headers from the `fmt ` parameters, then rebuilds
the setup header from the stripped one. Returns the mode block flags and the width of a mode
number, both of which the audio packet loop needs to reconstruct window flags. */
fn write_rebuilt_headers(
    wem: &Wem<'_>,
    stream: &Stream,
    library: &CodebookLibrary,
    out: &mut OggWriter,
) -> Result<(Vec<bool>, u32)> {
    let format = wem.format();

    write_packet_header(out, 1);
    out.write(0, 32);
    out.write(u32::from(format.channels), 8);
    out.write(format.sample_rate, 32);
    out.write(0, 32);
    out.write(format.avg_bytes_per_second.saturating_mul(8), 32);
    out.write(0, 32);
    out.write(stream.blocksize_0_pow, 4);
    out.write(stream.blocksize_1_pow, 4);
    out.write(1, 1);
    out.flush_page(false, false);

    write_packet_header(out, 3);
    out.write(VENDOR.len() as u32, 32);
    out.write_bytes(VENDOR);

    if stream.loop_count == 0 {
        out.write(0, 32);
    } else {
        out.write(2, 32);
        for comment in [
            format!("LoopStart={}", stream.loop_start),
            format!("LoopEnd={}", stream.loop_end),
        ] {
            out.write(comment.len() as u32, 32);
            out.write_bytes(comment.as_bytes());
        }
    }
    out.write(1, 1);
    out.flush_page(false, false);

    write_packet_header(out, 5);

    let data = wem.bytes();
    let setup_at = wem
        .data_offset
        .checked_add(stream.setup_packet_offset)
        .ok_or(Error::Wem("setup packet offset overflows"))?;
    let setup = PacketHeader::read(data, setup_at, stream.no_granule)?;
    if setup.granule != 0 {
        return Err(Error::Wem("setup packet has a non-zero granule"));
    }

    let mut bits = BitReader::new(data, setup.payload(setup_at));

    let codebook_count_less1 = bits.read(8)?;
    let codebook_count = codebook_count_less1 + 1;
    out.write(codebook_count_less1, 8);

    for _ in 0..codebook_count {
        let id = bits.read(10)?;
        library.rebuild(id as usize, out)?;
    }

    /* Time domain transforms: one, and always the placeholder value. */
    out.write(0, 6);
    out.write(0, 16);

    let modes = rebuild_setup(wem, &mut bits, out, codebook_count)?;

    out.write(1, 1);
    out.flush_page(false, false);

    Ok(modes)
}

/** Re-emits the floor, residue, mapping and mode configuration. Wwise narrows several fields
that the spec stores wider — a residue type is two bits here and sixteen there, a lookup type one
bit and four — so every field is read at its packed width and written at its spec width. Indices
into earlier tables are validated, since a bad one would otherwise produce a stream that decodes
to noise rather than failing. */
fn rebuild_setup(
    wem: &Wem<'_>,
    bits: &mut BitReader,
    out: &mut OggWriter,
    codebook_count: u32,
) -> Result<(Vec<bool>, u32)> {
    let channels = u32::from(wem.format().channels);

    let floor_count_less1 = bits.read(6)?;
    let floor_count = floor_count_less1 + 1;
    out.write(floor_count_less1, 6);

    for _ in 0..floor_count {
        out.write(1, 16);

        let partitions = bits.read(5)?;
        out.write(partitions, 5);

        let mut partition_classes = Vec::with_capacity(partitions as usize);
        let mut maximum_class = 0u32;
        for _ in 0..partitions {
            let class = bits.read(4)?;
            out.write(class, 4);
            partition_classes.push(class);
            maximum_class = maximum_class.max(class);
        }

        let mut class_dimensions = Vec::with_capacity(maximum_class as usize + 1);
        for _ in 0..=maximum_class {
            let dimensions_less1 = bits.read(3)?;
            out.write(dimensions_less1, 3);
            class_dimensions.push(dimensions_less1 + 1);

            let subclasses = bits.read(2)?;
            out.write(subclasses, 2);

            if subclasses != 0 {
                let masterbook = bits.read(8)?;
                out.write(masterbook, 8);
                if masterbook >= codebook_count {
                    return Err(Error::Wem("floor masterbook is out of range"));
                }
            }

            for _ in 0..(1u32 << subclasses) {
                out.write(bits.read(8)?, 8);
            }
        }

        out.write(bits.read(2)?, 2);
        let rangebits = bits.read(4)?;
        out.write(rangebits, 4);

        for &class in &partition_classes {
            let dimensions = class_dimensions
                .get(class as usize)
                .copied()
                .ok_or(Error::Wem("floor partition class is out of range"))?;
            for _ in 0..dimensions {
                out.write(bits.read(rangebits)?, rangebits);
            }
        }
    }

    let residue_count_less1 = bits.read(6)?;
    let residue_count = residue_count_less1 + 1;
    out.write(residue_count_less1, 6);

    for _ in 0..residue_count {
        let residue_type = bits.read(2)?;
        out.write(residue_type, 16);
        if residue_type > 2 {
            return Err(Error::Wem("invalid residue type"));
        }

        let begin = bits.read(24)?;
        let end = bits.read(24)?;
        let partition_size_less1 = bits.read(24)?;
        let classifications_less1 = bits.read(6)?;
        let classbook = bits.read(8)?;

        out.write(begin, 24);
        out.write(end, 24);
        out.write(partition_size_less1, 24);
        out.write(classifications_less1, 6);
        out.write(classbook, 8);

        if classbook >= codebook_count {
            return Err(Error::Wem("residue classbook is out of range"));
        }

        let mut cascade = Vec::with_capacity(classifications_less1 as usize + 1);
        for _ in 0..=classifications_less1 {
            let low = bits.read(3)?;
            out.write(low, 3);

            let has_high = bits.read(1)?;
            out.write(has_high, 1);

            let high = if has_high != 0 {
                let value = bits.read(5)?;
                out.write(value, 5);
                value
            } else {
                0
            };
            cascade.push(high * 8 + low);
        }

        for &entry in &cascade {
            for bit in 0..8u32 {
                if entry & (1 << bit) != 0 {
                    let book = bits.read(8)?;
                    out.write(book, 8);
                    if book >= codebook_count {
                        return Err(Error::Wem("residue book is out of range"));
                    }
                }
            }
        }
    }

    let mapping_count_less1 = bits.read(6)?;
    let mapping_count = mapping_count_less1 + 1;
    out.write(mapping_count_less1, 6);

    for _ in 0..mapping_count {
        out.write(0, 16);

        let has_submaps = bits.read(1)?;
        out.write(has_submaps, 1);

        let submaps = if has_submaps != 0 {
            let less1 = bits.read(4)?;
            out.write(less1, 4);
            less1 + 1
        } else {
            1
        };

        let square_polar = bits.read(1)?;
        out.write(square_polar, 1);

        if square_polar != 0 {
            let coupling_less1 = bits.read(8)?;
            out.write(coupling_less1, 8);

            if channels == 0 {
                return Err(Error::Wem("channel coupling on a zero-channel stream"));
            }
            let channel_bits = ilog(channels - 1);
            for _ in 0..=coupling_less1 {
                let magnitude = bits.read(channel_bits)?;
                let angle = bits.read(channel_bits)?;
                out.write(magnitude, channel_bits);
                out.write(angle, channel_bits);
            }
        }

        let reserved = bits.read(2)?;
        out.write(reserved, 2);
        if reserved != 0 {
            return Err(Error::Wem("mapping reserved field is non-zero"));
        }

        if submaps > 1 {
            for _ in 0..channels {
                out.write(bits.read(4)?, 4);
            }
        }

        for _ in 0..submaps {
            out.write(bits.read(8)?, 8);

            let floor = bits.read(8)?;
            out.write(floor, 8);
            if floor >= floor_count {
                return Err(Error::Wem("mapping floor index is out of range"));
            }

            let residue = bits.read(8)?;
            out.write(residue, 8);
            if residue >= residue_count {
                return Err(Error::Wem("mapping residue index is out of range"));
            }
        }
    }

    let mode_count_less1 = bits.read(6)?;
    let mode_count = mode_count_less1 + 1;
    out.write(mode_count_less1, 6);

    let mut blockflags = Vec::with_capacity(mode_count as usize);
    for _ in 0..mode_count {
        let blockflag = bits.read(1)?;
        out.write(blockflag, 1);
        blockflags.push(blockflag != 0);

        out.write(0, 16);
        out.write(0, 16);

        let mapping = bits.read(8)?;
        out.write(mapping, 8);
        if mapping >= mapping_count {
            return Err(Error::Wem("mode mapping index is out of range"));
        }
    }

    Ok((blockflags, ilog(mode_count - 1)))
}

/** The older layout keeps all three Vorbis headers in the file, so they are copied straight
through — only the codebooks need re-emitting, and they are already in spec form. */
fn write_embedded_headers(wem: &Wem<'_>, stream: &Stream, out: &mut OggWriter) -> Result<()> {
    let data = wem.bytes();
    let mut at = wem
        .data_offset
        .checked_add(stream.setup_packet_offset)
        .ok_or(Error::Wem("setup packet offset overflows"))?;

    for expected_type in [1u8, 3] {
        let packet = PacketHeader::read_old(data, at)?;
        if packet.granule != 0 {
            return Err(Error::Wem("header packet has a non-zero granule"));
        }
        let payload = data
            .get(packet.payload(at)..packet.payload(at) + packet.size)
            .ok_or(Error::Wem("header packet runs past end of wem"))?;
        if payload.first() != Some(&expected_type) {
            return Err(Error::Wem("unexpected Vorbis header packet type"));
        }
        out.write_bytes(payload);
        out.flush_page(false, false);
        at = packet.next(at);
    }

    let setup = PacketHeader::read_old(data, at)?;
    if setup.granule != 0 {
        return Err(Error::Wem("setup packet has a non-zero granule"));
    }

    let mut bits = BitReader::new(data, setup.payload(at));

    let setup_type = bits.read(8)?;
    if setup_type != 5 {
        return Err(Error::Wem("unexpected Vorbis setup packet type"));
    }
    out.write(setup_type, 8);
    for _ in 0..6 {
        out.write(bits.read(8)?, 8);
    }

    let codebook_count_less1 = bits.read(8)?;
    out.write(codebook_count_less1, 8);
    for _ in 0..=codebook_count_less1 {
        CodebookLibrary::copy(&mut bits, out)?;
    }

    while bits.total_bits_read() < setup.size * 8 {
        out.write(bits.read(1)?, 1);
    }

    out.flush_page(false, false);
    Ok(())
}
