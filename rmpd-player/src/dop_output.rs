/// DoP-specific audio output using integer samples
/// DoP requires exact bit patterns, so we use I32 format instead of F32
use crate::conversion::SampleBuffer;
use crate::cpal_utils::CpalDeviceConfig;
use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::{Device, SampleFormat, Stream, StreamConfig};
use rmpd_core::error::{Result, RmpdError};
use std::sync::mpsc::{SyncSender, TrySendError, sync_channel};
use std::time::{Duration, Instant};

/// Outcome of a bounded send on the DoP sample channel.
enum SendOutcome {
    Sent,
    TimedOut,
    Disconnected,
}

const DROP_WARN_EVERY: u32 = 8;

fn should_warn_on_drop_streak(consecutive_write_drops: u32) -> bool {
    consecutive_write_drops >= DROP_WARN_EVERY
        && consecutive_write_drops.is_multiple_of(DROP_WARN_EVERY)
}

fn validate_dop_sample_format(sample_format: SampleFormat) -> Result<()> {
    if sample_format == SampleFormat::I32 {
        Ok(())
    } else {
        Err(RmpdError::Player(format!(
            "DoP requires I32 output format, got {sample_format:?}"
        )))
    }
}

/// Send on the bounded channel, waiting up to `timeout` for space via
/// non-blocking `try_send` retries. `std::sync::mpsc` has no stable timed send,
/// and a plain blocking `send` would hang forever if the output callback stalls
/// (device xrun/disconnect) — leaking the exclusive ALSA device.
fn send_bounded(
    sender: &SyncSender<Vec<i32>>,
    mut payload: Vec<i32>,
    timeout: Duration,
) -> SendOutcome {
    let deadline = Instant::now() + timeout;
    loop {
        match sender.try_send(payload) {
            Ok(()) => return SendOutcome::Sent,
            Err(TrySendError::Disconnected(_)) => return SendOutcome::Disconnected,
            Err(TrySendError::Full(returned)) => {
                if Instant::now() >= deadline {
                    return SendOutcome::TimedOut;
                }
                payload = returned;
                std::thread::sleep(Duration::from_millis(2));
            }
        }
    }
}

pub struct DopOutput {
    device: Device,
    stream: Option<Stream>,
    sample_sender: Option<SyncSender<Vec<i32>>>,
    config: StreamConfig,
    is_paused: bool,
    consecutive_write_drops: u32,
    total_write_drops: u64,
}

impl DopOutput {
    pub fn new(sample_rate: u32, channels: u8) -> Result<Self> {
        let device_config = CpalDeviceConfig::new_dop(sample_rate, channels as u16)?;

        tracing::warn!(
            "DoP requires direct hardware access at {} Hz, ensure PipeWire/PulseAudio isn't resampling",
            sample_rate
        );

        Ok(Self {
            device: device_config.device,
            stream: None,
            sample_sender: None,
            config: device_config.config,
            is_paused: false,
            consecutive_write_drops: 0,
            total_write_drops: 0,
        })
    }

    fn reset_drop_streak(&mut self) {
        self.consecutive_write_drops = 0;
    }

    fn note_write_drop(&mut self, reason: &'static str) {
        self.consecutive_write_drops = self.consecutive_write_drops.saturating_add(1);
        self.total_write_drops = self.total_write_drops.saturating_add(1);

        if should_warn_on_drop_streak(self.consecutive_write_drops) {
            tracing::warn!(
                metric = "rmpd.dop_output.write_drops",
                reason,
                consecutive = self.consecutive_write_drops,
                total = self.total_write_drops,
                "DoP write channel is dropping buffers"
            );
        }
    }

    pub fn start(&mut self) -> Result<()> {
        if self.stream.is_some() {
            return Ok(());
        }

        let mut device_config = CpalDeviceConfig {
            device: self.device.clone(),
            config: self.config,
            sample_format: SampleFormat::I32,
        };
        let sample_format = device_config.find_dop_format()?;
        validate_dop_sample_format(sample_format)?;
        tracing::info!("requested sample rate: {:?} Hz", self.config.sample_rate);
        tracing::info!("requested channels: {}", self.config.channels);

        let (tx, rx) = sync_channel::<Vec<i32>>(32);

        let mut buf = SampleBuffer::new(rx);
        let stream = self
            .device
            .build_output_stream(
                self.config,
                move |data: &mut [i32], _: &cpal::OutputCallbackInfo| {
                    for sample in data.iter_mut() {
                        *sample = buf.next_sample();
                    }
                },
                |err| {
                    tracing::error!("DoP output error: {}", err);
                },
                None,
            )
            .map_err(|e| RmpdError::Player(format!("Failed to build DoP stream: {e}")))?;

        stream
            .play()
            .map_err(|e| RmpdError::Player(format!("Failed to start DoP stream: {e}")))?;

        self.stream = Some(stream);
        self.sample_sender = Some(tx.clone());
        self.is_paused = false;

        tracing::info!("DoP output started");

        std::thread::sleep(std::time::Duration::from_millis(50));

        tracing::info!("priming DAC with DoP-marked silence for format detection");

        let sample_rate_hz = self.config.sample_rate as usize;
        let primer_duration_ms = if sample_rate_hz <= 200000 {
            200
        } else if sample_rate_hz <= 400000 {
            100
        } else {
            50
        };
        let primer_frames = (sample_rate_hz * primer_duration_ms) / 1000;
        let mut primer_samples = Vec::with_capacity(primer_frames * self.config.channels as usize);

        for frame in 0..primer_frames {
            let marker = if frame % 2 == 0 { 0x05 } else { 0xFA };
            for _ in 0..self.config.channels {
                let dop_silence = (marker as i32) << 24;
                primer_samples.push(dop_silence);
            }
        }

        let chunk_size = self.config.sample_rate as usize / 50 * self.config.channels as usize;
        for chunk in primer_samples.chunks(chunk_size) {
            // Bounded wait so a stalled callback can't hang priming forever.
            if !matches!(
                send_bounded(&tx, chunk.to_vec(), Duration::from_millis(500)),
                SendOutcome::Sent
            ) {
                tracing::warn!("DoP primer send stalled; continuing");
                break;
            }
        }

        tracing::info!(
            "DoP primer sent ({} frames = {}ms)",
            primer_frames,
            (primer_frames * 1000) / self.config.sample_rate as usize
        );

        Ok(())
    }

