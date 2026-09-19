use rmpd_core::event::EventBus;
use rmpd_library::{Database, Scanner};
use std::path::Path;

fn default_music_dir(home: Option<&str>) -> String {
    let home = home.unwrap_or(".");
    format!("{home}/Music")
}

fn resolve_music_dir(cli_arg: Option<String>, home: Option<&str>) -> String {
    cli_arg.unwrap_or_else(|| default_music_dir(home))
}

fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    // Create database
    let db = Database::open("/tmp/rmpd_test.db")?;

    // Create event bus and scanner
    let event_bus = EventBus::new();
    let scanner = Scanner::new(event_bus, false);

    // Scan music directory
    let cli_arg = std::env::args().nth(1);
    let home = std::env::var("HOME").ok();
    let music_dir = resolve_music_dir(cli_arg, home.as_deref());

    println!("Scanning: {music_dir}");

    let stats = scanner.scan_directory(&db, Path::new(&music_dir))?;

    println!("\nScan Statistics:");
    println!("  Files scanned: {}", stats.scanned);
    println!("  Files added: {}", stats.added);
    println!("  Files updated: {}", stats.updated);
    println!("  Files removed: {}", stats.removed);
    println!("  Errors: {}", stats.errors);

    // Show database stats
    println!("\nDatabase Statistics:");
    println!("  Total songs: {}", db.count_songs()?);
    println!("  Total artists: {}", db.count_artists()?);
    println!("  Total albums: {}", db.count_albums()?);

    // List first 5 songs
    let songs = db.list_all_songs()?;
    println!("\nFirst 5 songs:");
    for song in songs.iter().take(5) {
        println!(
            "  {} - {} - {}",
            song.display_artist(),
            song.display_album(),
            song.display_title()
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_music_dir_uses_home_when_present() {
        assert_eq!(default_music_dir(Some("/home/alice")), "/home/alice/Music");
    }

    #[test]
    fn default_music_dir_falls_back_to_dot_when_home_missing() {
        assert_eq!(default_music_dir(None), "./Music");
    }

    #[test]
    fn resolve_music_dir_prefers_cli_arg() {
        let resolved = resolve_music_dir(Some("/mnt/music".to_string()), Some("/home/alice"));
        assert_eq!(resolved, "/mnt/music");
    }

    #[test]
    fn resolve_music_dir_uses_home_when_arg_missing() {
        let resolved = resolve_music_dir(None, Some("/home/alice"));
        assert_eq!(resolved, "/home/alice/Music");
    }

    #[test]
    fn resolve_music_dir_uses_dot_when_arg_and_home_missing() {
        let resolved = resolve_music_dir(None, None);
        assert_eq!(resolved, "./Music");
    }
}
