use crate::audio_output::{AudioOutput, PauseState};
use crate::conversion::{self, SampleBuffer};
use crate::cpal_utils::CpalDeviceConfig;
use crate::resampler::StreamResampler;
use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::{Device, SampleFormat, Stream, StreamConfig};
use rmpd_core::config::ResamplerQuality;
use rmpd_core::error::{Result, RmpdError};
use rmpd_core::song::AudioFormat;
use std::sync::mpsc::{SyncSender, sync_channel};

pub struct CpalOutput {
    device: Device,
    stream: Option<Stream>,
    sample_sender: Option<SyncSender<Vec<f32>>>,
    config: StreamConfig,
    pause_state: PauseState,
    resampler: Option<StreamResampler>,
    /// Output buffer time in milliseconds; sizes the sync-channel depth.
    buffer_time_ms: u32,
}

fn validate_started_state(has_stream: bool, has_sender: bool) -> Result<()> {
    match (has_stream, has_sender) {
        (true, true) => Ok(()),
        (false, false) => Err(RmpdError::Player("Output not started".to_owned())),
        _ => Err(RmpdError::Player(
            "Output internal state invalid (partially started)".to_owned(),
        )),
    }
}

fn channel_depth_for(buffer_time_ms: u32, sample_rate: u32, channels: u16) -> usize {
    // Compute channel depth from buffer_time_ms. Each chunk sent over the
    // channel holds ~4096 interleaved samples (engine BUFFER_SIZE).
    const SAMPLES_PER_CHUNK: u64 = 4096;
    if buffer_time_ms == 0 {
        return 32; // safe default if somehow zero
    }
    let samples_needed = (buffer_time_ms as u64 * sample_rate as u64 * channels as u64) / 1000;
    samples_needed.div_ceil(SAMPLES_PER_CHUNK).max(4) as usize
}

impl CpalOutput {
    pub fn new(
        format: AudioFormat,
        quality: ResamplerQuality,
        buffer_time_ms: u32,
    ) -> Result<Self> {
        Self::build(format, quality, buffer_time_ms, format.sample_rate)
    }

    /// Open the cpal stream at `target_device_rate` instead of
    /// `format.sample_rate`. The built-in resampler bridges the gap when they
    /// differ. Used by the DSD-to-PCM path to drive cpal at the device's native
    /// rate (e.g. 48000 Hz) rather than an advertised-but-resampled rate
    /// (e.g. 88200 Hz on a PipeWire 48 kHz graph), which prevents buffer
    /// underruns and keeps DSD ultrasonic shaped noise out of the audible band.
    pub fn with_target_rate(
        format: AudioFormat,
        quality: ResamplerQuality,
        buffer_time_ms: u32,
        target_device_rate: u32,
    ) -> Result<Self> {
        Self::build(format, quality, buffer_time_ms, target_device_rate)
    }

    fn build(
        format: AudioFormat,
        quality: ResamplerQuality,
        buffer_time_ms: u32,
        requested_device_rate: u32,
    ) -> Result<Self> {
        let device_config = CpalDeviceConfig::new(requested_device_rate, format.channels as u16)?;

        // If the device could not take the requested rate, CpalDeviceConfig
        // selected a supported one; resample to bridge the difference so the
        // file plays regardless of hardware constraints.
        let device_rate = device_config.config.sample_rate;
        let resampler = if device_rate != format.sample_rate {
            tracing::info!(
                "output device does not support {} Hz; resampling to {} Hz ({:?})",
                format.sample_rate,
                device_rate,
                quality
            );
            let rs = StreamResampler::new(
                format.sample_rate,
                device_rate,
                format.channels as usize,
                quality,
            );
            if rs.is_none() {
                tracing::error!(
                    "failed to build resampler {} -> {} Hz; audio may play at the wrong speed",
                    format.sample_rate,
                    device_rate
                );
            }
            rs
        } else {
            None
        };

        Ok(Self {
            device: device_config.device,
            stream: None,
            sample_sender: None,
            config: device_config.config,
            pause_state: PauseState::new(),
            resampler,
            buffer_time_ms,
        })
    }

