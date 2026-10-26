# PhotoDroid: PhotoCraft for Android

**English** | [فارسی](README.fa.md)

**PhotoDroid** ([`Idea-atmosphere/PhotoDroid`](https://github.com/Idea-atmosphere/PhotoDroid)) is an open-source Android port of the desktop [PhotoCraft](https://github.com/storytold/photocraft) image and photo editor — a clean-room Adobe Photoshop (PSD/PSB) editor written in pure Rust
(upstream commit `cece50d5`, synced from `2a952f16`; see [Upstream sync](#upstream-sync)). The Rust engine, the file codecs, and the egui interface are the same
code as the desktop app. What changes is the shell: instead of a winit window with native dialogs,
the app runs inside an Android `NativeActivity`, with the system file picker, MediaStore for saves,
and app-private storage for preferences, crash recovery and brush presets.

> **Status:** CI builds the arm64 library and an installable debug APK, and the host tests pass.
> The app has **not** been run on a device or an emulator yet, so its behaviour on Android is
> untested.

## What is in this repository

| Path | What it is |
|---|---|
| `crates/` | The shared engine and UI, from upstream at `cece50d5` (`photocraft-ui-egui`, `photocraft-engine`, codecs, I/O, the Affinity importer, text, GPU, …). Two desktop-only crates were left out: `automation` (the TCP/MCP control server) and `tablet` (X11/macOS pen input). |
| `assets/`, `contributors/` | Fonts, icons, dictionary and app icon the crates embed at build time, with their licences (see `ATTRIBUTION.md`). |
| `apps/photocraft-android/` | The Android shell: a Rust `cdylib` that eframe runs through `android-native-activity`. `src/shell.rs` is the eframe app, `src/services.rs` the platform services, `src/platform.rs` the JNI bridge, `src/protocol.rs` the text protocol with the Java side, `src/intake.rs` the reading of picked files (host-tested). |
| `android/` | The Gradle project: `PhotoCraftActivity` (Java), the manifest, and the launcher icons. |
| `.github/workflows/android.yml` | CI: host tests, the arm64 library, and the APK. `.github/scripts/report-failure.sh` turns a failed build's errors into annotations. |

### Upstream sync

The shared crates match upstream `cece50d5` (synced from `2a952f16`). All upstream commits up to `cece50d5` (including the `egui 0.37` upgrade, lazy CJK and non-Latin script font loading, Color Settings and ICC profile dialogs, stroke line/angle constraints, extracted feathering in `crates/vector/src/feather.rs`, unified `ImportOptions`, and IPP printing in `crates/ipp`) and the earlier commits after `95006995` and `ec03c602` were merged the same way: the Save Contents fix, upstream's Type formatting on selected layers (the phone's "all text layers" option still applies to every type layer), SVG group rasterizing depth and the Thai fallback fonts among them. Earlier: the upstream changes to `crates/`, the workspace `Cargo.toml`, the About credits (`contributors/people.toml`), `assets/icons/` and `ATTRIBUTION.md` in the 177 commits after `0050f29` (the previous sync's target) were merged onto the Android changes below. `crates/automation` (the desktop's TCP control) and `crates/tablet` (desktop-only) are not part of this port. Where both changed the same code, both are kept: `lib.rs` (the phone's chrome replaces the desktop bars on the phone, and upstream's per-tool brush switch runs first), `panels.rs` (the options bar's contents stay in `options_strip`; upstream's colour chips, eyedropper options and Rotate View options are kept), `canvas.rs` (the phone's two-finger pan, now through the Hand tool's mapping, and upstream's trackpad rotation), `gpu_canvas.rs` and `icon_data.rs` (the test and icon lists of both), the translations (`i18n/*.tsv`, the rows of both) and `ATTRIBUTION.md`, `prefs_ui.rs` (the phone's tabbed Preferences, with upstream's system theme) and `dialogs.rs` (the phone's dialog width, with upstream's Preferences width). Crash recovery on Android (`services.rs`) follows upstream's lazy `Recoverable` list: an entry is read when it is opened. Not synced: the desktop app (`apps/photocraft`), upstream's CI workflows, `xtask`, `docs/`, `book/`, `scorecard/` and `packaging/`, and the corpus data that `xtask` fetches (the corpus tests sit behind a `corpus` feature and do not run here). `Cargo.lock` is not synced: CI resolves it. The new upstream crate `photocraft-affinity` (the Affinity importer) is in the workspace; it adds `ruzstd` and `crc32fast`.

The shared crates are changed only where Android needs it, in a few small places:

- `crates/text/src/fonts.rs` scans `/system/fonts` on Android;
- `crates/ui-egui/src/gpu_canvas.rs` reads the physical memory on Android as on Linux;
- `crates/ui-egui/Cargo.toml` leaves eframe's `accesskit` feature out of the Android build;
- `crates/ui-egui/src/mobile.rs` (new) is the phone layout. It is used only when the shell sets
  `PhotocraftApp::phone_layout` and the window is narrow, so desktop and web keep their chrome. The
  rest of the change is the flag and its use in `lib.rs` and `canvas.rs` (two-finger pan, phone
  only), and `panels.rs`, where the options bar's contents moved into `options_strip` (re-indented
  only), so the phone can scroll them;
- The touch gestures (`mobile::touch_screen`, `mobile::row_sense`, `mobile::held_or_double`) replace
  the drag or the double-click that is a mouse's only way to do something. They apply only on the
  phone layout (`lib.rs` sets `mobile::set_gestures` each frame) and with a touch screen: the layer
  rows and eyes, the Colour chips, the effect rows (`panels.rs`), the
  swatches (`preset_panels.rs`), the brush rows and groups (`brushes_tab.rs`), the gradient stops
  (`gradient_ui.rs`), the smart filters (`smart_ui.rs`), the layer comps (`comps_ui.rs`), the actions
  (`actions.rs`) and the type panels (`type_panels_ui.rs`);
- `crates/ui-egui/src/lib.rs` and `jobs_ui.rs` gained `open_bytes_owned` and `bytes_owned`, which the
  inbox drain uses: a picked file's bytes go to the background open without a second copy. Opening
  from the desktop is unchanged.
- `crates/engine/src/type_extra_cmds.rs` (the Anti-Alias, Orientation and OpenType commands) takes an
  `allTextLayers` flag, which changes every type layer in one step. The phone's menu sends it
  (`mobile.rs`, `menu_params`); the desktop menus don't, so they change the active layer as before.
- `crates/ui-egui/src/lib.rs` gained the `Services::soft_keyboard` hook, which `type_tool.rs` calls
  to show the soft keyboard on each tap into text (and to hide it when the edit ends). winit asks
  for the keyboard implicitly, and Android may ignore an implicit request once the user has
  dismissed the keyboard, which would explain the keyboard coming up only the first time (not yet
  seen on a device). The hook is set
  only on Android; on the desktop and the web it is unset and nothing changes.
- `crates/ui-egui/src/dialogs.rs` and `prefs_ui.rs` fit the dialogs to the phone: a dialog takes the
  width the screen leaves it, a body taller than the screen scrolls above its buttons, and
  Preferences shows its sections as tabs above the fields (`prefs_phone_body`). All of it applies
  only with `mobile::active`; the desktop shows the same dialogs and the same Preferences as before.
- `crates/ui-egui/src/virtual_input.rs` (new) is the on-screen keyboard, a toggle in the phone's top
  bar (`mobile.rs`). The keyboard is a panel in place of the system keyboard, with English and Persian
  layouts and the modifier keys; its events go into the next frame's input in `raw_input_hook`, before
  the clipboard keys, so they take the hardware keys' route. While the keyboard is on, `type_tool.rs`
  does not ask for the system keyboard, and the dialogs and the menu keep clear of the keys
  (`dialogs.rs`, `mobile.rs`). `cjk_fonts.rs` exposes `add_to_all_families`, which the keyboard uses
  to load the system's Arabic font for its Persian keys (Android only).
- `crates/ui-egui/src/virtual_mouse.rs` (new) is the virtual mouse, a toggle in the phone's top bar, and
  the magnifier beside it (shown while the pointer is on). The pointer moves by how far a finger moves,
  not to where the finger is, so placing a finger again does not jump it, and it eases to the finger's
  place, so it glides instead of stepping. A slide presses nothing. A tap clicks where the pointer is.
  Two taps in a row are a double click. A second touch that moves after a tap is a drag, and it starts
  the drag lock: the button stays held across lifts, so a finger that lifts and moves again goes on
  dragging, until another double tap ends the lock (single taps then click nothing). Each lift, a second
  finger, or a press on the screen lets the button go, so a stroke ends where the pointer was, and the
  next movement starts a new one there; a view change never carries a stroke across it. Each stroke is
  an undo step. Two fingers that tap together are a right click; two that move pinch and pan as before.
  The magnifier draws a loupe above the pointer, which samples the canvas picture that `canvas.rs`
  records each frame (`record_view`); a rotated view has no such picture, so its frames forget the last
  one (`forget_view`). `raw_input_hook` takes the presses and moves of the first finger on
  the canvas (and only where no floating window is over it) and sends the pointer's in their place; while
  a press on the screen is held, the pointer sends no moves. `logic` asks for frames while the pointer
  eases. The three top-bar toggles use the Lucide icons `mouse-pointer-2`, `search` and `keyboard`, and their
  labels (Virtual mouse, Magnifier, Virtual keyboard) are in the translations. Off the phone, none of this is
  reachable.
- `crates/doc/src/diag.rs` (new) is a diagnostic log: short timestamped lines, kept in memory (the newest
  4000). The engine writes one for each text edit (the damage, the effects' reach, the re-render's
  pixels: `type_cmds.rs`), the canvas writes one for the repaint that follows (`canvas.rs`), and a text
  layer's effect key is written when it changes (`compose/src/lib.rs`). The log is on only in the phone
  build (`lib.rs`). While it changes, it is written to `Downloads/PhotoCraft/photocraft-log.txt` at once and then at most
  every two seconds (`menus.rs`, `tick_diagnostic_log`). Help › Save Diagnostic Log (listed on the phone only)
  saves it at once. `record_panic` puts a panic in the log without waiting for its lock (the panic hook uses it).
- `crates/engine/src/jobs.rs` writes an open's lines to the same log: the bytes read, the decode's time and
  the document's size, layers and text layers, each import warning (20 at most), and the failure or the
  cancel. The open job is the only path the phone uses, so these are the lines that show a PSD's open.
- `crates/ui-egui/src/canvas.rs` repaints the damage of a single edit grown by the document's effect
  reach (`grown_by_effect_reach`). The one-step path used to skip that margin, so a shadow or glow on a
  text layer kept its old pixels after the text changed. This is the fix for that report, and it is a
  shared change: a desktop canvas repaints the same margin when a layer has an effect, and nothing else
  about it changes.
- `crates/text/src/psd.rs` is upstream's, including `9b7349d` (#1560, #1469): an old PSD writer puts point
  text at a zero PointBase with its ink far from the origin, and its transform cancels that origin, so the
  layout that an edit makes lands far off the canvas. On import, such a transform is folded to the ink
  origin (only for that signature; other PSDs are not changed). The one change of ours is a diagnostic line
  in that fold, which the log notes.

## Diagnostic log

The phone keeps a log of what the app does (`crates/doc/src/diag.rs`, plus the lines the app writes in
`apps/photocraft-android/src/`). It is in `Downloads/PhotoCraft/photocraft-log.txt`, written as it changes, and
Help › Save Diagnostic Log saves it at once. The log has:

- the start of each run, with the version;
- each open: the bytes read, the decode's time, the size, the layers and text layers, the warnings, and the
  failure or cancel (`jobs.rs`); then `shown as document N` when the UI adds it;
- each text edit and its repaint (see the text notes above);
- `frames: …` summaries of the frames whose work (logic and UI, not painting) took more than 33 ms, once every
  five seconds at most, and `long frame: …` for a frame of 100 ms or more (`perf_log.rs`); a smooth stretch
  writes nothing;
- each panic (`PANIC: …`).

A crash can come before a save, so the log's latest copy is also kept in app-private storage (`last-log.txt`),
written with each save and at a panic. A run marks itself as running at launch and clears the mark when the app
is dropped. A launch that finds the mark, and no clean end, saves that copy to Downloads as
`photocraft-log-unclean-<seconds>.txt` (`crash_report.rs`). A process that was killed without ending its run
also produces such a file, so it is not proof of a crash.

## What works in v1

- Launch, touch editing, the full interface.
- **File › Open** (and Open As) through the Storage Access Framework. Several files can be picked.
- **Save**, **Save As** and **Export** write to `Downloads/PhotoCraft` through MediaStore. Saving
  the same name again replaces the copy this app wrote earlier.
- Preferences, crash recovery (autosave and restore) and brush presets live in app-private storage.
- Links (Help, and so on) open in the browser.
- The UI stays clear of the status bar, the navigation bar, the display cutout and the soft keyboard
  (`safe_area_insets`, fed from the window insets each frame).

## Known limitations

- Placing files (Place Embedded/Linked), scripts, presets import and notes import need a
  synchronous file picker, which Android doesn't have. They show a message and do nothing.
- Save doesn't ask for a location. Everything goes to `Downloads/PhotoCraft`. A document opened
  from elsewhere is saved as a new file there; it doesn't write back to its original location.
- Clipboard images, the stylus tilt and eraser (touch force only), and the displays' colour profiles
  are not wired up.
- The phone layout (see the design notes) is untested on a device. Tablets and windows at least
  600 pt on their shorter side keep the desktop layout, which is designed for a large screen.
- Some panels still open as floating windows (the brush settings, for one), which can cover much of
  a phone's screen. The sheet holds the dock's groups; the floating windows are not moved into it.
- Two-finger pan is in place on the canvas, but untested. There's no two-finger rotate.
- On the phone, layers, brushes and groups can't be dragged to reorder, and swatches can't be
  dropped on the canvas: use Menu › Layer › Arrange, and a swatch's menu (Apply). A swipe over these
  rows scrolls the list instead.
- A phone draws the document on the CPU, and the GPU draws only the interface (see the design notes).
  Large documents redraw more slowly: an edit composites the whole document on the CPU, and a
  document wider or taller than 4096 px shows at a reduced size on the canvas. Untested on a device.
- Screen readers (TalkBack) aren't supported: AccessKit is off on Android, because eframe doesn't
  allow it with the NativeActivity backend.
- The APK contains arm64-v8a code only, so it runs on arm64 devices (and arm64 emulator images).
  The library is not stripped: about 64 MB uncompressed, and the APK about 24 MB.
- The virtual keyboard and virtual mouse are unverified on a device: the Persian letters depend on a
  system Arabic font being present (`/system/fonts`), and the keys are not shaped, so a letter shows in
  its isolated form. Their layout (the ISIRI 2901 positions and the Shift symbols) is from memory of the
  standard, not checked against a keyboard. They have no suggestions, no long-press accents, and no
  handwriting. The double-tap window (0.35 s), the pointer's gain and its easing are not tuned on a device,
  and with the pipette tool the tool's own cursor is drawn at the same point as the pointer.
- Nothing was run on a device, so rendering, input, the picker and saves are all unverified.

## Building

You need: a stable Rust toolchain (1.95 or newer), the `aarch64-linux-android` target, Android NDK
`27.2.12479018`, JDK 17, Android SDK platform 35 with build-tools 35.0.0, and Gradle 8.10.2.

**1. Host tests** (no Android toolchain needed; this checks the protocol and the other pure code):

```sh
cargo test -p photocraft-android
```

**2. The native library:**

```sh
export NDK="$ANDROID_HOME/ndk/27.2.12479018"
export BIN="$NDK/toolchains/llvm/prebuilt/linux-x86_64/bin"
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$BIN/aarch64-linux-android29-clang"
export CC_aarch64_linux_android="$BIN/aarch64-linux-android29-clang"
export CXX_aarch64_linux_android="$BIN/aarch64-linux-android29-clang++"
export AR_aarch64_linux_android="$BIN/llvm-ar"

cargo build -p photocraft-android --lib --target aarch64-linux-android --profile android
mkdir -p android/app/src/main/jniLibs/arm64-v8a
cp target/aarch64-linux-android/android/libphotocraft_android.so android/app/src/main/jniLibs/arm64-v8a/
```

The `android` profile (in `Cargo.toml`) is release-optimised without fat LTO, to keep builds
reasonable on a hosted runner.

**3. The APK:**

```sh
gradle -p android :app:assembleDebug
# android/app/build/outputs/apk/debug/app-debug.apk
```

The debug APK is signed with the key that Gradle creates for debug builds, so a CI build and a
local build can have different keys. An app installed from one build may then need to be uninstalled
before another build installs (its data goes with it). A fixed key committed to the repository was tried
and reverted.

Install it with `adb install -r android/app/build/outputs/apk/debug/app-debug.apk`. A GitHub
artifact is a zip file: unzip `photocraft-android-debug-apk` and install `app-debug.apk` from it.

CI also checks the APK without a device (`.github/scripts/inspect-apk.sh`): the library is packaged
for `arm64-v8a` and exports `android_main` and `ANativeActivity_onCreate`, the manifest names the
activity and the library, and the dex files contain the `photocraftCall` method that the library
calls. The results appear as notices on the run.

CI (`.github/workflows/android.yml`) runs the host tests and all three steps, and uploads the APK.
`Cargo.lock` is not taken from upstream. When a build adds dependencies to it, CI commits the updated
lock file back to the branch (`[skip ci]`), which keeps the lock file in step with the code.

## Design notes

- **One shell, one UI.** `PhotocraftApp` (from `photocraft-ui-egui`) is the same app object the
  desktop and web shells create. The Android shell implements `eframe::App` around it, and provides
  `Services` (file access, storage, URLs), the same way the web shell does.
- **Files come in asynchronously.** The picker's files land in the `Services` inbox, which the UI
  drains every frame. The Java side hands each picked file over as an open descriptor (`fd` event,
  no copy); where the provider has no descriptor it copies the file into app cache (`file` event).
  Rust reads either on a `photocraft-read` thread, so the UI never waits on storage, and removes
  the copy afterwards. A document's bytes are held once while it opens.
- **The phone layout.** `mobile.rs` keeps the same panels and tools, arranged for a touch screen: a
  top bar (menus, the name, undo, redo, panels), the options of the active tool above a strip of
  every tool with 44 pt targets, a sheet for the dock's groups, and the menus as a sheet with search.
  The canvas takes what is left. Pinch zooms, and two fingers pan. On the phone, a press held down
  does a double-click's job (the Colour chips open the picker, a gradient stop's colour edits), and
  the rows, eyes and swatches under a finger take clicks, so a swipe scrolls the list. The desktop,
  the web and a tablet's desktop chrome keep the mouse's behaviour, even on a touch screen.
- **The document on a phone is drawn on the CPU.** `set_wgpu` doesn't create the wgpu canvas on
  Android (`gpu_canvas::SHARED_GPU_MEMORY`), so layer pages, effect maps and display textures stay
  off the GPU. A PSD open on a Mali-G720 phone churned 16–32 MB GPU allocations for several seconds,
  then the driver lost the device; egui can't draw after that, so the app ended. The 512 MB budget
  and 2048-px tiles tried before didn't prevent it. The canvas stays on the CPU on phones until a
  device run shows the GPU path is safe. Desktop and web are unchanged.
- **Saves stage bytes first.** The Java side copies from a file, so Rust writes the bytes into
  app-private storage, asks Java to copy them into MediaStore, and removes the staged copy.
- **The JNI surface is one method.** `PhotoCraftActivity.photocraftCall(String, String) -> String`
  takes a command and an argument, and returns `ok`, `ok<SEP>payload` or `err<SEP>message`. Events
  come back one per `poll`. `SEP` is U+001F.
- **`unsafe` is confined to `platform.rs`.** The workspace forbids `unsafe`. This crate is the one
  exception, and only for wrapping the activity's global reference, plus the `android_main` export.

## Licence

PhotoCraft's code and original assets are MIT OR Apache-2.0, as upstream (see `LICENSE-MIT`,
`LICENSE-APACHE` and `NOTICE`). Third-party assets keep their own licences, listed in
`ATTRIBUTION.md`.
