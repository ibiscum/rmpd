//! Pluggable PCM-to-wire encoders for network audio outputs.
//!
//! Each encoder converts interleaved f32 samples to the byte format expected
//! by a particular streaming protocol.  The trait is object-safe so outputs
//! can choose an encoder at construction time.

use crate::conversion;
use rmpd_core::song::AudioFormat;

/// Encodes interleaved f32 PCM into a wire byte stream for network outputs.
pub trait Encoder: Send {
    /// MIME type for the HTTP `Content-Type` header.
    fn content_type(&self) -> &str;

    /// Stream header bytes sent **once** to each new client on connect.
    /// Returns an empty `Vec` when no framing header is required.
    fn header(&self) -> Vec<u8>;

    /// Encode one chunk of interleaved f32 samples (−1.0 …= 1.0) to wire bytes.
    fn encode(&mut self, samples: &[f32]) -> Vec<u8>;
}

// ──────────────────────────────────────────────────────────────────────────────
// PcmEncoder
// ──────────────────────────────────────────────────────────────────────────────

/// Raw little-endian signed 16-bit PCM with no framing header.
///
/// Content-Type is `application/octet-stream`.  Suitable for Snapcast-style
/// raw-PCM consumers or as a building block for other encoders.
pub struct PcmEncoder;

impl PcmEncoder {
    pub fn new(_format: AudioFormat) -> Self {
        Self
    }
}

impl Encoder for PcmEncoder {
    fn content_type(&self) -> &str {
        "application/octet-stream"
    }

    fn header(&self) -> Vec<u8> {
        Vec::new()
    }

