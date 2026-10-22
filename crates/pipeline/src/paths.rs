//! Job directory layout: `out/` holds only the subtitles, `out/.sub0/` holds
//! the working state. Build every job path here rather than joining `out_dir`.

use std::path::{Path, PathBuf};

pub const WORK_DIR: &str = ".sub0";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobPaths {
    out: PathBuf,
    work: PathBuf,
}

impl JobPaths {
    pub fn new(out: impl Into<PathBuf>) -> Self {
        let out = out.into();
        let work = out.join(WORK_DIR);
        Self { out, work }
    }

    pub fn out(&self) -> &Path {
        &self.out
    }

    pub fn work(&self) -> &Path {
        &self.work
    }

    // Kept so re-entering a job doesn't re-run STT.

    pub fn words(&self) -> PathBuf {
        self.work.join("words.json")
    }

    pub fn source_track(&self) -> PathBuf {
        self.work.join("source.track.json")
    }

    pub fn meta(&self) -> PathBuf {
        self.work.join("meta.json")
    }

    pub fn translation_track(&self, lang: &str) -> PathBuf {
        self.work.join(format!("{lang}.track.json"))
    }

    // Diagnostics only.

    pub fn dropped_segments(&self) -> PathBuf {
        self.work.join("dropped-segments.json")
    }

    pub fn cleanup_deleted(&self) -> PathBuf {
        self.work.join("cleanup-deleted.json")
    }

    pub fn speech_ranges(&self) -> PathBuf {
        self.work.join("speech.json")
    }

    pub fn vad_dropped(&self) -> PathBuf {
        self.work.join("vad-dropped.json")
    }

    pub fn brief(&self) -> PathBuf {
        self.work.join("brief.json")
    }

    pub fn rules_deleted(&self) -> PathBuf {
        self.work.join("rules-deleted.json")
    }

    pub fn srt(&self, lang: &str) -> PathBuf {
        self.out.join(format!("{lang}.srt"))
    }

    pub fn temp_dir(&self) -> PathBuf {
        self.work.join("tmp")
    }

    pub fn temp_audio(&self) -> PathBuf {
        self.temp_dir().join("audio.flac")
    }

    pub fn temp_chunk(&self, start_ms: u32) -> PathBuf {
        self.temp_dir().join(format!("chunk_{start_ms:04}.flac"))
    }

    /// ffmpeg won't create parent dirs; it exits 254 on a missing `tmp/`.
    pub fn ensure_temp(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(self.temp_dir())
    }

