//! lib entry: modules plus a small stdlib-only IO layer used by main.

pub mod plan;

use std::path::{Path, PathBuf};

/// Expands the input list: paths given verbatim, or (when empty) one path
/// per line from stdin.
pub fn gather_inputs(explicit: &[PathBuf], stdin_lines: Vec<String>) -> Vec<PathBuf> {
    if !explicit.is_empty() {
        return explicit.to_vec();
    }
    stdin_lines
        .into_iter()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .map(PathBuf::from)
        .collect()
}

/// Applies the plan. Targets must not exist at execution time — this is
/// re-checked here, right before each rename.
pub fn apply(renames: &[plan::Rename]) -> Result<usize, (PathBuf, String)> {
    let mut done = 0;
    for r in renames {
        if r.to.exists() {
            return Err((r.to.clone(), "target appeared during execution".into()));
        }
        if let Err(e) = std::fs::rename(&r.from, &r.to) {
            return Err((r.from.clone(), e.to_string()));
        }
        done += 1;
    }
    Ok(done)
}

/// True when `path` exists on disk (files and directories).
pub fn exists_on_disk(path: &Path) -> bool {
    path.symlink_metadata().is_ok()
}
