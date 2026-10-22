//! Removes folders under the data root that no job references: leftovers from
//! crashes, moved jobs and the old `batch_N/job_NNN` layout. Paths are resolved
//! and must stay under the root, so a symlink can't aim the sweep elsewhere.

use std::path::{Path, PathBuf};

pub fn dead_folders(root: &Path, job_dirs: &[PathBuf]) -> Vec<PathBuf> {
    let mut dead = Vec::new();
    let mut alive = false;
    walk(root, job_dirs, &mut alive, &mut dead, 0);
    dead
}

// A folder is alive if it or anything below it belongs to a job. Recursive
// because the old layout nested jobs; depth-capped to keep startup fast.
fn walk(
    dir: &Path,
    job_dirs: &[PathBuf],
    alive: &mut bool,
    dead: &mut Vec<PathBuf>,
    depth: usize,
) {
    const MAX_DEPTH: usize = 4;
    if depth > MAX_DEPTH {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() || path == dir {
            continue;
        }
        if job_dirs.iter().any(|j| j == &path || j.starts_with(&path)) {
            *alive = true;
            continue;
        }
        let mut child_alive = false;
        let mut dead_children = Vec::new();
        walk(&path, job_dirs, &mut child_alive, &mut dead_children, depth + 1);
        if child_alive {
            *alive = true;
            dead.extend(dead_children);
        } else {
            dead.push(path);
            dead.extend(dead_children);
        }
    }
}

/// Best effort: a folder that can't be removed is skipped, not an error.
pub fn collect(root: &Path, job_dirs: &[PathBuf]) -> Vec<PathBuf> {
    if !root.is_dir() {
        return Vec::new();
    }
    let mut removed = Vec::new();
    // Compare resolved paths only: on macOS `/tmp` -> `/private/tmp`, and a
    // mixed comparison would make every live job look dead.
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let resolved_jobs: Vec<PathBuf> = job_dirs
        .iter()
        .map(|d| d.canonicalize().unwrap_or_else(|_| d.clone()))
        .collect();
    for dir in dead_folders(&root, &resolved_jobs) {
        // Re-check right before deleting.
        let canonical = dir.canonicalize().unwrap_or(dir.clone());
        if !canonical.starts_with(&root) || canonical == root {
            continue;
        }
        match std::fs::remove_dir_all(&canonical) {
            Ok(()) => removed.push(canonical),
            Err(_) => continue,
        }
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "subtitler-gc-{tag}-{}-{}",
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
    fn a_live_job_folder_is_kept() {
        let root = scratch("live");
        let job = root.join("film");
        std::fs::create_dir_all(&job).unwrap();
        std::fs::write(job.join("tr.srt"), b"x").unwrap();
        let removed = collect(&root, &[job.clone()]);
        assert!(removed.is_empty(), "canlı iş klasörü silinmemeli");
        assert!(job.join("tr.srt").exists());
    }

    #[test]
    fn an_orphaned_folder_is_reclaimed() {
        let root = scratch("orphan");
        let orphan = root.join("batch_6/job_000");
        std::fs::create_dir_all(&orphan).unwrap();
        let removed = collect(&root, &[]);
        assert_eq!(removed.len(), 1);
        assert!(!orphan.exists(), "sahipsiz klasör toplanmalı");
    }

    #[test]
    fn hidden_working_directories_go_with_their_job() {
        let root = scratch("hidden");
        let job = root.join("film");
        std::fs::create_dir_all(job.join(".sub0")).unwrap();
        std::fs::write(job.join(".sub0/words.json"), b"{}").unwrap();
        let removed = collect(&root, &[]);
        assert_eq!(removed.len(), 1);
        assert!(!job.exists(), "gizli çalışma klasörü de toplanmalı");
    }

    // Old `batch_N` parents used to pile up once all their jobs were gone.
    #[test]
    fn empty_parent_folders_are_reclaimed_too() {
        let root = scratch("parent");
        let batch = root.join("batch_1");
        std::fs::create_dir_all(batch.join("job_000")).unwrap();
        let removed = collect(&root, &[]);
        assert!(removed.iter().any(|p| p.ends_with("batch_1")));
        assert!(!batch.exists());
    }

    #[test]
    fn the_root_itself_is_never_removed() {
        let root = scratch("root-guard");
        std::fs::create_dir_all(root.join("leftover")).unwrap();
        let removed = collect(&root, &[]);
        assert!(!removed.iter().any(|p| p == &root), "kök silinmemeli");
        assert!(root.exists());
    }
}
