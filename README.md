# PhotoDroid: PhotoCraft for Android

**English** | [فارسی](README.fa.md)

PhotoDroid is an Android port of [PhotoCraft](https://github.com/storytold/photocraft), with its own Android shell, touch-oriented phone interface, system file picker, storage and diagnostics. Core code and assets are MIT OR Apache-2.0; see the licences, `NOTICE` and `ATTRIBUTION.md`.

## Upstream sync

The last PhotoCraft commit integrated on this branch is **`d2330fccea04d886c93fe7ee35757b72b5db6aff`**, merged in PhotoDroid commit `1ee4dd01a97ab448cef6cdba407d7c1f678a35ab`. The previous baseline was `cece50d5`. Later PhotoDroid commits contain Android-specific changes; files need not be identical to upstream.

For the next sync, fetch PhotoCraft and review commits and changes *after* the baseline, through the target you choose. For example, after configuring the `upstream` remote:

```sh
git fetch upstream
git log --oneline d2330fccea04d886c93fe7ee35757b72b5db6aff..upstream/main
git diff --stat d2330fccea04d886c93fe7ee35757b72b5db6aff upstream/main
```

Work on a branch, not `main`. Integrate the relevant changes while preserving PhotoDroid's Android shell, phone layout, touch/input, file storage and diagnostics. Resolve overlapping changes carefully instead of blindly replacing fork-specific code. The desktop app, `crates/automation`, `crates/tablet`, upstream workflows, `xtask`, documentation, book, scorecard and packaging are outside this Android port. Do not reintroduce an update checker or Internet permission. Run the Android build and host/interface tests, test on a device, and update the baseline above only to the upstream target actually integrated.

## Repository and testing

- `crates/`: shared engine and UI, with Android adaptations.
- `apps/photocraft-android/` and `android/`: Android Rust shell and Java activity.
- `.github/workflows/android.yml`: arm64 APK build and host/interface tests. Run it on the working branch; the APK is uploaded as an artifact.
- `Download/PhotoCraft/Projects` holds project files; `Output` holds other exports; `Fonts` and `log` hold user fonts and diagnostics. Save As opens the Android system picker for a user-selected destination.

CI and device tests complement each other; a successful build alone does not establish that every feature works on a device.
