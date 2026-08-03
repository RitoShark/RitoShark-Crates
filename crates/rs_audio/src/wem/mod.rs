mod bitio;
mod codebook;
mod encode;
mod encode_vorbis;
mod vorbis;

pub use encode::{PcmAudio, encode_pcm, silence};
pub use encode_vorbis::{VorbisTemplate, encode_vorbis, encode_vorbis_like};

use crate::error::{Error, Result};

/** The codec a `.wem` declares in its RIFF `fmt ` chunk. League ships Wwise Vorbis and nothing
else; the other variants exist so an unexpected file reports what it is instead of failing
anonymously. */
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WemCodec {
    /// `0xFFFF` — Wwise Vorbis with the codebooks stripped out.
    Vorbis,
    /// `0xFFFE` — WAVE_FORMAT_EXTENSIBLE, i.e. plain PCM samples.
    Pcm,
    Other(u16),
}

impl WemCodec {
    fn from_id(id: u16) -> Self {
        match id {
            0xFFFF => Self::Vorbis,
            0xFFFE => Self::Pcm,
            other => Self::Other(other),
        }
    }
}

/// The `fmt ` chunk's audio parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WemFormat {
    pub codec: WemCodec,
    pub channels: u16,
    pub sample_rate: u32,
    pub avg_bytes_per_second: u32,
    pub block_align: u16,
    pub bits_per_sample: u16,
}

/// The container a decoded `.wem` was written into.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioFormat {
    Ogg,
    Wav,
}

/// A decoded `.wem` as playable bytes, plus the parameters needed to interpret them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedAudio {
    pub data: Vec<u8>,
    pub format: AudioFormat,
    pub sample_rate: u32,
    pub channels: u16,
}

/** Where the Wwise-specific Vorbis parameters live. Older files carry a dedicated `vorb` chunk;
newer ones fold it into an extended 0x42-byte `fmt ` chunk, which the reference implementation
signals with a size of -1. */
#[derive(Debug, Clone, Copy)]
struct VorbLocation {
    offset: usize,
    /// `None` when the parameters are folded into an extended `fmt ` chunk.
    size: Option<usize>,
}

/** A Wwise `.wem`: a RIFF container whose `data` chunk holds codec-specific payload.

Parsing borrows the input and only locates chunks, so it is cheap and allocation-free. Decoding
is where work happens. A file whose codec we cannot decode still parses, so callers can inspect
and report it rather than being handed an opaque failure. */
#[derive(Debug, Clone)]
pub struct Wem<'a> {
    data: &'a [u8],
    format: WemFormat,
    data_offset: usize,
    data_size: usize,
    vorb: Option<VorbLocation>,
    smpl_offset: Option<usize>,
}

