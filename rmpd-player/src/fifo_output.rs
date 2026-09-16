//! Named FIFO (pipe) audio output — writes raw s16le PCM.
//!
//! Primarily used for Snapcast multi-room audio.

use crate::audio_output::{AudioOutput, PauseState};
use crate::conversion;
use rmpd_core::error::{Result, RmpdError};
use std::fs::OpenOptions;
use std::io::{BufWriter, Write};
#[cfg(unix)]
use std::os::unix::fs::FileTypeExt;
use tracing::info;
#[cfg(unix)]
use tracing::warn;

pub struct FifoOutput {
    path: String,
    writer: Option<BufWriter<std::fs::File>>,
    pause_state: PauseState,
    conversion_buf: Vec<u8>,
}

impl FifoOutput {
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            writer: None,
            pause_state: PauseState::new(),
            conversion_buf: Vec::new(),
        }
    }
}

impl AudioOutput for FifoOutput {
    fn start(&mut self) -> Result<()> {
        #[cfg(not(unix))]
        {
            return Err(RmpdError::Player(
                "fifo output is supported only on Unix platforms".to_owned(),
            ));
        }

        #[cfg(unix)]
        {
            let p = std::path::Path::new(&self.path);
            if !p.exists() {
                match std::process::Command::new("mkfifo")
                    .arg(&self.path)
                    .status()
                {
                    Ok(s) if s.success() => info!("created FIFO at {}", self.path),
                    _ => warn!("mkfifo failed for {}, opening anyway", self.path),
                }
            }

            let metadata = p
                .symlink_metadata()
                .map_err(|e| RmpdError::Player(format!("cannot stat FIFO {}: {e}", self.path)))?;
            if !metadata.file_type().is_fifo() {
                return Err(RmpdError::Player(format!(
                    "path is not a FIFO: {}",
                    self.path
                )));
            }

            let file = OpenOptions::new()
                .write(true)
                .open(&self.path)
                .map_err(|e| RmpdError::Player(format!("cannot open FIFO {}: {e}", self.path)))?;
            self.writer = Some(BufWriter::new(file));
            self.pause_state.set_paused(false);
            info!("FIFO output started: {}", self.path);
            Ok(())
        }
    }

    fn write(&mut self, samples: &[f32]) -> Result<()> {
        if self.is_paused() {
            return Ok(());
        }
        let Some(w) = &mut self.writer else {
            return Err(RmpdError::Player("FIFO output not started".to_owned()));
        };

        conversion::samples_to_s16le_into(samples, &mut self.conversion_buf);
        w.write_all(&self.conversion_buf)
            .map_err(|e| RmpdError::Player(format!("FIFO write error: {e}")))?;
        // Flushing each chunk keeps latency low for FIFO consumers.
        w.flush()
            .map_err(|e| RmpdError::Player(format!("FIFO flush error: {e}")))?;
        Ok(())
    }

    fn stop(&mut self) -> Result<()> {
        if let Some(mut w) = self.writer.take() {
            let _ = w.flush();
        }
        info!("FIFO output stopped");
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
    use super::FifoOutput;
    use crate::audio_output::AudioOutput;

    #[test]
    fn write_before_start_returns_error() {
        let mut out = FifoOutput::new("/tmp/rmpd-test-fifo");
        let err = out
            .write(&[0.0, 0.1])
            .expect_err("write before start must fail");
        assert!(err.to_string().contains("not started"));
    }

    #[test]
    fn write_after_stop_returns_error() {
        let mut out = FifoOutput::new("/tmp/rmpd-test-fifo");
        out.stop().expect("stop should be idempotent");
        let err = out
            .write(&[0.0, 0.1])
            .expect_err("write after stop must fail");
        assert!(err.to_string().contains("not started"));
    }
}
