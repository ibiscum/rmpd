//! Trait shared by PCM audio output backends.
//!
//! Note: native DSD/DoP output uses a separate path (`DopOutput`) because it
//! consumes i32 DoP-framed samples rather than f32 PCM.

use rmpd_core::error::Result;

/// Tracks pause state for output backends with simple flag-based pausing.
///
/// Backends that need hardware-level pause (e.g. cpal stream control) should
/// override the trait methods instead of relying on these defaults.
#[derive(Debug, Default)]
pub struct PauseState {
    paused: bool,
}

impl PauseState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }
}

/// An audio output backend.
///
/// All methods are called from a blocking (non-async) thread.
pub trait AudioOutput: Send {
    /// Open the output device / file / pipe and prepare for playback.
    ///
    /// Implementations should be idempotent: calling `start` on an already
    /// started backend should succeed.
    fn start(&mut self) -> Result<()>;

    /// Write interleaved f32 PCM samples (range −1.0 … +1.0).
    ///
    /// Lifecycle contract:
    /// - Before a successful `start`, this should return `Err`.
    /// - While paused, this should be a no-op that returns `Ok(())`.
    /// - After `stop`, this should return `Err` until `start` is called again.
    fn write(&mut self, samples: &[f32]) -> Result<()>;

    /// Stop playback and close the underlying resource.
    ///
    /// Implementations should be idempotent: calling `stop` on a stopped
    /// backend should succeed.
    fn stop(&mut self) -> Result<()>;

    /// Internal accessor for default `pause` / `resume` / `is_paused`
    /// implementations.
    ///
    /// Callers should use `pause()` / `resume()` instead of mutating pause
    /// state directly, otherwise backend-specific side effects may be skipped.
    #[doc(hidden)]
    fn pause_state(&self) -> &PauseState;

    /// Internal mutable accessor for default pause implementation.
    #[doc(hidden)]
    fn pause_state_mut(&mut self) -> &mut PauseState;

    /// Pause this backend's logical output state.
    ///
    /// Backends that use default pause/resume semantics should check
    /// [`AudioOutput::is_paused`] in `write` and perform a no-op while paused.
    /// Backends with hardware-level pause behavior may override these methods.
    fn pause(&mut self) -> Result<()> {
        self.pause_state_mut().set_paused(true);
        Ok(())
    }

    /// Resume after a pause.
    fn resume(&mut self) -> Result<()> {
        self.pause_state_mut().set_paused(false);
        Ok(())
    }

    /// Whether the output is currently paused.
    fn is_paused(&self) -> bool {
        self.pause_state().is_paused()
    }
}

#[cfg(test)]
mod tests {
    use super::{AudioOutput, PauseState};
    use rmpd_core::error::{Result, RmpdError};

    #[derive(Default)]
    struct DummyOutput {
        pause_state: PauseState,
        started: bool,
        written_samples: usize,
    }

    impl AudioOutput for DummyOutput {
        fn start(&mut self) -> Result<()> {
            self.started = true;
            Ok(())
        }

        fn write(&mut self, samples: &[f32]) -> Result<()> {
            if !self.started {
                return Err(RmpdError::Player("Output not started".to_owned()));
            }
            if self.is_paused() {
                return Ok(());
            }
            self.written_samples += samples.len();
            Ok(())
        }

        fn stop(&mut self) -> Result<()> {
            self.started = false;
            Ok(())
        }

        fn pause_state(&self) -> &PauseState {
            &self.pause_state
        }

        fn pause_state_mut(&mut self) -> &mut PauseState {
            &mut self.pause_state
        }
    }

    #[test]
    fn pause_state_defaults_to_unpaused() {
        let state = PauseState::new();
        assert!(!state.is_paused());
    }

    #[test]
    fn default_pause_and_resume_toggle_state() {
        let mut out = DummyOutput::default();
        assert!(!out.is_paused());

        out.pause().expect("pause should succeed");
        assert!(out.is_paused());

        out.resume().expect("resume should succeed");
        assert!(!out.is_paused());
    }

    #[test]
    fn write_before_start_returns_error() {
        let mut out = DummyOutput::default();
        let err = out
            .write(&[0.0, 0.1])
            .expect_err("write before start must fail");
        assert!(err.to_string().contains("Output not started"));
    }

    #[test]
    fn paused_write_is_noop_and_resume_writes_again() {
        let mut out = DummyOutput::default();
        out.start().expect("start should succeed");

        out.write(&[0.0, 0.1, 0.2])
            .expect("initial write should succeed");
        assert_eq!(out.written_samples, 3);

        out.pause().expect("pause should succeed");
        out.write(&[0.3, 0.4])
            .expect("paused write should be a no-op");
        assert_eq!(out.written_samples, 3);

        out.resume().expect("resume should succeed");
        out.write(&[0.5])
            .expect("write after resume should succeed");
        assert_eq!(out.written_samples, 4);
    }

    #[test]
    fn write_after_stop_returns_error_until_restart() {
        let mut out = DummyOutput::default();
        out.start().expect("start should succeed");
        out.stop().expect("stop should succeed");

        let err = out
            .write(&[0.1])
            .expect_err("write after stop must fail until restart");
        assert!(err.to_string().contains("Output not started"));

        out.start().expect("restart should succeed");
        out.write(&[0.2])
            .expect("write after restart should succeed");
        assert_eq!(out.written_samples, 1);
    }
}
