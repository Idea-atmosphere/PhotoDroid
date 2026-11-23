//! Sample the pixel under the loupe without blocking touch input. At most one bounded worker runs
//! at a time; stale samples are never used for another document, revision, or pixel.

use egui::Context;
use photocraft_doc::Document;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const MIN_INTERVAL: Duration = Duration::from_millis(200);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Key {
    doc: u64,
    revision: u64,
    x: i32,
    y: i32,
}

#[derive(Default)]
struct State {
    current: Option<Key>,
    cached: Option<(Key, Option<[f32; 4]>)>,
    busy: bool,
    last_started: Option<Instant>,
}

/// The current composite pixel when ready. Sampling may involve expensive effects, so it never
/// runs on the UI thread. While pending or off the image, the loupe uses a two-colour outline.
#[cfg(not(target_arch = "wasm32"))]
pub(super) fn pixel(ctx: &Context, doc: Arc<Document>, revision: u64, point: [f64; 2]) -> Option<[f32; 4]> {
    let id = egui::Id::unique("photodroid-loupe-contrast-worker");
    let state = ctx.data_mut(|data| {
        if let Some(state) = data.get_temp::<Arc<Mutex<State>>>(id) {
            state
        } else {
            let state = Arc::new(Mutex::new(State::default()));
            data.insert_temp(id, state.clone());
            state
        }
    });
    let key = photocraft_engine::cmd::sample_cmds::sample_rect(point[0], point[1], 1, doc.bounds()).map(|r| Key { doc: doc.id.0, revision, x: r.x0, y: r.y0 });
    let mut s = state.lock().unwrap_or_else(|e| e.into_inner());
    s.current = key;
    let key = key?;
    if let Some((_, pixel)) = s.cached.filter(|(cached, _)| *cached == key) {
        return pixel;
    }
    if s.busy {
        return None;
    }
    if let Some(remaining) = s.last_started.and_then(|t| MIN_INTERVAL.checked_sub(t.elapsed())) {
        ctx.request_repaint_after(remaining);
        return None;
    }
    s.busy = true;
    s.last_started = Some(Instant::now());
    drop(s);
    let worker_state = state.clone();
    let worker_ctx = ctx.clone();
    let spawned = std::thread::Builder::new().name("loupe-contrast".into()).spawn(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            use photocraft_engine::cmd::sample_cmds::{SampleLayers, sample_color};
            sample_color(&doc, None, f64::from(key.x), f64::from(key.y), 1, SampleLayers::All)
        }));
        let mut s = worker_state.lock().unwrap_or_else(|e| e.into_inner());
        s.busy = false;
        if s.current == Some(key) {
            // A failed sample falls back to the halo and is not retried until the key changes.
            s.cached = Some((key, result.ok()));
        }
        drop(s);
        worker_ctx.request_repaint();
    });
    if spawned.is_err() {
        let mut s = state.lock().unwrap_or_else(|e| e.into_inner());
        s.busy = false;
        ctx.request_repaint_after(MIN_INTERVAL);
    }
    None
}

#[cfg(target_arch = "wasm32")]
pub(super) fn pixel(_ctx: &Context, _doc: Arc<Document>, _revision: u64, _point: [f64; 2]) -> Option<[f32; 4]> {
    None
}
