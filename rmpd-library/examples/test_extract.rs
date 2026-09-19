use camino::Utf8PathBuf;
use rmpd_library::metadata::MetadataExtractor;
use std::ffi::OsString;

const USAGE: &str =
    "cargo run -p rmpd-library --example test_extract -- <audio-file> [<audio-file> ...]";

/// Manual helper: extract and print metadata for one or more audio files.
///
/// Usage: `cargo run -p rmpd-library --example test_extract -- <file> [<file> ...]`
fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let files = parse_input_paths(std::env::args_os().skip(1))?;
    if files.is_empty() {
        return Err(format!("usage: {USAGE}"));
    }

    let mut failures = 0usize;

    for path in files {
        println!("Testing extraction from: {path}");
        match MetadataExtractor::extract_from_file(&path) {
            Ok(song) => {
                println!("  Title: {}", opt_display(song.tag("title")));
                println!("  Artist: {}", opt_display(song.tag("artist")));
                println!("  Album: {}", opt_display(song.tag("album")));
                println!("  Date: {}", opt_display(song.tag("date")));
                println!("  Genre: {}", opt_display(song.tag("genre")));
                println!("  Sample Rate: {}", opt_display(song.sample_rate));
                println!("  Channels: {}", opt_display(song.channels));
                println!("  Bits Per Sample: {}", opt_display(song.bits_per_sample));
                println!("  Duration: {}", opt_debug(song.duration));
                println!(
                    "  MusicBrainz TrackID: {}",
                    opt_display(song.tag("musicbrainz_trackid"))
                );
            }
            Err(e) => {
                failures += 1;
                eprintln!("  Error extracting metadata: {e}");
            }
        }
        println!("\n---\n");
    }

    if failures > 0 {
        return Err(format!(
            "metadata extraction failed for {failures} input file(s)"
        ));
    }

    Ok(())
}

fn parse_input_paths<I>(args: I) -> Result<Vec<Utf8PathBuf>, String>
where
    I: IntoIterator<Item = OsString>,
{
    args.into_iter()
        .map(|arg| {
            let path = std::path::PathBuf::from(arg);
            Utf8PathBuf::from_path_buf(path)
                .map_err(|non_utf8| format!("non-UTF-8 path argument: {}", non_utf8.display()))
        })
        .collect()
}

fn opt_display<T: std::fmt::Display>(value: Option<T>) -> String {
    value
        .map(|v| v.to_string())
        .unwrap_or_else(|| "<missing>".to_string())
}

fn opt_debug<T: std::fmt::Debug>(value: Option<T>) -> String {
    value
        .map(|v| format!("{v:?}"))
        .unwrap_or_else(|| "<missing>".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_input_paths_accepts_utf8() {
        let args = vec![OsString::from("/tmp/a.flac"), OsString::from("b.mp3")];
        let parsed = parse_input_paths(args).expect("UTF-8 args should parse");
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0], "/tmp/a.flac");
        assert_eq!(parsed[1], "b.mp3");
    }

    #[cfg(unix)]
    #[test]
    fn parse_input_paths_rejects_non_utf8() {
        use std::os::unix::ffi::OsStringExt;

        let bad = OsString::from_vec(vec![0x66, 0x6f, 0x80]);
        let err = parse_input_paths(vec![bad]).expect_err("non-UTF-8 arg must fail");
        assert!(err.to_string().contains("non-UTF-8 path argument"));
    }

    #[test]
    fn opt_display_formats_option() {
        assert_eq!(opt_display(Some(42u32)), "42");
        assert_eq!(opt_display::<u32>(None), "<missing>");
    }

    #[test]
    fn opt_debug_formats_option() {
        assert_eq!(opt_debug(Some(std::time::Duration::from_secs(2))), "2s");
        assert_eq!(opt_debug::<u32>(None), "<missing>");
    }
}
