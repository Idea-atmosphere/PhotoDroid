//! Diagnostic-only summary of input delivery to the phone strips. These are UI-pass times, not
//! hardware touch timestamps or screen presentation times. No text or individual positions are kept.

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct SwipeCadence {
    frames: u32,
    touch_moves: u32,
    pointer_moves: u32,
    move_frames: u32,
    largest_batch: u32,
    start_time: Option<f64>,
    last_frame_time: Option<f64>,
    last_move_time: Option<f64>,
    max_frame_gap: f64,
    max_move_gap: f64,
}

impl SwipeCadence {
    pub(crate) fn observe(&mut self, now: f64, touch_moves: u32, pointer_moves: u32) {
        if !now.is_finite() {
            return;
        }
        self.start_time.get_or_insert(now);
        if let Some(last) = self.last_frame_time {
            self.max_frame_gap = self.max_frame_gap.max((now - last).max(0.0));
        }
        self.last_frame_time = Some(now);
        self.frames = self.frames.saturating_add(1);
        self.touch_moves = self.touch_moves.saturating_add(touch_moves);
        self.pointer_moves = self.pointer_moves.saturating_add(pointer_moves);
        self.largest_batch = self.largest_batch.max(touch_moves);
        if touch_moves > 0 || pointer_moves > 0 {
            self.move_frames = self.move_frames.saturating_add(1);
            if let Some(last) = self.last_move_time {
                self.max_move_gap = self.max_move_gap.max((now - last).max(0.0));
            }
            self.last_move_time = Some(now);
        }
    }

    pub(crate) fn summary(self) -> String {
        let duration = match (self.start_time, self.last_frame_time) {
            (Some(start), Some(end)) => (end - start).max(0.0),
            _ => 0.0,
        };
        format!(
            "UI-pass input cadence: span={duration:.3}s passes={} move_passes={} touch_moves={} pointer_moves={} max_touch_batch={} max_pass_gap={:.0}ms max_move_gap={:.0}ms (not display FPS or hardware timestamps)",
            self.frames,
            self.move_frames,
            self.touch_moves,
            self.pointer_moves,
            self.largest_batch,
            self.max_frame_gap * 1000.0,
            self.max_move_gap * 1000.0,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_frames_and_separates_sparse_input_from_ui_gaps() {
        let mut stats = SwipeCadence::default();
        stats.observe(1.0, 0, 0);
        stats.observe(1.02, 2, 2);
        stats.observe(1.04, 0, 0);
        stats.observe(1.22, 1, 0);
        let s = stats.summary();
        assert!(s.contains("passes=4 move_passes=2 touch_moves=3 pointer_moves=2"), "{s}");
        assert!(s.contains("max_touch_batch=2 max_pass_gap=180ms max_move_gap=200ms"), "{s}");
    }

    #[test]
    fn handles_empty_and_non_monotonic_times() {
        let mut stats = SwipeCadence::default();
        stats.observe(f64::NAN, 10, 10);
        stats.observe(3.0, 1, 1);
        stats.observe(2.0, 1, 1);
        let s = stats.summary();
        assert!(s.contains("span=0.000s passes=2"), "{s}");
        assert!(s.contains("max_pass_gap=0ms max_move_gap=0ms"), "{s}");
    }
}
