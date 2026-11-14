//! Menu bar generated from the engine command registry plus UI-level commands.

use serde_json::{Value, json};

use crate::PhotocraftApp;
use crate::state::DialogKind;

mod bar;
pub mod enable_rules;
mod invoke;
mod items;
pub mod menu_keys;
pub mod menu_nav;
pub mod native_menu;
pub mod parity;
pub mod plugin_ui;
mod render;
mod search;
mod state;
pub mod view_cmds;
pub mod workspace_ui;

pub use bar::menu_bar;
#[cfg(test)]
use bar::pointer_reaches_title;
pub use invoke::{apply_workspace, invoke};
pub(crate) use invoke::{invoke_unguarded, panel_alias};
pub(crate) use items::ItemCache;
pub use items::{MenuItem, is_live, menu_items};
pub use render::alt_click;
#[cfg(test)]
use render::submenu_enabled;
#[cfg(test)]
use search::HELP_SEARCH_MAX;
pub use search::{search_items, search_rank};
pub use state::is_enabled;
pub(crate) use state::reveal_label;
#[cfg(test)]
use state::{checked, translated_menu_label};

/// Shared gate for native menu clicks and control-channel `ui.menu.invoke`.
/// Dialogs and unsaved-changes prompts block edits, but allow the four view-navigation
/// commands also usable through shortcuts. Camera Raw, the SVG editor choice and full-window workspaces
/// (`distort_ui::modal_workspace`) block menu commands entirely, and a pending crop blocks the
/// commands Photoshop greys during one (`crop_ui::blocks`).
pub(crate) fn modal_allows(app: &PhotocraftApp, id: &str) -> bool {
    if app.camera_raw.is_some() || app.smart_svg.modal_open() || crate::distort_ui::modal_workspace(app).is_some() || crate::crop_ui::blocks(app, id) {
        return false;
    }
    (app.ui.dialogs.is_empty() && app.ui.layers_panel_dialog.is_none() && app.discard.is_none()) || crate::shortcuts::NAV_COMMANDS.contains(&id)
}

/// Top-level menus in Photoshop order.
pub const TOP_MENUS: [&str; 10] = ["File", "Edit", "Image", "Layer", "Type", "Select", "Filter", "View", "Window", "Help"];

/// UI-level commands (handled by the shell rather than the engine): id, label, menu, shortcut.
///
/// The table is the engine's ([`photocraft_engine::shell_commands::SHELL_COMMANDS`]), so the
/// commands and their keys exist without this UI (headless shortcut sets, lists, conflicts).
pub const UI_COMMANDS: &[(&str, &str, &[&str], Option<&str>)] = photocraft_engine::shell_commands::SHELL_COMMANDS;

/// Window › <document>: the open documents, listed at the end of the Window menu as in Photoshop.
/// The id ends in the document's index, which is its tab's position.
pub(crate) const DOCUMENT_ITEM: &str = "window.document.";


#[cfg(test)]
mod tests;

#[cfg(test)]
mod open_recent_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn tracks_dedupes_caps_and_lists() {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.push_recent("/tmp/a.png");
        app.push_recent("/tmp/b.psd");
        app.push_recent("/tmp/a.png"); // de-dupe → moves to front
        assert_eq!(app.ui.recent_files, vec!["/tmp/a.png".to_string(), "/tmp/b.psd".to_string()]);
        for i in 0..15 {
            app.push_recent(&format!("/tmp/f{i}.png"));
        }
        assert_eq!(app.ui.recent_files.len(), 17, "all kept below the default cap of 20");
        for i in 15..30 {
            app.push_recent(&format!("/tmp/f{i}.png"));
        }
        assert_eq!(app.ui.recent_files.len(), 20, "capped at Recent File List Contains (20)");
        assert_eq!(app.session.prefs().file_handling.recent_files, app.ui.recent_files, "stored in the preferences");

        // Menu lists them under File › Open Recent, with basenames, plus Clear Recent Files.
        app.ui.recent_files = vec!["/tmp/a.png".into(), "/dir/b.psd".into()];
        let items = menu_items(&app);
        let rp = vec!["File".to_string(), "Open Recent".to_string()];
        let labels: Vec<&str> = items.iter().filter(|i| i.id.starts_with("file.openRecent.")).map(|i| i.label.as_str()).collect();
        assert_eq!(labels, vec!["a.png", "b.psd"]);
        assert!(items.iter().any(|i| i.id == "file.clearRecent" && i.path == rp));

