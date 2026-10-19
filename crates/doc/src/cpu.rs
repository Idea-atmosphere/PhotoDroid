//! Active CPU core limit for parallel work (compositing, background jobs, redraws).
//!
//! By default (`ACTIVE_CORES == 0`) callers keep their existing thread pool (the global `rayon`
//! pool or a test's custom pool) and [`active_cores`] reports [`max_cores`]. Once the user picks a
//! core count with [`set_active_cores`], [`install`] runs parallel work inside a `rayon::ThreadPool`
//! sized to that exact core count, and scoped-thread callers read [`active_cores`].
//!
//! On Android big.LITTLE / DynamIQ phones (e.g. 6 efficiency + 2 prime cores, `cpu0..cpu7`),
//! `std::thread::available_parallelism()` (`sched_getaffinity` / `_SC_NPROCESSORS_ONLN`) can
//! fluctuate frame-to-frame between 6 and 8 as the OS governor parks or unparks prime cores 7 & 8.
//! [`max_cores`] therefore caches the total hardware core count once per process from sysfs
//! (`/sys/devices/system/cpu/present`, `/sys/devices/system/cpu/possible`, and `/sys/devices/system/cpu/cpu[0-9]*`)
//! combined with `available_parallelism()`, so the UI selector never flickers and cores 7 & 8 stay
//! stably selectable.

use std::sync::atomic::{AtomicUsize, Ordering};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::{Arc, Mutex, OnceLock};

/// `0` means "not overridden by the user; report [`max_cores`] and keep the caller's pool".
static ACTIVE_CORES: AtomicUsize = AtomicUsize::new(0);

#[cfg(not(target_arch = "wasm32"))]
static MAX_CORES_CACHE: OnceLock<usize> = OnceLock::new();

#[cfg(not(target_arch = "wasm32"))]
static POOL: Mutex<Option<(usize, Arc<rayon::ThreadPool>)>> = Mutex::new(None);

/// Parses a Linux sysfs CPU range list such as `"0-7"`, `"0-5"`, or `"0-3,4-7"` into a total CPU
/// count (`8`, `6`, `8`).
pub fn parse_cpu_list(text: &str) -> Option<usize> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut count = 0usize;
    let mut max_index: Option<usize> = None;
    for part in trimmed.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some((start_s, end_s)) = part.split_once('-') {
            let start = start_s.trim().parse::<usize>().ok()?;
            let end = end_s.trim().parse::<usize>().ok()?;
            if end < start {
                return None;
            }
            count = count.saturating_add(end - start + 1);
            max_index = Some(max_index.map_or(end, |m| m.max(end)));
        } else {
            let idx = part.parse::<usize>().ok()?;
            count = count.saturating_add(1);
            max_index = Some(max_index.map_or(idx, |m| m.max(idx)));
        }
    }
    let by_max = max_index.map_or(0, |m| m.saturating_add(1));
    let total = count.max(by_max);
    (total > 0).then_some(total)
}

#[cfg(not(target_arch = "wasm32"))]
fn read_sysfs_cpu_list(path: &str) -> Option<usize> {
    let text = std::fs::read_to_string(path).ok()?;
    parse_cpu_list(&text)
}

#[cfg(not(target_arch = "wasm32"))]
fn count_sysfs_cpu_dirs() -> Option<usize> {
    let entries = std::fs::read_dir("/sys/devices/system/cpu").ok()?;
    let mut count = 0usize;
    let mut max_id: Option<usize> = None;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(s) = name.to_str() else { continue };
        let Some(rest) = s.strip_prefix("cpu") else { continue };
        if !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit()) {
            count += 1;
            if let Ok(id) = rest.parse::<usize>() {
                max_id = Some(max_id.map_or(id, |m| m.max(id)));
            }
        }
    }
    let by_max = max_id.map_or(0, |m| m.saturating_add(1));
    let total = count.max(by_max);
    (total > 0).then_some(total)
}