fn le_u16(data: &[u8], at: usize) -> Result<u16> {
    data.get(at..at + 2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .ok_or(Error::Wem("read past end of wem"))
}

fn le_u32(data: &[u8], at: usize) -> Result<u32> {
    data.get(at..at + 4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .ok_or(Error::Wem("read past end of wem"))
}

impl<'a> Wem<'a> {
    /** Locates the RIFF chunks and reads the `fmt ` parameters. Every declared offset and size is
    bounded against the real input length, so a truncated or hostile file is an `Err`. */
    pub fn new(data: &'a [u8]) -> Result<Self> {
        if data.len() < 12 {
            return Err(Error::Wem("too small to be a RIFF container"));
        }

        match &data[..4] {
            b"RIFF" => {}
            b"RIFX" => return Err(Error::Unsupported("big-endian RIFX wem")),
            _ => return Err(Error::InvalidMagic),
        }

        if &data[8..12] != b"WAVE" {
            return Err(Error::Wem("RIFF container is not WAVE"));
        }

        let declared = le_u32(data, 4)? as usize;
        let riff_end = declared.saturating_add(8).min(data.len());

        let mut fmt: Option<(usize, usize)> = None;
        let mut vorb: Option<(usize, usize)> = None;
        let mut audio: Option<(usize, usize)> = None;
        let mut smpl_offset = None;

        let mut at = 12usize;
        while at + 8 <= riff_end {
            let tag = &data[at..at + 4];
            let len = le_u32(data, at + 4)? as usize;
            let body = at + 8;
            let Some(next) = body.checked_add(len) else {
                break;
            };
            if next > riff_end {
                break;
            }

            match tag {
                b"fmt " => fmt = Some((body, len)),
                b"vorb" => vorb = Some((body, len)),
                b"data" => audio = Some((body, len)),
                b"smpl" => smpl_offset = Some(body),
                _ => {}
            }

            at = next;
        }

        let (fmt_offset, fmt_size) = fmt.ok_or(Error::Wem("wem has no fmt chunk"))?;
        let (data_offset, data_size) = audio.ok_or(Error::Wem("wem has no data chunk"))?;

        if fmt_size < 16 {
            return Err(Error::Wem("fmt chunk is too short"));
        }

        let format = WemFormat {
            codec: WemCodec::from_id(le_u16(data, fmt_offset)?),
            channels: le_u16(data, fmt_offset + 2)?,
            sample_rate: le_u32(data, fmt_offset + 4)?,
            avg_bytes_per_second: le_u32(data, fmt_offset + 8)?,
            block_align: le_u16(data, fmt_offset + 12)?,
            bits_per_sample: le_u16(data, fmt_offset + 14)?,
        };

        let vorb = match vorb {
            Some((offset, size)) => Some(VorbLocation {
                offset,
                size: Some(size),
            }),
            None if fmt_size == 0x42 => Some(VorbLocation {
                offset: fmt_offset + 0x18,
                size: None,
            }),
            None if fmt_size == 0x18 || format.codec == WemCodec::Pcm => None,
            None => return Err(Error::Wem("fmt chunk size is neither 0x18 nor 0x42")),
        };

        Ok(Self {
            data,
            format,
            data_offset,
            data_size,
            vorb,
            smpl_offset,
        })
    }

    pub fn format(&self) -> &WemFormat {
        &self.format
    }

    /// The raw codec payload — the body of the RIFF `data` chunk.
    pub fn payload(&self) -> Result<&'a [u8]> {
        self.data
            .get(self.data_offset..self.data_offset + self.data_size)
            .ok_or(Error::Wem("data chunk runs past end of wem"))
    }

    /** Decodes to a playable stream: Wwise Vorbis is remuxed to Ogg Vorbis without re-encoding,
    and PCM is wrapped in a standard WAV header. Both are lossless. */
    pub fn decode(&self) -> Result<DecodedAudio> {
        let data = match self.format.codec {
            WemCodec::Vorbis => {
                return Ok(DecodedAudio {
                    data: self.to_ogg()?,
                    format: AudioFormat::Ogg,
                    sample_rate: self.format.sample_rate,
                    channels: self.format.channels,
                });
            }
            WemCodec::Pcm => self.to_wav()?,
            WemCodec::Other(id) => return Err(Error::UnsupportedCodec(id)),
        };

        Ok(DecodedAudio {
            data,
            format: AudioFormat::Wav,
            sample_rate: self.format.sample_rate,
            channels: self.format.channels,
        })
    }

    /** Rebuilds the Vorbis headers Wwise stripped and repacks the audio packets into Ogg pages.
    The audio itself is never re-encoded, so this is bit-exact, not a transcode. */
    pub fn to_ogg(&self) -> Result<Vec<u8>> {
        if self.format.codec != WemCodec::Vorbis {
            return Err(Error::Wem("wem is not Wwise Vorbis"));
        }
        let vorb = self.vorb.ok_or(Error::Wem(
            "Vorbis wem has neither a vorb chunk nor an extended fmt",
        ))?;
        vorbis::to_ogg(self, vorb)
    }

    /** Decodes all the way to interleaved 16-bit samples.

    This is the form sample-level work needs — trimming, gain, waveform display — and the input
    side of [`encode_pcm`]. Vorbis is decoded through the rebuilt Ogg stream; PCM is reinterpreted
    in place. */
    pub fn to_pcm(&self) -> Result<PcmAudio> {
        match self.format.codec {
            WemCodec::Vorbis => {
                let ogg = self.to_ogg()?;
                let mut reader =
                    lewton::inside_ogg::OggStreamReader::new(std::io::Cursor::new(ogg))
                        .map_err(|_| Error::Wem("rebuilt ogg stream has an unreadable header"))?;

                let sample_rate = reader.ident_hdr.audio_sample_rate;
                let channels = u16::from(reader.ident_hdr.audio_channels);

                let mut samples = Vec::new();
                while let Some(packet) = reader
                    .read_dec_packet_itl()
                    .map_err(|_| Error::Wem("rebuilt ogg stream has an undecodable packet"))?
                {
                    samples.extend_from_slice(&packet);
                }

                Ok(PcmAudio {
                    sample_rate,
                    channels,
                    samples,
                })
            }
            WemCodec::Pcm => {
                if self.format.bits_per_sample != 16 {
                    return Err(Error::Unsupported("PCM wem is not 16-bit"));
                }
                let payload = self.payload()?;
                let samples = payload
                    .chunks_exact(2)
                    .map(|pair| i16::from_le_bytes([pair[0], pair[1]]))
                    .collect();
                Ok(PcmAudio {
                    sample_rate: self.format.sample_rate,
                    channels: self.format.channels,
                    samples,
                })
            }
            WemCodec::Other(id) => Err(Error::UnsupportedCodec(id)),
        }
    }

    /// Wraps PCM sample data in a standard 44-byte WAV header.
    pub fn to_wav(&self) -> Result<Vec<u8>> {
        if self.format.codec != WemCodec::Pcm {
            return Err(Error::Wem("wem is not PCM"));
        }

        let samples = self.payload()?;
        let mut out = Vec::with_capacity(44 + samples.len());

        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&((36 + samples.len()) as u32).to_le_bytes());
        out.extend_from_slice(b"WAVE");

        out.extend_from_slice(b"fmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&self.format.channels.to_le_bytes());
        out.extend_from_slice(&self.format.sample_rate.to_le_bytes());
        out.extend_from_slice(&self.format.avg_bytes_per_second.to_le_bytes());
        out.extend_from_slice(&self.format.block_align.to_le_bytes());
        out.extend_from_slice(&self.format.bits_per_sample.to_le_bytes());

        out.extend_from_slice(b"data");
        out.extend_from_slice(&(samples.len() as u32).to_le_bytes());
        out.extend_from_slice(samples);

        Ok(out)
    }

    fn bytes(&self) -> &'a [u8] {
        self.data
    }
}