        // Clear Recent Files empties the list.
        let ctx = egui::Context::default();
        invoke(&mut app, &ctx, "file.clearRecent", json!({})).unwrap();
        assert!(app.ui.recent_files.is_empty());
        assert!(app.session.prefs().file_handling.recent_files.is_empty(), "cleared in the preferences too");
        // No recent items in the menu once cleared (only the disabled "Clear Recent Files").
        assert!(!menu_items(&app).iter().any(|i| i.id.starts_with("file.openRecent.")));
        // A bad recent index errors gracefully (no panic).
        assert!(invoke(&mut app, &ctx, "file.openRecent.5", json!({})).is_err());

        // Remove one entry (the Home screen's ×, #2691): the others keep their order.
        for p in ["/tmp/c.tif", "/tmp/b.psd", "/tmp/a.png"] {
            app.push_recent(p);
        }
        invoke(&mut app, &ctx, "file.removeRecent", json!({"path": "/tmp/b.psd"})).unwrap();
        assert_eq!(app.ui.recent_files, vec!["/tmp/a.png".to_string(), "/tmp/c.tif".to_string()]);
        assert_eq!(app.session.prefs().file_handling.recent_files, app.ui.recent_files, "removed in the preferences too");
        invoke(&mut app, &ctx, "file.removeRecent", json!({"path": "/tmp/not-listed.png"})).unwrap();
        assert_eq!(app.ui.recent_files.len(), 2, "a path that isn't listed changes nothing");
        assert!(invoke(&mut app, &ctx, "file.removeRecent", json!({})).is_err(), "needs a path");
    }

    #[test]
    fn push_recent_evicts_thumbnails_beyond_cap() {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        {
            let mut cache = app.thumbnail_cache.lock().unwrap();
            for i in 0..25 {
                cache.entries.insert(
                    format!("/tmp/item_{i}.png"),
                    crate::home_recent_ui::ThumbnailEntry {
                        key: crate::home_recent_ui::CacheKey::OpenDoc { revision: 1 },
                        texture: None,
                        #[cfg(not(target_arch = "wasm32"))]
                        last_checked: std::time::Instant::now(),
                    },
                );
            }
        }
        for i in 0..25 {
            app.push_recent(&format!("/tmp/item_{i}.png"));
        }
        let cache = app.thumbnail_cache.lock().unwrap();
        for i in 0..5 {
            assert!(!cache.entries.contains_key(&format!("/tmp/item_{i}.png")), "evicted item_{i} must be removed from cache");
        }
        assert_eq!(app.ui.recent_files.len(), 20);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn opening_a_file_records_it_as_recent() {
        use photocraft_color::{ColorMode, SampleType};
        use photocraft_geom::Size;
        let services = crate::Services {
            import: Some(Box::new(|_n: &str, _b: &[u8], _opts: &photocraft_io::ImportOptions| {
                Ok((photocraft_doc::Document::new("t", Size::new(4, 4), ColorMode::Rgb, SampleType::U8), Vec::new()))
            })),
            ..Default::default()
        };
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), services);
        let path = std::env::temp_dir().join("photocraft_recent_test.pcraft");
        std::fs::write(&path, b"x").unwrap();
        let p = path.to_string_lossy().to_string();
        let ctx = egui::Context::default();
        invoke(&mut app, &ctx, "file.open", json!({ "path": p })).unwrap();
        assert_eq!(app.ui.recent_files.first().map(String::as_str), Some(p.as_str()));
        // Re-open via the recent entry once closed (an open file shows its tab instead, #6598).
        app.session.close(0);
        invoke(&mut app, &ctx, "file.openRecent.0", json!({})).unwrap();
        assert_eq!(app.session.documents().len(), 1);
        assert_eq!(app.session.active().and_then(|d| d.path.as_deref()), Some(p.as_str()));
        let _ = std::fs::remove_file(&path);
    }
}

#[cfg(test)]
mod quick_export_label_tests {
    use super::*;

    #[test]
    fn file_quick_export_menu_reflects_selected_format_without_renaming_layer_export() {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        for (value, expected) in [("png", "PNG"), ("jpg", "JPG"), ("gif", "GIF"), ("webp", "WebP"), ("exr", "EXR")] {
            app.run("prefs.set", json!({"values": {"export.quickExportFormat": value}})).unwrap();
            let items = menu_items(&app);
            let quick: Vec<_> = items.iter().filter(|i| i.id == "file.export.quickExportAsPng").collect();
            assert_eq!(quick.len(), 1, "the catalogue and UI command must not create duplicate items");
            let shown_format = if cfg!(target_arch = "wasm32") { "PNG" } else { expected };
            assert_eq!(quick[0].label, format!("Quick Export as {shown_format}"));
            let id = crate::i18n::Lang::from_code("id").unwrap();
            let localized = translated_menu_label(id, quick[0]);
            assert_eq!(localized, crate::i18n::tr_id(id, "file.export.quickExportAsPng", "Quick Export as PNG").replace("PNG", shown_format));
            let layer = items.iter().find(|i| i.id == "layer.quickExportAsPng").unwrap();
            assert_eq!(layer.label, "Quick Export as PNG", "layer export always creates PNG");
        }
    }
}