    /// Whether the default output device natively supports `rate`. Lets callers
    /// prefer a bit-exact rate before falling back to resampling.
    pub fn supports_rate(rate: u32) -> bool {
        CpalDeviceConfig::default_device_supports_rate(rate)
    }

    /// The default output device's preferred sample rate (Hz), if known.
    pub fn default_output_rate() -> Option<u32> {
        CpalDeviceConfig::default_output_rate()
    }

    #[cfg(feature = "jack")]
    pub fn new_jack(format: AudioFormat, buffer_time_ms: u32) -> Result<Self> {
        let device_config = CpalDeviceConfig::new_jack(format.sample_rate, format.channels as u16)?;
        Ok(Self {
            device: device_config.device,
            stream: None,
            sample_sender: None,
            config: device_config.config,
            pause_state: PauseState::new(),
            resampler: None,
            buffer_time_ms,
        })
    }

    #[cfg(all(feature = "asio", target_os = "windows"))]
    pub fn new_asio(format: AudioFormat, buffer_time_ms: u32) -> Result<Self> {
        let device_config = CpalDeviceConfig::new_asio(format.sample_rate, format.channels as u16)?;
        Ok(Self {
            device: device_config.device,
            stream: None,
            sample_sender: None,
            config: device_config.config,
            pause_state: PauseState::new(),
            resampler: None,
            buffer_time_ms,
        })
    }

    pub fn start(&mut self) -> Result<()> {
        if self.stream.is_some() || self.sample_sender.is_some() {
            match validate_started_state(self.stream.is_some(), self.sample_sender.is_some()) {
                Ok(()) => return Ok(()),
                Err(e) => {
                    tracing::warn!(
                        "output had inconsistent start state (stream={}, sender={}): {}; rebuilding",
                        self.stream.is_some(),
                        self.sample_sender.is_some(),
                        e
                    );
                    self.stream = None;
                    self.sample_sender = None;
                    self.pause_state.set_paused(false);
                }
            }
        }

        let mut device_config = CpalDeviceConfig {
            device: self.device.clone(),
            config: self.config,
            sample_format: SampleFormat::F32,
        };
        let sample_format = device_config.find_pcm_format()?;

        let channel_depth = channel_depth_for(
            self.buffer_time_ms,
            self.config.sample_rate,
            self.config.channels,
        );
        let (tx, rx) = sync_channel::<Vec<f32>>(channel_depth);

        let stream = match sample_format {
            SampleFormat::F32 => {
                let mut buf = SampleBuffer::new(rx);
                self.device
                    .build_output_stream(
                        self.config,
                        move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                            for sample in data.iter_mut() {
                                *sample = buf.next_sample();
                            }
                        },
                        |err| {
                            tracing::error!("pcm output error: {}", err);
                        },
                        None,
                    )
                    .map_err(|e| RmpdError::Player(format!("Failed to build F32 stream: {e}")))?
            }
            SampleFormat::I16 => {
                let mut buf = SampleBuffer::new(rx);
                self.device
                    .build_output_stream(
                        self.config,
                        move |data: &mut [i16], _: &cpal::OutputCallbackInfo| {
                            for sample in data.iter_mut() {
                                *sample = conversion::f32_to_i16(buf.next_sample());
                            }
                        },
                        |err| {
                            tracing::error!("pcm output error: {}", err);
                        },
                        None,
                    )
                    .map_err(|e| RmpdError::Player(format!("Failed to build I16 stream: {e}")))?
            }
            SampleFormat::I32 => {
                let mut buf = SampleBuffer::new(rx);
                self.device
                    .build_output_stream(
                        self.config,
                        move |data: &mut [i32], _: &cpal::OutputCallbackInfo| {
                            for sample in data.iter_mut() {
                                *sample = conversion::f32_to_i32(buf.next_sample());
                            }
                        },
                        |err| {
                            tracing::error!("pcm output error: {}", err);
                        },
                        None,
                    )
                    .map_err(|e| RmpdError::Player(format!("Failed to build I32 stream: {e}")))?
            }
            _ => {
                return Err(RmpdError::Player(format!(
                    "Unsupported sample format: {sample_format:?}"
                )));
            }
        };

