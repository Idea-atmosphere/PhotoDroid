//! The platform services the shared UI calls (`photocraft_ui_egui::Services`), for Android.
//!
//! Files come in through the system file picker: the activity hands each picked file over (as a
//! descriptor, or as a copy in app cache) and a reader thread puts its bytes in [`Link::inbox`],
//! for the UI to open. The UI thread never reads a file itself. Saves go to `Downloads/PhotoCraft`
//! through MediaStore (see `PhotoCraftActivity.save`). Preferences, crash recovery and brush
//! presets live in app-private storage. Anything that needs a desktop facility is left `None`, so
//! the UI hides or skips it, as it does on the web.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use photocraft_codecs::{ChannelLayout, EncodeOptions, Image};
use photocraft_doc::Document;
use photocraft_format::RecoveryStore;
use photocraft_ui_egui::{ExportSettings, FileDialogAnswer, FileDialogFn, FileDialogReply, FileDialogRequest, Inbox, Recoverable, Services};

use crate::platform;
use crate::protocol;

/// App-private storage: the files directory the system gives the app, and what lives in it.
#[derive(Clone)]
pub struct Dirs {
    root: PathBuf,
}

impl Dirs {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn presets(&self) -> PathBuf {
        self.root.join("Presets")
    }

    fn preferences(&self) -> PathBuf {
        self.root.join("preferences.json")
    }

    fn recovery(&self) -> PathBuf {
        self.root.join("Recovery")
    }

    fn staging(&self) -> PathBuf {
        self.root.join("staging")
    }

    fn export_index(&self) -> PathBuf {
        self.root.join("exports.txt")
    }
}

/// A message for the user, shown as a notice on the next frame.
pub struct Message {
    pub error: bool,
    pub text: String,
}

/// What the services, the shell and the reader threads share. The reader threads can't touch the
/// UI, so everything they hand over is locked, and `ctx` wakes the UI when there is something new.
#[derive(Clone)]
pub struct Link {
    /// Files read from the picker, drained by the UI each frame (see `PhotocraftApp::logic`).
    pub inbox: Inbox,
    messages: Arc<Mutex<Vec<Message>>>,
    picking: Arc<AtomicBool>,
    ctx: egui::Context,
}

impl Link {
    pub fn new(ctx: egui::Context) -> Self {
        Self { inbox: Inbox::default(), messages: Arc::default(), picking: Arc::default(), ctx }
    }

    pub fn notify(&self, error: bool, text: impl Into<String>) {
        locked(&self.messages).push(Message { error, text: text.into() });
        self.ctx.request_repaint();
    }

    /// A file has been read: it goes to the UI's inbox.
    pub fn deliver(&self, name: String, bytes: Vec<u8>) {
        locked(&self.inbox).push((name, bytes));
        self.ctx.request_repaint();
    }

    pub fn take_messages(&self) -> Vec<Message> {
        std::mem::take(&mut *locked(&self.messages))
    }

    /// The system picker is open, so its files and its end are still to come.
    pub fn is_picking(&self) -> bool {
        self.picking.load(Ordering::Relaxed)
    }

    pub fn set_picking(&self, picking: bool) {
        self.picking.store(picking, Ordering::Relaxed);
    }

    fn start_picker(&self) {
        self.set_picking(true);
        if let Err(e) = platform::call("open", "").and_then(|reply| protocol::parse_reply(&reply)) {
            self.set_picking(false);
            self.notify(true, format!("The file picker could not be opened: {e}"));
        }
    }
}

/// Lock a queue; a thread that panicked while holding it leaves the queue usable.
fn locked<T>(queue: &Mutex<T>) -> MutexGuard<'_, T> {
    queue.lock().unwrap_or_else(|e| e.into_inner())
}

/// The file dialog service (`Services::file_dialog`). Save has no dialog: the suggested name is
/// used, in Downloads/PhotoCraft. File › Open starts the system picker, and the files it picks come
/// back through the inbox to be opened there, so the dialog is answered as cancelled at once. A
/// command that reads one file (place, scripts, presets, notes) would otherwise open it as a new
/// document, so it gets a message instead and is cancelled.
fn file_dialog(link: Link) -> FileDialogFn {
    Box::new(move |request: FileDialogRequest, _parent: Option<&eframe::Frame>, reply: FileDialogReply| match request {
        FileDialogRequest::Save { suggested } => reply.send(Some(FileDialogAnswer::SaveTo(protocol::file_name(&suggested)))),
        FileDialogRequest::Open { multiple: true, .. } => {
            link.start_picker();
            reply.send(None);
        }
        FileDialogRequest::Open { multiple: false, .. } | FileDialogRequest::Folder { .. } => {
            link.notify(false, "Placing files, scripts, presets and notes isn't available on Android yet. Use File › Open.");
            reply.send(None);
        }
    })
}