    /// Nothing reads the audio after STT; everything downstream uses `words.json`.
    pub fn remove_temp(&self) -> std::io::Result<()> {
        match std::fs::remove_dir_all(self.temp_dir()) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        }
    }

    /// Move working files from the old flat layout into `.sub0/`. Never
    /// overwrites, and `.srt` files stay put.
    pub fn migrate_legacy(&self) -> std::io::Result<()> {
        let legacy = [
            "words.json",
            "source.track.json",
            "meta.json",
            "dropped-segments.json",
            "cleanup-deleted.json",
        ];
        if !self.out.is_dir() || self.work.is_dir() {
            return Ok(());
        }
        std::fs::create_dir_all(&self.work)?;
        for name in legacy {
            let from = self.out.join(name);
            let to = self.work.join(name);
            if from.is_file() && !to.exists() {
                std::fs::rename(&from, &to)?;
            }
        }
        for entry in std::fs::read_dir(&self.out)? {
            let entry = entry?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if !name.ends_with(".track.json") || name == "source.track.json" {
                continue;
            }
            let to = self.work.join(name);
            if !to.exists() {
                std::fs::rename(entry.path(), &to)?;
            }
        }
        for name in ["audio.flac"] {
            let from = self.out.join(name);
            if from.is_file() {
                std::fs::remove_file(from)?;
            }
        }
        std::fs::remove_dir_all(self.out.join("tmp")).ok();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "subtitler-paths-{tag}-{}-{}",
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
    fn only_subtitles_land_in_the_visible_folder() {
        let p = JobPaths::new(scratch("layout"));
        assert!(p.srt("tr").starts_with(p.out()), "SRT kullanıcıya görünür");
        assert!(p.srt("tr").ends_with("tr.srt"));
        assert!(p.words().starts_with(p.work()), "transcript gizli");
        assert!(p.source_track().starts_with(p.work()));
        assert!(p.translation_track("de").starts_with(p.work()));
        assert!(p.meta().starts_with(p.work()));
        assert!(p.temp_audio().starts_with(p.work()));
        assert!(p.dropped_segments().starts_with(p.work()));
        assert!(p.cleanup_deleted().starts_with(p.work()));
    }

    #[test]
    fn the_working_directory_is_hidden() {
        let p = JobPaths::new("/tmp/x");
        assert_eq!(
            p.work().file_name().and_then(|n| n.to_str()),
            Some(WORK_DIR),
            "gizli dizin nokta ile başlamalı"
        );
        assert!(WORK_DIR.starts_with('.'), "macOS/Linux gizli kuralı");
        // Directly under `out/`, so deleting the job removes it too.
        assert_eq!(p.work().parent(), Some(p.out()));
    }

    #[test]
    fn temp_audio_is_removed_but_canonical_files_survive() {
        let root = scratch("temp");
        let p = JobPaths::new(&root);
        std::fs::create_dir_all(p.temp_dir()).unwrap();
        std::fs::write(p.temp_audio(), b"30mb of audio").unwrap();
        std::fs::write(p.temp_chunk(0), b"chunk").unwrap();
        std::fs::write(p.words(), b"{\"words\":[]}").unwrap();

        p.remove_temp().unwrap();

        assert!(!p.temp_audio().exists(), "ses geçici dosyası silinmeli");
        assert!(!p.temp_chunk(0).exists(), "chunk silinmeli");
        assert!(p.words().exists(), "transcript kalmalı (yeniden giriş için)");
        p.remove_temp().unwrap();
    }

    // Regression: nothing created `tmp/`, so ffmpeg failed every fresh job with exit 254.
    #[test]
    fn the_temp_directory_exists_before_ffmpeg_writes_to_it() {
        let p = JobPaths::new(scratch("ensure-temp"));
        assert!(!p.temp_dir().exists(), "tmp henüz oluşmamalı");

        p.ensure_temp().unwrap();

        assert!(p.temp_dir().is_dir());
        assert_eq!(p.temp_audio().parent(), Some(p.temp_dir().as_path()));
        p.ensure_temp().unwrap();
    }

    #[test]
    fn migration_moves_working_files_and_keeps_subtitles() {
        let root = scratch("migrate");
        std::fs::write(root.join("words.json"), b"{}").unwrap();
        std::fs::write(root.join("source.track.json"), b"{}").unwrap();
        std::fs::write(root.join("meta.json"), b"{}").unwrap();
        std::fs::write(root.join("tr.track.json"), b"{}").unwrap();
        std::fs::write(root.join("audio.flac"), b"audio").unwrap();
        std::fs::write(root.join("tr.srt"), b"subtitle").unwrap();

        let p = JobPaths::new(&root);
        p.migrate_legacy().unwrap();

        assert!(p.words().exists(), "words gizliye taşınmalı");
        assert!(p.source_track().exists());
        assert!(p.meta().exists());
        assert!(p.translation_track("tr").exists(), "çeviri track'i taşınmalı");
        assert!(!root.join("audio.flac").exists(), "eski ses dosyası silinmeli");
        assert!(root.join("tr.srt").exists(), "altyazı kullanıcıda kalmalı");

        let visible: Vec<String> = std::fs::read_dir(&root)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| !n.starts_with('.'))
            .collect();
        assert_eq!(
            visible,
            vec!["tr.srt".to_string()],
            "kullanıcı yalnızca altyazı görmeli"
        );
    }

    #[test]
    fn migration_is_idempotent() {
        let root = scratch("migrate-twice");
        std::fs::write(root.join("words.json"), b"{\"v\":1}").unwrap();
        let p = JobPaths::new(&root);
        p.migrate_legacy().unwrap();
        p.migrate_legacy().unwrap();
        assert_eq!(
            std::fs::read_to_string(p.words()).unwrap(),
            "{\"v\":1}",
            "ikinci çalıştırma içeriği bozmamalı"
        );
    }
}
