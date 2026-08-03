use std::collections::HashMap;

use super::bitio::{BitBuffer, BitReader, BitSink};
use super::codebook::CodebookLibrary;
use crate::error::{Error, Result};

/** Maps a codebook, rendered to its canonical spec form, back to its index in the packed library.

Wwise does not store codebooks in the file — it stores ten-bit indices into an external library.
Encoding therefore has to recognise each codebook a Vorbis encoder emitted and recover the index
Wwise would have used. Rendering both sides through the same canonicaliser makes that a lookup:
library entries are expanded from packed form, stream entries are copied from spec form, and both
land as byte strings starting at bit zero.

This only works because the encoder and the library share an ancestry — the library is a dump of
aoTuV's built-in codebooks, so an aoTuV-derived encoder selects from the same set. A codebook that
is not in the library cannot be expressed in this format at all, which is why an unmatched one is
a hard error rather than something to paper over. */
pub(crate) struct CodebookIndex {
    by_canonical: HashMap<Vec<u8>, u32>,
}

impl CodebookIndex {
    pub(crate) fn build(library: &CodebookLibrary) -> Self {
        let mut by_canonical = HashMap::with_capacity(library.count());
        for id in 0..library.count() {
            let mut buffer = BitBuffer::new();
            if library.rebuild(id, &mut buffer).is_ok() {
                by_canonical.entry(buffer.finish()).or_insert(id as u32);
            }
        }
        Self { by_canonical }
    }

    /** Consumes one spec-form codebook from `input` and returns its library index. */
    pub(crate) fn lookup(&self, input: &mut BitReader) -> Result<u32> {
        let mut buffer = BitBuffer::new();
        CodebookLibrary::copy(input, &mut buffer)?;
        self.by_canonical
            .get(&buffer.finish())
            .copied()
            .ok_or(Error::Wem(
                "encoder produced a codebook that is not in the aoTuV library",
            ))
    }
}

/** Splits a standard Ogg stream into its logical packets, in order.

Only used on streams this crate just produced, but written defensively anyway: a malformed page
returns an error rather than indexing past the end. */
pub(crate) fn ogg_packets(stream: &[u8]) -> Result<Vec<Vec<u8>>> {
    let mut packets = Vec::new();
    let mut pending: Vec<u8> = Vec::new();
    let mut at = 0usize;

    while at + 27 <= stream.len() {
        if &stream[at..at + 4] != b"OggS" {
            return Err(Error::Wem("ogg page is missing its capture pattern"));
        }
        let segments = stream[at + 26] as usize;
        let table_at = at + 27;
        let body_at = table_at + segments;
        if body_at > stream.len() {
            return Err(Error::Wem("ogg lacing table runs past end of stream"));
        }

        let mut body = body_at;
        for i in 0..segments {
            let lacing = stream[table_at + i] as usize;
            if body + lacing > stream.len() {
                return Err(Error::Wem("ogg segment runs past end of stream"));
            }
            pending.extend_from_slice(&stream[body..body + lacing]);
            body += lacing;
            /* A lacing value below 255 terminates the packet; 255 means it continues. */
            if lacing < 255 {
                packets.push(std::mem::take(&mut pending));
            }
        }
        at = body;
    }

    Ok(packets)
}

/** The 42-byte Wwise parameter block that lives at `fmt + 0x18` in an extended `fmt ` chunk.

Most of it is derivable from the stream. Three fields are not — they are fixed per channel count
across every shipped file, and are reproduced here as measured. Two more (`seed` and `unknown`)
vary per file with no relationship to anything measurable; no decoder consults them, but a real
reference block can be cloned instead of guessing via [`VorbisTemplate::from_reference`]. */
#[derive(Debug, Clone, Copy)]
pub struct VorbisTemplate {
    channel_config: u32,
    config_a: u32,
    config_b: u32,
    uid: u32,
    seed: u16,
    unknown: u16,
}

impl VorbisTemplate {
    /** The measured template for a channel count. Mono and stereo are the only layouts League
    ships, and the only ones this encoder writes. */
    pub fn for_channels(channels: u16) -> Result<Self> {
        match channels {
            1 => Ok(Self {
                channel_config: 0x0000_4101,
                config_a: 16192,
                config_b: 16616,
                uid: 0xFC77_2C73,
                seed: 1,
                unknown: 209,
            }),
            2 => Ok(Self {
                channel_config: 0x0000_3102,
                config_a: 14316,
                config_b: 14764,
                uid: 0xBB7D_FB8F,
                seed: 1,
                unknown: 209,
            }),
            _ => Err(Error::Unsupported(
                "only mono and stereo Wwise Vorbis can be encoded",
            )),
        }
    }

