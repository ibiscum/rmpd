use rmpd_core::error::RmpdError;
use rmpd_core::storage::platform::get_default_backend;

use std::path::Path;

#[test]
fn storage_mount_rejects_invalid_uri_format() {
    let backend = get_default_backend();
    let err = backend
        .mount("not-a-uri", Path::new("/tmp/rmpd-core-mount-test"), &[])
        .expect_err("invalid URI should fail");

    match err {
        RmpdError::Storage(msg) => {
            assert!(
                msg.contains("Invalid URI format") || msg.contains("not supported on this platform"),
                "unexpected storage error: {msg}"
            );
        }
        other => panic!("unexpected error type: {other:?}"),
    }
}

#[test]
fn storage_mount_rejects_unsupported_protocol() {
    let backend = get_default_backend();
    let err = backend
        .mount("ftp://example.com/music", Path::new("/tmp/rmpd-core-mount-test"), &[])
        .expect_err("unsupported protocol should fail");

    match err {
        RmpdError::Storage(msg) => {
            assert!(
                msg.contains("Unsupported protocol") || msg.contains("not supported on this platform"),
                "unexpected storage error: {msg}"
            );
        }
        other => panic!("unexpected error type: {other:?}"),
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[test]
fn storage_unmount_rejects_non_utf8_mountpoint() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;
    use std::path::PathBuf;

    let backend = get_default_backend();
    let non_utf8 = PathBuf::from(OsString::from_vec(vec![0x66, 0x80, 0x6f]));

    let err = backend
        .unmount(&non_utf8)
        .expect_err("non-utf8 mountpoint should fail");

    match err {
        RmpdError::Storage(msg) => {
            assert!(msg.contains("Invalid UTF-8 in path"), "unexpected: {msg}");
        }
        other => panic!("unexpected error type: {other:?}"),
    }
}