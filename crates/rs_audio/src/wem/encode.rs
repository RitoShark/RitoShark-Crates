use crate::error::{Error, Result};

/** Interleaved 16-bit samples plus the parameters needed to interpret them. This is the handoff
between the library and whatever produced or wants to manipulate the audio: decoding a `.wem`
yields one of these, and encoding takes one back. */
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PcmAudio {
    pub sample_rate: u32,
    pub channels: u16,
    /// Interleaved by frame: `[l, r, l, r, …]` for stereo.
    pub samples: Vec<i16>,
}

impl PcmAudio {
    pub fn new(sample_rate: u32, channels: u16, samples: Vec<i16>) -> Self {
        Self {
            sample_rate,
            channels,
            samples,
        }
    }

    /// Silence of the given length, matching an existing stream's rate and channel count.
    pub fn silence(sample_rate: u32, channels: u16, frames: usize) -> Self {
        Self {
            sample_rate,
            channels,
            samples: vec![0; frames.saturating_mul(channels.max(1) as usize)],
        }
    }

    pub fn frames(&self) -> usize {
        self.samples.len() / self.channels.max(1) as usize
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }
}

/** The speaker mask Wwise expects for a given channel count. Only mono and stereo occur in
League; anything wider gets a front pair plus whatever the standard order assigns, which is the
same convention `WAVEFORMATEXTENSIBLE` uses. */
fn channel_mask(channels: u16) -> u32 {
    match channels {
        0 => 0,
        1 => 0x4,   // front centre
        2 => 0x3,   // front left + right
        3 => 0x7,   // front left + right + centre
        4 => 0x33,  // front pair + back pair
        6 => 0x3F,  // 5.1
        8 => 0x63F, // 7.1
        n => (1u32 << n.min(18)) - 1,
    }
}

/** Writes a PCM `.wem`: a RIFF container declaring codec `0xFFFE` with a 24-byte extended `fmt `
chunk, then raw little-endian samples.

This is the encoder that removes the need for an external Wwise toolchain. It is deliberately the
simplest thing the sound engine can play — PCM is a core source rather than a codec plugin, so it
needs no codebooks and no bitstream surgery. The cost is size: PCM is several times larger than
the Vorbis the game ships, so this trades disk space for not shipping a converter.

Note that League itself contains no PCM `.wem`, so the layout here follows the documented Wwise
extended-format structure rather than being derived from a shipped example. */
pub fn encode_pcm(audio: &PcmAudio) -> Result<Vec<u8>> {
    if audio.channels == 0 {
        return Err(Error::Wem("cannot encode zero channels"));
    }
    if audio.sample_rate == 0 {
        return Err(Error::Wem("cannot encode a zero sample rate"));
    }
    if audio.samples.len() % audio.channels as usize != 0 {
        return Err(Error::Wem("sample count is not a whole number of frames"));
    }

    const BITS: u16 = 16;
    let block_align = audio.channels * (BITS / 8);
    let avg_bytes_per_second = audio.sample_rate * u32::from(block_align);
    let payload_len = audio.samples.len() * 2;

    let fmt_len: u32 = 0x18;
    let mut out = Vec::with_capacity(12 + 8 + fmt_len as usize + 8 + payload_len);

    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&((4 + 8 + fmt_len as usize + 8 + payload_len) as u32).to_le_bytes());
    out.extend_from_slice(b"WAVE");

    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&fmt_len.to_le_bytes());
    out.extend_from_slice(&0xFFFEu16.to_le_bytes());
    out.extend_from_slice(&audio.channels.to_le_bytes());
    out.extend_from_slice(&audio.sample_rate.to_le_bytes());
    out.extend_from_slice(&avg_bytes_per_second.to_le_bytes());
    out.extend_from_slice(&block_align.to_le_bytes());
    out.extend_from_slice(&BITS.to_le_bytes());
    /* cbSize covers the two extension fields that follow: valid bits, then the speaker mask. */
    out.extend_from_slice(&6u16.to_le_bytes());
    out.extend_from_slice(&BITS.to_le_bytes());
    out.extend_from_slice(&channel_mask(audio.channels).to_le_bytes());

    out.extend_from_slice(b"data");
    out.extend_from_slice(&(payload_len as u32).to_le_bytes());
    for sample in &audio.samples {
        out.extend_from_slice(&sample.to_le_bytes());
    }

    Ok(out)
}

/** A silent PCM `.wem` matching an existing stream's rate and channel count.

Muting a sound by replacing it with silence is the common case, and doing it this way keeps the
bank structurally intact: the entry stays present with the same id, so every event, action and
container that referenced it still resolves. Deleting the entry instead leaves dangling references
in the object hierarchy. */
pub fn silence(sample_rate: u32, channels: u16, frames: usize) -> Result<Vec<u8>> {
    encode_pcm(&PcmAudio::silence(sample_rate, channels, frames))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wem::{Wem, WemCodec};

    #[test]
    fn encoded_pcm_parses_back_with_the_same_parameters() {
        let audio = PcmAudio::new(44100, 2, vec![0, 1, -1, 2, 3, -3]);
        let bytes = encode_pcm(&audio).unwrap();

        let wem = Wem::new(&bytes).expect("our own output must parse");
        assert_eq!(wem.format().codec, WemCodec::Pcm);
        assert_eq!(wem.format().channels, 2);
        assert_eq!(wem.format().sample_rate, 44100);
        assert_eq!(wem.format().bits_per_sample, 16);
        assert_eq!(wem.format().block_align, 4);
        assert_eq!(wem.format().avg_bytes_per_second, 44100 * 4);
    }

    #[test]
    fn pcm_round_trips_sample_for_sample() {
        let audio = PcmAudio::new(22050, 1, (-500i16..500).collect());
        let bytes = encode_pcm(&audio).unwrap();
        let decoded = Wem::new(&bytes).unwrap().to_pcm().unwrap();
        assert_eq!(decoded, audio);
    }

    #[test]
    fn silence_is_silent_and_the_right_length() {
        let bytes = silence(48000, 2, 128).unwrap();
        let decoded = Wem::new(&bytes).unwrap().to_pcm().unwrap();
        assert_eq!(decoded.frames(), 128);
        assert_eq!(decoded.channels, 2);
        assert_eq!(decoded.sample_rate, 48000);
        assert!(decoded.samples.iter().all(|&s| s == 0));
    }

    #[test]
    fn a_zero_length_silence_is_still_a_valid_wem() {
        let bytes = silence(44100, 1, 0).unwrap();
        let wem = Wem::new(&bytes).expect("empty payload must still parse");
        assert_eq!(wem.to_pcm().unwrap().frames(), 0);
    }

    #[test]
    fn degenerate_parameters_are_rejected() {
        assert!(encode_pcm(&PcmAudio::new(44100, 0, vec![])).is_err());
        assert!(encode_pcm(&PcmAudio::new(0, 1, vec![])).is_err());
        assert!(encode_pcm(&PcmAudio::new(44100, 2, vec![1, 2, 3])).is_err());
    }
}
