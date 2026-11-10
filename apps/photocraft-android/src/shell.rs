//! The eframe app on Android: starts the shared UI on the activity, and each frame gives it what
//! the platform knows (picked files, system bars, the keyboard) and lays it out around them.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use photocraft_doc::diag;
use photocraft_engine::Session;
use photocraft_ui_egui::theme::ThemeKind;
use photocraft_ui_egui::{PhotocraftApp, notices};
use winit::platform::android::activity::AndroidApp;

use crate::perf_log::FrameLog;
use crate::platform;
use crate::protocol;
use crate::services::{self, Dirs, Link};
use crate::{crash_report, intake, ui_state};

/// How often frames come while something is still arriving (picked files, the keyboard's size).
const POLL_INTERVAL: Duration = Duration::from_millis(100);
/// After launch, keep polling this long: the system reports its bars a moment after the window.
const SETTLE_TIME: Duration = Duration::from_secs(3);

/// Start PhotoCraft on the activity and run until it closes.
pub fn run(app: AndroidApp) {
    android_logger::init_once(android_logger::Config::default().with_max_level(log::LevelFilter::Info).with_tag("PhotoCraft"));
    crate::crash_guard::install_hook(record_panic);
    let Some(root) = app.internal_data_path() else {
        log::error!("PhotoCraft has no app-private storage, so it can't start");
        return;
    };
    platform::set_activity(app.activity_as_ptr());
    let dirs = Dirs::new(root);
    // The activity creates Download/PhotoCraft/Fonts and Download/PhotoCraft/log through
    // MediaStore. A persisted, user-granted SAF tree lets its font worker read files that a file
    // manager placed there; no broad all-files permission is requested.
    log::info!("PhotoCraft user folder: Download/PhotoCraft");
    // The system font scan can take seconds: run it off the UI thread. The user folder is loaded
    // on a second worker once the shared engine exists (see `warm_user_fonts`).
    photocraft_ui_egui::type_tool::warm_fonts();
    crash_report::set_root(dirs.root());
    let shell_root = dirs.root().to_path_buf();
    // eframe restores the saved window and panel layout before our code runs; drop a layout that
    // would abort startup (see `ui_state`).
    let layout = dirs.root().join("ui.ron");
    ui_state::sanitize(Some(layout.as_path()));
    let mut options = eframe::NativeOptions { android_app: Some(app), persistence_path: Some(layout), ..Default::default() };
    // The adapter's real texture limits, so large documents stay on the GPU.
    photocraft_ui_egui::gpu_canvas::use_adapter_limits(&mut options.wgpu_options.wgpu_setup);
    let result = eframe::run_native(
        "PhotoDroid",
        options,
        Box::new(move |cc| {
            let link = Link::new(cc.egui_ctx.clone());
            let font_dir = dirs.fonts();
            warm_user_fonts(font_dir.clone(), cc.egui_ctx.clone());
            let font_ctx = cc.egui_ctx.clone();
            let svc = services::services(&dirs, &link);
            PhotocraftApp::setup_context(&cc.egui_ctx, ThemeKind::Pro);
            let mut photo = PhotocraftApp::new(Session::new(), svc);
            // No window decorations on Android: the app's own top bar is the only bar.
            photo.custom_titlebar = false;
            photo.integrated_titlebar = false;
            photo.background_jobs = true;
            // Phones get the phone layout (`photocraft_ui_egui::mobile`) when the window is narrow.
            photo.phone_layout = true;
            photo.set_theme(&cc.egui_ctx, ThemeKind::Pro);
            // Brush presets load in the background and attach when they arrive.
            photo.services.preset_store = Some(photocraft_engine::preset_store::open_dir_async(dirs.presets()));
            if let Some(render_state) = cc.wgpu_render_state.clone() {
                photo.set_wgpu(render_state);
            }
            if let Some(saved_cores) = crash_report::load_cpu_cores(dirs.root()) {
                photocraft_doc::cpu::set_active_cores(saved_cores);
            }
            let debug_on = crash_report::debug_enabled(dirs.root());
            diag::set_enabled(debug_on);
            if debug_on {
                diag::note(|| {
                    format!(
                        "PhotoCraft {} started ({})",
                        env!("CARGO_PKG_VERSION"),
                        photocraft_doc::cpu::diag_summary()
                    )
                });
                // A run that did not end normally left its log (see `crash_report`): save it in log/ now.
                if let Some((name, bytes)) = crash_report::begin(dirs.root()) {
                    let saved = photo.services.write.as_mut().map(|write| write(name.as_str(), bytes.as_slice()));
                    match saved {
                        Some(Ok(())) => diag::note(|| format!("the last run did not end normally: its log is saved as {name}")),
                        Some(Err(e)) => log::warn!("PhotoCraft: the last run's log could not be saved: {e}"),
                        None => {}
                    }
                }
            } else {
                diag::clear();
                crash_report::set_debug_enabled(dirs.root(), false);
                if let Some(cb) = photo.services.set_debug_log.as_mut() {
                    let _ = cb(false);
                }
            }
            Ok(Box::new(AndroidShell {
                app: photo,
                link,
                started: Instant::now(),
                frames: FrameLog::default(),
                frame_started: None,
                frame_work: Duration::ZERO,
                root: shell_root,
                font_dir,
                font_ctx,
                last_insets_px: [0; 4],
                last_insets_diag: String::new(),
            }))
        }),
    );
    if let Err(e) = result {
        log::error!("PhotoCraft stopped: {e}");
    }
}

