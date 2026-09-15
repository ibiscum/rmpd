use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::time::Duration;
use crate::tag::tag_fallback_chain;

/// Well-known MPD tag names. Using static references avoids per-song String allocation.
pub fn intern_tag_key(key: &str) -> Cow<'static, str> {
    match key.to_lowercase().as_str() {
        "artist" => Cow::Borrowed("artist"),
        "album" => Cow::Borrowed("album"),
        "title" => Cow::Borrowed("title"),
        "track" => Cow::Borrowed("track"),
        "name" => Cow::Borrowed("name"),
        "genre" => Cow::Borrowed("genre"),
        "date" => Cow::Borrowed("date"),
        "composer" => Cow::Borrowed("composer"),
        "performer" => Cow::Borrowed("performer"),
        "comment" => Cow::Borrowed("comment"),
        "disc" => Cow::Borrowed("disc"),
        "label" => Cow::Borrowed("label"),
        "albumartist" => Cow::Borrowed("albumartist"),
        "musicbrainz_artistid" => Cow::Borrowed("musicbrainz_artistid"),
        "musicbrainz_albumid" => Cow::Borrowed("musicbrainz_albumid"),
        "musicbrainz_albumartistid" => Cow::Borrowed("musicbrainz_albumartistid"),
        "musicbrainz_trackid" => Cow::Borrowed("musicbrainz_trackid"),
        "musicbrainz_releasetrackid" => Cow::Borrowed("musicbrainz_releasetrackid"),
        "musicbrainz_workid" => Cow::Borrowed("musicbrainz_workid"),
        "originaldate" => Cow::Borrowed("originaldate"),
        "albumsort" => Cow::Borrowed("albumsort"),
        "artistsort" => Cow::Borrowed("artistsort"),
        "albumartistsort" => Cow::Borrowed("albumartistsort"),
        "titlesort" => Cow::Borrowed("titlesort"),
        "work" => Cow::Borrowed("work"),
        "grouping" => Cow::Borrowed("grouping"),
        "conductor" => Cow::Borrowed("conductor"),
        "ensemble" => Cow::Borrowed("ensemble"),
        "movement" => Cow::Borrowed("movement"),
        "movementnumber" => Cow::Borrowed("movementnumber"),
        "location" => Cow::Borrowed("location"),
        "mood" => Cow::Borrowed("mood"),
        "composersort" => Cow::Borrowed("composersort"),
        "musicbrainz_releasegroupid" => Cow::Borrowed("musicbrainz_releasegroupid"),
        _ => Cow::Owned(key.to_lowercase()),
    }
}

/// Map lowercase tag name to canonical MPD display name.
pub fn canonical_tag_name(tag: &str) -> Cow<'_, str> {
    let tag_lower = tag.to_lowercase();
    match tag_lower.as_str() {
        "artist" => Cow::Borrowed("Artist"),
        "artistsort" => Cow::Borrowed("ArtistSort"),
        "album" => Cow::Borrowed("Album"),
        "albumsort" => Cow::Borrowed("AlbumSort"),
        "albumartist" => Cow::Borrowed("AlbumArtist"),
        "albumartistsort" => Cow::Borrowed("AlbumArtistSort"),
        "title" => Cow::Borrowed("Title"),
        "titlesort" => Cow::Borrowed("TitleSort"),
        "track" => Cow::Borrowed("Track"),
        "name" => Cow::Borrowed("Name"),
        "genre" => Cow::Borrowed("Genre"),
        "mood" => Cow::Borrowed("Mood"),
        "date" => Cow::Borrowed("Date"),
        "originaldate" => Cow::Borrowed("OriginalDate"),
        "composer" => Cow::Borrowed("Composer"),
        "composersort" => Cow::Borrowed("ComposerSort"),
        "performer" => Cow::Borrowed("Performer"),
        "conductor" => Cow::Borrowed("Conductor"),
        "work" => Cow::Borrowed("Work"),
        "movement" => Cow::Borrowed("Movement"),
        "movementnumber" => Cow::Borrowed("MovementNumber"),
        "ensemble" => Cow::Borrowed("Ensemble"),
        "location" => Cow::Borrowed("Location"),
        "grouping" => Cow::Borrowed("Grouping"),
        "comment" => Cow::Borrowed("Comment"),
        "disc" => Cow::Borrowed("Disc"),
        "label" => Cow::Borrowed("Label"),
        "musicbrainz_artistid" => Cow::Borrowed("MUSICBRAINZ_ARTISTID"),
        "musicbrainz_albumid" => Cow::Borrowed("MUSICBRAINZ_ALBUMID"),
        "musicbrainz_albumartistid" => Cow::Borrowed("MUSICBRAINZ_ALBUMARTISTID"),
        "musicbrainz_trackid" => Cow::Borrowed("MUSICBRAINZ_TRACKID"),
        "musicbrainz_releasetrackid" => Cow::Borrowed("MUSICBRAINZ_RELEASETRACKID"),
        "musicbrainz_releasegroupid" => Cow::Borrowed("MUSICBRAINZ_RELEASEGROUPID"),
        "musicbrainz_workid" => Cow::Borrowed("MUSICBRAINZ_WORKID"),
        _ => Cow::Owned(tag_lower),
    }
}

