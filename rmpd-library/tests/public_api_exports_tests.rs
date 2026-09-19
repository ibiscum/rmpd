use rmpd_library::{
    AlbumArtExtractor, Artwork, ArtworkData, CueTrack, Database, DbPool, DirectoryListing,
    FilesystemWatcher, Fingerprinter, MetadataExtractor, PlaylistInfo, ScanStats, Scanner,
    WalkEntry,
};

#[test]
fn public_reexports_are_accessible() {
    // Compile-time smoke check for crate root re-exports.
    let exported_types = [
        std::any::type_name::<AlbumArtExtractor>(),
        std::any::type_name::<Artwork>(),
        std::any::type_name::<ArtworkData>(),
        std::any::type_name::<CueTrack>(),
        std::any::type_name::<Database>(),
        std::any::type_name::<DbPool>(),
        std::any::type_name::<DirectoryListing>(),
        std::any::type_name::<FilesystemWatcher>(),
        std::any::type_name::<Fingerprinter>(),
        std::any::type_name::<MetadataExtractor>(),
        std::any::type_name::<PlaylistInfo>(),
        std::any::type_name::<ScanStats>(),
        std::any::type_name::<Scanner>(),
        std::any::type_name::<WalkEntry>(),
    ];

    assert_eq!(exported_types.len(), 14);
    assert!(exported_types.iter().all(|name| !name.is_empty()));
}
