//! Non-blocking fan-out to N simultaneous audio outputs.
//!
//! `MultiOutput` owns one worker thread per backend.  The PRIMARY output
//! (index 0) drives back-pressure: `write` blocks until it accepts a chunk,
//! preserving the real-time clock.  Every secondary output receives chunks
//! via `try_send`; if its channel is full the chunk is dropped so a stalled
//! secondary can never block the primary.
//!
//! Chunks are shared as `Arc<[f32]>` — a single ref-count bump per secondary,
//! no deep copies.
//!
//! ## Pause/stop responsiveness
//!
//! `Pause`/`Resume`/`Stop` are queued on the SAME per-worker channel as audio
//! chunks, so a control message can land behind up to `depth` already-queued
//! chunks. If workers only acted on the enum message, pausing would have to
//! wait for the worker to actually *play out* that whole backlog first (worst
//! case, `depth` chunks at real-time pace — hundreds of ms). To keep control
//! instantaneous, `active` is a shared atomic checked before writing every
//! dequeued `Samples` chunk: `pause()`/`stop()`/`Drop` clear it immediately,
//! so any already-queued chunk is discarded (not played) the moment the
//! worker next dequeues it, draining the backlog in one recv-loop pass rather
//! than in real time. The `Pause`/`Resume` enum messages still flow through
//! for the hardware-level `AudioOutput::pause`/`resume` call (device state),
//! but the audible effect no longer waits on their queue position.

use crate::audio_output::AudioOutput;
use crate::filter::{AudioFilter, VolumeFilter};
use rmpd_core::error::{Result, RmpdError};
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::mpsc::{SyncSender, sync_channel};
use std::thread::{self, JoinHandle};
use tracing::{debug, warn};

enum OutputMsg {
    Samples(Arc<[f32]>),
    Pause,
    Resume,
    Stop,
}

struct Worker {
    tx: SyncSender<OutputMsg>,
    handle: Option<JoinHandle<()>>,
    primary: bool,
}

pub struct MultiOutput {
    workers: Vec<Worker>,
    /// Shared, checked by every worker before writing a dequeued `Samples`
    /// chunk. `false` while paused or stopping — see module docs.
    active: Arc<AtomicBool>,
    /// Last primary-backend failure, surfaced from worker to caller.
    primary_error: Arc<StdMutex<Option<String>>>,
}

impl MultiOutput {
    /// Spawn one worker thread per output.
    ///
    /// `outputs[0]` is the primary (clock-bearing).  `depth` is the bounded
    /// channel capacity for each worker.  Workers call `start()` on their own
    /// thread; if the primary fails to start, the channel becomes disconnected
    /// and the first `write()` call will return `Err`.  A secondary that fails
    /// to start is logged and dropped.
    pub fn spawn(
        outputs: Vec<Box<dyn AudioOutput>>,
        depth: usize,
        volume: Arc<AtomicU8>,
    ) -> Result<Self> {
        if outputs.is_empty() {
            return Err(RmpdError::Player(
                "multi-output requires at least one backend".to_owned(),
            ));
        }

        let active = Arc::new(AtomicBool::new(true));
        let primary_error = Arc::new(StdMutex::new(None));
        let mut workers = Vec::with_capacity(outputs.len());

        for (idx, mut out) in outputs.into_iter().enumerate() {
            let primary = idx == 0;
            let (tx, rx) = sync_channel::<OutputMsg>(depth);
            let vol_arc = volume.clone();
            let worker_active = active.clone();
            let worker_primary_error = Arc::clone(&primary_error);

            let handle = thread::Builder::new()
                .name(if primary {
                    "rmpd-primary-out".to_owned()
                } else {
                    format!("rmpd-secondary-out-{idx}")
                })
                .spawn(move || {
                    if let Err(e) = out.start() {
                        if primary && let Ok(mut g) = worker_primary_error.lock() {
                            *g = Some(format!("primary output failed to start: {e}"));
                        }
                        warn!(
                            "{} output worker failed to start: {}",
                            if primary { "primary" } else { "secondary" },
                            e
                        );
                        return;
                    }
                    debug!(
                        "{} output worker started",
                        if primary { "primary" } else { "secondary" }
                    );
                    let mut vol = VolumeFilter::new(vol_arc);
                    let mut output_paused = false;
                    loop {
                        let desired_active = worker_active.load(Ordering::Acquire);
                        if !desired_active && !output_paused {
                            let _ = out.pause();
                            output_paused = true;
                        } else if desired_active && output_paused {
                            let _ = out.resume();
                            output_paused = false;
                        }

                        match rx.recv() {
                            Ok(OutputMsg::Samples(arc)) => {
                                if !worker_active.load(Ordering::Acquire) {
                                    // Paused/stopping: discard rather than play out a
                                    // chunk queued before the transition — see module
                                    // docs. Keeps the backlog drain instantaneous
                                    // instead of real-time-paced.
                                    continue;
                                }
                                let mut buf = arc.to_vec();
                                vol.apply(&mut buf);
                                if let Err(e) = out.write(&buf) {
                                    if primary {
                                        if let Ok(mut g) = worker_primary_error.lock() {
                                            *g = Some(format!("primary output write failed: {e}"));
                                        }
                                        warn!("primary output write failed: {}", e);
                                        let _ = out.stop();
                                        break;
                                    }
                                    warn!("secondary output write failed: {}", e);
                                }
                            }
                            Ok(OutputMsg::Pause) => {
                                let _ = out.pause();
                                output_paused = true;
                            }
                            Ok(OutputMsg::Resume) => {
                                let _ = out.resume();
                                output_paused = false;
                            }
                            Ok(OutputMsg::Stop) => {
                                let _ = out.stop();
                                break;
                            }
                            Err(_) => {
                                // Sender side dropped — clean up and exit.
                                let _ = out.stop();
                                break;
                            }
                        }
                    }
                    debug!(
                        "{} output worker stopped",
                        if primary { "primary" } else { "secondary" }
                    );
                })
                .map_err(|e| RmpdError::Player(format!("failed to spawn output thread: {e}")))?;

            workers.push(Worker {
                tx,
                handle: Some(handle),
                primary,
            });
        }

        Ok(MultiOutput {
            workers,
            active,
            primary_error,
        })
    }