/// The file the diagnostic log is saved as, in `Download/PhotoCraft/log` (see [`save_diagnostic_log`]).
const DIAGNOSTIC_LOG: &str = "photocraft-log.txt";

/// Help › Save Diagnostic Log: the kept lines of `photocraft_doc::diag` go to [`DIAGNOSTIC_LOG`], through the
/// save service (`Download/PhotoCraft/log` on the phone). The item is listed on the phone build only.
fn save_diagnostic_log(app: &mut PhotocraftApp) -> Result<Value, String> {
    let lines = write_diagnostic_log(app)?;
    app.ui.status = format!("Diagnostic log saved as log/{DIAGNOSTIC_LOG}");
    Ok(json!({"file": DIAGNOSTIC_LOG, "lines": lines}))
}

/// Writes the kept lines to [`DIAGNOSTIC_LOG`] through the save service, and returns how many lines went.
fn write_diagnostic_log(app: &mut PhotocraftApp) -> Result<usize, String> {
    let text = photocraft_doc::diag::dump();
    let write = app.services.write.as_mut().ok_or("no writer configured")?;
    write(DIAGNOSTIC_LOG, text.as_bytes())?;
    Ok(text.lines().count())
}

/// How long the log waits after its last write before it is written again (see [`tick_diagnostic_log`]).
const LOG_SAVE_EVERY: std::time::Duration = std::time::Duration::from_secs(2);

/// When the log was last written, in this process.
static LOG_WRITTEN: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);

/// Whether the log may be written, given the time since its last write.
fn log_save_due(since: Option<std::time::Duration>) -> bool {
    since.is_none_or(|s| s >= LOG_SAVE_EVERY)
}

/// Writes the log to `Download/PhotoCraft/log/photocraft-log.txt` shortly after it changes, so that a change's lines are in
/// the file without a trip to the menu. Phone only: the desktop keeps no log, and has no save service here.
pub(crate) fn tick_diagnostic_log(app: &mut PhotocraftApp, ctx: &egui::Context) {
    if !photocraft_doc::diag::enabled() || app.services.write.is_none() || !photocraft_doc::diag::take_dirty() {
        return;
    }
    let mut written = LOG_WRITTEN.lock().unwrap_or_else(|e| e.into_inner());
    let since = written.as_ref().map(|t| t.elapsed());
    if !log_save_due(since) {
        photocraft_doc::diag::mark_dirty();
        ctx.request_repaint_after(LOG_SAVE_EVERY);
        return;
    }
    *written = Some(std::time::Instant::now());
    drop(written);
    // The save runs on the UI thread, so its time is in the frame timing too. The line says how long it took, but
    // does not make the log due again (see `diag::note_quiet`).
    let began = std::time::Instant::now();
    match write_diagnostic_log(app) {
        Ok(lines) => photocraft_doc::diag::note_quiet(move || format!("log saved: {lines} lines in {} ms", began.elapsed().as_millis())),
        Err(e) => log::warn!("the diagnostic log could not be saved: {e}"),
    }
}