/// Registers the user's Fonts directory without stalling startup. `warm_fonts` may still be
/// scanning the operating system's fonts; taking the shared engine waits for that worker, then
/// adds this directory and wakes the UI so its generation-keyed family list is rebuilt.
fn warm_user_fonts(dir: PathBuf, ctx: egui::Context) {
    let worker_dir = dir.clone();
    let worker = std::thread::Builder::new().name("photocraft-user-fonts".into()).spawn(move || {
        let staged = worker_dir.to_string_lossy().into_owned();
        let synced = platform::call("sync_fonts", &staged).and_then(|reply| protocol::parse_reply(&reply));
        if let Err(e) = synced {
            log::warn!("PhotoCraft could not read Download/PhotoCraft/Fonts: {e}");
            return;
        }
        let engine = photocraft_text::shared();
        let mut engine = engine.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let count = engine.fonts.load_font_dir(&worker_dir);
        drop(engine);
        log::info!("PhotoCraft loaded {count} font candidate(s) from Download/PhotoCraft/Fonts");
        ctx.request_repaint();
    });
    if let Err(e) = worker {
        log::warn!("PhotoCraft could not start the user font scan for Download/PhotoCraft/Fonts: {e}");
    }
}

/// The panic hook's record (see `crash_guard`): the panic goes into the diagnostic log, and the log's copy in
/// app-private storage is kept, so the panic is in the file even if the app does not get to save it.
fn record_panic(message: &str) {
    if !diag::enabled() {
        return;
    }
    if let Some(text) = diag::record_panic(message) {
        crash_report::keep_copy(text.as_bytes());
    }
}

/// The UI, plus the platform state it can't ask for itself.
struct AndroidShell {
    app: PhotocraftApp,
    link: Link,
    started: Instant,
    /// Frame timing for the diagnostic log (see `perf_log`).
    frames: FrameLog,
    /// When the current frame's logic started, and the time its logic and UI have taken so far.
    frame_started: Option<Instant>,
    frame_work: Duration,
    /// App-private storage, where the run's mark is (see `crash_report`).
    root: PathBuf,
    /// Private staging copy refreshed from Download/PhotoCraft/Fonts by the activity.
    font_dir: PathBuf,
    /// Wakes the UI after the background font scan, including a scan triggered by a new SAF grant.
    font_ctx: egui::Context,
    /// Last known system bar & IME insets in device pixels `[top, bottom, left, right]`.
    last_insets_px: [i32; 4],
    /// Last logged diagnostic breakdown from `PhotoCraftActivity` (`insets_diag`).
    last_insets_diag: String,
}

impl Drop for AndroidShell {
    /// Finish any queued recovery write before marking the run clean. An abrupt process kill may
    /// not run Drop, which is why revisions are also queued continuously while editing.
    fn drop(&mut self) {
        self.flush_recovery();
        crash_report::finish(&self.root);
    }
}

impl AndroidShell {
    fn flush_recovery(&mut self) {
        let Some(flush) = self.app.services.flush_autosaves.as_mut() else { return };
        for (id, revision, result) in flush() {
            if let Err(e) = result {
                log::warn!("PhotoCraft could not finish recovery save for document {id}, revision {revision}: {e}");
            }
        }
    }