/// Builds the services for one launch. `link` is the same one the shell polls.
pub fn services(dirs: &Dirs, link: &Link) -> Services {
    let store: SharedRecovery = Rc::new(RefCell::new(Some(RecoveryStore::new(dirs.recovery()))));
    let (autosave_store, results_store, discard_store, adopt_store) = (store.clone(), store.clone(), store.clone(), store);
    let recovery_dir = dirs.recovery();
    let saver = Rc::new(Saver::new(dirs.staging(), dirs.export_index()));
    let saver_for_debug = saver.clone();
    let (debug_root, cpu_root) = (dirs.root().to_path_buf(), dirs.root().to_path_buf());
    let (prefs_load, prefs_save) = (dirs.preferences(), dirs.preferences());
    Services {
        import: Some(Box::new(|name: &str, bytes: &[u8], opts: &photocraft_io::ImportOptions| {
            crate::crash_guard::guard("Open", || {
                photocraft_io::import_with_options(name, bytes, opts).map(|r| (r.document, r.warnings)).map_err(|e| e.to_string())
            })
        })),
        export: Some(Box::new(export_document)),
        file_dialog: Some(file_dialog(link.clone())),
        write: Some(Box::new(move |path: &str, bytes: &[u8]| {
            // The diagnostic log keeps a copy in app-private storage too, for a crash (see `crash_report`).
            if path == crate::crash_report::LOG_NAME && photocraft_doc::diag::enabled() {
                crate::crash_report::keep_copy(bytes);
            }
            saver.save(path, bytes)
        })),
        set_debug_log: Some(Box::new(move |enabled: bool| {
            crate::crash_report::set_debug_enabled(&debug_root, enabled);
            if !enabled {
                saver_for_debug.remove_logs()?;
            }
            Ok(())
        })),
        set_cpu_cores: Some(Box::new(move |cores: usize| {
            crate::crash_report::save_cpu_cores(&cpu_root, cores);
            Ok(())
        })),
        encode_png: Some(Box::new(encode_png)),
        open_url: Some(Box::new(open_url)),
        // The activity makes the request explicit (see `softKeyboard` there); a failed request
        // leaves the keyboard as it is.
        soft_keyboard: Some(Box::new(|show: bool| {
            let _ = platform::call("keyboard", if show { "show" } else { "hide" });
        })),
        inbox: Some(link.inbox.clone()),
        load_prefs: Some(Box::new(move || std::fs::read_to_string(&prefs_load).ok())),
        save_prefs: Some(Box::new(move |text: &str| write_atomic(&prefs_save, text.as_bytes()))),
        append_text: Some(Box::new(append_text)),
        // Queued, then written in the background: the queue's refusal is the error returned here, and the
        // outcome of each write comes back through `autosave_results`, which the app polls every frame.
        autosave: Some(Box::new(move |doc: &Arc<Document>, revision: u64, path: Option<&str>| {
            with_store(&autosave_store, |s| s.autosave_checked(doc, revision, path.map(str::to_string))).and_then(|queued| queued)
        })),
        autosave_results: Some(Box::new(move || with_store(&results_store, |s| s.take_completed()).unwrap_or_default())),
        discard_autosave: Some(Box::new(move |id: u64| {
            let _ = with_store(&discard_store, |s| s.discard(id));
        })),
        // Listed now; each document is read only when the user opens it (the shell runs `load` on a worker).
        recover: Some(Box::new(move || {
            photocraft_format::list_recovery(&recovery_dir)
                .into_iter()
                .map(|e| Recoverable {
                    key: e.info.key.clone(),
                    name: e.info.document_name.clone(),
                    path: e.info.original_path.clone(),
                    load: Box::new(move || photocraft_format::recover(&e).map_err(|err| err.to_string())),
                })
                .collect()
        })),
        adopt_autosave: Some(Box::new(move |id: u64, key: &str| {
            let _ = with_store(&adopt_store, |s| s.adopt(id, key));
        })),
        // Clipboard images, OS events, displays, automation and the desktop-only file dialogs
        // (open-paths-as-arguments, rfd's save dialog) are not used here: the defaults are None.
        ..Default::default()
    }
}

fn export_document(doc: &Document, path: &str, settings: &ExportSettings) -> Result<(Vec<u8>, Vec<String>), String> {
    let opts = settings.io_options();
    crate::crash_guard::guard("Export", || photocraft_io::export(doc, path, &opts).map(|r| (r.bytes, r.warnings)).map_err(|e| e.to_string()))
}

fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>, String> {
    let img = Image::from_u8(width, height, ChannelLayout::Rgba, rgba.to_vec()).map_err(|e| e.to_string())?;
    photocraft_codecs::encode(&img, photocraft_codecs::Format::Png, &EncodeOptions::default()).map_err(|e| e.to_string())
}

/// Open a web link in the browser (or the app that handles it).
fn open_url(url: &str) -> Result<(), String> {
    let reply = platform::call("url", url)?;
    protocol::parse_reply(&reply).map(|_| ())
}