/// Toggles diagnostic `.txt` logging from the phone menu. Turning it on starts recording and immediately
/// saves `Download/PhotoCraft/log/photocraft-log.txt`; turning it off stops recording, clears the in-memory log,
/// and asks the platform service to remove any saved diagnostic `.txt` files.
pub(crate) fn set_diagnostic_debug(app: &mut PhotocraftApp, enabled: bool) {
    if enabled {
        photocraft_doc::diag::set_enabled(true);
        let tool = app.ui.tool;
        let docs = app.session.documents().len();
        let lang = app.session.prefs().interface.language.clone();
        let rtl = crate::direction::is_rtl();
        photocraft_doc::diag::note(move || {
            format!(
                "PhotoCraft {} diagnostic log enabled ({}, tool={tool:?}, docs={docs}, lang={lang}, rtl={rtl})",
                env!("CARGO_PKG_VERSION"),
                photocraft_doc::cpu::diag_summary()
            )
        });
        if let Some(cb) = app.services.set_debug_log.as_mut()
            && let Err(e) = cb(true)
        {
            log::warn!("could not persist debug log state: {e}");
        }
        let _ = photocraft_doc::diag::take_dirty();
        *LOG_WRITTEN.lock().unwrap_or_else(|e| e.into_inner()) = Some(std::time::Instant::now());
        if app.services.write.is_some() {
            match write_diagnostic_log(app) {
                Ok(lines) => {
                    photocraft_doc::diag::note_quiet(move || format!("log saved: {lines} lines"));
                    app.ui.status = format!("Diagnostic log saved as log/{DIAGNOSTIC_LOG}");
                }
                Err(e) => log::warn!("the diagnostic log could not be saved: {e}"),
            }
        }
    } else {
        photocraft_doc::diag::set_enabled(false);
        photocraft_doc::diag::clear();
        *LOG_WRITTEN.lock().unwrap_or_else(|e| e.into_inner()) = None;
        if let Some(cb) = app.services.set_debug_log.as_mut()
            && let Err(e) = cb(false)
        {
            log::warn!("could not remove diagnostic log files: {e}");
        }
        app.ui.status = format!("Diagnostic log disabled ({DIAGNOSTIC_LOG} removed)");
    }
}

/// Updates the active CPU core count from the phone menu and persists it through the platform service.
pub(crate) fn set_active_cpu_cores(app: &mut PhotocraftApp, cores: usize) {
    let applied = photocraft_doc::cpu::set_active_cores(cores);
    if let Some(cb) = app.services.set_cpu_cores.as_mut()
        && let Err(e) = cb(applied)
    {
        log::warn!("could not persist active CPU core count: {e}");
    }
    app.ui.status = format!("Active CPU cores: {applied}/{}", photocraft_doc::cpu::max_cores());
}

#[cfg(test)]
mod diagnostic_log_tests {
    use super::*;

    #[test]
    fn the_diagnostic_log_is_saved_through_the_save_service() {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        let saved = std::rc::Rc::new(std::cell::RefCell::new(Vec::<(String, Vec<u8>)>::new()));
        let sink = saved.clone();
        app.services.write = Some(Box::new(move |name: &str, bytes: &[u8]| -> Result<(), String> {
            sink.borrow_mut().push((name.to_string(), bytes.to_vec()));
            Ok(())
        }));
        let result = save_diagnostic_log(&mut app).expect("the log is saved");
        let saved = saved.borrow();
        assert_eq!(saved.len(), 1, "one file is written");
        assert_eq!(saved[0].0, "photocraft-log.txt");
        assert!(String::from_utf8_lossy(&saved[0].1).starts_with("PhotoCraft diagnostic log"), "the header leads the file");
        assert_eq!(result["file"], "photocraft-log.txt");
        assert!(app.ui.status.contains("log/photocraft-log.txt"), "the status line says where it went");
    }

    #[test]
    fn the_diagnostic_log_waits_between_its_saves() {
        assert!(log_save_due(None), "the first save is due at once");
        assert!(!log_save_due(Some(std::time::Duration::from_millis(300))), "a save just made is not due");
        assert!(log_save_due(Some(std::time::Duration::from_secs(3))), "a save made long ago is due");
    }
}

#[cfg(test)]
mod reveal_label_tests {
    use super::*;

    #[test]
    fn reveal_label_names_the_platform_file_manager() {
        let expected = if cfg!(target_os = "macos") {
            "Reveal in Finder"
        } else if cfg!(windows) {
            "Reveal in Explorer"
        } else {
            "Show in Folder"
        };
        assert_eq!(reveal_label(), expected);
    }

    #[test]
    fn smart_objects_reveal_item_names_the_platform_file_manager() {
        let app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        let id = "layer.smartObjects.revealInFinder";
        let items = menu_items(&app);
        let reveal: Vec<_> = items.iter().filter(|i| i.id == id).collect();
        assert_eq!(reveal.len(), 1);
        assert_eq!(reveal[0].path, ["Layer", "Smart Objects"]);
        assert_eq!(reveal[0].label, reveal_label());
        if !cfg!(target_os = "macos") {
            assert!(!reveal[0].label.contains("Finder"), "{}", reveal[0].label);
        }
        let shortcuts = crate::prefs_ui::shortcut_items(&app);
        assert!(shortcuts.iter().any(|(i, label, _, _)| i == id && label == reveal_label()), "the shortcut list shows the menu label");
        let de = crate::i18n::Lang::from_code("de").unwrap();
        assert_eq!(translated_menu_label(de, reveal[0]), crate::i18n::tr(de, reveal_label()));
        // The menu catalogue (and docs/parity-checklist.md) keeps Photoshop's macOS name.
        assert!(crate::menu_catalog::CATALOG.iter().any(|(_, label, _, i)| *i == id && *label == "Reveal in Finder"));
    }
}

