//! WAV files: the header for recordings, reading PCM and float WAV files for
//! playback, and turning microphone audio into 16 kHz (pure, no egui).

/// Sample rate of recordings. Speech recognition tools work at 16 kHz.
pub const MEMO_RATE: u32 = 16_000;
/// Size of the header written before the samples.
pub const HEADER_BYTES: usize = 44;

/// The 44-byte header of a 16-bit PCM WAV file with `data_bytes` of samples.
pub fn header(sample_rate: u32, channels: u16, data_bytes: u32) -> [u8; HEADER_BYTES] {
    let block_align = channels * 2;
    let byte_rate = sample_rate * u32::from(block_align);
    let mut bytes = [0_u8; HEADER_BYTES];
    let mut put = |at: usize, part: &[u8]| bytes[at..at + part.len()].copy_from_slice(part);
    put(0, b"RIFF");
    put(4, &data_bytes.saturating_add(36).to_le_bytes());
    put(8, b"WAVEfmt ");
    put(16, &16_u32.to_le_bytes());
    put(20, &1_u16.to_le_bytes());
    put(22, &channels.to_le_bytes());
    put(24, &sample_rate.to_le_bytes());
    put(28, &byte_rate.to_le_bytes());
    put(32, &block_align.to_le_bytes());
    put(34, &16_u16.to_le_bytes());
    put(36, b"data");
    put(40, &data_bytes.to_le_bytes());
    bytes
}

/// A sample count as a float, for positions and lengths in seconds.
pub fn count(samples: usize) -> f64 {
    #[expect(clippy::cast_precision_loss, reason = "audio lengths are far below 2^52 samples")]
    let value = samples as f64;
    value
}

/// A sample from -1.0 to 1.0 as a 16-bit value.
pub fn to_i16(sample: f32) -> i16 {
    #[expect(clippy::cast_possible_truncation, reason = "the value is clamped to the i16 range first")]
    let value = (sample.clamp(-1.0, 1.0) * f32::from(i16::MAX)).round() as i16;
    value
}

/// Why a WAV file cannot be played.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WavError {
    /// Not a RIFF/WAVE file.
    NotWav,
    /// A WAV encoding rubrique does not read (compressed, or odd sample sizes).
    Unsupported,
}

impl WavError {
    pub fn message(&self) -> &'static str {
        match self {
            WavError::NotWav => "it is not a WAV file",
            WavError::Unsupported => "this WAV encoding is not supported (use 16-bit PCM)",
        }
    }
}

/// How the samples in a WAV file are stored.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Format {
    float: bool,
    channels: u16,
    sample_rate: u32,
    bits: u16,
}

impl Format {
    fn bytes_per_frame(self) -> usize {
        usize::from(self.channels) * usize::from(self.bits / 8)
    }
}

/// Where the samples are: the format and the byte range of the data chunk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Layout {
    format: Format,
    data_start: usize,
    /// Bytes of samples; for a recording that was cut off, all bytes to the end.
    data_bytes: usize,
}

/// Find the format and data chunks. `file_len` is the whole file's size,
/// which may be more than `bytes` when only the start was read.
fn layout(bytes: &[u8], file_len: usize) -> Result<Layout, WavError> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(WavError::NotWav);
    }
    let read_u16 = |at: usize| bytes.get(at..at + 2).map(|b| u16::from_le_bytes([b[0], b[1]]));
    let read_u32 = |at: usize| bytes.get(at..at + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    let mut format = None;
    let mut at = 12;
    while at + 8 <= bytes.len() {
        let id = &bytes[at..at + 4];
        let size = read_u32(at + 4).ok_or(WavError::NotWav)? as usize;
        let body = at + 8;
        if id == b"fmt " {
            let tag = read_u16(body).ok_or(WavError::NotWav)?;
            let channels = read_u16(body + 2).ok_or(WavError::NotWav)?;
            let sample_rate = read_u32(body + 4).ok_or(WavError::NotWav)?;
            let bits = read_u16(body + 14).ok_or(WavError::NotWav)?;
            // WAVE_FORMAT_EXTENSIBLE keeps the real tag in its sub-format.
            let tag = if tag == 0xFFFE { read_u16(body + 24).ok_or(WavError::NotWav)? } else { tag };
            let float = match (tag, bits) {
                (1, 8 | 16 | 24 | 32) => false,
                (3, 32) => true,
                _ => return Err(WavError::Unsupported),
            };
            if channels == 0 || sample_rate == 0 {
                return Err(WavError::Unsupported);
            }
            format = Some(Format { float, channels, sample_rate, bits });
        } else if id == b"data" {
            let format = format.ok_or(WavError::NotWav)?;
            let rest = file_len.saturating_sub(body);
            // 0 or "unknown" sizes come from recordings that were never closed.
            let data_bytes = if size == 0 || size == u32::MAX as usize { rest } else { size.min(rest) };
            return Ok(Layout { format, data_start: body, data_bytes });
        }
        at = body + size + size % 2;
    }
    Err(WavError::NotWav)
}

