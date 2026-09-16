//! Pipe (external-command) audio output — writes raw s16le PCM to stdin.

use crate::audio_output::{AudioOutput, PauseState};
use crate::conversion;
use rmpd_core::error::{Result, RmpdError};
use std::io::{BufWriter, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use tracing::{info, warn};

const STOP_TIMEOUT: Duration = Duration::from_millis(750);

fn validate_started_state(has_child: bool, has_stdin: bool) -> Result<()> {
    match (has_child, has_stdin) {
        (true, true) => Ok(()),
        (false, false) => Err(RmpdError::Player("Output not started".to_owned())),
        _ => Err(RmpdError::Player(
            "Output internal state invalid (partially started)".to_owned(),
        )),
    }
}

fn wait_with_timeout_or_kill(child: &mut Child, timeout: Duration) -> Result<()> {
    let start = Instant::now();
    loop {
        match child
            .try_wait()
            .map_err(|e| RmpdError::Player(format!("pipe wait error: {e}")))?
        {
            Some(_) => return Ok(()),
            None => {
                if start.elapsed() >= timeout {
                    break;
                }
                thread::sleep(Duration::from_millis(10));
            }
        }
    }

    child
        .kill()
        .map_err(|e| RmpdError::Player(format!("pipe kill error: {e}")))?;
    child
        .wait()
        .map_err(|e| RmpdError::Player(format!("pipe wait after kill error: {e}")))?;
    Ok(())
}

pub struct PipeOutput {
    command: String,
    child: Option<Child>,
    stdin: Option<BufWriter<ChildStdin>>,
    pause_state: PauseState,
    conversion_buf: Vec<u8>,
}

impl PipeOutput {
    pub fn new(command: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            child: None,
            stdin: None,
            pause_state: PauseState::new(),
            conversion_buf: Vec::new(),
        }
    }
}

impl AudioOutput for PipeOutput {
    fn start(&mut self) -> Result<()> {
        if self.child.is_some() || self.stdin.is_some() {
            match validate_started_state(self.child.is_some(), self.stdin.is_some()) {
                Ok(()) => return Ok(()),
                Err(e) => {
                    warn!(
                        "pipe output had inconsistent start state (child={}, stdin={}): {}; rebuilding",
                        self.child.is_some(),
                        self.stdin.is_some(),
                        e
                    );
                    let _ = self.stop();
                }
            }
        }

        let command = self.command.trim();
        if command.is_empty() {
            return Err(RmpdError::Player("empty pipe command".to_owned()));
        }

        let mut parts = command.split_whitespace();
        let program = parts
            .next()
            .ok_or_else(|| RmpdError::Player("empty pipe command".to_owned()))?;

        let mut child = Command::new(program)
            .args(parts)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| RmpdError::Player(format!("cannot spawn '{}': {e}", command)))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| RmpdError::Player("pipe has no stdin".to_owned()))?;
        self.stdin = Some(BufWriter::new(stdin));
        self.child = Some(child);
        self.pause_state.set_paused(false);
        info!("pipe output started: {}", self.command);
        Ok(())
    }

    fn write(&mut self, samples: &[f32]) -> Result<()> {
        validate_started_state(self.child.is_some(), self.stdin.is_some())?;
        if self.is_paused() {
            return Ok(());
        }

        let w = self.stdin.as_mut().ok_or_else(|| {
            RmpdError::Player("Output internal state invalid (missing stdin)".to_owned())
        })?;
        conversion::samples_to_s16le_into(samples, &mut self.conversion_buf);
        w.write_all(&self.conversion_buf)
            .map_err(|e| RmpdError::Player(format!("pipe write error: {e}")))?;
        Ok(())
    }

    fn stop(&mut self) -> Result<()> {
        drop(self.stdin.take());
        if let Some(mut c) = self.child.take() {
            wait_with_timeout_or_kill(&mut c, STOP_TIMEOUT)?;
        }
        self.pause_state.set_paused(false);
        info!("pipe output stopped");
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
    use super::PipeOutput;
    use crate::audio_output::AudioOutput;
    use std::time::{Duration, Instant};

    #[test]
    fn write_before_start_returns_error() {
        let mut out = PipeOutput::new("cat");
        let err = out
            .write(&[0.0, 0.1])
            .err()
            .expect("write before start must fail");
        assert!(err.to_string().to_ascii_lowercase().contains("not started"));
    }

    #[test]
    fn write_after_stop_returns_error_until_restart() {
        let mut out = PipeOutput::new("cat");
        out.start().expect("start should succeed");
        out.stop().expect("stop should succeed");

        let err = out
            .write(&[0.0])
            .err()
            .expect("write after stop must fail");
        assert!(err.to_string().to_ascii_lowercase().contains("not started"));

        out.start().expect("restart should succeed");
        out.write(&[0.2, 0.3])
            .expect("write after restart should succeed");
        out.stop().expect("final stop should succeed");
    }

    #[test]
    fn start_is_idempotent() {
        let mut out = PipeOutput::new("cat");
        out.start().expect("initial start should succeed");
        let pid1 = out
            .child
            .as_ref()
            .expect("child should exist after start")
            .id();

        out.start().expect("second start should be idempotent");
        let pid2 = out
            .child
            .as_ref()
            .expect("child should still exist after second start")
            .id();

        assert_eq!(pid1, pid2, "idempotent start must not respawn child");
        out.stop().expect("stop should succeed");
    }

    #[test]
    fn stop_uses_timeout_then_kill_for_non_reader() {
        let mut out = PipeOutput::new("sleep 5");
        out.start().expect("start should succeed");

        let t0 = Instant::now();
        out.stop().expect("stop should terminate non-reader child");
        let elapsed = t0.elapsed();

        assert!(
            elapsed < Duration::from_secs(3),
            "stop should not block for full sleep duration; elapsed={elapsed:?}"
        );
    }
}