#[cfg(test)]
mod window_documents_tests {
    use super::*;
    use serde_json::json;

    fn app(docs: usize) -> PhotocraftApp {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        for _ in 0..docs {
            app.run("file.new", json!({"width": 64, "height": 48})).unwrap();
        }
        app.sync_views();
        app
    }

    /// The rows after Window's last own item: a separator and the documents.
    fn listed(app: &PhotocraftApp) -> Vec<(String, String, Option<bool>)> {
        let items = menu_items(app);
        let window: Vec<&MenuItem> = items.iter().filter(|i| i.path == ["Window"]).collect();
        let at = window.iter().position(|i| i.id.starts_with(DOCUMENT_ITEM)).unwrap_or(window.len());
        if at > 0 && at < window.len() {
            assert_eq!(window[at - 1].label, "---", "a separator comes before the documents");
        }
        window[at..].iter().map(|i| (i.id.clone(), i.label.clone(), i.checked)).collect()
    }

    /// #2991: Window ends with the open documents in tab order, the active one checked, as in
    /// Photoshop; choosing one brings it to the front.
    #[test]
    fn the_window_menu_lists_the_open_documents_and_switches_between_them() {
        assert!(listed(&app(0)).is_empty(), "no documents, no list (and no separator)");

        let mut app = app(3);
        let ctx = egui::Context::default();
        let names: Vec<String> = app.session.documents().iter().map(|d| d.doc.name.clone()).collect();
        let row = |i: usize, active: bool| (format!("window.document.{i}"), names[i].clone(), Some(active));
        assert_eq!(listed(&app), [row(0, false), row(1, false), row(2, true)]);
        // It stays last in Window, after the panels.
        let items = menu_items(&app);
        let last = items.iter().rposition(|i| i.path.first().map(String::as_str) == Some("Window")).unwrap();
        assert_eq!(items[last].id, "window.document.2");

        assert_eq!(invoke(&mut app, &ctx, "window.document.0", json!({})).unwrap(), json!({"document": 0}));
        assert_eq!(app.session.active_index(), Some(0));
        assert_eq!(listed(&app), [row(0, true), row(1, false), row(2, false)]);

        // A stale or malformed id (a document closed since the menu was built) is an error.
        for id in ["window.document.3", "window.document.", "window.document.x", "window.document.-1"] {
            assert!(is_enabled(&app, id), "{id}: rows are always enabled");
            assert!(invoke(&mut app, &ctx, id, json!({})).is_err(), "{id}");
        }
        assert_eq!(app.session.active_index(), Some(0));
    }

    /// The rows show the document's name as it is, never a translation of it, and follow a
    /// rename (Save As).
    #[test]
    fn document_names_are_shown_as_they_are_and_follow_a_rename() {
        let mut app = app(1);
        let before = crate::native_menu::state_hash(&app, None);
        if let Some(d) = app.session.active_mut() {
            d.saved_to("/tmp/Layers".into());
        }
        assert_ne!(crate::native_menu::state_hash(&app, None), before, "a rename updates the native menu");
        let de = crate::i18n::Lang::from_code("de").unwrap();
        let items = menu_items(&app);
        let row = items.iter().find(|i| i.id == "window.document.0").unwrap();
        assert_eq!(translated_menu_label(de, row), "Layers");
        let bar = crate::native_menu::photocraft_layout(&items, de, "de").bar;
        assert_eq!(bar.find("window.document.0").map(|i| i.label.as_str()), Some("Layers"));
        let panel = items.iter().find(|i| i.path == ["Window"] && i.label == "Layers").unwrap();
        assert_eq!(bar.find(&panel.id).map(|i| i.label.clone()), Some(translated_menu_label(de, panel)), "Window › Layers is still translated");
    }

    /// The rows go through the control channel's `ui.menu.invoke`, like a click.
    #[test]
    fn control_can_choose_a_document_from_the_window_menu() {
        let mut app = app(2);
        let ctx = egui::Context::default();
        let (req, _rx) = crate::control::ControlRequest::new("ui.menu.invoke", json!({"id": "window.document.0"}));
        let crate::control::Outcome::Done(r) = crate::control::handle(&mut app, &ctx, &req) else { panic!("expected an immediate reply") };
        assert_eq!(r["ok"], true, "{r}");
        assert_eq!(app.session.active_index(), Some(0));
    }
}