    /** Clones the parameter block of an existing Wwise Vorbis `.wem`.

    This is the safest way to encode: every field whose meaning is not established is taken from a
    file the engine already accepts, so only the values derived from the new audio differ. */
    pub fn from_reference(reference: &[u8]) -> Result<Self> {
        let wem = super::Wem::new(reference)?;
        if wem.format.codec != super::WemCodec::Vorbis {
            return Err(Error::Wem("reference wem is not Wwise Vorbis"));
        }
        let vorb = wem
            .vorb
            .ok_or(Error::Wem("reference wem has no Vorbis parameter block"))?;
        let data = wem.data;
        let fmt_at = vorb
            .offset
            .checked_sub(0x18)
            .ok_or(Error::Wem("reference wem has an unexpected fmt layout"))?;

        Ok(Self {
            channel_config: super::le_u32(data, fmt_at + 0x14)?,
            config_a: super::le_u32(data, vorb.offset + 0x1C)?,
            config_b: super::le_u32(data, vorb.offset + 0x20)?,
            uid: super::le_u32(data, vorb.offset + 0x24)?,
            seed: super::le_u16(data, vorb.offset + 0x0E)?,
            unknown: super::le_u16(data, vorb.offset + 0x18)?,
        })
    }
}

/** The identification header's fixed fields, needed to size the Wwise header. */
struct Identification {
    blocksize_0_pow: u32,
    blocksize_1_pow: u32,
}

fn read_identification(packet: &[u8]) -> Result<Identification> {
    /* type(1) + "vorbis"(6) + version(4) + channels(1) + rate(4) + three bitrates(12) = 28 */
    let blocksizes = *packet
        .get(28)
        .ok_or(Error::Wem("identification header is truncated"))?;
    Ok(Identification {
        blocksize_0_pow: u32::from(blocksizes & 0x0F),
        blocksize_1_pow: u32::from(blocksizes >> 4),
    })
}

/** What the audio packet framing needs from the setup header. */
struct Modes {
    blockflags: Vec<bool>,
    bits: u32,
}

