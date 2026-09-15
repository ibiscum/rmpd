#![allow(clippy::cargo_common_metadata)]

// Music library APIs: database, scanning, metadata/artwork, fingerprinting, and file watching
pub mod artwork;
pub mod cue;
pub mod database;
pub mod fingerprint;
pub mod metadata;
pub mod scanner;
pub mod watcher;

pub use artwork::{AlbumArtExtractor, ArtworkData};
pub use cue::{CueTrack, parse_cue};
pub use database::{Database, DbPool, DirectoryListing, PlaylistInfo, WalkEntry};
pub use fingerprint::Fingerprinter;
pub use metadata::{Artwork, MetadataExtractor};
pub use scanner::{ScanStats, Scanner};
pub use watcher::FilesystemWatcher;
