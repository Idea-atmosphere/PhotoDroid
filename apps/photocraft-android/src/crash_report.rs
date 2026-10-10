//! Keeps the diagnostic log through a crash. The log lives in memory, and the Help menu (and a periodic save) writes it
//! to `Downloads/PhotoCraft/photocraft-log.txt`. A panic or a kill can come before that write, so the log is also
//! copied to app-private storage each time it is saved, and at a panic. A run marks itself as running at launch and
//! clears the mark when the app is dropped. A launch that finds the mark has no copy of a run that ended normally, so
//! the last copy is saved to Downloads under a name that says the run ended early.
//!
//! These are plain file operations, so they run under `cargo test` on any host.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

/// The name the log has in Downloads (the Help menu's Save Diagnostic Log).
pub const LOG_NAME: &str = "photocraft-log.txt";
/// The copy of the log in app-private storage.
const LAST_COPY: &str = "last-log.txt";
/// Present while a run is going; a launch that finds it did not see the run end.
const RUNNING: &str = "running";

static ROOT: OnceLock<PathBuf> = OnceLock::new();

/// Sets the app-private storage the copies go to. Only the first call counts.
pub fn set_root(root: &Path) {
    let _ = ROOT.set(root.to_path_buf());
}

/// Keeps a copy of the log (the bytes the Help menu writes), once the root is set.
pub fn keep_copy(bytes: &[u8]) {
    if let Some(root) = ROOT.get() {
        keep_copy_in(root, bytes);
    }
}

fn keep_copy_in(root: &Path, bytes: &[u8]) {
    let _ = std::fs::write(root.join(LAST_COPY), bytes);
}

/// At launch: the copy the last run kept, when that run did not end normally, with the name to save it as. The copy is
/// removed either way. Marks this run as running (see [`finish`]).
pub fn begin(root: &Path) -> Option<(String, Vec<u8>)> {
    let unclean = root.join(RUNNING).exists();
    let copy = root.join(LAST_COPY);
    let saved = if unclean { std::fs::read(&copy).ok().map(|bytes| (unclean_name(unix_seconds()), bytes)) } else { None };
    let _ = std::fs::remove_file(&copy);
    let _ = std::fs::write(root.join(RUNNING), b"");
    saved
}

/// When the app is dropped at the end of a run: clears the running mark.
pub fn finish(root: &Path) {
    let _ = std::fs::remove_file(root.join(RUNNING));
}

/// The name a log from a run that did not end normally is saved as in Downloads.
pub fn unclean_name(seconds: u64) -> String {
    format!("photocraft-log-unclean-{seconds}.txt")
}

fn unix_seconds() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh directory for one test, under the system's temporary directory.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("photocraft-crash-report-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("a temporary directory");
        dir
    }

    #[test]
    fn a_first_launch_has_nothing_to_save_and_marks_the_run() {
        let dir = scratch("first");
        assert!(begin(&dir).is_none());
        assert!(dir.join(RUNNING).exists(), "this run is marked as running");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_run_that_did_not_finish_keeps_its_log_for_the_next_launch() {
        let dir = scratch("unclean");
        assert!(begin(&dir).is_none());
        keep_copy_in(&dir, b"PhotoCraft diagnostic log\nopen a.psd: decoded\n");
        // No `finish`: the run ended some other way (a crash, or the process was killed).
        let (name, bytes) = begin(&dir).expect("the last run's log");
        assert!(name.starts_with("photocraft-log-unclean-") && name.ends_with(".txt"), "{name}");
        assert_eq!(bytes, b"PhotoCraft diagnostic log\nopen a.psd: decoded\n");
        assert!(!dir.join(LAST_COPY).exists(), "the copy is taken once");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_run_that_finished_leaves_nothing_to_save() {
        let dir = scratch("finished");
        assert!(begin(&dir).is_none());
        keep_copy_in(&dir, b"the log\n");
        finish(&dir);
        assert!(begin(&dir).is_none(), "a clean end is not reported");
        assert!(!dir.join(LAST_COPY).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_unclean_name_has_the_time() {
        assert_eq!(unclean_name(1234), "photocraft-log-unclean-1234.txt");
    }
}
