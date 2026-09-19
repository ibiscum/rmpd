/// Shared tag utilities: fallback chains, decimal normalization, and Vorbis key mappings.
use std::collections::HashMap;
use std::sync::LazyLock;

/// Lazy-initialized HashMap for O(1) VorbisComment tag lookups
static VORBIS_TAG_MAP_HASH: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    m.insert("title", "title");
    m.insert("artist", "artist");
    m.insert("album", "album");
    m.insert("albumartist", "albumartist");
    m.insert("albumartistsort", "albumartistsort");
    m.insert("artistsort", "artistsort");
    m.insert("composer", "composer");
    m.insert("composersort", "composersort");
    m.insert("performer", "performer");
    m.insert("conductor", "conductor");
    m.insert("comment", "comment");
    m.insert("description", "comment");
    m.insert("genre", "genre");
    m.insert("mood", "mood");
    m.insert("work", "work");
    m.insert("movement", "movement");
    m.insert("movementnumber", "movementnumber");
    m.insert("ensemble", "ensemble");
    m.insert("location", "location");
    m.insert("grouping", "grouping");
    m.insert("track", "track");
    m.insert("tracknumber", "track");
    m.insert("disc", "disc");
    m.insert("discnumber", "disc");
    m.insert("date", "date");
    m.insert("originaldate", "originaldate");
    m.insert("label", "label");
    m.insert("musicbrainz_trackid", "musicbrainz_trackid");
    m.insert("musicbrainz_albumid", "musicbrainz_albumid");
    m.insert("musicbrainz_artistid", "musicbrainz_artistid");
    m.insert("musicbrainz_albumartistid", "musicbrainz_albumartistid");
    m.insert("musicbrainz_releasegroupid", "musicbrainz_releasegroupid");
    m.insert("musicbrainz_releasetrackid", "musicbrainz_releasetrackid");
    m.insert("musicbrainz_workid", "musicbrainz_workid");
    m
});

/// Return the fallback chain for a tag (MPD's Fallback.hxx).
/// Input matching is case-insensitive for known tags.
/// For most tags, returns a single-element vec.
pub fn tag_fallback_chain(tag: &str) -> Vec<&str> {
    let tag_lower = tag.to_lowercase();
    match tag_lower.as_str() {
        "albumartist" => vec!["albumartist", "artist"],
        "artistsort" => vec!["artistsort", "artist"],
        "albumartistsort" => vec!["albumartistsort", "albumartist", "artistsort", "artist"],
        "albumsort" => vec!["albumsort", "album"],
        "titlesort" => vec!["titlesort", "title"],
        "composersort" => vec!["composersort", "composer"],
        "artist" => vec!["artist"],
        "album" => vec!["album"],
        "title" => vec!["title"],
        "track" => vec!["track"],
        "date" => vec!["date"],
        "originaldate" => vec!["originaldate"],
        "genre" => vec!["genre"],
        "composer" => vec!["composer"],
        "performer" => vec!["performer"],
        "disc" => vec!["disc"],
        "comment" => vec!["comment"],
        "grouping" => vec!["grouping"],
        "label" => vec!["label"],
        "musicbrainz_artistid" => vec!["musicbrainz_artistid"],
        "musicbrainz_albumid" => vec!["musicbrainz_albumid"],
        "musicbrainz_albumartistid" => vec!["musicbrainz_albumartistid"],
        "musicbrainz_trackid" => vec!["musicbrainz_trackid"],
        "musicbrainz_releasetrackid" => vec!["musicbrainz_releasetrackid"],
        "musicbrainz_releasegroupid" => vec!["musicbrainz_releasegroupid"],
        "musicbrainz_workid" => vec!["musicbrainz_workid"],
        _ => vec![tag], // unknown tags: search as-is
    }
}

/// Normalize a Track or Disc value the same way MPD does (Handler.cxx NormalizeDecimal):
/// strip leading zeros, strip non-digit suffix, preserve all-zero result as "0".
pub fn normalize_decimal(s: &str) -> Option<String> {
    let s = s.trim();
    // Find first non-zero digit
    let start = s.chars().position(|c| c != '0').unwrap_or(s.len());
    // Take only ASCII digits from that position
    let tail: String = s[start..]
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    if tail.is_empty() {
        // All digits were zeros (e.g. "0", "00") -- preserve as "0"
        if s.starts_with('0') {
            Some("0".to_string())
        } else {
            None
        }
    } else {
        Some(tail)
    }
}

/// MPD-canonical VorbisComment key -> tag name mapping.
/// Derived from MPD's tag/Names.cxx (tag_item_names) + lib/xiph/XiphTags.cxx.
/// Only these key names are recognized for structured tags (lookup is case-insensitive).
/// Returns the tag name for a given VorbisComment key, or None if not found.
pub fn vorbis_tag_map_get(key: &str) -> Option<&'static str> {
    let key_lower = key.to_lowercase();
    VORBIS_TAG_MAP_HASH.get(key_lower.as_str()).copied()
}

#[cfg(test)]
mod tests {
    use super::{normalize_decimal, tag_fallback_chain, vorbis_tag_map_get};

    #[test]
    fn fallback_chain_known_tags_is_case_insensitive() {
        assert_eq!(
            tag_fallback_chain("AlbumArtist"),
            vec!["albumartist", "artist"]
        );
        assert_eq!(tag_fallback_chain("TitleSort"), vec!["titlesort", "title"]);
    }

    #[test]
    fn fallback_chain_unknown_tag_is_preserved() {
        assert_eq!(tag_fallback_chain("CustomTag"), vec!["CustomTag"]);
    }

    #[test]
    fn normalize_decimal_edge_cases() {
        assert_eq!(normalize_decimal("0"), Some("0".to_string()));
        assert_eq!(normalize_decimal("00"), Some("0".to_string()));
        assert_eq!(normalize_decimal("003/12"), Some("3".to_string()));
        assert_eq!(normalize_decimal("000abc"), Some("0".to_string()));
        assert_eq!(normalize_decimal("  0012x  "), Some("12".to_string()));
        assert_eq!(normalize_decimal("abc"), None);
        assert_eq!(normalize_decimal(""), None);
    }

    #[test]
    fn vorbis_map_lookup_is_case_insensitive() {
        assert_eq!(vorbis_tag_map_get("TRACKNUMBER"), Some("track"));
        assert_eq!(vorbis_tag_map_get("Description"), Some("comment"));
        assert_eq!(vorbis_tag_map_get("unknown_key"), None);
    }
}
