use camino::Utf8PathBuf;
use rmpd_core::error::RmpdError;
use rmpd_core::test_utils::make_test_song;
use rmpd_library::cue::parse_cue;
use rmpd_library::database::Database;
use rmpd_library::metadata::MetadataExtractor;

fn open_temp_db() -> (tempfile::TempDir, Database) {
    let temp_dir = tempfile::TempDir::new().expect("create temp dir");
    let db_path = temp_dir.path().join("invalid-params.db");
    let db = Database::open(db_path.to_str().expect("utf8 db path")).expect("open db");
    (temp_dir, db)
}

#[test]
fn metadata_extract_missing_file_returns_library_error() {
    let path = Utf8PathBuf::from("/definitely/missing/rmpd-library-nope.flac");
    let err = MetadataExtractor::extract_from_file(&path).expect_err("missing file must fail");

    match err {
        RmpdError::Library(msg) => assert!(msg.contains("Failed to read file metadata"), "{msg}"),
        other => panic!("unexpected error type: {other:?}"),
    }
}

#[test]
fn metadata_read_raw_comments_missing_file_returns_library_error() {
    let path = Utf8PathBuf::from("/definitely/missing/rmpd-library-nope.mp3");
    let err = MetadataExtractor::read_raw_comments(&path).expect_err("missing file must fail");

    match err {
        RmpdError::Library(msg) => assert!(msg.contains("Failed to open file"), "{msg}"),
        other => panic!("unexpected error type: {other:?}"),
    }
}

#[test]
fn metadata_supported_file_extension_is_case_insensitive_and_rejects_missing_ext() {
    assert!(MetadataExtractor::is_supported_file(&Utf8PathBuf::from("song.FLAC")));
    assert!(MetadataExtractor::is_supported_file(&Utf8PathBuf::from("song.Mp3")));
    assert!(!MetadataExtractor::is_supported_file(&Utf8PathBuf::from("README")));
    assert!(!MetadataExtractor::is_supported_file(&Utf8PathBuf::from("archive.tar.gz")));
}

#[test]
fn cue_parser_handles_malformed_track_number_and_index_timecode() {
    let cue = r#"
FILE "album.flac" WAVE
  TRACK XX AUDIO
    TITLE "Broken"
    INDEX 01 00:99:99
"#;

    let tracks = parse_cue(cue);
    assert_eq!(tracks.len(), 1);
    assert_eq!(tracks[0].number, 0);
    assert_eq!(tracks[0].start, 0.0);
    assert_eq!(tracks[0].end, None);
    assert_eq!(tracks[0].title.as_deref(), Some("Broken"));
}

#[test]
fn cue_parser_falls_back_to_index00_when_index01_invalid() {
    let cue = r#"
FILE "album.flac" WAVE
  TRACK 01 AUDIO
    INDEX 00 00:00:10
    INDEX 01 bad
"#;

    let tracks = parse_cue(cue);
    assert_eq!(tracks.len(), 1);
    assert!((tracks[0].start - (10.0 / 75.0)).abs() < 1e-6);
}

#[test]
fn database_list_directory_nonexistent_returns_error() {
    let (_tmp, db) = open_temp_db();
    let err = db
        .list_directory("does/not/exist")
        .expect_err("missing directory should fail");

    match err {
        RmpdError::Library(msg) => assert!(msg.contains("No such directory"), "{msg}"),
        other => panic!("unexpected error type: {other:?}"),
    }
}

#[test]
fn database_walk_recursive_nonexistent_returns_error() {
    let (_tmp, db) = open_temp_db();
    let err = db
        .walk_recursive("does/not/exist", &mut |_entry| Ok(()))
        .expect_err("missing directory should fail");

    match err {
        RmpdError::Library(msg) => assert!(msg.contains("No such directory"), "{msg}"),
        other => panic!("unexpected error type: {other:?}"),
    }
}

#[test]
fn database_playlist_invalid_parameters_return_errors() {
    let (_tmp, db) = open_temp_db();

    // Initialize an empty playlist row.
    db.save_playlist("empty", &[]).expect("create playlist");

    let err = db
        .playlist_delete_pos("empty", 0)
        .expect_err("deleting missing position must fail");
    match err {
        RmpdError::Library(msg) => assert!(msg.contains("Position not found"), "{msg}"),
        other => panic!("unexpected error type: {other:?}"),
    }

    let err = db
        .playlist_add("empty", "missing/song.flac")
        .expect_err("adding missing song URI must fail");
    match err {
        RmpdError::Library(msg) => assert!(msg.contains("Song not found"), "{msg}"),
        other => panic!("unexpected error type: {other:?}"),
    }

    // Ensure valid operations still work after invalid calls.
    let song = make_test_song("existing/song.flac", 0);
    db.add_song(&song).expect("add song");
    db.playlist_add("empty", "existing/song.flac")
        .expect("add existing song to playlist");
    let loaded = db.load_playlist("empty").expect("load playlist");
    assert_eq!(loaded.len(), 1);
}