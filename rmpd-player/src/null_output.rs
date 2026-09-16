//! Null audio output — discards all samples.
//!
//! Used as the output backend for outputs that are disabled or when no
//! audio device is available.
//!
//! Like MPD's `null_output_plugin`, this paces writes in real time by
//! default (`sync = true`): MPD's plugin drives a `Timer` from the audio
//! format and reports the remaining delay to the player thread, so a null
//! output plays a track over its real duration instead of instantly. Without
//! pacing, a client sees the queue race to the end the moment playback
//! starts. Set `sync = false` on the output to discard samples as fast as
//! the decoder produces them.

use crate::audio_output::{AudioOutput, PauseState};
use rmpd_core::error::{Result, RmpdError};
use rmpd_core::song::AudioFormat;
use std::time::{Duration, Instant};

/// Real-time pacer, mirroring MPD's `Timer` (`src/output/Timer.cxx`): it
/// tracks how many frames have been handed over and sleeps out the
/// difference between that playback position and the wall clock.
struct Pacer {
    frames_per_second: f64,
    channels: usize,
    started: Option<Instant>,
    frames: u64,
    pending_samples: usize,
}

impl Pacer {
    fn new(format: AudioFormat) -> Self {
        Self {
            frames_per_second: f64::from(format.sample_rate.max(1)),
            channels: usize::from(format.channels).max(1),
            started: None,
            frames: 0,
            pending_samples: 0,
        }
    }

    fn add(&mut self, samples: usize) {
        let start = *self.started.get_or_insert_with(Instant::now);
        self.pending_samples += samples;
        let whole_frames = self.pending_samples / self.channels;
        self.pending_samples %= self.channels;
        self.frames += whole_frames as u64;
        let target = Duration::from_secs_f64(self.frames as f64 / self.frames_per_second);
        if let Some(remaining) = target.checked_sub(start.elapsed()) {
            std::thread::sleep(remaining);
        }
    }

    fn reset(&mut self) {
        self.started = None;
        self.frames = 0;
        self.pending_samples = 0;
    }
}

pub struct NullOutput {
    started: bool,
    pause_state: PauseState,
    pacer: Option<Pacer>,
}

impl NullOutput {
    /// Unpaced null output: samples are dropped as fast as they arrive
    /// (MPD's `sync = false`).
    pub fn new() -> Self {
        Self {
            started: false,
            pause_state: PauseState::new(),
            pacer: None,
        }
    }

    /// Null output paced to `format`'s sample rate, matching MPD's default
    /// `sync = true`.
    pub fn synced(format: AudioFormat) -> Self {
        Self {
            started: false,
            pause_state: PauseState::new(),
            pacer: Some(Pacer::new(format)),
        }
    }
}

impl Default for NullOutput {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioOutput for NullOutput {
    fn start(&mut self) -> Result<()> {
        self.started = true;
        self.pause_state.set_paused(false);
        Ok(())
    }

    fn write(&mut self, samples: &[f32]) -> Result<()> {
        if !self.started {
            return Err(RmpdError::Player("Null output not started".to_owned()));
        }
        if self.is_paused() {
            return Ok(());
        }
        if let Some(pacer) = self.pacer.as_mut() {
            pacer.add(samples.len());
        }
        Ok(())
    }

    fn stop(&mut self) -> Result<()> {
        self.started = false;
        if let Some(pacer) = self.pacer.as_mut() {
            pacer.reset();
        }
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

    fn fmt() -> AudioFormat {
        AudioFormat {
            sample_rate: 44_100,
            channels: 2,
            bits_per_sample: 16,
        }
    }

    #[test]
    fn write_before_start_returns_error() {
        let mut out = NullOutput::new();
        let err = out
            .write(&[0.0, 0.1])
            .expect_err("write before start must fail");
        assert!(err.to_string().contains("not started"));
    }

    #[test]
    fn write_after_stop_returns_error() {
        let mut out = NullOutput::new();
        out.start().expect("start should succeed");
        out.stop().expect("stop should succeed");

        let err = out
            .write(&[0.0, 0.1])
            .expect_err("write after stop must fail");
        assert!(err.to_string().contains("not started"));
    }

    #[test]
    fn start_resets_pause_state_to_unpaused() {
        let mut out = NullOutput::new();
        out.start().expect("start should succeed");
        out.pause().expect("pause should succeed");
        assert!(out.is_paused());

        out.start().expect("idempotent start should succeed");
        assert!(!out.is_paused(), "start should reset paused=false");
    }

    #[test]
    fn synced_pacer_tracks_partial_frames_without_drift() {
        let mut pacer = Pacer::new(fmt());
        // Force elapsed > target so add() never sleeps in this unit test.
        pacer.started = Some(Instant::now() - Duration::from_secs(1));

        pacer.add(1);
        assert_eq!(pacer.frames, 0);
        assert_eq!(pacer.pending_samples, 1);

        pacer.add(1);
        assert_eq!(pacer.frames, 1);
        assert_eq!(pacer.pending_samples, 0);

        pacer.add(3);
        assert_eq!(pacer.frames, 2);
        assert_eq!(pacer.pending_samples, 1);
    }
}