/// Length in seconds of a WAV file, from its first bytes and its size.
pub fn duration_seconds(start: &[u8], file_len: usize) -> Option<f64> {
    let layout = layout(start, file_len).ok()?;
    let frames = layout.data_bytes / layout.format.bytes_per_frame();
    Some(count(frames) / f64::from(layout.format.sample_rate))
}

/// Decoded audio: one channel (channels are mixed), -1.0 to 1.0.
#[derive(Clone, Debug, PartialEq)]
pub struct Audio {
    pub sample_rate: u32,
    pub samples: Vec<f32>,
}

/// Read a whole WAV file, mixing its channels into one.
///
/// # Errors
///
/// Fails for files that are not WAV, or use an encoding rubrique cannot read.
pub fn decode(bytes: &[u8]) -> Result<Audio, WavError> {
    let Layout { format, data_start, data_bytes } = layout(bytes, bytes.len())?;
    let data = &bytes[data_start..data_start + data_bytes];
    let sample_bytes = usize::from(format.bits / 8);
    let frame = format.bytes_per_frame();
    let sample = |raw: &[u8]| -> f32 {
        match (format.float, format.bits) {
            (true, _) => f32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]),
            (false, 8) => (f32::from(raw[0]) - 128.0) / 128.0,
            (false, 16) => f32::from(i16::from_le_bytes([raw[0], raw[1]])) / 32_768.0,
            (false, 24) => {
                let value = i32::from_le_bytes([0, raw[0], raw[1], raw[2]]) >> 8;
                scaled(value, 8_388_608.0)
            }
            (false, _) => scaled(i32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]), 2_147_483_648.0),
        }
    };
    let samples = data
        .chunks_exact(frame)
        .map(|frame| {
            let sum: f32 = frame.chunks_exact(sample_bytes).map(sample).sum();
            sum / f32::from(format.channels)
        })
        .collect();
    Ok(Audio { sample_rate: format.sample_rate, samples })
}

/// `value / full` as a sample, for 24 and 32-bit integers.
fn scaled(value: i32, full: f64) -> f32 {
    #[expect(clippy::cast_possible_truncation, reason = "a sample from -1.0 to 1.0 fits an f32")]
    let sample = (f64::from(value) / full) as f32;
    sample
}

/// Turns one channel of microphone audio into [`MEMO_RATE`] by averaging
/// the samples that fall into each output sample (plenty for speech). Audio
/// already at or below that rate is kept as it is.
#[derive(Clone, Debug)]
pub struct Downsampler {
    in_rate: u32,
    out_rate: u32,
    sum: f32,
    count: u32,
    /// Grows by `out_rate` per input sample; one output per `in_rate`.
    phase: u32,
}

impl Downsampler {
    pub fn new(in_rate: u32) -> Self {
        Downsampler { in_rate, out_rate: MEMO_RATE.min(in_rate), sum: 0.0, count: 0, phase: 0 }
    }

    /// The rate of the samples [`Downsampler::push`] returns.
    pub fn out_rate(&self) -> u32 {
        self.out_rate
    }

