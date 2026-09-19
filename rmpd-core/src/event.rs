use crate::song::Song;
use crate::state::PlayerState;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::sync::broadcast;

/// Events that can be emitted by any component
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Event {
    // Player events
    PlayerStateChanged(PlayerState),
    SongChanged(Option<Song>),
    PositionChanged(Duration),
    VolumeChanged(u8),
    BitrateChanged(Option<u32>), // Instantaneous bitrate in kbps (for VBR files)
    SongFinished,
    /// The engine advanced to the look-ahead (next) song in-thread — gaplessly
    /// or via crossfade — instead of stopping. The protocol promotes its fed
    /// "next" to current and feeds the following song.
    AdvancedToNext,
    /// A remote stream's ICY "now playing" title changed. Carries the new
    /// title (None clears it). Notifies the `player` subsystem so idle clients
    /// re-query `currentsong`.
    StreamTitleChanged(Option<String>),

    // Queue events
    QueueChanged,
    QueueOptionsChanged,

    // Stored playlist events
    /// The set or contents of on-disk stored playlists changed (save, rm,
    /// rename, playlistadd, playlistdelete, playlistclear, playlistmove,
    /// searchaddpl). Notifies the `stored_playlist` idle subsystem so clients
    /// re-query `listplaylists` / `listplaylistinfo`.
    StoredPlaylistChanged,

    // Database events
    DatabaseUpdateStarted,
    DatabaseUpdateProgress {
        scanned: u32,
        total: u32,
    },
    DatabaseUpdateFinished,

    // Output events
    OutputsChanged,

    // Partition events
    PartitionsChanged,

    // Mount events
    MountsChanged,

    // Filesystem watcher events
    FilesystemWatchStarted,
    FilesystemWatchStopped,
    SongAdded(Song),
    SongUpdated(Song),
    SongDeleted {
        path: String,
    },

    // Sticker events
    /// A sticker was set, incremented/decremented, or deleted.
    StickerChanged,

    // Client-to-client messaging events
    /// A client subscribed to or unsubscribed from a channel.
    SubscriptionChanged,
    /// A message was delivered to at least one subscriber.
    MessageReceived,
}

/// Maps to MPD's idle subsystems
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Subsystem {
    Database,
    Update,
    StoredPlaylist,
    Playlist,
    Player,
    Mixer,
    Output,
    Options,
    Partition,
    Sticker,
    Subscription,
    Message,
    Neighbor,
    Mount,
}

impl Event {
    pub fn subsystems(&self) -> &'static [Subsystem] {
        match self {
            // Only notify idle for significant player events (state/song changes)
            // NOT for position/bitrate changes - those are too frequent and should be polled
            Event::PlayerStateChanged(_)
            | Event::SongChanged(_)
            | Event::SongFinished
            | Event::StreamTitleChanged(_) => &[Subsystem::Player],
            // Position and bitrate changes are internal - don't notify idle
            Event::PositionChanged(_) | Event::BitrateChanged(_) => &[],
            Event::VolumeChanged(_) => &[Subsystem::Mixer],
            Event::QueueChanged => &[Subsystem::Playlist],
            Event::QueueOptionsChanged => &[Subsystem::Options],
            Event::StoredPlaylistChanged => &[Subsystem::StoredPlaylist],
            Event::DatabaseUpdateStarted | Event::DatabaseUpdateProgress { .. } => {
                &[Subsystem::Update]
            }
            Event::DatabaseUpdateFinished => &[Subsystem::Database, Subsystem::Update],
            Event::SongAdded(_) | Event::SongUpdated(_) | Event::SongDeleted { .. } => {
                &[Subsystem::Database]
            }
            Event::OutputsChanged => &[Subsystem::Output],
            Event::PartitionsChanged => &[Subsystem::Partition],
            Event::MountsChanged => &[Subsystem::Mount],
            Event::StickerChanged => &[Subsystem::Sticker],
            Event::SubscriptionChanged => &[Subsystem::Subscription],
            Event::MessageReceived => &[Subsystem::Message],
            _ => &[],
        }
    }
}

/// Central event bus
#[derive(Debug, Clone)]
pub struct EventBus {
    sender: broadcast::Sender<Event>,
}

