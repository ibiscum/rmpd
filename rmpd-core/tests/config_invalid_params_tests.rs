use rmpd_core::config::{Config, DiscoverOptions};
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
    let err = Config::discover(Some(&missing), DiscoverOptions::default())
        .expect_err("missing config should fail");

    match err {
        RmpdError::Config(msg) => assert!(msg.contains("failed to read config"), "{msg}"),
        other => panic!("unexpected error type: {other:?}"),
    }
}

#[test]
fn config_load_malformed_toml_returns_parse_error() {
    let path = unique_temp_path("malformed.toml");
    fs::write(&path, "[general\nmusic_directory = \"/tmp\"\n").expect("write malformed config");

    let err = Config::discover(Some(&path), DiscoverOptions::default())
        .expect_err("malformed TOML should fail");
    let _ = fs::remove_file(&path);

    match err {
        RmpdError::Config(msg) => assert!(msg.contains("failed to parse config"), "{msg}"),
        other => panic!("unexpected error type: {other:?}"),
    }
}

#[test]
fn config_load_nonexistent_music_directory_emits_warning() {
    let missing_music = unique_temp_path("no-music-dir");
    let cfg_path = unique_temp_path("invalid-music-config.toml");
    fs::write(
        &cfg_path,
        minimal_config(missing_music.to_string_lossy().as_ref()),
    )
    .expect("write config");

    let load = Config::discover(Some(&cfg_path), DiscoverOptions::default())
        .expect("nonexistent music dir should load with warning");
    let _ = fs::remove_file(&cfg_path);

    let expected = missing_music.to_string_lossy().to_string();
    assert!(
        load.diagnostics
            .iter()
            .any(|d| d.message.contains("music directory")
                && d.message.contains("does not exist")
                && d.message.contains(&expected)),
        "missing-directory warning not found: {:?}",
        load.diagnostics
    );
}
