use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{DebounceEventResult, Debouncer, RecommendedCache, new_debouncer};
use rmpd_core::error::{Result, RmpdError};
use rmpd_core::event::{Event as RmpdEvent, EventBus};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::sync::mpsc;
use tracing::{debug, error, info, warn};

use crate::database::Database;
use crate::metadata::MetadataExtractor;

const DEBOUNCE_DURATION: Duration = Duration::from_millis(300);
const EVENT_CHANNEL_SIZE: usize = 1024;

pub struct FilesystemWatcher {
    music_dir: PathBuf,
    db: Arc<Mutex<Database>>,
    event_bus: EventBus,
    debouncer: Option<Debouncer<RecommendedWatcher, RecommendedCache>>,
}

impl fmt::Debug for FilesystemWatcher {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FilesystemWatcher")
            .field("music_dir", &self.music_dir)
            .field("event_bus", &self.event_bus)
            .field("debouncer_active", &self.debouncer.is_some())
            .finish_non_exhaustive()
    }
}

impl FilesystemWatcher {
    pub fn new(music_dir: PathBuf, db: Arc<Mutex<Database>>, event_bus: EventBus) -> Result<Self> {
        Ok(Self {
            music_dir,
            db,
            event_bus,
            debouncer: None,
        })
    }

    /// Start watching the music directory
    pub async fn start(&mut self) -> Result<()> {
        info!("starting filesystem watcher for {:?}", self.music_dir);

        let (tx, mut rx) = mpsc::channel(EVENT_CHANNEL_SIZE);
        let db = Arc::clone(&self.db);
        let event_bus = self.event_bus.clone();
        let music_dir = self.music_dir.clone();

        // Create debouncer
        let debouncer = new_debouncer(
            DEBOUNCE_DURATION,
            None,
            move |result: DebounceEventResult| {
                // This callback runs on notify's own dedicated thread, which has
                // no Tokio runtime — so we must NOT `tokio::spawn` here (that
                // panics with "no reactor running"). `blocking_send` bridges the
                // event into the async handler task below.
                if let Err(e) = tx.blocking_send(result) {
                    error!("failed to send watch event: {}", e);
                }
            },
        )
        .map_err(|e| RmpdError::Library(format!("Failed to create watcher: {e}")))?;

        // Watch the music directory recursively
        let mut watcher = debouncer;
        watcher
            .watch(&self.music_dir, RecursiveMode::Recursive)
            .map_err(|e| RmpdError::Library(format!("Failed to watch directory: {e}")))?;

        self.debouncer = Some(watcher);

        // Emit start event
        self.event_bus.emit(RmpdEvent::FilesystemWatchStarted);

        // Spawn event handler task
        tokio::spawn(async move {
            while let Some(result) = rx.recv().await {
                match result {
                    Ok(events) => {
                        for event in events {
                            if let Err(e) =
                                handle_fs_event(&event, &music_dir, &db, &event_bus).await
                            {
                                error!("failed to handle filesystem event: {}", e);
                            }
                        }
                    }
                    Err(errors) => {
                        for error in errors {
                            error!("filesystem watch error: {}", error);
                        }
                    }
                }
            }
        });

        Ok(())
    }

    /// Stop watching (graceful shutdown)
    pub fn stop(&mut self) {
        if self.debouncer.is_some() {
            info!("stopping filesystem watcher");
            self.debouncer = None;
            self.event_bus.emit(RmpdEvent::FilesystemWatchStopped);
        }
    }
}

impl Drop for FilesystemWatcher {
    fn drop(&mut self) {
        self.stop();
    }
}

fn is_supported_audio_file(path: &Path) -> bool {
    if let Some(name) = path.file_name()
        && name.to_string_lossy().starts_with('.')
    {
        return false;
    }

    let utf8_path = match camino::Utf8PathBuf::from_path_buf(path.to_path_buf()) {
        Ok(p) => p,
        Err(_) => return false,
    };

    MetadataExtractor::is_supported_file(&utf8_path)
}

