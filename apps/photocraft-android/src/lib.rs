//! PhotoCraft for Android.
//!
//! The same egui interface as the desktop app (`photocraft-ui-egui`), run by eframe's
//! NativeActivity backend. This crate provides what the desktop shell does natively:
//!
//! - file access through the system file picker (Storage Access Framework) and saves to
//!   `Downloads/PhotoCraft` (MediaStore), both through `PhotoCraftActivity` (Java; see `platform`);
//! - app-private storage for preferences, crash recovery and brush presets;
//! - the system bars and soft keyboard, which the UI keeps clear of (`shell`).
//!
//! The pieces that don't need Android (the protocol text, the saved-layout check, the crash guard, the crash
//! report and the frame timing) are plain Rust and run under `cargo test` on any host.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod crash_guard;
pub mod crash_report;
pub mod intake;
pub mod perf_log;
pub mod protocol;
pub mod ui_state;

#[cfg(target_os = "android")]
mod platform;
#[cfg(target_os = "android")]
mod services;
#[cfg(target_os = "android")]
mod shell;

/// The entry point android-activity calls once the activity exists. The library name is set by
/// the manifest's `android.app.lib_name` (`photocraft_android`).
#[cfg(target_os = "android")]
#[allow(unsafe_code)]
#[unsafe(no_mangle)]
fn android_main(app: winit::platform::android::activity::AndroidApp) {
    shell::run(app);
}