/// Append to a text file (the History Log).
fn append_text(path: &str, text: &str) -> Result<(), String> {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new().create(true).append(true).open(path).map_err(|e| e.to_string())?;
    file.write_all(text.as_bytes()).map_err(|e| e.to_string())
}

/// Write `bytes` crash-safely (temp file beside the target, fsync, rename; see
/// `photocraft_format::atomic_write`), so an interrupted save never destroys the old file.
fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    photocraft_format::atomic_write(path, bytes).map_err(|e| e.to_string())
}

/// Saves exports into `Downloads/PhotoCraft`. It remembers the copy each name was saved to, so a
/// second save of the same name replaces that copy instead of adding "name (1)".
struct Saver {
    staging: PathBuf,
    index: PathBuf,
    saved: RefCell<HashMap<String, String>>,
}

impl Saver {
    fn new(staging: PathBuf, index: PathBuf) -> Self {
        let saved = std::fs::read_to_string(&index).map(|text| protocol::parse_index(&text)).unwrap_or_default();
        Self { staging, index, saved: RefCell::new(saved) }
    }

    fn save(&self, path: &str, bytes: &[u8]) -> Result<(), String> {
        let name = protocol::file_name(path);
        let previous = self.saved.borrow().get(&name).cloned().unwrap_or_default();
        // The activity copies from a file, so the bytes are staged in app-private storage first.
        let staged = self.stage(&name, bytes)?;
        let staged_path = staged.to_string_lossy().into_owned();
        let argument = protocol::join_fields(&[name.as_str(), protocol::mime_type(&name), staged_path.as_str(), previous.as_str()]);
        let reply = platform::call("save", &argument);
        let _ = std::fs::remove_file(&staged);
        let uri = protocol::parse_reply(&reply?).map_err(|e| format!("{name} could not be saved: {e}"))?;
        self.saved.borrow_mut().insert(name, uri);
        let index = protocol::format_index(&self.saved.borrow());
        if let Err(e) = write_atomic(&self.index, index.as_bytes()) {
            log::warn!("could not remember where exports were saved: {e}");
        }
        Ok(())
    }

    fn stage(&self, name: &str, bytes: &[u8]) -> Result<PathBuf, String> {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let fail = |e: std::io::Error| format!("{name} could not be staged: {e}");
        std::fs::create_dir_all(&self.staging).map_err(fail)?;
        let staged = self.staging.join(format!("{}-{name}", NEXT.fetch_add(1, Ordering::Relaxed)));
        std::fs::write(&staged, bytes).map_err(fail)?;
        Ok(staged)
    }

    fn remove_logs(&self) -> Result<(), String> {
        let mut uris = Vec::new();
        {
            let mut saved = self.saved.borrow_mut();
            let log_keys: Vec<String> = saved
                .keys()
                .filter(|k| *k == crate::crash_report::LOG_NAME || k.starts_with("photocraft-log-unclean-"))
                .cloned()
                .collect();
            for key in log_keys {
                if let Some(uri) = saved.remove(&key)
                    && !uri.is_empty()
                {
                    uris.push(uri);
                }
            }
            let index = protocol::format_index(&saved);
            if let Err(e) = write_atomic(&self.index, index.as_bytes()) {
                log::warn!("could not update export index after removing logs: {e}");
            }
        }
        let refs: Vec<&str> = uris.iter().map(String::as_str).collect();
        let argument = protocol::join_fields(&refs);
        let reply = platform::call("delete_logs", &argument)?;
        protocol::parse_reply(&reply).map(|_| ())
    }
}

type SharedRecovery = Rc<RefCell<Option<RecoveryStore>>>;

/// Run `f` on the recovery store. The service closures never call each other, so the store is
/// never borrowed twice.
fn with_store<R>(store: &SharedRecovery, f: impl FnOnce(&mut RecoveryStore) -> R) -> Result<R, String> {
    let mut slot = store.try_borrow_mut().map_err(|_| "crash recovery is busy".to_string())?;
    Ok(f(slot.as_mut().ok_or("no storage directory")?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_export_index_is_read_back_after_a_restart() {
        let dir = std::env::temp_dir().join(format!("photocraft-android-index-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let index = dir.join("exports.txt");
        let saver = Saver::new(dir.join("staging"), index.clone());
        saver.saved.borrow_mut().insert("cat.psd".into(), "content://media/external_primary/downloads/7".into());
        write_atomic(&saver.index, protocol::format_index(&saver.saved.borrow()).as_bytes()).unwrap();
        let restarted = Saver::new(dir.join("staging"), index);
        assert_eq!(restarted.saved.borrow().get("cat.psd").map(String::as_str), Some("content://media/external_primary/downloads/7"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_failed_recovery_store_reports_instead_of_panicking() {
        let store: SharedRecovery = Rc::new(RefCell::new(None));
        assert_eq!(with_store(&store, |_| ()), Err("no storage directory".to_string()));
    }
}
