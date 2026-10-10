//! Frame timing for the diagnostic log (see `shell.rs`). Each frame's work is the time its logic and its UI took,
//! on the UI thread. Painting and the GPU are not in it, and neither is the time between frames, which is idle
//! while nothing asks for one. A frame whose work takes long is noted on its own; a window of frames with slow ones
//! is noted as a summary, so the log shows where the interface stalls without a line for every frame.

use std::time::{Duration, Instant};

/// A frame whose work takes at least this long is noted on its own.
pub const LONG_FRAME: Duration = Duration::from_millis(100);
/// A frame whose work takes longer than this misses the budget of a 30 frames-a-second screen. It counts as slow.
pub const SLOW_FRAME: Duration = Duration::from_millis(33);
/// The span one summary covers.
pub const WINDOW: Duration = Duration::from_secs(5);

/// The frames of the current window, and the lines they have produced.
#[derive(Default)]
pub struct FrameLog {
    window: Option<Window>,
}

/// What one window of frames adds up to.
struct Window {
    began: Instant,
    ended: Instant,
    frames: u32,
    work: Duration,
    worst: Duration,
    slow: u32,
}

impl Window {
    fn new(began: Instant) -> Self {
        Self { began, ended: began, frames: 0, work: Duration::ZERO, worst: Duration::ZERO, slow: 0 }
    }

    fn add(&mut self, start: Instant, work: Duration) {
        self.ended = start;
        self.frames += 1;
        self.work += work;
        self.worst = self.worst.max(work);
        if work > SLOW_FRAME {
            self.slow += 1;
        }
    }

    /// The summary line, when the window has slow frames (a smooth window is not noted).
    fn summary(&self) -> Option<String> {
        if self.slow == 0 || self.frames == 0 {
            return None;
        }
        let span = self.ended.saturating_duration_since(self.began).as_secs_f64();
        let average = self.work.as_secs_f64() * 1000.0 / f64::from(self.frames);
        Some(format!(
            "frames: {} in {span:.1} s, average work {average:.1} ms, slowest {} ms, {} over {} ms",
            self.frames,
            self.worst.as_millis(),
            self.slow,
            SLOW_FRAME.as_millis()
        ))
    }
}

impl FrameLog {
    /// Records a frame that started at `start` and whose work took `work`. Returns the lines to note: the frame's own
    /// line when it is long, and the window's summary when a window has closed with slow frames in it.
    pub fn record(&mut self, start: Instant, work: Duration) -> Vec<String> {
        let mut lines = Vec::new();
        if work >= LONG_FRAME {
            lines.push(format!("long frame: {} ms of work", work.as_millis()));
        }
        let window = self.window.get_or_insert_with(|| Window::new(start));
        window.add(start, work);
        if start.saturating_duration_since(window.began) >= WINDOW {
            lines.extend(window.summary());
            self.window = Some(Window::new(start));
        }
        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn a_smooth_window_notes_nothing() {
        let t0 = Instant::now();
        let mut log = FrameLog::default();
        for i in 0..40 {
            assert!(log.record(t0 + ms(i * 100), ms(4)).is_empty(), "frame {i}");
        }
    }

    #[test]
    fn a_long_frame_is_noted_on_its_own() {
        let t0 = Instant::now();
        let mut log = FrameLog::default();
        let lines = log.record(t0, ms(120));
        assert_eq!(lines, vec!["long frame: 120 ms of work".to_string()]);
        assert!(log.record(t0 + ms(10), ms(99)).is_empty(), "under the limit is not noted");
    }

    #[test]
    fn a_window_with_slow_frames_is_summarised_when_it_closes() {
        let t0 = Instant::now();
        let mut log = FrameLog::default();
        assert!(log.record(t0, ms(10)).is_empty());
        assert!(log.record(t0 + ms(100), ms(50)).is_empty(), "the window is still open");
        assert!(log.record(t0 + ms(200), ms(20)).is_empty());
        let lines = log.record(t0 + WINDOW, ms(4));
        assert_eq!(lines.len(), 1, "{lines:?}");
        let line = &lines[0];
        assert!(line.contains("frames: 4 in 5.0 s"), "the frames and the span: {line}");
        assert!(line.contains("average work 21.0 ms"), "(10 + 50 + 20 + 4) / 4: {line}");
        assert!(line.contains("slowest 50 ms"), "{line}");
        assert!(line.contains("1 over 33 ms"), "only the 50 ms frame is over the limit: {line}");
    }

    #[test]
    fn a_new_window_starts_with_the_frame_that_closed_the_last() {
        let t0 = Instant::now();
        let mut log = FrameLog::default();
        let _ = log.record(t0, ms(40));
        // The next window starts at the closing frame, so its summary counts only the frames after it.
        let _ = log.record(t0 + WINDOW, ms(2));
        let lines = log.record(t0 + WINDOW + ms(500), ms(60));
        assert!(lines.is_empty(), "the window is still open");
        let lines = log.record(t0 + WINDOW * 2, ms(1));
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(lines[0].contains("frames: 2 in 5.0 s"), "{}", lines[0]);
        assert!(lines[0].contains("1 over 33 ms"), "{}", lines[0]);
    }
}
