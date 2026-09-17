//! `MusicSource` SPI — transport-agnostic music-source trait + error types.
//!
//! Lives in `rmpd-plugin` so it can be a dependency-light contract crate
//! (`rmpd-core` + `async-trait` only). Concrete backends live in `rmpd-source`.

use async_trait::async_trait;
use rmpd_core::song::Song;
use std::fmt;

// ─── Error ───────────────────────────────────────────────────────────────────

/// Transport-agnostic source error.
///
/// `Display` and `Debug` implementations MUST NOT echo credentials or secrets.
/// Messages are structurally scrubbed on construction.
pub enum SourceError {
    /// Network unreachable, DNS failure, TLS error, or connection timeout.
    Unreachable(String),
    /// Server rejected credentials (401 / 403).
    Auth(String),
    /// Unknown id or virtual path (404-equivalent).
    NotFound(String),
    /// Malformed server response or unexpected protocol behaviour.
    Protocol(String),
    /// Missing or invalid configuration (URL, credentials, settings).
    Config(String),
}

impl SourceError {
    fn scrub_message(msg: impl Into<String>) -> String {
        let raw = msg.into();
        // Keep user-facing diagnostics single-line and bounded.
        let mut out = raw
            .replace('\n', " ")
            .replace('\r', " ")
            .trim()
            .to_owned();
        if out.len() > 512 {
            out.truncate(512);
        }
        out
    }

    pub fn unreachable(msg: impl Into<String>) -> Self {
        Self::Unreachable(Self::scrub_message(msg))
    }

    pub fn auth(msg: impl Into<String>) -> Self {
        Self::Auth(Self::scrub_message(msg))
    }

    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::NotFound(Self::scrub_message(msg))
    }

    pub fn protocol(msg: impl Into<String>) -> Self {
        Self::Protocol(Self::scrub_message(msg))
    }

    pub fn config(msg: impl Into<String>) -> Self {
        Self::Config(Self::scrub_message(msg))
    }
}

impl fmt::Debug for SourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (kind, msg) = match self {
            SourceError::Unreachable(msg) => ("Unreachable", msg),
            SourceError::Auth(msg) => ("Auth", msg),
            SourceError::NotFound(msg) => ("NotFound", msg),
            SourceError::Protocol(msg) => ("Protocol", msg),
            SourceError::Config(msg) => ("Config", msg),
        };
        f.debug_struct("SourceError")
            .field("kind", &kind)
            .field("message", msg)
            .finish()
    }
}

impl fmt::Display for SourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Deliberately opaque: the variant tag + a safe summary only.
        // The inner String must already be scrubbed of secrets by the caller.
        match self {
            SourceError::Unreachable(msg) => write!(f, "source unreachable: {msg}"),
            SourceError::Auth(msg) => write!(f, "source auth error: {msg}"),
            SourceError::NotFound(msg) => write!(f, "source not found: {msg}"),
            SourceError::Protocol(msg) => write!(f, "source protocol error: {msg}"),
            SourceError::Config(msg) => write!(f, "source config error: {msg}"),
        }
    }
}

impl std::error::Error for SourceError {}

// ─── Result alias ────────────────────────────────────────────────────────────

pub type SourceResult<T> = Result<T, SourceError>;

// ─── SourceEntry ─────────────────────────────────────────────────────────────

/// One child in a virtual browse listing (one `lsinfo` level).
pub enum SourceEntry {
    /// A playable track; tags + virtual `path` already populated.
    Song(Song),
    /// A virtual subdirectory in canonical mount-style form,
    /// e.g. `"alarm-music/AC%2FDC"`.
    Dir(String),
}

// ─── MusicSource trait ───────────────────────────────────────────────────────

/// Object-safe, `Send + Sync` trait that every music-source backend implements.
///
/// Selection is compile-time (sync const fn-pointer table in `rmpd-source`);
/// the methods here are async because I/O happens when you *call* them, never
/// at registry lookup time.
#[async_trait]
pub trait MusicSource: Send + Sync {
    /// URI scheme this backend owns, e.g. `"subsonic"`, `"file"`.
    fn scheme(&self) -> &str;