impl EventBus {
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(4096);
        Self { sender }
    }

    pub fn emit(&self, event: Event) {
        if let Err(e) = self.sender.send(event) {
            tracing::debug!("event dropped (no active subscribers): {}", e);
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.sender.subscribe()
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{Event, EventBus, Subsystem};
    use crate::song::{Song, intern_tag_key};
    use crate::state::PlayerState;
    use std::time::Duration;

    fn sample_song(path: &str) -> Song {
        Song {
            id: 1,
            path: path.into(),
            duration: Some(Duration::from_secs(180)),
            sample_rate: Some(44_100),
            channels: Some(2),
            bits_per_sample: Some(16),
            bitrate: Some(320),
            replay_gain_track_gain: None,
            replay_gain_track_peak: None,
            replay_gain_album_gain: None,
            replay_gain_album_peak: None,
            added_at: 0,
            last_modified: 0,
            tags: vec![(intern_tag_key("title"), "Sample".to_owned())],
        }
    }

    #[test]
    fn subsystem_mapping_covers_significant_events() {
        let song = sample_song("music/test.flac");

        let cases: Vec<(Event, &[Subsystem])> = vec![
            (Event::PlayerStateChanged(PlayerState::Play), &[Subsystem::Player]),
            (Event::SongChanged(None), &[Subsystem::Player]),
            (
                Event::StreamTitleChanged(Some("Radio Track".to_owned())),
                &[Subsystem::Player],
            ),
            (Event::SongFinished, &[Subsystem::Player]),
            (Event::VolumeChanged(42), &[Subsystem::Mixer]),
            (Event::QueueChanged, &[Subsystem::Playlist]),
            (Event::QueueOptionsChanged, &[Subsystem::Options]),
            (Event::StoredPlaylistChanged, &[Subsystem::StoredPlaylist]),
            (Event::DatabaseUpdateStarted, &[Subsystem::Update]),
            (
                Event::DatabaseUpdateProgress {
                    scanned: 1,
                    total: 2,
                },
                &[Subsystem::Update],
            ),
            (
                Event::DatabaseUpdateFinished,
                &[Subsystem::Database, Subsystem::Update],
            ),
            (Event::SongAdded(song.clone()), &[Subsystem::Database]),
            (Event::SongUpdated(song), &[Subsystem::Database]),
            (
                Event::SongDeleted {
                    path: "music/deleted.flac".to_owned(),
                },
                &[Subsystem::Database],
            ),
            (Event::OutputsChanged, &[Subsystem::Output]),
            (Event::PartitionsChanged, &[Subsystem::Partition]),
            (Event::MountsChanged, &[Subsystem::Mount]),
            (Event::StickerChanged, &[Subsystem::Sticker]),
            (Event::SubscriptionChanged, &[Subsystem::Subscription]),
            (Event::MessageReceived, &[Subsystem::Message]),
        ];

        for (event, expected) in cases {
            assert_eq!(event.subsystems(), expected);
        }
    }

    #[test]
    fn subsystem_mapping_ignores_high_frequency_or_internal_events() {
        let cases = vec![
            Event::PositionChanged(Duration::from_secs(12)),
            Event::BitrateChanged(Some(256)),
            Event::FilesystemWatchStarted,
            Event::FilesystemWatchStopped,
            Event::AdvancedToNext,
        ];

        for event in cases {
            assert!(event.subsystems().is_empty());
        }
    }

    #[test]
    fn event_bus_delivers_to_single_subscriber() {
        let bus = EventBus::new();
        let mut rx = bus.subscribe();

        bus.emit(Event::QueueChanged);

        let event = rx.try_recv().expect("subscriber should receive emitted event");
        assert!(matches!(event, Event::QueueChanged));
    }

    #[test]
    fn event_bus_delivers_to_multiple_subscribers() {
        let bus = EventBus::new();
        let mut rx1 = bus.subscribe();
        let mut rx2 = bus.subscribe();

        bus.emit(Event::OutputsChanged);

        assert!(matches!(
            rx1.try_recv().expect("subscriber 1 should receive event"),
            Event::OutputsChanged
        ));
        assert!(matches!(
            rx2.try_recv().expect("subscriber 2 should receive event"),
            Event::OutputsChanged
        ));
    }

    #[test]
    fn event_bus_emit_without_subscribers_is_safe() {
        let bus = EventBus::default();
        bus.emit(Event::MessageReceived);
    }
}
