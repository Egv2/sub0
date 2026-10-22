//! Job folder names, derived from the source and safe on every OS.
//! Shared by the app and the CLI so the two can't drift.

use std::path::{Path, PathBuf};

// Well under the usual 255-byte limit, leaving room for a " (n)" suffix.
const MAX_NAME_CHARS: usize = 100;

const FALLBACK: &str = "video";

// Windows refuses these, with or without an extension.
const RESERVED: &[&str] = &[
    "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// File stem for paths; last path segment for URLs, or the host if there is none.
pub fn display_name(source_ref: &str) -> String {
    let trimmed = source_ref.trim();
    if trimmed.is_empty() {
        return FALLBACK.to_string();
    }
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        return url_name(trimmed);
    }
    Path::new(trimmed)
        .file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string())
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| FALLBACK.to_string())
}

fn url_name(url: &str) -> String {
    let rest = url
        .split_once("://")
        .map(|(_, r)| r)
        .unwrap_or(url);
    let without_query = rest.split(['?', '#']).next().unwrap_or(rest);
    let (host, path_part) = match without_query.split_once('/') {
        Some((h, p)) => (h, p),
        None => (without_query, ""),
    };
    let last_segment = path_part
        .rsplit('/')
        .find(|s| !s.is_empty() && *s != "..")
        .unwrap_or("");
    if last_segment.is_empty() {
        // A host isn't a filename, so don't strip an "extension" from it.
        let host = percent_decode(host);
        return if host.trim().is_empty() {
            FALLBACK.to_string()
        } else {
            host
        };
    }
    let decoded = percent_decode(last_segment);
    if decoded.trim().is_empty() {
        return FALLBACK.to_string();
    }
    Path::new(decoded.as_str())
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(&decoded)
        .to_string()
}

fn percent_decode(s: &str) -> String {
    if !s.contains('%') {
        return s.to_string();
    }
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = &s[i + 1..i + 3];
            if let Ok(byte) = u8::from_str_radix(hex, 16) {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| s.to_string())
}

pub fn sanitize(raw: &str) -> String {
    let mut cleaned = String::with_capacity(raw.len());
    for ch in raw.chars() {
        match ch {
            '/' | '\\' => cleaned.push(' '),
            // Illegal on Windows.
            ':' | '*' | '?' | '"' | '<' | '>' | '|' => cleaned.push(' '),
            c if c.is_control() => {}
            c => cleaned.push(c),
        }
    }

    // Windows silently strips trailing dots and spaces.
    let collapsed = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut name = collapsed
        .trim_end_matches(['.', ' '])
        .to_string();

    if name.is_empty() {
        return FALLBACK.to_string();
    }

    // Keep the tail: that's usually the distinctive part of a long name.
    if name.chars().count() > MAX_NAME_CHARS {
        let keep: String = name
            .chars()
            .rev()
            .take(MAX_NAME_CHARS)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        name = keep.trim_start().trim_end_matches(['.', ' ']).to_string();
    }
    if name.is_empty() {
        name = FALLBACK.to_string();
    }

    let stem = name.split('.').next().unwrap_or(&name).to_ascii_lowercase();
    if RESERVED.contains(&stem.as_str()) {
        name.push_str(" -");
    }
    name
}

