use lofty::prelude::*;
use lofty::probe::Probe;
use std::ffi::OsString;

fn main() {
    if let Err(e) = run() {
        eprintln!("ERROR: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let Some(path) = parse_input_path(std::env::args_os().nth(1))? else {
        return Err("usage: test_m4a_artwork <path-to-m4a-file>".to_string());
    };

    if !is_m4a_path(&path) {
        return Err(format!(
            "expected an .m4a input file, got: {}",
            path.as_str()
        ));
    }

    println!("Testing M4A artwork extraction from: {path}");

    match Probe::open(&path) {
        Ok(probe) => {
            match probe.read() {
                Ok(tagged_file) => {
                    println!("OK: file opened successfully");
                    println!("  File type: {:?}", tagged_file.file_type());

                    // Check primary tag
                    if let Some(primary_tag) = tagged_file.primary_tag() {
                        println!("OK: primary tag found");
                        print_picture_details("primary tag", primary_tag.pictures());
                    } else {
                        println!("NOTE: no primary tag found");
                    }

                    // Check all tags
                    println!("\nChecking all tags:");
                    for tag in tagged_file.tags() {
                        println!("  Tag type: {:?}", tag.tag_type());
                        print_picture_details("this tag", tag.pictures());
                    }
                }
                Err(e) => {
                    return Err(format!("failed to read file: {e}"));
                }
            }
        }
        Err(e) => {
            return Err(format!("failed to open file: {e}"));
        }
    }

    Ok(())
}

fn parse_input_path(arg: Option<OsString>) -> Result<Option<camino::Utf8PathBuf>, String> {
    match arg {
        None => Ok(None),
        Some(arg) => {
            let path = std::path::PathBuf::from(arg);
            camino::Utf8PathBuf::from_path_buf(path)
                .map(Some)
                .map_err(|p| format!("path argument is not valid UTF-8: {}", p.display()))
        }
    }
}

fn is_m4a_path(path: &camino::Utf8PathBuf) -> bool {
    path.extension()
        .map(|ext| ext.eq_ignore_ascii_case("m4a"))
        .unwrap_or(false)
}

fn print_picture_details(label: &str, pictures: &[lofty::picture::Picture]) {
    println!("  Number of pictures in {label}: {}", pictures.len());
    for (i, pic) in pictures.iter().enumerate() {
        println!(
            "    Picture {}: type={:?}, mime={:?}, size={} bytes",
            i,
            pic.pic_type(),
            pic.mime_type(),
            pic.data().len()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_m4a_path_accepts_case_insensitive_extension() {
        assert!(is_m4a_path(&camino::Utf8PathBuf::from("song.m4a")));
        assert!(is_m4a_path(&camino::Utf8PathBuf::from("song.M4A")));
        assert!(!is_m4a_path(&camino::Utf8PathBuf::from("song.flac")));
        assert!(!is_m4a_path(&camino::Utf8PathBuf::from("song")));
    }

    #[test]
    fn parse_input_path_none_is_ok() {
        let parsed = parse_input_path(None).expect("None should not error");
        assert!(parsed.is_none());
    }

    #[test]
    fn parse_input_path_utf8_is_ok() {
        let parsed = parse_input_path(Some(OsString::from("/tmp/test.m4a")))
            .expect("UTF-8 path should parse")
            .expect("path should be present");
        assert_eq!(parsed, "/tmp/test.m4a");
    }

    #[cfg(unix)]
    #[test]
    fn parse_input_path_rejects_non_utf8() {
        use std::os::unix::ffi::OsStringExt;

        let bad = OsString::from_vec(vec![0x66, 0x6f, 0x80]);
        let err = parse_input_path(Some(bad)).expect_err("non-UTF-8 path must fail");
        assert!(err.contains("not valid UTF-8"));
    }
}
