//! Active CPU core limit for parallel work (compositing, background jobs, redraws).
//!
//! By default (`ACTIVE_CORES == 0`) callers keep their existing thread pool (the global `rayon`
//! pool or a test's custom pool) and [`active_cores`] reports [`max_cores`]. Once the user picks a
//! core count with [`set_active_cores`], [`install`] runs parallel work inside a `rayon::ThreadPool`
//! sized to that exact core count, and scoped-thread callers read [`active_cores`].

use std::sync::atomic::{AtomicUsize, Ordering};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::{Arc, Mutex};

/// `0` means "not overridden by the user; report [`max_cores`] and keep the caller's pool".
static ACTIVE_CORES: AtomicUsize = AtomicUsize::new(0);

#[cfg(not(target_arch = "wasm32"))]
static POOL: Mutex<Option<(usize, Arc<rayon::ThreadPool>)>> = Mutex::new(None);

/// Total logical CPU cores available on this device (`1` on `wasm32`).
pub fn max_cores() -> usize {
    #[cfg(target_arch = "wasm32")]
    {
        1
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::thread::available_parallelism().map_or(1, |n| n.get()).max(1)
    }
}

/// Currently configured active CPU core count (`1..=max_cores()`).
pub fn active_cores() -> usize {
    let max = max_cores();
    let raw = ACTIVE_CORES.load(Ordering::Relaxed);
    if raw == 0 { max } else { raw.clamp(1, max) }
}

/// Sets the active CPU core count (clamped to `1..=max_cores()`) and returns the applied count.
pub fn set_active_cores(cores: usize) -> usize {
    let max = max_cores();
    let clamped = cores.clamp(1, max);
    ACTIVE_CORES.store(clamped, Ordering::Relaxed);
    crate::diag::note(|| format!("CPU active cores set to {clamped}/{max}"));
    clamped
}

/// Resets the active CPU core override back to the default (`0`, meaning [`max_cores`] without
/// overriding custom test thread pools).
pub fn reset_active_cores() {
    ACTIVE_CORES.store(0, Ordering::Relaxed);
}

/// Runs `op` inside the configured `rayon` thread pool when an active CPU core count has been set.
pub fn install<R: Send>(op: impl FnOnce() -> R + Send) -> R {
    #[cfg(target_arch = "wasm32")]
    {
        op()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let raw = ACTIVE_CORES.load(Ordering::Relaxed);
        if raw == 0 {
            return op();
        }
        let wanted = raw.clamp(1, max_cores());
        if rayon::current_num_threads() == wanted {
            return op();
        }
        let pool = {
            let mut guard = POOL.lock().unwrap_or_else(|e| e.into_inner());
            if let Some((threads, existing)) = guard.as_ref()
                && *threads == wanted
            {
                Some(existing.clone())
            } else {
                match rayon::ThreadPoolBuilder::new().num_threads(wanted).thread_name(|i| format!("photocraft-cpu-{i}")).build() {
                    Ok(built) => {
                        let arc = Arc::new(built);
                        *guard = Some((wanted, arc.clone()));
                        Some(arc)
                    }
                    Err(_) => None,
                }
            }
        };
        match pool {
            Some(p) => p.install(op),
            None => op(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_cores_clamps_and_installs_matching_rayon_pool() {
        let max = max_cores();
        assert!(max >= 1);
        assert_eq!( set_active_cores(0), 1);
        assert_eq!(active_cores(), 1);
        let observed = install(rayon::current_num_threads);
        assert_eq!(observed, 1);
        assert_eq!(set_active_cores(max.saturating_add(10)), max);
        assert_eq!(active_cores(), max);
        reset_active_cores();
        assert_eq!(active_cores(), max);
    }
}