    pub fn write(&mut self, samples: &[i32]) -> Result<usize> {
        if self.is_paused {
            return Ok(0);
        }

        let Some(ref sender) = self.sample_sender else {
            return Err(RmpdError::Player("DoP output not started".to_owned()));
        };

        // Bounded send: normally returns quickly (backpressure paces the decoder
        // to playback rate). If the output callback stalls (device xrun or
        // disconnect), don't block forever — drop this buffer so the playback
        // loop stays responsive to stop/seek and the device can be released.
        match send_bounded(sender, samples.to_vec(), Duration::from_millis(500)) {
            SendOutcome::Sent => {
                self.reset_drop_streak();
                Ok(samples.len())
            }
            SendOutcome::TimedOut => {
                self.note_write_drop("timeout");
                Ok(0)
            }
            SendOutcome::Disconnected => {
                self.note_write_drop("disconnected");
                Err(RmpdError::Player("DoP output stream closed".to_owned()))
            }
        }
    }

    pub fn pause(&mut self) -> Result<()> {
        if let Some(ref stream) = self.stream {
            stream
                .pause()
                .map_err(|e| RmpdError::Player(format!("Failed to pause: {e}")))?;
            self.is_paused = true;
        }
        Ok(())
    }

    pub fn resume(&mut self) -> Result<()> {
        if let Some(ref stream) = self.stream {
            stream
                .play()
                .map_err(|e| RmpdError::Player(format!("Failed to resume: {e}")))?;
            self.is_paused = false;
        }
        Ok(())
    }

    pub fn stop(&mut self) -> Result<()> {
        tracing::info!("sending PCM reset sequence to switch DAC back to PCM mode");

        if let Some(ref sender) = self.sample_sender {
            let reset_frames = self.config.sample_rate as usize / 10;
            let mut reset_samples =
                Vec::with_capacity(reset_frames * self.config.channels as usize);

            for _ in 0..(reset_frames * self.config.channels as usize) {
                reset_samples.push(0);
            }

            // Best-effort: never block shutdown if the callback isn't draining.
            match sender.try_send(reset_samples) {
                Ok(()) => tracing::info!("PCM reset sequence sent"),
                Err(TrySendError::Full(_)) => {
                    tracing::warn!("PCM reset sequence not queued: channel full")
                }
                Err(TrySendError::Disconnected(_)) => {
                    tracing::warn!("PCM reset sequence not queued: stream disconnected")
                }
            }

            std::thread::sleep(std::time::Duration::from_millis(150));
        }

        if let Some(stream) = self.stream.take() {
            drop(stream);
            // Let ALSA/USB fully release the exclusive device before any
            // subsequent open, avoiding spurious "busy" on rapid track switches.
            std::thread::sleep(std::time::Duration::from_millis(80));
        }
        self.sample_sender = None;
        self.is_paused = false;
        self.reset_drop_streak();
        Ok(())
    }

    pub fn is_paused(&self) -> bool {
        self.is_paused
    }
}

impl Drop for DopOutput {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drop_warning_threshold_is_every_nth_drop() {
        assert!(!should_warn_on_drop_streak(0));
        assert!(!should_warn_on_drop_streak(1));
        assert!(!should_warn_on_drop_streak(7));
        assert!(should_warn_on_drop_streak(8));
        assert!(!should_warn_on_drop_streak(9));
        assert!(should_warn_on_drop_streak(16));
    }

    #[test]
    fn dop_sample_format_guard_accepts_i32() {
        assert!(validate_dop_sample_format(SampleFormat::I32).is_ok());
    }

    #[test]
    fn dop_sample_format_guard_rejects_non_i32() {
        let err = validate_dop_sample_format(SampleFormat::I24)
            .expect_err("I24 must be rejected for DoP until callback support exists");
        let msg = err.to_string();
        assert!(msg.contains("requires I32 output format"));

        let err = validate_dop_sample_format(SampleFormat::F32)
            .expect_err("F32 must be rejected for bit-exact DoP");
        let msg = err.to_string();
        assert!(msg.contains("requires I32 output format"));
    }
}
