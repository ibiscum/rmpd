//! Trait shared by all audio output backends.

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
    fn start(&mut self) -> Result<()>;

    /// Write interleaved f32 PCM samples (range −1.0 … +1.0).
    fn write(&mut self, samples: &[f32]) -> Result<()>;

    /// Stop playback and close the underlying resource.
    fn stop(&mut self) -> Result<()>;

    /// Access the embedded [`PauseState`].  Required for default
    /// `pause` / `resume` / `is_paused` implementations.
    fn pause_state(&self) -> &PauseState;

    /// Mutable access to the embedded [`PauseState`].
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
    use rmpd_core::error::Result;

    #[derive(Default)]
    struct DummyOutput {
        pause_state: PauseState,
    }

    impl AudioOutput for DummyOutput {
        fn start(&mut self) -> Result<()> {
            Ok(())
        }

        fn write(&mut self, _samples: &[f32]) -> Result<()> {
            Ok(())
        }

        fn stop(&mut self) -> Result<()> {
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
}
