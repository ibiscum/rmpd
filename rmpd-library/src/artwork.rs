use lofty::file::TaggedFileExt;
use lofty::picture::PictureType;
use rmpd_core::error::{Result, RmpdError};
use sha2::{Digest, Sha256};
use std::path::Path;

use crate::database::Database;

const MAX_ARTWORK_SIZE: usize = 5 * 1024 * 1024; // 5MB

fn infer_mime(data: &[u8]) -> &'static str {
    if data.starts_with(b"\xFF\xD8\xFF") {
        "image/jpeg"
    } else if data.starts_with(b"\x89PNG\r\n\x1a\n") {
        "image/png"
    } else if data.starts_with(b"GIF8") {
        "image/gif"
    } else if data.len() >= 12 && &data[0..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        "image/webp"
    } else {
        "application/octet-stream"
    }
}

fn preferred_picture_index(types: &[PictureType]) -> Option<usize> {
    types
        .iter()
        .position(|t| matches!(t, PictureType::CoverFront | PictureType::Other))
        .or_else(|| (!types.is_empty()).then_some(0))
}

fn should_extract_from_file(file_path: &str) -> Result<bool> {
    if file_path.is_empty() {
        // Source-backed artwork lookups use an empty local path and rely on cache only.
        return Ok(false);
    }
    if !Path::new(file_path).is_absolute() {
        return Err(RmpdError::Library(format!(
            "Artwork file path must be absolute: {file_path}"
        )));
    }
    Ok(true)
}

fn select_picture_from_tag(tag: &lofty::tag::Tag) -> Option<&lofty::picture::Picture> {
    let pictures = tag.pictures();
    let types: Vec<PictureType> = pictures.iter().map(|p| p.pic_type()).collect();
    preferred_picture_index(&types).and_then(|idx| pictures.get(idx))
}

fn select_picture(tagged_file: &lofty::file::TaggedFile) -> Option<&lofty::picture::Picture> {
    if let Some(primary_tag) = tagged_file.primary_tag()
        && let Some(pic) = select_picture_from_tag(primary_tag)
    {
        return Some(pic);
    }

    tagged_file.tags().iter().find_map(select_picture_from_tag)
}