#[cfg(not(target_arch = "wasm32"))]
fn detect_hardware_cores() -> usize {
    let parallelism = std::thread::available_parallelism().map_or(1, |n| n.get());
    let present = read_sysfs_cpu_list("/sys/devices/system/cpu/present").unwrap_or(0);
    let possible = read_sysfs_cpu_list("/sys/devices/system/cpu/possible").unwrap_or(0);
    let dirs = count_sysfs_cpu_dirs().unwrap_or(0);
    parallelism.max(present).max(possible).max(dirs).max(1)
}

/// Total logical CPU cores present on this device (`1` on `wasm32`). Cached once per process so
/// OS governor hotplugging or affinity changes on cores 7 & 8 never cause UI flickering.
pub fn max_cores() -> usize {
    #[cfg(target_arch = "wasm32")]
    {
        1
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        *MAX_CORES_CACHE.get_or_init(detect_hardware_cores)
    }
}

/// Current OS-scheduled parallelism (`available_parallelism()`), which may be lower than
/// [`max_cores`] when the Android governor temporarily parks prime cores.
pub fn current_os_parallelism() -> usize {
    #[cfg(target_arch = "wasm32")]
    {
        1
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::thread::available_parallelism().map_or(1, |n| n.get()).max(1)
    }
}

/// Snapshot of CPU topology and governor state for the diagnostic `.txt` log.
pub fn diag_summary() -> String {
    #[cfg(target_arch = "wasm32")]
    {
        format!("active={}/{} os_avail=1 (wasm32)", active_cores(), max_cores())
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let read_trim = |p: &str| {
            std::fs::read_to_string(p)
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|_| "n/a".to_string())
        };
        let present = read_trim("/sys/devices/system/cpu/present");
        let online = read_trim("/sys/devices/system/cpu/online");
        let possible = read_trim("/sys/devices/system/cpu/possible");
        let offline = read_trim("/sys/devices/system/cpu/offline");
        let avail = current_os_parallelism();
        format!(
            "active={}/{} os_avail={} present={present} online={online} offline={offline} possible={possible}",
            active_cores(),
            max_cores(),
            avail
        )
    }
}

/// Records a diagnostic log entry whenever the OS governor changes `online` cores or
/// `available_parallelism` (e.g. parking/unparking cores 7 & 8 on big.LITTLE Android SoCs).
pub fn note_governor_state() {
    if !crate::diag::enabled() {
        return;
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let avail = current_os_parallelism() as u64;
        let online_text = std::fs::read_to_string("/sys/devices/system/cpu/online")
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        let online_count = parse_cpu_list(&online_text).unwrap_or(avail as usize) as u64;
        let key = (avail << 32) | (online_count & 0xffff_ffff) ^ ((active_cores() as u64) << 16);
        crate::diag::note_changed(0x4350_5501, key, || {
            format!("CPU state: {}", diag_summary())
        });
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
    let prev = active_cores();
    ACTIVE_CORES.store(clamped, Ordering::Relaxed);
    crate::diag::note(|| {
        format!(
            "CPU active cores set: requested={cores} applied={clamped}/{max} (prev={prev}, {})",
            diag_summary()
        )
    });
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
    fn parse_cpu_list_handles_android_sysfs_formats() {
        assert_eq!(parse_cpu_list("0-7\n"), Some(8));
        assert_eq!(parse_cpu_list("0-5\n"), Some(6));
        assert_eq!(parse_cpu_list("0-3,4-7"), Some(8));
        assert_eq!(parse_cpu_list("0,2,4-7"), Some(8));
        assert_eq!(parse_cpu_list("0"), Some(1));
        assert_eq!(parse_cpu_list(""), None);
        assert_eq!(parse_cpu_list("7-2"), None);
    }

    #[test]
    fn active_cores_clamps_and_installs_matching_rayon_pool() {
        let max = max_cores();
        assert!(max >= 1);
        assert_eq!(max_cores(), max, "cached hardware core count never fluctuates");
        assert_eq!(set_active_cores(0), 1);
        assert_eq!(active_cores(), 1);
        let observed = install(rayon::current_num_threads);
        assert_eq!(observed, 1);
        assert_eq!(set_active_cores(max.saturating_add(10)), max);
        assert_eq!(active_cores(), max);
        reset_active_cores();
        assert_eq!(active_cores(), max);
    }
}
