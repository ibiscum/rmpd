//! Recorder audio output — writes a WAV file.

use crate::audio_output::{AudioOutput, PauseState};
use crate::conversion;
use rmpd_core::error::{Result, RmpdError};
use rmpd_core::song::AudioFormat;
use std::fs::File;
use std::io::{BufWriter, Seek, SeekFrom, Write};
use tracing::info;

pub struct RecorderOutput {
    path: String,
    format: AudioFormat,
    writer: Option<BufWriter<File>>,
    frames_written: u64,
    pause_state: PauseState,
    conversion_buf: Vec<u8>,
}

impl RecorderOutput {
    pub fn new(path: impl Into<String>, format: AudioFormat) -> Self {
        Self {
            path: path.into(),
            format,
            writer: None,
            frames_written: 0,
            pause_state: PauseState::new(),
            conversion_buf: Vec::new(),
        }
    }

    fn write_wav_header(w: &mut BufWriter<File>, sample_rate: u32, channels: u8) -> Result<()> {
        let bps: u16 = 16;
        let byte_rate = sample_rate * channels as u32 * bps as u32 / 8;
        let block_align = channels as u16 * bps / 8;

        let e = |e: std::io::Error| RmpdError::Player(e.to_string());
        w.write_all(b"RIFF").map_err(e)?;
        w.write_all(&0u32.to_le_bytes()).map_err(e)?;
        w.write_all(b"WAVE").map_err(e)?;
        w.write_all(b"fmt ").map_err(e)?;
        w.write_all(&16u32.to_le_bytes()).map_err(e)?;
        w.write_all(&1u16.to_le_bytes()).map_err(e)?;
        w.write_all(&(channels as u16).to_le_bytes()).map_err(e)?;
        w.write_all(&sample_rate.to_le_bytes()).map_err(e)?;
        w.write_all(&byte_rate.to_le_bytes()).map_err(e)?;
        w.write_all(&block_align.to_le_bytes()).map_err(e)?;
        w.write_all(&bps.to_le_bytes()).map_err(e)?;
        w.write_all(b"data").map_err(e)?;
        w.write_all(&0u32.to_le_bytes()).map_err(e)?;
        Ok(())
    }

    /// Patches the RIFF and data chunk sizes in the WAV header once recording stops.
    ///
    /// WAV's classic RIFF format uses 32-bit little-endian size fields, which is a hard
    /// format limit (~4 GiB). Frame/byte counts are accumulated in `u64` to avoid silent
    /// wraparound during long/high-rate recordings, but if the final byte count still
    /// exceeds `u32::MAX` it is clamped (with a warning) rather than wrapped — this keeps
    /// the header internally consistent (if truncated) instead of corrupt. A correct fix
    /// for recordings beyond ~4 GiB of PCM data would require RF64/BWF, out of scope here.
    fn finalize(path: &str, frames: u64, channels: u8) -> Result<()> {
        let data_bytes_u64 = frames * channels as u64 * 2;
        let riff_size_u64 = 36 + data_bytes_u64;
        let data_bytes = if data_bytes_u64 > u32::MAX as u64 {
            tracing::warn!(
                "recorder output: data size {data_bytes_u64} bytes exceeds WAV's 32-bit \
                 limit; clamping header field to u32::MAX (file content is unaffected)"
            );
            u32::MAX
        } else {
            data_bytes_u64 as u32
        };
        let riff_size = riff_size_u64.min(u32::MAX as u64) as u32;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .open(path)
            .map_err(|e| RmpdError::Player(format!("recorder finalize open {}: {e}", path)))?;
        f.seek(SeekFrom::Start(4))
            .and_then(|_| f.write_all(&riff_size.to_le_bytes()))
            .map_err(|e| RmpdError::Player(format!("recorder finalize riff size: {e}")))?;
        f.seek(SeekFrom::Start(40))
            .and_then(|_| f.write_all(&data_bytes.to_le_bytes()))
            .map_err(|e| RmpdError::Player(format!("recorder finalize data size: {e}")))?;
        Ok(())
    }
}

impl AudioOutput for RecorderOutput {
    fn start(&mut self) -> Result<()> {
        if self.writer.is_some() {
            return Ok(());
        }
        let file = File::create(&self.path)
            .map_err(|e| RmpdError::Player(format!("cannot create {}: {e}", self.path)))?;
        let mut w = BufWriter::new(file);
        Self::write_wav_header(&mut w, self.format.sample_rate, self.format.channels)?;
        self.writer = Some(w);
        self.frames_written = 0;
        self.pause_state.set_paused(false);
        info!("recorder output started: {}", self.path);
        Ok(())
    }

    fn write(&mut self, samples: &[f32]) -> Result<()> {
        if self.is_paused() {
            return Ok(());
        }
        if let Some(w) = &mut self.writer {
            conversion::samples_to_s16le_into(samples, &mut self.conversion_buf);
            w.write_all(&self.conversion_buf)
                .map_err(|e| RmpdError::Player(format!("recorder write: {e}")))?;
            self.frames_written += (samples.len() / self.format.channels as usize) as u64;
        }
        Ok(())
    }

    fn stop(&mut self) -> Result<()> {
        if let Some(mut w) = self.writer.take() {
            w.flush()
                .map_err(|e| RmpdError::Player(format!("recorder flush: {e}")))?;
            Self::finalize(&self.path, self.frames_written, self.format.channels)?;
        }
        self.pause_state.set_paused(false);
        info!("recorder output stopped: {}", self.path);
        Ok(())
    }

    fn pause_state(&self) -> &PauseState {
        &self.pause_state
    }
    fn pause_state_mut(&mut self) -> &mut PauseState {
        &mut self.pause_state
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn test_format() -> AudioFormat {
        AudioFormat {
            sample_rate: 44_100,
            channels: 2,
            bits_per_sample: 16,
        }
    }

    #[test]
    fn start_is_idempotent_noop_when_already_started() {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let path = tmp.path().join("rec.wav");
        let path = path.to_string_lossy().to_string();

        let mut out = RecorderOutput::new(path, test_format());
        out.start().expect("initial start should succeed");
        out.write(&[0.0, 0.1, 0.2, 0.3])
            .expect("write should succeed");
        let frames_before = out.frames_written;

        out.start().expect("second start should be idempotent");
        assert_eq!(
            out.frames_written, frames_before,
            "idempotent start must not reset recording state"
        );

        out.stop().expect("stop should succeed");
    }

    #[test]
    fn finalize_missing_file_returns_error() {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let missing = tmp.path().join("missing.wav");
        let missing = missing.to_string_lossy().to_string();

        let err = RecorderOutput::finalize(&missing, 0, 2)
            .err()
            .expect("finalize should fail when file is missing");
        assert!(err.to_string().contains("finalize open"));
    }

    #[test]
    fn stop_propagates_finalize_failure() {
        let tmp = tempfile::tempdir().expect("create temp dir");
        let path = tmp.path().join("rec.wav");
        let path_string = path.to_string_lossy().to_string();

        let mut out = RecorderOutput::new(path_string.clone(), test_format());
        out.start().expect("start should succeed");

        // Remove the file before stop so header patching cannot reopen it.
        fs::remove_file(&path).expect("remove recording file");

        let err = out
            .stop()
            .err()
            .expect("stop should surface finalize failure");
        assert!(err.to_string().contains("finalize open"));
    }
}