/** Rewrites a standard Vorbis setup header into Wwise's stripped form.

This is the exact inverse of what decoding does. Wwise narrows fields the spec stores wider — a
residue type is two bits here and sixteen there, a lookup type one bit and four — drops the packet
type and `vorbis` signature, drops the placeholder time-domain-transform block, and replaces every
codebook with a ten-bit index into the external library.

Because it is an inverse, it is also a check: anything the decoder would not accept coming back
out is rejected here rather than written to a file. */
fn strip_setup(spec: &[u8], index: &CodebookIndex, channels: u32) -> Result<(Vec<u8>, Modes)> {
    use super::bitio::ilog;

    let mut input = BitReader::new(spec, 0);
    let mut out = BitBuffer::new();

    if input.read(8)? != 5 {
        return Err(Error::Wem("setup header has the wrong packet type"));
    }
    for _ in 0..6 {
        input.read(8)?;
    }

    let codebook_count_less1 = input.read(8)?;
    out.write(codebook_count_less1, 8);
    for _ in 0..=codebook_count_less1 {
        out.write(index.lookup(&mut input)?, 10);
    }
    let codebook_count = codebook_count_less1 + 1;

    /* Placeholder time-domain transforms: present in the spec, absent in Wwise. */
    if input.read(6)? != 0 || input.read(16)? != 0 {
        return Err(Error::Wem("unexpected time domain transform block"));
    }

    let floor_count_less1 = input.read(6)?;
    let floor_count = floor_count_less1 + 1;
    out.write(floor_count_less1, 6);

    for _ in 0..floor_count {
        if input.read(16)? != 1 {
            return Err(Error::Wem("only floor type 1 can be encoded"));
        }

        let partitions = input.read(5)?;
        out.write(partitions, 5);

        let mut classes = Vec::with_capacity(partitions as usize);
        let mut maximum_class = 0u32;
        for _ in 0..partitions {
            let class = input.read(4)?;
            out.write(class, 4);
            classes.push(class);
            maximum_class = maximum_class.max(class);
        }

        let mut dimensions = Vec::with_capacity(maximum_class as usize + 1);
        for _ in 0..=maximum_class {
            let less1 = input.read(3)?;
            out.write(less1, 3);
            dimensions.push(less1 + 1);

            let subclasses = input.read(2)?;
            out.write(subclasses, 2);
            if subclasses != 0 {
                let masterbook = input.read(8)?;
                out.write(masterbook, 8);
                if masterbook >= codebook_count {
                    return Err(Error::Wem("floor masterbook is out of range"));
                }
            }
            for _ in 0..(1u32 << subclasses) {
                out.write(input.read(8)?, 8);
            }
        }

        out.write(input.read(2)?, 2);
        let rangebits = input.read(4)?;
        out.write(rangebits, 4);

        for &class in &classes {
            let count = dimensions
                .get(class as usize)
                .copied()
                .ok_or(Error::Wem("floor partition class is out of range"))?;
            for _ in 0..count {
                out.write(input.read(rangebits)?, rangebits);
            }
        }
    }

    let residue_count_less1 = input.read(6)?;
    let residue_count = residue_count_less1 + 1;
    out.write(residue_count_less1, 6);

    for _ in 0..residue_count {
        let residue_type = input.read(16)?;
        if residue_type > 2 {
            return Err(Error::Wem("invalid residue type"));
        }
        out.write(residue_type, 2);

        out.write(input.read(24)?, 24);
        out.write(input.read(24)?, 24);
        out.write(input.read(24)?, 24);
        let classifications_less1 = input.read(6)?;
        out.write(classifications_less1, 6);
        let classbook = input.read(8)?;
        out.write(classbook, 8);
        if classbook >= codebook_count {
            return Err(Error::Wem("residue classbook is out of range"));
        }

        let mut cascade = Vec::with_capacity(classifications_less1 as usize + 1);
        for _ in 0..=classifications_less1 {
            let low = input.read(3)?;
            out.write(low, 3);
            let has_high = input.read(1)?;
            out.write(has_high, 1);
            let high = if has_high != 0 {
                let value = input.read(5)?;
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
                    let book = input.read(8)?;
                    out.write(book, 8);
                    if book >= codebook_count {
                        return Err(Error::Wem("residue book is out of range"));
                    }
                }
            }
        }
    }

    let mapping_count_less1 = input.read(6)?;
    let mapping_count = mapping_count_less1 + 1;
    out.write(mapping_count_less1, 6);

    for _ in 0..mapping_count {
        if input.read(16)? != 0 {
            return Err(Error::Wem("only mapping type 0 can be encoded"));
        }

        let has_submaps = input.read(1)?;
        out.write(has_submaps, 1);
        let submaps = if has_submaps != 0 {
            let less1 = input.read(4)?;
            out.write(less1, 4);
            less1 + 1
        } else {
            1
        };

        let square_polar = input.read(1)?;
        out.write(square_polar, 1);
        if square_polar != 0 {
            let coupling_less1 = input.read(8)?;
            out.write(coupling_less1, 8);
            if channels == 0 {
                return Err(Error::Wem("channel coupling on a zero-channel stream"));
            }
            let channel_bits = ilog(channels - 1);
            for _ in 0..=coupling_less1 {
                out.write(input.read(channel_bits)?, channel_bits);
                out.write(input.read(channel_bits)?, channel_bits);
            }
        }

        let reserved = input.read(2)?;
        out.write(reserved, 2);
        if reserved != 0 {
            return Err(Error::Wem("mapping reserved field is non-zero"));
        }

        if submaps > 1 {
            for _ in 0..channels {
                out.write(input.read(4)?, 4);
            }
        }

        for _ in 0..submaps {
            out.write(input.read(8)?, 8);
            let floor = input.read(8)?;
            out.write(floor, 8);
            if floor >= floor_count {
                return Err(Error::Wem("mapping floor index is out of range"));
            }
            let residue = input.read(8)?;
            out.write(residue, 8);
            if residue >= residue_count {
                return Err(Error::Wem("mapping residue index is out of range"));
            }
        }
    }

    let mode_count_less1 = input.read(6)?;
    let mode_count = mode_count_less1 + 1;
    out.write(mode_count_less1, 6);

    let mut blockflags = Vec::with_capacity(mode_count as usize);
    for _ in 0..mode_count {
        let blockflag = input.read(1)?;
        out.write(blockflag, 1);
        blockflags.push(blockflag != 0);

        if input.read(16)? != 0 || input.read(16)? != 0 {
            return Err(Error::Wem("unexpected window or transform type"));
        }

        let mapping = input.read(8)?;
        out.write(mapping, 8);
        if mapping >= mapping_count {
            return Err(Error::Wem("mode mapping index is out of range"));
        }
    }

    Ok((
        out.finish(),
        Modes {
            blockflags,
            bits: ilog(mode_count - 1),
        },
    ))
}