        stream
            .play()
            .map_err(|e| RmpdError::Player(format!("Failed to start stream: {e}")))?;

        self.stream = Some(stream);
        self.sample_sender = Some(tx);
        self.pause_state.set_paused(false);

        tracing::info!(
            "pcm output started: {:?} format, {} Hz, {} channels",
            sample_format,
            self.config.sample_rate,
            self.config.channels
        );

        Ok(())
    }

    pub fn write(&mut self, samples: &[f32]) -> Result<usize> {
        validate_started_state(self.stream.is_some(), self.sample_sender.is_some())?;
        if self.pause_state.is_paused() {
            return Ok(0);
        }

        // Resample to the device rate when required (bridges unsupported rates).
        let out = match self.resampler {
            Some(ref mut rs) => rs.process(samples)?,
            None => samples.to_vec(),
        };
        let n = out.len();

        match self.sample_sender {
            Some(ref sender) => {
                if n > 0 {
                    sender.send(out).map_err(|_| {
                        RmpdError::Player("Failed to send samples to output".to_owned())
                    })?;
                }
                Ok(n)
            }
            None => Err(RmpdError::Player(
                "Output internal state invalid (missing sender)".to_owned(),
            )),
        }
    }

    pub fn pause(&mut self) -> Result<()> {
        validate_started_state(self.stream.is_some(), self.sample_sender.is_some())?;
        if let Some(ref stream) = self.stream {
            stream
                .pause()
                .map_err(|e| RmpdError::Player(format!("Failed to pause: {e}")))?;
            self.pause_state.set_paused(true);
        }
        Ok(())
    }

    pub fn resume(&mut self) -> Result<()> {
        validate_started_state(self.stream.is_some(), self.sample_sender.is_some())?;
        if let Some(ref stream) = self.stream {
            stream
                .play()
                .map_err(|e| RmpdError::Player(format!("Failed to resume: {e}")))?;
            self.pause_state.set_paused(false);
        }
        Ok(())
    }

    pub fn stop(&mut self) -> Result<()> {
        if let Some(ref mut rs) = self.resampler {
            let tail = rs.flush()?;
            if !tail.is_empty()
                && let Some(ref sender) = self.sample_sender
            {
                sender.send(tail).map_err(|_| {
                    RmpdError::Player("Failed to send flushed samples to output".to_owned())
                })?;
            }
        }

        if let Some(stream) = self.stream.take() {
            drop(stream);
        }
        self.sample_sender = None;
        self.pause_state.set_paused(false);
        Ok(())
    }

    pub fn is_paused(&self) -> bool {
        self.pause_state.is_paused()
    }
}

impl Drop for CpalOutput {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

impl AudioOutput for CpalOutput {
    fn start(&mut self) -> rmpd_core::error::Result<()> {
        CpalOutput::start(self)
    }
    fn write(&mut self, samples: &[f32]) -> rmpd_core::error::Result<()> {
        CpalOutput::write(self, samples).map(|_| ())
    }
    fn stop(&mut self) -> rmpd_core::error::Result<()> {
        CpalOutput::stop(self)
    }
    fn pause_state(&self) -> &PauseState {
        &self.pause_state
    }
    fn pause_state_mut(&mut self) -> &mut PauseState {
        &mut self.pause_state
    }
    fn pause(&mut self) -> rmpd_core::error::Result<()> {
        CpalOutput::pause(self)
    }
    fn resume(&mut self) -> rmpd_core::error::Result<()> {
        CpalOutput::resume(self)
    }
    fn is_paused(&self) -> bool {
        CpalOutput::is_paused(self)
    }
}

#[cfg(test)]
mod tests {
    use super::{channel_depth_for, validate_started_state};
    use crate::cpal_utils::set_output_device;
    use crate::output::CpalOutput;
    use rmpd_core::config::ResamplerQuality;
    use rmpd_core::song::AudioFormat;