    /// Fan one chunk to all outputs.
    ///
    /// Blocks on the primary for back-pressure; uses `try_send` (drop-on-full)
    /// for every secondary.  Returns `Err` only if the primary worker is gone.
    pub fn write(&self, chunk: Arc<[f32]>) -> Result<()> {
        for w in &self.workers {
            if w.primary {
                w.tx.send(OutputMsg::Samples(chunk.clone())).map_err(|_| {
                    let msg = self
                        .primary_error
                        .lock()
                        .ok()
                        .and_then(|g| g.clone())
                        .unwrap_or_else(|| "primary output stopped".to_owned());
                    RmpdError::Player(msg)
                })?;
            } else {
                // Best-effort: silently drop on Full or Disconnected.
                let _ = w.tx.try_send(OutputMsg::Samples(chunk.clone()));
            }
        }
        Ok(())
    }

    /// Pause all outputs (best-effort, non-blocking).
    pub fn pause(&self) {
        self.active.store(false, Ordering::Release);
        for w in &self.workers {
            let _ = w.tx.try_send(OutputMsg::Pause);
        }
    }

    /// Resume all outputs (best-effort, non-blocking).
    pub fn resume(&self) {
        self.active.store(true, Ordering::Release);
        for w in &self.workers {
            let _ = w.tx.try_send(OutputMsg::Resume);
        }
    }

    /// Send `Stop` to all workers and join cleanly.
    ///
    /// If the primary worker has already exited, it is joined.
    /// Secondaries are sent `Stop` on a best-effort basis (the channel may be
    /// full if the secondary is stalled) and their threads are detached.
    /// A still-busy primary is also detached so shutdown cannot hang forever on
    /// a backend write that never returns.
    pub fn stop(mut self) {
        // Stop feeding audio to the device immediately; without this the
        // primary worker would play out its entire backlog (up to `depth`
        // chunks) before it even reaches the Stop message behind them.
        self.active.store(false, Ordering::Release);
        // Send Stop best-effort for every worker. Blocking here can deadlock
        // shutdown if a channel is full while that worker is stalled in write().
        for w in &self.workers {
            let _ = w.tx.try_send(OutputMsg::Stop);
        }
        // Join an already-finished primary; detach busy workers.
        for w in &mut self.workers {
            if w.primary {
                if let Some(h) = w.handle.take() {
                    if h.is_finished() {
                        let _ = h.join();
                    } else {
                        warn!("primary output worker is still busy during stop; detaching");
                    }
                }
            } else {
                w.handle.take(); // detach
            }
        }
    }
}