    /// Add one input sample; returns an output sample when one is complete.
    pub fn push(&mut self, sample: f32) -> Option<f32> {
        self.sum += sample;
        self.count += 1;
        self.phase += self.out_rate;
        if self.phase < self.in_rate {
            return None;
        }
        self.phase -= self.in_rate;
        #[expect(clippy::cast_precision_loss, reason = "at most a few hundred samples are averaged")]
        let average = self.sum / self.count as f32;
        self.sum = 0.0;
        self.count = 0;
        Some(average)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pcm16(rate: u32, channels: u16, samples: &[i16]) -> Vec<u8> {
        let data: Vec<u8> = samples.iter().flat_map(|sample| sample.to_le_bytes()).collect();
        let mut bytes = header(rate, channels, u32::try_from(data.len()).unwrap()).to_vec();
        bytes.extend(data);
        bytes
    }

    #[test]
    fn header_and_decode_round_trip() {
        let bytes = pcm16(16_000, 1, &[0, 16_384, -16_384, i16::MAX]);
        assert_eq!(bytes.len(), HEADER_BYTES + 8);
        let audio = decode(&bytes).unwrap();
        assert_eq!(audio.sample_rate, 16_000);
        assert_eq!(audio.samples.len(), 4);
        assert!((audio.samples[1] - 0.5).abs() < 1e-4);
        assert!((audio.samples[2] + 0.5).abs() < 1e-4);
    }

    #[test]
    fn stereo_is_mixed_into_one_channel() {
        let audio = decode(&pcm16(48_000, 2, &[16_384, -16_384, 8_192, 8_192])).unwrap();
        assert_eq!(audio.samples.len(), 2);
        assert!(audio.samples[0].abs() < 1e-4);
        assert!((audio.samples[1] - 0.25).abs() < 1e-4);
    }

    #[test]
    fn duration_from_the_header_and_the_file_size() {
        let bytes = pcm16(16_000, 1, &vec![0; 32_000]);
        assert_eq!(duration_seconds(&bytes[..64], bytes.len()), Some(2.0));
        // A recording that was never closed has a data size of 0.
        let mut open = bytes.clone();
        open[40..44].copy_from_slice(&0_u32.to_le_bytes());
        assert_eq!(duration_seconds(&open[..64], open.len()), Some(2.0));
        assert_eq!(decode(&open).unwrap().samples.len(), 32_000);
    }

    #[test]
    fn other_files_are_refused() {
        assert_eq!(decode(b"ID3 not a wave file"), Err(WavError::NotWav));
        let mut adpcm = pcm16(16_000, 1, &[0, 0]);
        adpcm[20..22].copy_from_slice(&2_u16.to_le_bytes());
        assert_eq!(decode(&adpcm), Err(WavError::Unsupported));
    }

    #[test]
    fn skips_chunks_before_the_data() {
        let mut bytes = pcm16(16_000, 1, &[100, 200]);
        let list = [b"LIST".as_slice(), &3_u32.to_le_bytes(), b"abc\0"].concat();
        bytes.splice(36..36, list);
        assert_eq!(decode(&bytes).unwrap().samples.len(), 2);
    }

    #[test]
    fn float_samples() {
        let mut bytes = header(8_000, 1, 8).to_vec();
        bytes[20..22].copy_from_slice(&3_u16.to_le_bytes());
        bytes[34..36].copy_from_slice(&32_u16.to_le_bytes());
        bytes.extend(0.25_f32.to_le_bytes());
        bytes.extend((-1.0_f32).to_le_bytes());
        assert_eq!(decode(&bytes).unwrap().samples, [0.25, -1.0]);
    }

    #[test]
    fn downsampling_48k_averages_three_samples() {
        let mut down = Downsampler::new(48_000);
        assert_eq!(down.out_rate(), 16_000);
        let out: Vec<f32> = [0.3, 0.3, 0.3, 0.6, 0.0, 0.0].into_iter().filter_map(|s| down.push(s)).collect();
        assert_eq!(out.len(), 2);
        assert!((out[0] - 0.3).abs() < 1e-6 && (out[1] - 0.2).abs() < 1e-6);
    }

    #[test]
    fn downsampling_keeps_the_right_count_for_44k() {
        let mut down = Downsampler::new(44_100);
        let count = (0..44_100).filter_map(|_| down.push(0.1)).count();
        assert_eq!(count, 16_000);
    }

    #[test]
    fn low_rates_are_kept() {
        let mut down = Downsampler::new(8_000);
        assert_eq!(down.out_rate(), 8_000);
        assert_eq!((0..10).filter_map(|_| down.push(0.0)).count(), 10);
    }

    #[test]
    fn samples_are_clamped_to_16_bits() {
        assert_eq!(to_i16(2.0), i16::MAX);
        assert_eq!(to_i16(-2.0), -i16::MAX);
        assert_eq!(to_i16(0.0), 0);
    }
}