async fn handle_fs_event(
    event: &Event,
    music_dir: &Path,
    db: &Arc<Mutex<Database>>,
    event_bus: &EventBus,
) -> Result<()> {
    match event.kind {
        EventKind::Create(_) | EventKind::Modify(_) => {
            for path in &event.paths {
                if !is_supported_audio_file(path) {
                    continue;
                }

                // Make path relative to music directory
                let relative_path = match path.strip_prefix(music_dir) {
                    Ok(p) => p,
                    Err(_) => {
                        debug!("path outside music directory: {:?}", path);
                        continue;
                    }
                };

                let relative_utf8 = match camino::Utf8PathBuf::from_path_buf(relative_path.to_path_buf()) {
                    Ok(p) => p,
                    Err(_) => {
                        warn!("skipping non-UTF8 relative path: {:?}", relative_path);
                        continue;
                    }
                };
                let path_str = relative_utf8.to_string();

                debug!("file created/modified: {}", path_str);

                // Extract metadata
                let path_buf = match camino::Utf8PathBuf::from_path_buf(path.to_path_buf()) {
                    Ok(p) => p,
                    Err(_) => {
                        warn!("skipping non-UTF8 path: {:?}", path);
                        continue;
                    }
                };
                match MetadataExtractor::extract_from_file(&path_buf) {
                    Ok(mut song) => {
                        song.path = relative_utf8;

                        // Database operations need to be done with lock
                        let db_guard = db.lock().await;

                        // Check if song already exists
                        let exists = db_guard.get_song_by_path(&path_str)?.is_some();

                        // Add/update in database
                        db_guard.add_song(&song)?;

                        drop(db_guard); // Release lock before emitting event

                        // Emit appropriate event
                        if exists {
                            debug!("song updated: {}", path_str);
                            event_bus.emit(RmpdEvent::SongUpdated(song));
                        } else {
                            debug!("song added: {}", path_str);
                            event_bus.emit(RmpdEvent::SongAdded(song));
                        }
                    }
                    Err(e) => {
                        warn!("failed to extract metadata from {}: {}", path_str, e);
                    }
                }
            }
        }
        EventKind::Remove(_) => {
            for path in &event.paths {
                if !is_supported_audio_file(path) {
                    continue;
                }

                let relative_path = match path.strip_prefix(music_dir) {
                    Ok(p) => p,
                    Err(_) => continue,
                };

                let relative_utf8 = match camino::Utf8PathBuf::from_path_buf(relative_path.to_path_buf()) {
                    Ok(p) => p,
                    Err(_) => {
                        warn!("skipping non-UTF8 relative path: {:?}", relative_path);
                        continue;
                    }
                };
                let path_str = relative_utf8.as_str();

                debug!("file removed: {}", path_str);

                // Remove from database
                let db_guard = db.lock().await;
                db_guard.delete_song_by_path(path_str)?;
                drop(db_guard);

                // Emit event
                event_bus.emit(RmpdEvent::SongDeleted {
                    path: path_str.to_string(),
                });
            }
        }
        _ => {
            // Ignore other event types (access, metadata changes, etc.)
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{handle_fs_event, is_supported_audio_file};
    use crate::database::Database;
    use notify::event::CreateKind;
    use notify::{Event, EventKind};
    use rmpd_core::event::{Event as RmpdEvent, EventBus};
    use std::path::PathBuf;
    use std::sync::Arc;
    use tempfile::TempDir;
    use tokio::runtime::Builder;
    use tokio::sync::Mutex;

    #[test]
    fn supported_audio_file_uses_metadata_rules_and_skips_hidden() {
        assert!(is_supported_audio_file(std::path::Path::new("visible.flac")));
        assert!(is_supported_audio_file(std::path::Path::new("track.dsf")));
        assert!(is_supported_audio_file(std::path::Path::new("track.dff")));
        assert!(!is_supported_audio_file(std::path::Path::new(".hidden.flac")));
    }

    #[cfg(unix)]
    #[test]
    fn supported_audio_file_rejects_non_utf8_paths() {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;

        let non_utf8 = PathBuf::from(OsString::from_vec(vec![0x66, 0x80, b'.', b'f', b'l', b'a', b'c']));
        assert!(!is_supported_audio_file(non_utf8.as_path()));
    }

    #[test]
    fn create_event_stores_relative_path_and_emits_added() {
        let rt = Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime");

        rt.block_on(async {
            let temp = TempDir::new().expect("temp dir");
            let music_dir = temp.path().join("music");
            std::fs::create_dir_all(music_dir.join("album")).expect("create music dirs");

            let fixture = rmpd_core::test_utils::get_fixture(env!("CARGO_MANIFEST_DIR"), "basic.flac");
            let song_path = music_dir.join("album/song.flac");
            std::fs::copy(&fixture, &song_path).expect("copy fixture");

            let db_path = temp.path().join("watcher.db");
            let db = Database::open(db_path.to_str().expect("utf8 db path")).expect("open db");
            let db = Arc::new(Mutex::new(db));

            let event_bus = EventBus::new();
            let mut rx = event_bus.subscribe();

            let event = Event {
                kind: EventKind::Create(CreateKind::Any),
                paths: vec![song_path.clone()],
                attrs: Default::default(),
            };

            handle_fs_event(&event, music_dir.as_path(), &db, &event_bus)
                .await
                .expect("handle create event");

            let guard = db.lock().await;
            let stored_rel = guard
                .get_song_by_path("album/song.flac")
                .expect("query by relative path");
            assert!(stored_rel.is_some(), "expected relative song path in DB");

            let abs_path_str = song_path.to_str().expect("utf8 song path");
            let stored_abs = guard
                .get_song_by_path(abs_path_str)
                .expect("query by absolute path");
            assert!(stored_abs.is_none(), "absolute path should not be stored");
            drop(guard);

            let mut saw_added = false;
            while let Ok(ev) = rx.try_recv() {
                if let RmpdEvent::SongAdded(song) = ev {
                    assert_eq!(song.path.as_str(), "album/song.flac");
                    saw_added = true;
                }
            }
            assert!(saw_added, "expected SongAdded event");
        });
    }
}