    fn encode(&mut self, samples: &[f32]) -> Vec<u8> {
        conversion::samples_to_s16le(samples)
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// WavEncoder
// ──────────────────────────────────────────────────────────────────────────────

/// Streaming WAV — a canonical 44-byte RIFF/WAVE PCM-16 header followed by
/// s16le sample frames.
///
/// Both `riff_size` and `data_size` are set to `0xFFFF_FFFF` to signal an
/// unknown / streaming length.  Browsers and most media players accept this.
pub struct WavEncoder {
    format: AudioFormat,
}

fn wav_pcm16_timing_fields(format: AudioFormat) -> (u16, u32, u16) {
    let channels = u16::from(format.channels);
    const BYTES_PER_SAMPLE: u16 = 2;
    let block_align = channels.saturating_mul(BYTES_PER_SAMPLE);
    let byte_rate = format.sample_rate.saturating_mul(u32::from(block_align));
    (channels, byte_rate, block_align)
}

impl WavEncoder {
    pub fn new(format: AudioFormat) -> Self {
        Self { format }
    }
}

impl Encoder for WavEncoder {
    fn content_type(&self) -> &str {
        "audio/wav"
    }

    fn header(&self) -> Vec<u8> {
        let sample_rate = self.format.sample_rate;
        let (channels, byte_rate, block_align) = wav_pcm16_timing_fields(self.format);
        const BITS_PER_SAMPLE: u16 = 16;
        // Use 0xFFFF_FFFF for both RIFF and data sizes — standard trick for
        // streaming WAV where the total length is not known up front.
        const STREAMING: u32 = 0xFFFF_FFFF;

        let mut h = Vec::with_capacity(44);

        // RIFF chunk descriptor (12 bytes)
        h.extend_from_slice(b"RIFF");
        h.extend_from_slice(&STREAMING.to_le_bytes()); // riff_size
        h.extend_from_slice(b"WAVE");

        // "fmt " sub-chunk (24 bytes)
        h.extend_from_slice(b"fmt ");
        h.extend_from_slice(&16u32.to_le_bytes()); // sub-chunk size
        h.extend_from_slice(&1u16.to_le_bytes()); // AudioFormat = PCM
        h.extend_from_slice(&channels.to_le_bytes());
        h.extend_from_slice(&sample_rate.to_le_bytes());
        h.extend_from_slice(&byte_rate.to_le_bytes());
        h.extend_from_slice(&block_align.to_le_bytes());
        h.extend_from_slice(&BITS_PER_SAMPLE.to_le_bytes());

        // "data" sub-chunk header (8 bytes)
        h.extend_from_slice(b"data");
        h.extend_from_slice(&STREAMING.to_le_bytes()); // data_size

        // Total: 12 + 24 + 8 = 44 bytes
        h
    }

    fn encode(&mut self, samples: &[f32]) -> Vec<u8> {
        conversion::samples_to_s16le(samples)
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use rmpd_core::song::AudioFormat;

    fn stereo_44100() -> AudioFormat {
        AudioFormat {
            sample_rate: 44100,
            channels: 2,
            bits_per_sample: 16,
        }
    }

    // ── PcmEncoder ──────────────────────────────────────────────────────────

    #[test]
    fn pcm_positive_full_scale_is_i16_max() {
        let mut enc = PcmEncoder::new(stereo_44100());
        let bytes = enc.encode(&[1.0_f32]);
        assert_eq!(bytes.len(), 2, "one sample must produce 2 bytes");
        let v = i16::from_le_bytes([bytes[0], bytes[1]]);
        assert_eq!(v, i16::MAX); // 0x7FFF
    }

    #[test]
    fn pcm_negative_full_scale_is_minus_32767() {
        let mut enc = PcmEncoder::new(stereo_44100());
        let bytes = enc.encode(&[-1.0_f32]);
        assert_eq!(bytes.len(), 2);
        let v = i16::from_le_bytes([bytes[0], bytes[1]]);
        // (-1.0 * 32767.0) as i16 = -32767 (0x8001 in two's-complement)
        assert_eq!(v, -32767_i16);
    }

    #[test]
    fn pcm_output_length_is_samples_times_2() {
        let mut enc = PcmEncoder::new(stereo_44100());
        let samples = [0.0_f32; 128];
        assert_eq!(enc.encode(&samples).len(), 256);
    }

    #[test]
    fn pcm_header_is_empty() {
        let enc = PcmEncoder::new(stereo_44100());
        assert!(enc.header().is_empty());
    }

    // ── WavEncoder ──────────────────────────────────────────────────────────

    #[test]
    fn wav_header_is_44_bytes() {
        let enc = WavEncoder::new(stereo_44100());
        assert_eq!(enc.header().len(), 44);
    }

    #[test]
    fn wav_header_starts_with_riff() {
        let enc = WavEncoder::new(stereo_44100());
        assert_eq!(&enc.header()[0..4], b"RIFF");
    }

    #[test]
    fn wav_header_contains_wave_fmt_data_markers() {
        let enc = WavEncoder::new(stereo_44100());
        let h = enc.header();
        assert_eq!(&h[8..12], b"WAVE", "WAVE marker at offset 8");
        assert_eq!(&h[12..16], b"fmt ", "fmt  marker at offset 12");
        assert_eq!(&h[36..40], b"data", "data marker at offset 36");
    }

    #[test]
    fn wav_header_has_streaming_sizes() {
        let enc = WavEncoder::new(stereo_44100());
        let h = enc.header();
        let riff_size = u32::from_le_bytes(h[4..8].try_into().unwrap());
        let data_size = u32::from_le_bytes(h[40..44].try_into().unwrap());
        assert_eq!(riff_size, 0xFFFF_FFFF);
        assert_eq!(data_size, 0xFFFF_FFFF);
    }

    #[test]
    fn wav_encode_length_is_samples_times_2() {
        let mut enc = WavEncoder::new(stereo_44100());
        let samples = [0.0_f32; 64];
        assert_eq!(enc.encode(&samples).len(), 128);
    }

    #[test]
    fn wav_encodes_positive_full_scale() {
        let mut enc = WavEncoder::new(stereo_44100());
        let bytes = enc.encode(&[1.0_f32]);
        let v = i16::from_le_bytes([bytes[0], bytes[1]]);
        assert_eq!(v, i16::MAX);
    }

    #[test]
    fn encoders_handle_non_finite_samples_consistently() {
        let mut pcm = PcmEncoder::new(stereo_44100());
        let mut wav = WavEncoder::new(stereo_44100());
        let samples = [f32::NAN, f32::INFINITY, f32::NEG_INFINITY];

        let pcm_bytes = pcm.encode(&samples);
        let wav_bytes = wav.encode(&samples);

        assert_eq!(pcm_bytes, wav_bytes);
        assert_eq!(pcm_bytes.len(), 6);

        let s0 = i16::from_le_bytes([pcm_bytes[0], pcm_bytes[1]]);
        let s1 = i16::from_le_bytes([pcm_bytes[2], pcm_bytes[3]]);
        let s2 = i16::from_le_bytes([pcm_bytes[4], pcm_bytes[5]]);

        assert_eq!(s0, 0);
        assert_eq!(s1, i16::MAX);
        assert_eq!(s2, -i16::MAX);
    }

    #[test]
    fn wav_header_fields_match_audio_format() {
        let fmt = AudioFormat {
            sample_rate: 48000,
            channels: 2,
            bits_per_sample: 24,
        };
        let enc = WavEncoder::new(fmt);
        let h = enc.header();

        let audio_format = u16::from_le_bytes(h[20..22].try_into().unwrap());
        let channels = u16::from_le_bytes(h[22..24].try_into().unwrap());
        let sample_rate = u32::from_le_bytes(h[24..28].try_into().unwrap());
        let byte_rate = u32::from_le_bytes(h[28..32].try_into().unwrap());
        let block_align = u16::from_le_bytes(h[32..34].try_into().unwrap());
        let bits_per_sample = u16::from_le_bytes(h[34..36].try_into().unwrap());

        assert_eq!(audio_format, 1);
        assert_eq!(channels, 2);
        assert_eq!(sample_rate, 48000);
        assert_eq!(byte_rate, 48000 * 2 * 2);
        assert_eq!(block_align, 4);
        assert_eq!(bits_per_sample, 16);
    }

    #[test]
    fn wav_header_byte_rate_saturates_on_extreme_format() {
        let fmt = AudioFormat {
            sample_rate: u32::MAX,
            channels: u8::MAX,
            bits_per_sample: 16,
        };
        let (_, byte_rate, block_align) = wav_pcm16_timing_fields(fmt);
        assert_eq!(block_align, u16::from(u8::MAX) * 2);
        assert_eq!(byte_rate, u32::MAX);
    }

    #[test]
    fn pcm_and_wav_payload_are_identical_for_same_samples() {
        let mut pcm = PcmEncoder::new(stereo_44100());
        let mut wav = WavEncoder::new(stereo_44100());
        let samples = [0.0_f32, 0.5, -0.5, 1.0, -1.0, 1.5, -1.5];

        assert_eq!(pcm.encode(&samples), wav.encode(&samples));
    }
}