    fn run_output_lifecycle(format: AudioFormat, buffer_time_ms: u32) -> Result<(), String> {
        let mut out = CpalOutput::new(format, ResamplerQuality::default(), buffer_time_ms)
            .map_err(|e| format!("construct output: {e}"))?;

        out.start().map_err(|e| format!("start: {e}"))?;

        let samples = vec![0.0_f32; 4096];
        let written = out
            .write(&samples)
            .map_err(|e| format!("write while started: {e}"))?;
        if written != samples.len() {
            return Err(format!(
                "unexpected write size: got {written}, expected {}",
                samples.len()
            ));
        }

        // Some backends/devices do not support hardware pause. We still assert
        // core start/write/stop behavior and only validate pause state when it works.
        if out.pause().is_ok() {
            if !out.is_paused() {
                return Err("pause succeeded but output is not marked paused".to_owned());
            }
            out.resume().map_err(|e| format!("resume: {e}"))?;
            if out.is_paused() {
                return Err("output stayed paused after resume".to_owned());
            }
        }

        out.stop().map_err(|e| format!("stop: {e}"))?;

        let stopped_err = out
            .write(&samples)
            .err()
            .ok_or_else(|| "write after stop unexpectedly succeeded".to_owned())?;
        if !stopped_err
            .to_string()
            .to_ascii_lowercase()
            .contains("not started")
        {
            return Err(format!("unexpected post-stop error: {stopped_err}"));
        }

        Ok(())
    }

    #[test]
    fn started_state_validation_is_strict() {
        assert!(validate_started_state(true, true).is_ok());

        let not_started = validate_started_state(false, false)
            .err()
            .expect("false/false should be not started");
        assert!(not_started.to_string().contains("not started"));

        let partial_a = validate_started_state(true, false)
            .err()
            .expect("true/false should be invalid");
        assert!(partial_a.to_string().contains("partially started"));

        let partial_b = validate_started_state(false, true)
            .err()
            .expect("false/true should be invalid");
        assert!(partial_b.to_string().contains("partially started"));
    }

    #[test]
    fn channel_depth_respects_defaults_and_minimums() {
        assert_eq!(channel_depth_for(0, 48_000, 2), 32);
        assert_eq!(channel_depth_for(1, 48_000, 2), 4);
        assert_eq!(channel_depth_for(500, 48_000, 2), 12);
        assert_eq!(channel_depth_for(1000, 44_100, 2), 22);
    }

    #[test]
    fn cpal_e2e_via_pulse_virtual_sink_opt_in() {
        if std::env::var("RMPD_E2E_AUDIO_TEST").ok().as_deref() != Some("1") {
            return;
        }

        set_output_device(Some("pulse".to_owned()));

        let format = AudioFormat {
            sample_rate: 48_000,
            channels: 2,
            bits_per_sample: 16,
        };

        let run = run_output_lifecycle(format, 100);

        set_output_device(None);

        if let Err(msg) = run {
            panic!("{msg}");
        }
    }

    #[test]
    fn cpal_e2e_with_configured_hardware_device_opt_in() {
        if std::env::var("RMPD_HW_AUDIO_TEST").ok().as_deref() != Some("1") {
            return;
        }

        let Some(device) = std::env::var("RMPD_HW_AUDIO_DEVICE")
            .ok()
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
        else {
            eprintln!(
                "Skipping hardware audio test: set RMPD_HW_AUDIO_DEVICE (example: hw:CARD=1,DEV=0)"
            );
            return;
        };

        set_output_device(Some(device.clone()));

        let format = AudioFormat {
            sample_rate: 48_000,
            channels: 2,
            bits_per_sample: 16,
        };

        let run = run_output_lifecycle(format, 100);

        set_output_device(None);

        if let Err(msg) = run {
            panic!("hardware CPAL test failed for '{device}': {msg}");
        }
    }
}