fn sha256_hex(data: &[u8]) -> String {
    Sha256::digest(data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[derive(Debug)]
pub struct AlbumArtExtractor {
    db: Database,
}

impl AlbumArtExtractor {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Extract album art from a file and cache it
    /// `cache_key`: relative path for cache lookup (e.g., "01.m4a")
    /// `file_path`: absolute path for file reading (e.g., "/home/user/Music/01.m4a")
    pub fn extract_and_cache(
        &self,
        cache_key: &str,
        file_path: &str,
    ) -> Result<Option<(Vec<u8>, String)>> {
        // Check cache first using relative path as key
        if let Some((data, mime)) = self.db.get_artwork(cache_key, "front")? {
            return Ok(Some((data, mime)));
        }

        if !should_extract_from_file(file_path)? {
            return Ok(None);
        }

        // Not in cache, extract from file using absolute path
        let abs_path = Path::new(file_path);
        let tagged_file = lofty::read_from_path(abs_path)
            .map_err(|e| RmpdError::Library(format!("Failed to read file: {e}")))?;

        // Prefer primary-tag cover art, then scan all tags for fallback.
        let picture = select_picture(&tagged_file);

        if let Some(pic) = picture {
            let data = pic.data();

            // Check size limit
            if data.len() > MAX_ARTWORK_SIZE {
                return Err(RmpdError::Library(format!(
                    "Artwork too large: {} bytes (max {})",
                    data.len(),
                    MAX_ARTWORK_SIZE
                )));
            }

            let hash = sha256_hex(data);

            // Get MIME type from tag, fall back to magic-byte inference
            let mime_type = pic
                .mime_type()
                .map(|m| m.to_string())
                .unwrap_or_else(|| infer_mime(data).to_owned());

            // Store in cache using relative path as key
            self.db
                .store_artwork(cache_key, "front", &mime_type, data, &hash)?;

            Ok(Some((data.to_vec(), mime_type)))
        } else {
            Ok(None)
        }
    }

    /// Store externally-fetched artwork (e.g. from a remote music source such as
    /// Subsonic) in the cache under `cache_key`, so subsequent chunked
    /// [`get_artwork`](Self::get_artwork) calls serve it from cache without
    /// re-fetching. The MIME type is inferred from the image magic bytes.
    pub fn cache_external(&self, cache_key: &str, data: &[u8]) -> Result<()> {
        if data.is_empty() {
            return Ok(());
        }
        if data.len() > MAX_ARTWORK_SIZE {
            return Err(RmpdError::Library(format!(
                "Artwork too large: {} bytes (max {})",
                data.len(),
                MAX_ARTWORK_SIZE
            )));
        }
        let mime_type = infer_mime(data);
        let hash = sha256_hex(data);
        self.db
            .store_artwork(cache_key, "front", mime_type, data, &hash)
    }

    /// Whether artwork is already cached for `cache_key`.
    pub fn is_cached(&self, cache_key: &str) -> Result<bool> {
        self.db.has_artwork(cache_key, "front")
    }

    /// Get album art from cache or extract if not cached
    /// `cache_key`: relative path for cache lookup (e.g., "01.m4a")
    /// `file_path`: absolute path for file reading (e.g., "/home/user/Music/01.m4a")
    pub fn get_artwork(
        &self,
        cache_key: &str,
        file_path: &str,
        offset: usize,
    ) -> Result<Option<ArtworkData>> {
        let (data, stored_mime) = match self.extract_and_cache(cache_key, file_path)? {
            Some(result) => result,
            None => return Ok(None),
        };

        // Use the stored MIME type; fall back to magic-byte inference only when empty.
        let mime_type = if stored_mime.is_empty() {
            infer_mime(&data).to_owned()
        } else {
            stored_mime
        };

        // Handle offset for chunked transfer
        // MPD protocol uses 8KB (8192 byte) chunks
        const CHUNK_SIZE: usize = 8192;

        let chunk = if offset >= data.len() {
            // Return empty chunk when offset is past the end
            // This is needed for proper MPD protocol compliance
            Vec::new()
        } else {
            let end = (offset + CHUNK_SIZE).min(data.len());
            data[offset..end].to_vec()
        };

        Ok(Some(ArtworkData {
            mime_type,
            total_size: data.len(),
            data: chunk,
        }))
    }
}

#[derive(Debug)]
pub struct ArtworkData {
    pub mime_type: String,
    pub total_size: usize,
    pub data: Vec<u8>,
}

pub(crate) fn picture_type_to_string(pic_type: PictureType) -> String {
    match pic_type {
        PictureType::CoverFront => "front",
        PictureType::CoverBack => "back",
        PictureType::Icon => "icon",
        PictureType::OtherIcon => "other_icon",
        PictureType::Leaflet => "leaflet",
        PictureType::Media => "media",
        PictureType::LeadArtist => "artist",
        PictureType::Artist => "artist",
        PictureType::Conductor => "conductor",
        PictureType::Band => "band",
        PictureType::Composer => "composer",
        PictureType::Lyricist => "lyricist",
        PictureType::RecordingLocation => "recording_location",
        PictureType::DuringRecording => "during_recording",
        PictureType::DuringPerformance => "during_performance",
        PictureType::ScreenCapture => "screen_capture",
        PictureType::BrightFish => "bright_fish",
        PictureType::Illustration => "illustration",
        PictureType::BandLogo => "band_logo",
        PictureType::PublisherLogo => "publisher_logo",
        _ => "other",
    }
    .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infer_mime_detects_webp_with_12_byte_header() {
        let data = b"RIFFxxxxWEBP";
        assert_eq!(infer_mime(data), "image/webp");
    }

    #[test]
    fn infer_mime_defaults_to_octet_stream() {
        assert_eq!(infer_mime(b"not-an-image"), "application/octet-stream");
    }

    #[test]
    fn preferred_picture_index_prioritizes_front_then_other_then_first() {
        let types = vec![PictureType::Media, PictureType::CoverFront, PictureType::Icon];
        assert_eq!(preferred_picture_index(&types), Some(1));

        let types = vec![PictureType::Media, PictureType::Other, PictureType::Icon];
        assert_eq!(preferred_picture_index(&types), Some(1));

        let types = vec![PictureType::Media, PictureType::Icon];
        assert_eq!(preferred_picture_index(&types), Some(0));

        let types: Vec<PictureType> = Vec::new();
        assert_eq!(preferred_picture_index(&types), None);
    }

    #[test]
    fn should_extract_from_file_allows_empty_source_path() {
        assert!(matches!(should_extract_from_file(""), Ok(false)));
    }

    #[test]
    fn should_extract_from_file_rejects_relative_path() {
        let err = should_extract_from_file("relative/path.flac").expect_err("relative path");
        assert!(err
            .to_string()
            .contains("Artwork file path must be absolute"));
    }

    #[test]
    fn should_extract_from_file_accepts_absolute_path() {
        assert!(matches!(should_extract_from_file("/tmp/song.flac"), Ok(true)));
    }
}
