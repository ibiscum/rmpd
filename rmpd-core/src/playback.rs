//! Playback-related types and utilities

use crate::song::Song;
use camino::Utf8PathBuf;
use std::sync::Arc;

/// A song prepared for playback with a resolved media location.
/// Avoids cloning the full Song — shares it via Arc.
///
/// `resolved_path` is either:
/// - an absolute local file path, or
/// - a directly playable stream URI (for example `http://...`).
pub struct PlaybackSong {
    pub song: Arc<Song>,
    pub resolved_path: Utf8PathBuf,
    /// Optional playback range `(start, end)` in seconds (CUE virtual tracks,
    /// `rangeid`/`addid` ranges). `None` plays the whole file.
    pub range: Option<(f64, f64)>,
}

impl PlaybackSong {
    /// Whether `resolved_path` points to a stream URI.
    #[must_use]
    pub fn is_stream_uri(&self) -> bool {
        crate::path::is_uri(self.resolved_path.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::borrow::Cow;

    fn test_song(path: &str) -> Song {
        Song {
            id: 1,
            path: Utf8PathBuf::from(path),
            duration: None,
            sample_rate: None,
            channels: None,
            bits_per_sample: None,
            bitrate: None,
            replay_gain_track_gain: None,
            replay_gain_track_peak: None,
            replay_gain_album_gain: None,
            replay_gain_album_peak: None,
            added_at: 0,
            last_modified: 0,
            tags: vec![(Cow::Borrowed("title"), "Test Song".to_owned())],
        }
    }

    #[test]
    fn playback_song_identifies_stream_uri() {
        let ps = PlaybackSong {
            song: Arc::new(test_song("radio")),
            resolved_path: Utf8PathBuf::from("https://radio.example/stream"),
            range: None,
        };
        assert!(ps.is_stream_uri());
    }

    #[test]
    fn playback_song_identifies_local_path() {
        let ps = PlaybackSong {
            song: Arc::new(test_song("music/track.flac")),
            resolved_path: Utf8PathBuf::from("/srv/music/track.flac"),
            range: Some((5.0, 10.0)),
        };
        assert!(!ps.is_stream_uri());
    }
}