    /// Take in what the activity reported since the last frame: picked files go to the UI's inbox,
    /// failures and notes become notices, and committed keyboard text joins this frame's egui input.
    fn pump(&mut self, raw_input: &mut egui::RawInput) {
        loop {
            let line = match platform::call("poll", "") {
                Ok(line) => line,
                Err(e) => {
                    self.link.notify(true, e);
                    break;
                }
            };
            if line.is_empty() {
                break;
            }
            match protocol::parse_event(&line) {
                Some(protocol::Event::Descriptor { name, fd }) => {
                    self.read_picked(name, Source::Descriptor(platform::take_descriptor(fd)));
                }
                Some(protocol::Event::File { name, path }) => self.read_picked(name, Source::Copy(path)),
                Some(protocol::Event::Error { name, message }) => {
                    let text = if name.is_empty() { message } else { format!("{name}: {message}") };
                    self.link.notify(true, text);
                }
                Some(protocol::Event::Done) => self.link.set_picking(false),
                Some(protocol::Event::SaveAs { name, uri }) => self.link.resolve_save_as(name, uri),
                Some(protocol::Event::FontsChanged) => {
                    warm_user_fonts(self.font_dir.clone(), self.font_ctx.clone());
                }
                Some(protocol::Event::Text(text)) => raw_input.events.push(egui::Event::Text(text)),
                Some(protocol::Event::Key { name, pressed, repeat }) => {
                    if let Some(key) = android_key(&name) {
                        raw_input.events.push(egui::Event::Key {
                            key,
                            physical_key: None,
                            pressed,
                            repeat,
                            modifiers: egui::Modifiers::default(),
                        });
                    } else {
                        log::warn!("PhotoCraft: unknown Android editing key {name:?}");
                    }
                }
                Some(protocol::Event::KeyboardStatus(message)) => {
                    diag::note(move || format!("Android keyboard: {message}"));
                }
                None => log::warn!("PhotoCraft: unexpected event from the activity: {line:?}"),
            }
        }
        for message in self.link.take_messages() {
            let _ = notices::post(&mut self.app, "PhotoDroid", vec![message.text], message.error, None);
        }
    }

    /// Read a picked file on a worker thread, then hand its bytes to the UI's inbox. The UI thread
    /// never opens or reads a file itself, however large the document is.
    fn read_picked(&self, name: String, source: Source) {
        let copy = match &source {
            Source::Copy(path) => Some(path.clone()),
            Source::Descriptor(_) => None,
        };
        let link = self.link.clone();
        let label = name.clone();
        let worker = std::thread::Builder::new().name("photocraft-read".into()).spawn(move || match read_source(source) {
            Ok(bytes) => link.deliver(name, bytes),
            Err(e) => link.notify(true, format!("{name} could not be read: {e}")),
        });
        if let Err(e) = worker {
            // The thread never ran, so nothing else will remove the app-cache copy.
            if let Some(path) = copy {
                let _ = std::fs::remove_file(path);
            }
            self.link.notify(true, format!("{label} could not be read: {e}"));
        }
    }
}

/// Translate the protocol's stable editing-key names without exposing Android keycodes to egui.
fn android_key(name: &str) -> Option<egui::Key> {
    match name {
        "Backspace" => Some(egui::Key::Backspace),
        "Delete" => Some(egui::Key::Delete),
        "Enter" => Some(egui::Key::Enter),
        "ArrowLeft" => Some(egui::Key::ArrowLeft),
        "ArrowRight" => Some(egui::Key::ArrowRight),
        "ArrowUp" => Some(egui::Key::ArrowUp),
        "ArrowDown" => Some(egui::Key::ArrowDown),
        "Tab" => Some(egui::Key::Tab),
        "Escape" => Some(egui::Key::Escape),
        "Home" => Some(egui::Key::Home),
        "End" => Some(egui::Key::End),
        "PageUp" => Some(egui::Key::PageUp),
        "PageDown" => Some(egui::Key::PageDown),
        _ => None,
    }
}

/// Where a picked file's bytes come from: a descriptor the picker opened, or the copy the activity
/// made in app cache for a provider that gave none.
enum Source {
    Descriptor(std::fs::File),
    Copy(String),
}

/// Reads a picked file's bytes. An app-cache copy is removed once it has been read, whatever the
/// outcome.
fn read_source(source: Source) -> std::io::Result<Vec<u8>> {
    match source {
        Source::Descriptor(mut file) => read_open(&mut file),
        Source::Copy(path) => {
            let bytes = std::fs::File::open(&path).and_then(|mut file| read_open(&mut file));
            let _ = std::fs::remove_file(&path);
            bytes
        }
    }
}

