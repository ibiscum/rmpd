//! MPD-faithful audio output registry.
//!
//! Maps an `OutputConfig.output_type` string to a factory function that
//! constructs the appropriate `AudioOutput` backend.

use crate::audio_output::AudioOutput;
use crate::fifo_output::FifoOutput;
use crate::httpd_output::HttpdOutput;
use crate::null_output::NullOutput;
use crate::output::CpalOutput;
use crate::pipe_output::PipeOutput;
use crate::recorder_output::RecorderOutput;
use rmpd_core::config::{OutputConfig, ResamplerQuality};
use rmpd_core::error::{Result, RmpdError};
use rmpd_core::song::AudioFormat;

pub type OutputFactory =
    fn(AudioFormat, ResamplerQuality, &OutputConfig) -> Result<Box<dyn AudioOutput>>;

fn bool_setting_enabled(value: &str) -> bool {
    !matches!(value.to_ascii_lowercase().as_str(), "0" | "no" | "false")
}

fn cpal_buffer_time_ms(cfg: &OutputConfig) -> u32 {
    parse_buffer_time_ms(
        cfg.setting_str("buffer_time_ms")
            .or_else(|| cfg.setting_str("buffer_time"))
            .as_deref(),
    )
}

fn parse_buffer_time_ms(raw: Option<&str>) -> u32 {
    raw.and_then(|s| s.parse::<u32>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(500)
}

fn normalized_output_type(output_type: &str, _output_name: &str) -> String {
    let type_lower = output_type.to_lowercase();
    #[cfg(not(all(feature = "pipewire", target_os = "linux")))]
    {
        if type_lower == "pipewire" {
            tracing::warn!(
                "pipewire feature not built; routing output \"{}\" via the default cpal device",
                _output_name
            );
            return "default".to_owned();
        }
    }
    type_lower
}

// cpal_factory is kept for the OutputPlugins table but is not used by
// create_output (which routes cpal directly to carry buffer_time_ms).
fn cpal_factory(
    format: AudioFormat,
    quality: ResamplerQuality,
    cfg: &OutputConfig,
) -> Result<Box<dyn AudioOutput>> {
    Ok(Box::new(CpalOutput::new(
        format,
        quality,
        cpal_buffer_time_ms(cfg),
    )?))
}

fn null_factory(
    format: AudioFormat,
    _quality: ResamplerQuality,
    cfg: &OutputConfig,
) -> Result<Box<dyn AudioOutput>> {
    // MPD's null plugin: `sync` defaults to true, pacing playback in real
    // time via a Timer.
    let sync = cfg
        .setting_str("sync")
        .map(|v| bool_setting_enabled(&v))
        .unwrap_or(true);
    Ok(Box::new(if sync {
        NullOutput::synced(format)
    } else {
        NullOutput::new()
    }))
}

fn fifo_factory(
    _format: AudioFormat,
    _quality: ResamplerQuality,
    cfg: &OutputConfig,
) -> Result<Box<dyn AudioOutput>> {
    let path = cfg
        .setting_str("path")
        .or_else(|| cfg.setting_str("fifo_path"))
        .ok_or_else(|| {
            RmpdError::Player("fifo output requires a 'path' (or 'fifo_path') setting".into())
        })?;
    Ok(Box::new(FifoOutput::new(path)))
}

fn pipe_factory(
    _format: AudioFormat,
    _quality: ResamplerQuality,
    cfg: &OutputConfig,
) -> Result<Box<dyn AudioOutput>> {
    let command = cfg
        .setting_str("command")
        .ok_or_else(|| RmpdError::Player("pipe output requires a 'command' setting".into()))?;
    Ok(Box::new(PipeOutput::new(command)))
}

fn recorder_factory(
    format: AudioFormat,
    _quality: ResamplerQuality,
    cfg: &OutputConfig,
) -> Result<Box<dyn AudioOutput>> {
    let path = cfg
        .setting_str("path")
        .ok_or_else(|| RmpdError::Player("recorder output requires a 'path' setting".into()))?;
    Ok(Box::new(RecorderOutput::new(path, format)))
}

#[cfg(feature = "jack")]
fn jack_factory(
    format: AudioFormat,
    _quality: ResamplerQuality,
    _cfg: &OutputConfig,
) -> Result<Box<dyn AudioOutput>> {
    Ok(Box::new(CpalOutput::new_jack(format, 500)?))
}

#[cfg(all(feature = "asio", target_os = "windows"))]
fn asio_factory(
    format: AudioFormat,
    _quality: ResamplerQuality,
    _cfg: &OutputConfig,
) -> Result<Box<dyn AudioOutput>> {
    Ok(Box::new(CpalOutput::new_asio(format, 500)?))
}

fn httpd_factory(
    format: AudioFormat,
    _quality: ResamplerQuality,
    cfg: &OutputConfig,
) -> Result<Box<dyn AudioOutput>> {
    Ok(Box::new(HttpdOutput::new(format, cfg)))
}

pub static OUTPUT_PLUGINS: &[(&str, OutputFactory)] = &[
    ("cpal", cpal_factory),
    ("default", cpal_factory),
    ("null", null_factory),
    ("fifo", fifo_factory),
    ("pipe", pipe_factory),
    ("recorder", recorder_factory),
    #[cfg(feature = "jack")]
    ("jack", jack_factory),
    #[cfg(all(feature = "asio", target_os = "windows"))]
    ("asio", asio_factory),
    ("httpd", httpd_factory),
];

pub fn create_output(
    format: AudioFormat,
    quality: ResamplerQuality,
    cfg: &OutputConfig,
    buffer_time_ms: u32,
    dsd_target_rate: Option<u32>,
) -> Result<Box<dyn AudioOutput>> {
    let type_lower = normalized_output_type(&cfg.output_type, &cfg.name);
    // Route cpal-family types directly so buffer_time_ms is forwarded.
    match type_lower.as_str() {
        "cpal" | "default" => {
            let out = match dsd_target_rate {
                Some(rate) => CpalOutput::with_target_rate(format, quality, buffer_time_ms, rate)?,
                None => CpalOutput::new(format, quality, buffer_time_ms)?,
            };
            return Ok(Box::new(out));
        }
        #[cfg(all(feature = "pipewire", target_os = "linux"))]
        "pipewire" => {
            // PipeWire owns the graph rate: we open at the decoded rate and let
            // it follow/resample, so `dsd_target_rate` is intentionally ignored.
            return Ok(Box::new(crate::pipewire_output::PipeWireOutput::new(
                format,
                cfg,
                buffer_time_ms,
            )?));
        }
        #[cfg(feature = "jack")]
        "jack" => return Ok(Box::new(CpalOutput::new_jack(format, buffer_time_ms)?)),
        #[cfg(all(feature = "asio", target_os = "windows"))]
        "asio" => return Ok(Box::new(CpalOutput::new_asio(format, buffer_time_ms)?)),
        _ => {}
    }
    OUTPUT_PLUGINS
        .iter()
        .find(|(name, _)| *name == type_lower)
        .map(|(_, factory)| factory(format, quality, cfg))
        .unwrap_or_else(|| {
            Err(RmpdError::Player(format!(
                "unknown audio output type: {}",
                cfg.output_type
            )))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_format() -> AudioFormat {
        AudioFormat {
            sample_rate: 44_100,
            channels: 2,
            bits_per_sample: 16,
        }
    }

    fn cfg_with_type(output_type: &str) -> OutputConfig {
        let mut cfg = OutputConfig::cpal_default();
        cfg.name = "test-output".to_owned();
        cfg.output_type = output_type.to_owned();
        cfg
    }

    #[test]
    fn sync_setting_false_is_case_insensitive() {
        assert!(!bool_setting_enabled("FALSE"));
        assert!(!bool_setting_enabled("No"));
        assert!(!bool_setting_enabled("0"));
        assert!(bool_setting_enabled("yes"));
        assert!(bool_setting_enabled("true"));
    }

    #[test]
    fn fifo_requires_path_or_fifo_path() {
        let cfg = cfg_with_type("fifo");
        let err = create_output(test_format(), ResamplerQuality::default(), &cfg, 500, None)
            .err()
            .expect("fifo without path must fail");
        assert!(err.to_string().contains("fifo output requires"));
    }

    #[test]
    fn pipe_requires_command() {
        let cfg = cfg_with_type("pipe");
        let err = create_output(test_format(), ResamplerQuality::default(), &cfg, 500, None)
            .err()
            .expect("pipe without command must fail");
        assert!(err.to_string().contains("pipe output requires"));
    }

    #[test]
    fn recorder_requires_path() {
        let cfg = cfg_with_type("recorder");
        let err = create_output(test_format(), ResamplerQuality::default(), &cfg, 500, None)
            .err()
            .expect("recorder without path must fail");
        assert!(err.to_string().contains("recorder output requires"));
    }

    #[test]
    fn unknown_output_type_returns_error() {
        let cfg = cfg_with_type("not-a-real-output");
        let err = create_output(test_format(), ResamplerQuality::default(), &cfg, 500, None)
            .err()
            .expect("unknown type must fail");
        assert!(err.to_string().contains("unknown audio output type"));
    }

    #[cfg(not(all(feature = "pipewire", target_os = "linux")))]
    #[test]
    fn pipewire_type_normalizes_to_default_when_backend_unavailable() {
        let normalized = normalized_output_type("PiPeWiRe", "fallback-test");
        assert_eq!(normalized, "default");
    }

    #[cfg(all(feature = "pipewire", target_os = "linux"))]
    #[test]
    fn pipewire_type_stays_pipewire_when_backend_available() {
        let normalized = normalized_output_type("PiPeWiRe", "native-test");
        assert_eq!(normalized, "pipewire");
    }

    #[test]
    fn cpal_buffer_time_parses_from_cfg_with_fallback() {
        assert_eq!(parse_buffer_time_ms(Some("123")), 123);
        assert_eq!(parse_buffer_time_ms(Some("234")), 234);
        assert_eq!(parse_buffer_time_ms(Some("0")), 500);
        assert_eq!(parse_buffer_time_ms(Some("-1")), 500);
        assert_eq!(parse_buffer_time_ms(Some("bad")), 500);
        assert_eq!(parse_buffer_time_ms(None), 500);
    }
}
