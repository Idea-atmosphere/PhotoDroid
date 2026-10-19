//! A diagnostic log: short, timestamped lines that the editing code writes where a change is applied (a text
//! edit, a text layer's re-render, the canvas's repaint, an effect's key). The newest [`CAPACITY`] lines are kept
//! in memory for the user to save. It is off unless a build turns it on (the phone build does), so the desktop
//! app records nothing.
//!
//! The phone's Help › Save Diagnostic Log writes [`dump`] to `Downloads/PhotoCraft/photocraft-log.txt`.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::Instant;

/// Most lines kept; the oldest go first.
pub const CAPACITY: usize = 8000;
/// Slots remembered by [`note_changed`]; the table starts again past this many.
const SLOTS: usize = 2048;

static ENABLED: AtomicBool = AtomicBool::new(false);
static TEXT_EDIT: AtomicBool = AtomicBool::new(false);
static DIRTY: AtomicBool = AtomicBool::new(false);
static START: OnceLock<Instant> = OnceLock::new();
static LINES: Mutex<VecDeque<String>> = Mutex::new(VecDeque::new());
static KEYS: Mutex<Option<HashMap<u64, u64>>> = Mutex::new(None);

fn locked<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Turns the log on or off. Lines written while it is off are not kept.
pub fn set_enabled(on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
}

/// Whether lines are kept.
pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// Adds a line, stamped with the seconds since the log started, when the log is on. `line` runs only then. The log
/// is then due to be saved (see [`take_dirty`]).
pub fn note(line: impl FnOnce() -> String) {
    note_as(line, true);
}

/// Adds a line like [`note`], but does not make the log due to be saved. A line about the saving itself, or about
/// frame timing, uses this: a save that marks the log would start the next save, and so on.
pub fn note_quiet(line: impl FnOnce() -> String) {
    note_as(line, false);
}

fn note_as(line: impl FnOnce() -> String, saves: bool) {
    if !enabled() {
        return;
    }
    let stamp = START.get_or_init(Instant::now).elapsed().as_secs_f64();
    let text = format!("{stamp:>10.3}s {}", line());
    let mut lines = locked(&LINES);
    if lines.len() >= CAPACITY {
        lines.pop_front();
    }
    lines.push_back(text);
    if saves {
        DIRTY.store(true, Ordering::Relaxed);
    }
}

/// Adds a line only when `key` differs from the key last noted for `slot`. A state that repeats (a render after
/// render, with the same effect) is noted once, and a change to it is noted again.
pub fn note_changed(slot: u64, key: u64, line: impl FnOnce() -> String) {
    if !enabled() {
        return;
    }
    {
        let mut keys = locked(&KEYS);
        let map = keys.get_or_insert_with(HashMap::new);
        if map.get(&slot) == Some(&key) {
            return;
        }
        if map.len() >= SLOTS {
            map.clear();
        }
        map.insert(slot, key);
    }
    note(line);
}

/// Whether lines were added since the last call. The phone saves the log when they were (see `menus.rs`).
pub fn take_dirty() -> bool {
    DIRTY.swap(false, Ordering::Relaxed)
}

/// Marks the log as changed again, for a save that was not due yet.
pub fn mark_dirty() {
    DIRTY.store(true, Ordering::Relaxed);
}

/// Marks that a text edit was applied. The canvas notes its repaint for the edit once (see [`take_text_edit`]).
pub fn mark_text_edit() {
    if enabled() {
        TEXT_EDIT.store(true, Ordering::Relaxed);
    }
}

/// Whether a text edit was marked since the last call. Clears the mark.
pub fn take_text_edit() -> bool {
    TEXT_EDIT.swap(false, Ordering::Relaxed)
}

/// The kept lines, oldest first, under a header that names the platform.
pub fn dump() -> String {
    render(&locked(&LINES))
}

fn render(lines: &VecDeque<String>) -> String {
    let mut out = format!(
        "PhotoCraft diagnostic log: {} lines, oldest at the top ({} {})\n",
        lines.len(),
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    for line in lines.iter() {
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// Notes a panic and returns the log, for a panic hook. It never waits for the lock: the panic may come from a
/// thread that holds it, and a locked log gives `None`. The message's line breaks become spaces, so each panic is
/// one line.
pub fn record_panic(message: &str) -> Option<String> {
    let mut lines = match LINES.try_lock() {
        Ok(guard) => guard,
        Err(std::sync::TryLockError::Poisoned(e)) => e.into_inner(),
        Err(std::sync::TryLockError::WouldBlock) => return None,
    };
    let stamp = START.get_or_init(Instant::now).elapsed().as_secs_f64();
    if lines.len() >= CAPACITY {
        lines.pop_front();
    }
    lines.push_back(format!("{stamp:>10.3}s PANIC: {}", message.replace('\n', " | ")));
    DIRTY.store(true, Ordering::Relaxed);
    Some(render(&lines))
}

/// Drops the kept lines and the remembered keys.
pub fn clear() {
    locked(&LINES).clear();
    let mut keys = locked(&KEYS);
    if let Some(map) = keys.as_mut() {
        map.clear();
    }
    DIRTY.store(false, Ordering::Relaxed);
    TEXT_EDIT.store(false, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    // One test owns the log's global state, so the tests of this module do not race over it.
    #[test]
    fn the_log_keeps_the_newest_lines_while_it_is_on_and_notes_a_repeated_key_once() {
        set_enabled(false);
        clear();
        note(|| "not kept".into());
        assert!(!dump().contains("not kept"), "a log that is off keeps nothing");
        set_enabled(true);
        note(|| "first".into());
        note_changed(7, 1, || "key one".into());
        note_changed(7, 1, || "key one repeated".into());
        note_changed(7, 2, || "key two".into());
        let text = dump();
        assert!(text.contains("first") && text.contains("key two"), "{text}");
        assert!(!text.contains("key one repeated"), "a repeated key is not noted again: {text}");
        assert_eq!(text.matches("key one").count(), 1, "{text}");
        for i in 0..CAPACITY + 10 {
            note(|| format!("line {i}"));
        }
        let text = dump();
        assert!(!text.contains("first"), "the oldest line went first");
        assert!(text.contains(&format!("line {}", CAPACITY + 9)), "the newest line is kept");
        assert_eq!(text.lines().count(), CAPACITY + 1, "the header and the kept lines");
        set_enabled(true);
        assert!(take_dirty(), "the notes made the log dirty");
        assert!(!take_dirty(), "and taking it clears the mark");
        note_quiet(|| "a quiet line".into());
        assert!(!take_dirty(), "a quiet line does not make the log due to be saved");
        assert!(dump().contains("a quiet line"), "a quiet line is kept");
        mark_dirty();
        assert!(take_dirty(), "a mark is kept until it is taken");
        assert!(take_text_edit() == false, "no edit is marked yet");
        mark_text_edit();
        assert!(take_text_edit(), "a marked edit is taken once");
        assert!(!take_text_edit());
        let text = record_panic("boom\nsecond line").unwrap_or_default();
        assert!(text.contains("PANIC: boom | second line"), "a panic is one line in the log: {text}");
        assert!(take_dirty(), "a panic makes the log dirty");
        set_enabled(false);
        clear();
    }
}