fn read_open(file: &mut std::fs::File) -> std::io::Result<Vec<u8>> {
    let size = file.metadata().map(|m| m.len()).unwrap_or(0);
    intake::read_to_bytes(file, size)
}

impl AndroidShell {
    /// The system bars and the soft keyboard, in egui points: the parts the UI keeps clear of.
    /// Keeps the last known value in effect if a JNI poll ever fails, and records full diagnostic
    /// details when `.txt` logging is enabled.
    fn system_insets(&mut self, ctx: &egui::Context, raw_input: &egui::RawInput) -> egui::SafeAreaInsets {
        let native_ppp = raw_input
            .viewports
            .get(&raw_input.viewport_id)
            .and_then(|v| v.native_pixels_per_point)
            .or_else(|| ctx.native_pixels_per_point());
        let per_point = native_ppp
            .map_or_else(|| ctx.pixels_per_point(), |n| n * ctx.zoom_factor())
            .max(f32::EPSILON);

        if let Ok(reply) = platform::call("insets", "")
            && let Ok(payload) = protocol::parse_reply(&reply)
            && let Some(px) = protocol::parse_insets(&payload)
        {
            let changed = px != self.last_insets_px;
            self.last_insets_px = px;
            if diag::enabled() && (changed || self.last_insets_diag.is_empty()) {
                let detail = platform::call("insets_diag", "")
                    .ok()
                    .and_then(|r| protocol::parse_reply(&r).ok())
                    .unwrap_or_default();
                if changed || detail != self.last_insets_diag {
                    self.last_insets_diag = detail.clone();
                    let [top, bottom, left, right] = px;
                    diag::note(move || {
                        format!(
                            "Android insets updated: px=(top={top}, bottom={bottom}, left={left}, right={right}) ppp={per_point:.2} pt=(top={:.1}, bottom={:.1}, left={:.1}, right={:.1}) [{detail}]",
                            top.max(0) as f32 / per_point,
                            bottom.max(0) as f32 / per_point,
                            left.max(0) as f32 / per_point,
                            right.max(0) as f32 / per_point,
                        )
                    });
                }
            }
        }

        let [top, bottom, left, right] = self.last_insets_px;
        let points = |pixels: i32| pixels.max(0) as f32 / per_point;
        egui::SafeAreaInsets(egui::epaint::MarginF32 {
            left: points(left),
            right: points(right),
            top: points(top),
            bottom: points(bottom),
        })
    }
}

impl eframe::App for AndroidShell {
    fn on_exit(&mut self) {
        // AndroidShell wraps the shared app, so forward its final preference/tool-state save.
        eframe::App::on_exit(&mut self.app);
        self.flush_recovery();
    }

    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        let began = Instant::now();
        self.frame_started = Some(began);
        self.app.logic(ctx, frame);
        self.frame_work = began.elapsed();
        // Keep frames coming while something is still to arrive: a picker's files, the keyboard's
        // height while text is being typed, a held finger (so long-press gestures and tool flyouts
        // fire while a finger rests still), and the first seconds after launch.
        if self.link.is_picking()
            || self.link.is_keyboard_active()
            || ctx.text_edit_focused()
            || ctx.input(|i| i.pointer.any_down() || i.any_touches())
            || self.started.elapsed() < SETTLE_TIME
        {
            ctx.request_repaint_after(POLL_INTERVAL);
        }
    }

    fn raw_input_hook(&mut self, ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        let insets = self.system_insets(ctx, raw_input);
        raw_input.safe_area_insets = Some(insets);
        // Drain before the shared app's hook so system text takes the same route as its internal
        // Persian keyboard and reaches both ordinary TextEdits and the custom Type editor.
        self.pump(raw_input);
        self.app.raw_input_hook(ctx, raw_input);
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let began = Instant::now();
        // The surface covers the whole window, system bars included. Paint all of it in the panel
        // colour, and lay the UI out in the safe area that the bars and the keyboard leave free.
        let panel = ui.style().visuals.panel_fill;
        ui.painter().rect_filled(ui.max_rect(), 0.0, panel);
        let safe = ui.ctx().content_rect();
        let _ = ui.scope_builder(egui::UiBuilder::new().max_rect(safe), |inner| self.app.ui(inner, frame));
        self.frame_work += began.elapsed();
        // The frame's work goes to the diagnostic log: a long frame on its own, a window of slow ones as a summary.
        if let Some(start) = self.frame_started.take() {
            for line in self.frames.record(start, self.frame_work) {
                diag::note_quiet(move || line);
            }
        }
    }
}