/// Whether a tag is a recognized MPD tag type.
#[must_use]
pub fn is_known_tag_name(tag: &str) -> bool {
    matches!(canonical_tag_name(tag), Cow::Borrowed(_))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Song {
    pub id: u64,
    pub path: Utf8PathBuf,

    // Audio properties (NOT tags — these stay as struct fields)
    pub duration: Option<Duration>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u8>,
    pub bits_per_sample: Option<u16>,
    pub bitrate: Option<u32>,

    // ReplayGain
    pub replay_gain_track_gain: Option<f32>,
    pub replay_gain_track_peak: Option<f32>,
    pub replay_gain_album_gain: Option<f32>,
    pub replay_gain_album_peak: Option<f32>,

    // Timestamps
    pub added_at: i64,
    pub last_modified: i64,

    /// All tags as (lowercase_tag_name, value) pairs.
    /// The same tag name may appear multiple times for multi-valued tags.
    /// Tags are stored in file insertion order; output preserves this order to match MPD.
    /// Tag keys are interned using Cow<'static, str> to reduce memory usage.
    pub tags: Vec<(Cow<'static, str>, String)>,
}

impl Song {
    /// Get the first value for a tag, or None if the tag is not present.
    pub fn tag(&self, name: &str) -> Option<&str> {
        let name_lower = name.to_lowercase();
        self.tags
            .iter()
            .find(|(k, _)| k.as_ref() == name_lower.as_str())
            .map(|(_, v)| v.as_str())
    }

    /// Get all values for a tag.
    pub fn tag_values(&self, name: &str) -> impl Iterator<Item = &str> {
        let name_lower = name.to_lowercase();
        self.tags
            .iter()
            .filter(move |(k, _)| k.as_ref() == name_lower.as_str())
            .map(|(_, v)| v.as_str())
    }

    /// Get the first value for a tag with MPD-style fallback chains.
    /// E.g. albumartist falls back to artist, artistsort falls back to artist, etc.
    pub fn tag_with_fallback(&self, name: &str) -> Option<&str> {
        let name_lower = name.to_lowercase();
        for candidate in tag_fallback_chain(&name_lower) {
            if let Some(v) = self.tag(candidate) {
                return Some(v);
            }
        }
        None
    }

    /// Get all values for a tag with MPD-style fallback.
    /// If the primary tag has values, return those.
    /// Otherwise, return fallback tag values.
    pub fn tag_values_with_fallback(&self, name: &str) -> Vec<&str> {
        let name_lower = name.to_lowercase();
        for candidate in tag_fallback_chain(&name_lower) {
            let values: Vec<&str> = self.tag_values(candidate).collect();
            if !values.is_empty() {
                return values;
            }
        }
        Vec::new()
    }

    /// Check if a song's tag matches an exact value (checks all values for multi-valued tags).
    pub fn tag_eq(&self, tag: &str, value: &str) -> bool {
        self.tag_values(tag).any(|v| v == value)
    }

    /// Check if a song's tag contains a value (case-insensitive, checks all values for multi-valued tags).
    pub fn tag_contains(&self, tag: &str, value: &str) -> bool {
        let needle = value.to_lowercase();
        self.tag_values(tag)
            .any(|v| v.to_lowercase().contains(&needle))
    }

    pub fn display_title(&self) -> &str {
        self.tag("title")
            .unwrap_or_else(|| self.path.file_name().unwrap_or("Unknown"))
    }

    pub fn display_artist(&self) -> &str {
        self.tag("artist")
            .or_else(|| self.tag("albumartist"))
            .unwrap_or("Unknown Artist")
    }

    pub fn display_album(&self) -> &str {
        self.tag("album").unwrap_or("Unknown Album")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioFormat {
    pub sample_rate: u32,
    pub channels: u8,
    pub bits_per_sample: u8,
}

impl AudioFormat {
    pub fn new(sample_rate: u32, channels: u8, bits_per_sample: u8) -> Self {
        Self {
            sample_rate,
            channels,
            bits_per_sample,
        }
    }
}