/** Rewrites one standard Vorbis audio packet into Wwise's mod-packet form.

Wwise drops the leading packet-type bit, and for long windows drops the two window flags as well,
recovering them at decode time by looking at the neighbouring packets. Everything after that is
the original packet's bits, shifted down and re-aligned to a byte boundary. */
fn strip_audio_packet(spec: &[u8], modes: &Modes) -> Result<Vec<u8>> {
    let mut input = BitReader::new(spec, 0);

    if input.read(1)? != 0 {
        return Err(Error::Wem("audio packet is not type 0"));
    }
    let mode = input.read(modes.bits)?;
    let long_window = modes
        .blockflags
        .get(mode as usize)
        .copied()
        .ok_or(Error::Wem("audio packet names an unknown mode"))?;

    let total_bits = spec.len() * 8;
    let mut consumed = 1 + modes.bits as usize;

    if long_window {
        input.read(2)?;
        consumed += 2;
    }

    /* A near-silent packet can be shorter than the bits being stripped out of it. Vorbis packets
    are self-terminating, so the shortfall is padded with zeroes the decoder will never read,
    rather than treated as a truncated packet. */
    let wanted = (8 - modes.bits) as usize;
    let available = total_bits.saturating_sub(consumed).min(wanted);
    let remainder = if available > 0 {
        input.read(available as u32)?
    } else {
        0
    };
    consumed += available;

    let mut out = Vec::with_capacity(spec.len());
    out.push((mode | (remainder << modes.bits)) as u8);

    /* Whatever is left is whole bytes; the tail of the final byte is spec padding. */
    for _ in 0..total_bits.saturating_sub(consumed) / 8 {
        out.push(input.read(8)? as u8);
    }

    Ok(out)
}

/// Encodes samples to a standard Ogg Vorbis stream with the bundled aoTuV encoder.
fn encode_reference_stream(audio: &super::PcmAudio, quality: f32) -> Result<Vec<u8>> {
    use std::num::{NonZeroU8, NonZeroU32};

    let rate =
        NonZeroU32::new(audio.sample_rate).ok_or(Error::Wem("cannot encode a zero sample rate"))?;
    let channels = u8::try_from(audio.channels)
        .ok()
        .and_then(NonZeroU8::new)
        .ok_or(Error::Wem("unsupported channel count"))?;

    let mut stream = Vec::new();
    {
        let mut builder = vorbis_rs::VorbisEncoderBuilder::new(rate, channels, &mut stream)
            .map_err(|_| Error::Wem("could not create a Vorbis encoder"))?;
        builder.bitrate_management_strategy(
            vorbis_rs::VorbisBitrateManagementStrategy::QualityVbr {
                target_quality: quality.clamp(-0.2, 1.0),
            },
        );
        let mut encoder = builder
            .build()
            .map_err(|_| Error::Wem("could not configure the Vorbis encoder"))?;

        let planar: Vec<Vec<f32>> = (0..audio.channels as usize)
            .map(|channel| {
                audio
                    .samples
                    .iter()
                    .skip(channel)
                    .step_by(audio.channels as usize)
                    .map(|&s| f32::from(s) / 32768.0)
                    .collect()
            })
            .collect();

        if !planar.is_empty() && !planar[0].is_empty() {
            encoder
                .encode_audio_block(&planar)
                .map_err(|_| Error::Wem("Vorbis encoding failed"))?;
        }
        encoder
            .finish()
            .map_err(|_| Error::Wem("Vorbis encoder could not finish the stream"))?;
    }

    Ok(stream)
}