impl Drop for MultiOutput {
    fn drop(&mut self) {
        // Mirror stop() — handles may be None if stop() was already called.
        self.active.store(false, Ordering::Release);
        for w in &self.workers {
            let _ = w.tx.try_send(OutputMsg::Stop);
        }
        for w in &mut self.workers {
            if w.primary {
                if let Some(h) = w.handle.take() {
                    if h.is_finished() {
                        let _ = h.join();
                    }
                }
            } else {
                w.handle.take();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio_output::PauseState;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    // ── Test outputs ──────────────────────────────────────────────────────────

    struct CountingOutput {
        count: Arc<AtomicUsize>,
        state: PauseState,
    }

    impl AudioOutput for CountingOutput {
        fn start(&mut self) -> rmpd_core::error::Result<()> {
            Ok(())
        }
        fn write(&mut self, _samples: &[f32]) -> rmpd_core::error::Result<()> {
            self.count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
        fn stop(&mut self) -> rmpd_core::error::Result<()> {
            Ok(())
        }
        fn pause_state(&self) -> &PauseState {
            &self.state
        }
        fn pause_state_mut(&mut self) -> &mut PauseState {
            &mut self.state
        }
    }

    /// An output whose `write` blocks for ~1 hour, simulating a stalled sink.
    struct BlockingOutput {
        state: PauseState,
    }

    impl AudioOutput for BlockingOutput {
        fn start(&mut self) -> rmpd_core::error::Result<()> {
            Ok(())
        }
        fn write(&mut self, _samples: &[f32]) -> rmpd_core::error::Result<()> {
            std::thread::sleep(Duration::from_secs(3600));
            Ok(())
        }
        fn stop(&mut self) -> rmpd_core::error::Result<()> {
            Ok(())
        }
        fn pause_state(&self) -> &PauseState {
            &self.state
        }
        fn pause_state_mut(&mut self) -> &mut PauseState {
            &mut self.state
        }
    }

    /// An output whose `write` takes a fixed, non-trivial time — simulates
    /// real-time playback pacing so a test can tell "played out" apart from
    /// "discarded".
    struct SlowOutput {
        count: Arc<AtomicUsize>,
        delay: Duration,
        state: PauseState,
    }

    impl AudioOutput for SlowOutput {
        fn start(&mut self) -> rmpd_core::error::Result<()> {
            Ok(())
        }
        fn write(&mut self, _samples: &[f32]) -> rmpd_core::error::Result<()> {
            std::thread::sleep(self.delay);
            self.count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
        fn stop(&mut self) -> rmpd_core::error::Result<()> {
            Ok(())
        }
        fn pause_state(&self) -> &PauseState {
            &self.state
        }
        fn pause_state_mut(&mut self) -> &mut PauseState {
            &mut self.state
        }
    }

    struct FailingWriteOutput {
        state: PauseState,
    }

    impl AudioOutput for FailingWriteOutput {
        fn start(&mut self) -> rmpd_core::error::Result<()> {
            Ok(())
        }
        fn write(&mut self, _samples: &[f32]) -> rmpd_core::error::Result<()> {
            Err(RmpdError::Player("simulated write failure".to_owned()))
        }
        fn stop(&mut self) -> rmpd_core::error::Result<()> {
            Ok(())
        }
        fn pause_state(&self) -> &PauseState {
            &self.state
        }
        fn pause_state_mut(&mut self) -> &mut PauseState {
            &mut self.state
        }
    }

    // ── Tests ─────────────────────────────────────────────────────────────────

    /// The primary output must receive every chunk even when the secondary is
    /// completely stalled.  `write` must never block on the secondary.
    #[test]
    fn fan_out_does_not_block_on_stalled_secondary() {
        let count = Arc::new(AtomicUsize::new(0));

        let primary = CountingOutput {
            count: Arc::clone(&count),
            state: PauseState::new(),
        };
        let secondary = BlockingOutput {
            state: PauseState::new(),
        };

        // depth=4: secondary's channel fills after 4 chunks; try_send drops the rest.
        let multi = MultiOutput::spawn(
            vec![Box::new(primary), Box::new(secondary)],
            4,
            Arc::new(std::sync::atomic::AtomicU8::new(100)),
        )
        .expect("spawn failed");

        // 100 writes should all succeed and complete quickly regardless of the
        // stalled secondary.
        let chunk: Arc<[f32]> = Arc::from(vec![0.0f32; 64].as_slice());
        for _ in 0..100 {
            assert!(
                multi.write(Arc::clone(&chunk)).is_ok(),
                "write must not fail"
            );
        }

        // Let the primary worker drain its channel before we join it via stop().
        std::thread::sleep(Duration::from_millis(100));

        // stop() joins the primary (not the stalled secondary) so it returns fast.
        multi.stop();

        // Primary processed exactly 100 chunks.
        assert_eq!(
            count.load(Ordering::SeqCst),
            100,
            "primary must have received all 100 chunks"
        );
    }
    /// Pausing must not wait for already-queued chunks to be *played*: it
    /// should discard them so the audible effect is near-instant instead of
    /// paced out over `depth` chunks worth of real time.
    #[test]
    fn pause_discards_queued_backlog_instead_of_playing_it_out() {
        let count = Arc::new(AtomicUsize::new(0));
        let depth = 16;

        let primary = SlowOutput {
            count: Arc::clone(&count),
            delay: Duration::from_millis(50),
            state: PauseState::new(),
        };

        let multi = MultiOutput::spawn(
            vec![Box::new(primary)],
            depth,
            Arc::new(std::sync::atomic::AtomicU8::new(100)),
        )
        .expect("spawn failed");

        // Fill the channel to capacity, then pause immediately. If pause
        // waited for the backlog to be *played*, draining `depth` chunks at
        // 50ms each would take ~800ms.
        let chunk: Arc<[f32]> = Arc::from(vec![0.0f32; 64].as_slice());
        for _ in 0..depth {
            multi
                .write(Arc::clone(&chunk))
                .expect("write must not fail");
        }
        multi.pause();

        // Give the worker enough time to drain the backlog if it's discarding
        // (near-instant) but nowhere near enough to have played it all out
        // for real (~800ms).
        std::thread::sleep(Duration::from_millis(150));

        let played = count.load(Ordering::SeqCst);
        assert!(
            played < depth,
            "pause should have discarded most of the backlog instead of playing it \
             out (played {played} of {depth} queued chunks within 150ms at 50ms/chunk)"
        );

        multi.stop();
    }

    #[test]
    fn spawn_without_outputs_returns_error() {
        let err = MultiOutput::spawn(vec![], 4, Arc::new(AtomicU8::new(100)))
            .err()
            .expect("spawn with no outputs must fail");
        assert!(
            err.to_string().contains("at least one backend"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn primary_write_failure_surfaces_to_caller() {
        let multi = MultiOutput::spawn(
            vec![Box::new(FailingWriteOutput {
                state: PauseState::new(),
            })],
            4,
            Arc::new(AtomicU8::new(100)),
        )
        .expect("spawn failed");

        let chunk: Arc<[f32]> = Arc::from(vec![0.0f32; 64].as_slice());
        let deadline = Instant::now() + Duration::from_millis(500);
        let mut saw_error = None;

        while Instant::now() < deadline {
            match multi.write(Arc::clone(&chunk)) {
                Ok(()) => std::thread::sleep(Duration::from_millis(5)),
                Err(e) => {
                    saw_error = Some(e.to_string());
                    break;
                }
            }
        }

        let err = saw_error.expect("expected write to surface primary failure");
        assert!(
            err.contains("simulated write failure") || err.contains("primary output write failed"),
            "unexpected error: {err}"
        );

        multi.stop();
    }

    #[test]
    fn stop_returns_promptly_with_blocked_primary() {
        let primary = BlockingOutput {
            state: PauseState::new(),
        };

        let multi = MultiOutput::spawn(vec![Box::new(primary)], 1, Arc::new(AtomicU8::new(100)))
            .expect("spawn failed");

        let chunk: Arc<[f32]> = Arc::from(vec![0.0f32; 64].as_slice());
        multi.write(Arc::clone(&chunk)).expect("write must succeed");
        std::thread::sleep(Duration::from_millis(50));

        let start = Instant::now();
        multi.stop();
        assert!(
            start.elapsed() < Duration::from_millis(500),
            "stop should not block on a stalled primary"
        );
    }
}