/// Create and reserve a job folder under `root`. `create_dir` failing on an
/// existing name is the atomic collision check; later duplicates get `" (2)"` etc.
pub fn job_dir(root: &Path, source_ref: &str) -> std::io::Result<PathBuf> {
    let base = sanitize(&display_name(source_ref));
    std::fs::create_dir_all(root)?;

    let mut candidate = root.join(&base);
    if std::fs::create_dir(&candidate).is_ok() {
        return Ok(candidate);
    }

    for n in 2..1000 {
        candidate = root.join(format!("{base} ({n})"));
        match std::fs::create_dir(&candidate) {
            Ok(()) => return Ok(candidate),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    let unique = format!("{base} ({})", std::process::id());
    std::fs::create_dir_all(&unique)?;
    Ok(root.join(unique))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "subtitler-naming-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_file_name_becomes_the_folder_name() {
        assert_eq!(display_name("/Users/serin/videos/İzmir.mp4"), "İzmir");
        assert_eq!(display_name("relative/path/film.mkv"), "film");
        assert_eq!(display_name("no-extension"), "no-extension");
    }

    #[test]
    fn urls_use_their_last_segment_then_the_host() {
        assert_eq!(
            display_name("https://youtube.com/watch?v=abc"),
            "watch",
            "dosya uzantısı olmayan son segment"
        );
        assert_eq!(
            display_name("https://cdn.example.com/media/Talk%20Show.mp4"),
            "Talk Show",
            "yüzde kodlaması çözülmeli"
        );
        assert_eq!(display_name("https://example.com/"), "example.com");
        assert_eq!(display_name("https://example.com/a/b/"), "b");
    }

    #[test]
    fn separators_and_control_characters_never_create_nesting() {
        let name = sanitize("../../etc/passwd");
        assert!(!name.contains('/'), "klasör içi yol olamaz: {name}");
        assert!(!name.contains('\\'));
        let control = sanitize("video\u{0}name\u{1}");
        assert_eq!(control, "videoname");
    }

    #[test]
    fn windows_reserved_names_are_usable() {
        for reserved in ["con", "NUL", "aux"] {
            let name = sanitize(reserved);
            assert!(
                !RESERVED.contains(&name.to_ascii_lowercase().as_str()),
                "{reserved} olduğu gibi kalmamalı"
            );
        }
        let with_ext = sanitize("con.mp4");
        assert!(!with_ext.eq_ignore_ascii_case("con.mp4"));
    }

    #[test]
    fn trailing_dots_and_spaces_are_dropped() {
        assert_eq!(sanitize("video..."), "video");
        assert_eq!(sanitize("video   "), "video");
    }

    #[test]
    fn empty_and_useless_names_fall_back() {
        assert_eq!(sanitize(""), FALLBACK);
        assert_eq!(sanitize("   "), FALLBACK);
        assert_eq!(sanitize("///"), FALLBACK);
        assert_eq!(sanitize("\u{0}\u{1}\u{2}"), FALLBACK);
        assert_eq!(display_name(""), FALLBACK);
    }

    #[test]
    fn turkish_characters_survive() {
        assert_eq!(sanitize("İstanbul Göğüş Çırpınış"), "İstanbul Göğüş Çırpınış");
        assert_eq!(display_name("/v/ışık.mkv"), "ışık");
    }

    #[test]
    fn long_names_are_capped_but_keep_their_tail() {
        let long = format!("{}.mp4", "a".repeat(400));
        let name = sanitize(&long);
        assert!(name.chars().count() <= MAX_NAME_CHARS, "uzunluk sınırlı");
    }

    #[test]
    fn collisions_are_disambiguated() {
        let root = scratch("collide");
        let a = job_dir(&root, "/v/talk.mp4").unwrap();
        let b = job_dir(&root, "/other/talk.mp4").unwrap();
        let c = job_dir(&root, "/third/TALK.mp4").unwrap();
        assert_eq!(a.file_name().unwrap(), "talk");
        assert_ne!(a, b);
        assert_ne!(b, c);
        assert!(b.file_name().unwrap().to_string_lossy().contains("(2)"));
        assert!(c.file_name().unwrap().to_string_lossy().contains("(3)"));
    }

    #[test]
    fn the_result_always_stays_under_root() {
        let root = scratch("escape");
        for source in [
            "../../../etc",
            "/",
            "C:\\Windows\\System32",
            "https://example.com/..",
            "..",
        ] {
            let dir = job_dir(&root, source).unwrap();
            assert!(
                dir.starts_with(&root),
                "{source:?} root dışına çıktı: {dir:?}"
            );
        }
    }

    #[test]
    fn the_folder_is_created_and_reserved() {
        let root = scratch("reserve");
        let dir = job_dir(&root, "/v/clip.mp4").unwrap();
        assert!(dir.is_dir(), "klasör oluşturulmalı");
        let again = job_dir(&root, "/v/clip.mp4").unwrap();
        assert_ne!(dir, again, "ikinci çağrı aynı klasörü alamaz");
    }

    #[test]
    fn unicode_names_are_created_successfully() {
        let root = scratch("unicode");
        let dir = job_dir(&root, "/v/İstanbul Göğüş.mkv").unwrap();
        assert!(dir.is_dir());
        assert_eq!(dir.file_name().unwrap(), "İstanbul Göğüş");
    }
}