/** Encodes samples into a Wwise Vorbis `.wem` — the format the game actually ships.

Unlike [`super::encode_pcm`], the result is comparable in size to the audio it replaces, which is
what makes it a real substitute for an external Wwise toolchain rather than a stopgap.

The route is: encode with the bundled aoTuV Vorbis encoder, recognise each codebook it emitted and
replace it with its index in the external library, re-narrow the setup header to Wwise's field
widths, and re-frame every audio packet the way Wwise does. Nothing is transcoded twice — the
compressed audio is the encoder's output, copied.

`quality` follows the Vorbis convention: -0.2 worst, 1.0 best, 0.5 a good default.

Prefer [`encode_vorbis_like`] when replacing an existing sound: it clones the header fields whose
meaning is not established rather than using measured defaults. */
pub fn encode_vorbis(audio: &super::PcmAudio, quality: f32) -> Result<Vec<u8>> {
    let template = VorbisTemplate::for_channels(audio.channels)?;
    encode_with_template(audio, quality, template)
}

/** Encodes samples into a Wwise Vorbis `.wem`, cloning the header template of an existing one.

Use this when replacing a sound in a bank: pass the payload being replaced. Every field this crate
cannot derive is then taken from a file the engine already loads. */
pub fn encode_vorbis_like(
    reference: &[u8],
    audio: &super::PcmAudio,
    quality: f32,
) -> Result<Vec<u8>> {
    let template = VorbisTemplate::from_reference(reference)?;
    encode_with_template(audio, quality, template)
}

