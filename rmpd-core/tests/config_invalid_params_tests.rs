use rmpd_core::config::Config;
use rmpd_core::error::RmpdError;

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn unique_temp_path(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock before unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("rmpd-core-{name}-{}-{nanos}", std::process::id()))
}

fn minimal_config(music_dir: &str) -> String {
    format!(
        "[general]\n\
         music_directory = \"{music_dir}\"\n\
         [network]\n\
         [audio]\n"
    )
}

#[test]
fn config_load_nonexistent_file_returns_read_error() {
    let missing = unique_temp_path("missing-config.toml");
    let err = Config::load_from_path(&missing).expect_err("missing config should fail");

    match err {
        RmpdError::Config(msg) => assert!(msg.contains("Failed to read config"), "{msg}"),
        other => panic!("unexpected error type: {other:?}"),
    }
}

#[test]
fn config_load_malformed_toml_returns_parse_error() {
    let path = unique_temp_path("malformed.toml");
    fs::write(&path, "[general\nmusic_directory = \"/tmp\"\n")
        .expect("write malformed config");

    let err = Config::load_from_path(&path).expect_err("malformed TOML should fail");
    let _ = fs::remove_file(&path);

    match err {
        RmpdError::Config(msg) => assert!(msg.contains("Failed to parse config"), "{msg}"),
        other => panic!("unexpected error type: {other:?}"),
    }
}

#[test]
fn config_load_nonexistent_music_directory_returns_validation_error() {
    let missing_music = unique_temp_path("no-music-dir");
    let cfg_path = unique_temp_path("invalid-music-config.toml");
    fs::write(&cfg_path, minimal_config(missing_music.to_string_lossy().as_ref()))
        .expect("write config");

    let err = Config::load_from_path(&cfg_path).expect_err("nonexistent music dir should fail");
    let _ = fs::remove_file(&cfg_path);

    match err {
        RmpdError::Config(msg) => {
            assert!(msg.contains("Music directory not found"), "{msg}");
            assert!(msg.contains(missing_music.to_string_lossy().as_ref()), "{msg}");
        }
        other => panic!("unexpected error type: {other:?}"),
    }
}