    /// Instance name from `[[source]] name =`. Becomes the authority component
    /// of mount-style virtual paths: `<name>/...`.
    fn name(&self) -> &str;

    /// Cheap liveness / auth probe. MUST NOT log credentials.
    async fn ping(&self) -> SourceResult<()>;

    /// List immediate children of a virtual directory (`""` = source root).
    ///
    /// Canonical path contract: mount-style `<name>/...` (no `scheme://`),
    /// matching runtime source ownership and playback resolution.
    async fn browse(&self, dir: &str) -> SourceResult<Vec<SourceEntry>>;

    /// Full catalog enumeration for `update` / sync → DB population.
    /// Each returned `Song` carries MPD tags + mount-style virtual `path`.
    async fn list_all(&self) -> SourceResult<Vec<Song>>;

    /// Server-side search (maps to MPD `find`/`search` base).
    async fn search(&self, query: &str) -> SourceResult<Vec<Song>>;

    /// Map a remote song id to a directly-playable `http(s)://` stream URL,
    /// consumed unchanged by `rmpd_stream::HttpSource` via `decoder.rs`.
    /// Returns `String` (not `url::Url`) so `rmpd-plugin` never needs `url`
    /// or `reqwest`.
    async fn resolve_stream_uri(&self, song_id: &str) -> SourceResult<String>;

    /// Fetch raw cover-art bytes for a song id (e.g. Subsonic `getCoverArt`).
    ///
    /// Default: `Ok(None)` — filesystem-backed sources serve embedded art via
    /// the local extractor, so they need not implement this. Remote sources
    /// override it; the caller caches the bytes and infers the MIME type.
    async fn cover_art(&self, _song_id: &str) -> SourceResult<Option<Vec<u8>>> {
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::Arc;
    use std::task::{Context, Poll, Wake, Waker};

    fn block_on<F: Future>(future: F) -> F::Output {
        struct NoopWake;
        impl Wake for NoopWake {
            fn wake(self: Arc<Self>) {}
        }

        let waker = Waker::from(Arc::new(NoopWake));
        let mut cx = Context::from_waker(&waker);
        let mut future = Pin::from(Box::new(future));

        loop {
            match future.as_mut().poll(&mut cx) {
                Poll::Ready(v) => return v,
                Poll::Pending => std::thread::yield_now(),
            }
        }
    }

    #[test]
    fn source_error_constructors_scrub_newlines_and_limit_length() {
        let err = SourceError::protocol("line1\nline2\rline3");
        let s = err.to_string();
        assert!(!s.contains('\n'));
        assert!(!s.contains('\r'));
        assert!(s.contains("line1 line2 line3"));

        let long = "x".repeat(2000);
        let err = SourceError::config(long);
        let rendered = err.to_string();
        // Prefix + bounded message.
        assert!(rendered.len() <= 600);
    }

    #[test]
    fn source_error_debug_is_structured_and_safe() {
        let err = SourceError::auth("invalid token\nplease retry");
        let d = format!("{err:?}");
        assert!(d.contains("SourceError"));
        assert!(d.contains("kind"));
        assert!(d.contains("Auth"));
        assert!(!d.contains('\n'));
    }

    struct DummySource;

    #[async_trait]
    impl MusicSource for DummySource {
        fn scheme(&self) -> &str {
            "dummy"
        }

        fn name(&self) -> &str {
            "dummy-name"
        }

        async fn ping(&self) -> SourceResult<()> {
            Ok(())
        }

        async fn browse(&self, _dir: &str) -> SourceResult<Vec<SourceEntry>> {
            Ok(vec![])
        }

        async fn list_all(&self) -> SourceResult<Vec<Song>> {
            Ok(vec![])
        }

        async fn search(&self, _query: &str) -> SourceResult<Vec<Song>> {
            Ok(vec![])
        }

        async fn resolve_stream_uri(&self, _song_id: &str) -> SourceResult<String> {
            Ok("http://example.invalid/stream".to_owned())
        }
    }

    #[test]
    fn default_cover_art_returns_none() {
        let src = DummySource;
        let got = block_on(src.cover_art("any")).expect("default cover_art should succeed");
        assert!(got.is_none());
    }
}