fn encode_with_template(
    audio: &super::PcmAudio,
    quality: f32,
    template: VorbisTemplate,
) -> Result<Vec<u8>> {
    if audio.channels == 0 {
        return Err(Error::Wem("cannot encode zero channels"));
    }
    if audio.samples.len() % audio.channels as usize != 0 {
        return Err(Error::Wem("sample count is not a whole number of frames"));
    }

    let stream = encode_reference_stream(audio, quality)?;
    let packets = ogg_packets(&stream)?;
    if packets.len() < 4 {
        return Err(Error::Wem("encoded stream has no audio packets"));
    }

    let identification = read_identification(&packets[0])?;
    let library = CodebookLibrary::packed()?;
    let index = CodebookIndex::build(&library);
    let (setup, modes) = strip_setup(&packets[2], &index, u32::from(audio.channels))?;

    /* Wwise frames each packet with a two-byte length and no granule; the setup packet leads. */
    let mut body = Vec::new();
    body.extend_from_slice(
        &u16::try_from(setup.len())
            .map_err(|_| Error::Wem("setup header is too large"))?
            .to_le_bytes(),
    );
    body.extend_from_slice(&setup);
    let audio_offset = body.len();

    for packet in &packets[3..] {
        if packet.is_empty() {
            continue;
        }
        let framed = strip_audio_packet(packet, &modes)?;
        body.extend_from_slice(
            &u16::try_from(framed.len())
                .map_err(|_| Error::Wem("audio packet is too large"))?
                .to_le_bytes(),
        );
        body.extend_from_slice(&framed);
    }

    let frames = u32::try_from(audio.frames()).map_err(|_| Error::Wem("too many samples"))?;
    let average_bytes = if audio.frames() > 0 {
        (body.len() as u64 * u64::from(audio.sample_rate) / audio.frames() as u64) as u32
    } else {
        0
    };

    let mut fmt = Vec::with_capacity(0x42);
    fmt.extend_from_slice(&0xFFFFu16.to_le_bytes());
    fmt.extend_from_slice(&audio.channels.to_le_bytes());
    fmt.extend_from_slice(&audio.sample_rate.to_le_bytes());
    fmt.extend_from_slice(&average_bytes.to_le_bytes());
    fmt.extend_from_slice(&0u16.to_le_bytes()); // block align is unused for Vorbis
    fmt.extend_from_slice(&0u16.to_le_bytes()); // as are bits per sample
    fmt.extend_from_slice(&0x30u16.to_le_bytes()); // size of everything that follows
    fmt.extend_from_slice(&0u16.to_le_bytes());
    fmt.extend_from_slice(&template.channel_config.to_le_bytes());

    let setup_offset = 0u32;
    let audio_offset = u32::try_from(audio_offset).map_err(|_| Error::Wem("setup is too large"))?;
    let data_size = u32::try_from(body.len()).map_err(|_| Error::Wem("stream is too large"))?;

    fmt.extend_from_slice(&frames.to_le_bytes());
    fmt.extend_from_slice(&(audio_offset - setup_offset).to_le_bytes());
    fmt.extend_from_slice(&(data_size - setup_offset).to_le_bytes());
    fmt.extend_from_slice(&0u16.to_le_bytes());
    fmt.extend_from_slice(&template.seed.to_le_bytes());
    fmt.extend_from_slice(&setup_offset.to_le_bytes());
    fmt.extend_from_slice(&audio_offset.to_le_bytes());
    fmt.extend_from_slice(&template.unknown.to_le_bytes());
    fmt.extend_from_slice(&template.seed.to_le_bytes());
    fmt.extend_from_slice(&template.config_a.to_le_bytes());
    fmt.extend_from_slice(&template.config_b.to_le_bytes());
    fmt.extend_from_slice(&template.uid.to_le_bytes());
    fmt.push(identification.blocksize_0_pow as u8);
    fmt.push(identification.blocksize_1_pow as u8);

    debug_assert_eq!(fmt.len(), 0x42);

    let mut out = Vec::with_capacity(12 + 8 + fmt.len() + 8 + body.len());
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&((4 + 8 + fmt.len() + 8 + body.len()) as u32).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&(fmt.len() as u32).to_le_bytes());
    out.extend_from_slice(&fmt);
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_size.to_le_bytes());
    out.extend_from_slice(&body);

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Encodes a short signal with the bundled aoTuV encoder and returns the Ogg stream.
    pub(super) fn encode_reference(sample_rate: u32, channels: u16, frames: usize) -> Vec<u8> {
        use std::num::{NonZeroU8, NonZeroU32};

        let mut out = Vec::new();
        {
            let mut encoder = vorbis_rs::VorbisEncoderBuilder::new(
                NonZeroU32::new(sample_rate).unwrap(),
                NonZeroU8::new(channels as u8).unwrap(),
                &mut out,
            )
            .unwrap()
            .build()
            .unwrap();

            let block: Vec<Vec<f32>> = (0..channels)
                .map(|c| {
                    (0..frames)
                        .map(|i| {
                            let t = i as f32 / sample_rate as f32;
                            (t * 440.0 * (1.0 + c as f32 * 0.5) * std::f32::consts::TAU).sin() * 0.4
                        })
                        .collect()
                })
                .collect();

            encoder.encode_audio_block(&block).unwrap();
            encoder.finish().unwrap();
        }
        out
    }

    #[test]
    fn ogg_packet_splitting_finds_the_three_headers() {
        let stream = encode_reference(44100, 1, 8192);
        let packets = ogg_packets(&stream).expect("split packets");

        assert!(packets.len() > 3, "expected headers plus audio");
        assert_eq!(
            packets[0][0], 1,
            "first packet is the identification header"
        );
        assert_eq!(packets[1][0], 3, "second packet is the comment header");
        assert_eq!(packets[2][0], 5, "third packet is the setup header");
        for header in &packets[..3] {
            assert_eq!(&header[1..7], b"vorbis");
        }
    }

    /** The gate for Wwise Vorbis encoding: every codebook the encoder emits must exist in the
    packed aoTuV library, because the format can only reference them by index. */
    #[test]
    fn encoder_codebooks_are_all_present_in_the_packed_library() {
        let library = CodebookLibrary::packed().unwrap();
        let index = CodebookIndex::build(&library);
        eprintln!("library: {} codebooks", library.count());

        for (rate, channels) in [(44100u32, 1u16), (44100, 2), (22050, 1), (48000, 2)] {
            let stream = encode_reference(rate, channels, 16384);
            let packets = ogg_packets(&stream).unwrap();
            let setup = &packets[2];

            let mut reader = BitReader::new(setup, 0);
            reader.read(8).unwrap();
            for _ in 0..6 {
                reader.read(8).unwrap();
            }
            let count = reader.read(8).unwrap() + 1;

            let mut matched = 0u32;
            let mut ids = Vec::new();
            for n in 0..count {
                match index.lookup(&mut reader) {
                    Ok(id) => {
                        matched += 1;
                        ids.push(id);
                    }
                    Err(e) => panic!("{rate}Hz/{channels}ch: codebook {n}/{count}: {e}"),
                }
            }

            eprintln!(
                "{rate} Hz {channels}ch: {matched}/{count} codebooks matched, ids {:?}…",
                &ids[..ids.len().min(8)]
            );
            assert_eq!(matched, count);
        }
    }
}
