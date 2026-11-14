//! On the phone two fingers on the canvas pan and zoom the view; they must not paint, select or move anything.
//!
//! A finger alone already drives the active tool, so by the time the second finger lands the first one has started a
//! stroke. When the second finger comes soon after the first ([`QUICK_SECS`], the way a pinch starts), what the first
//! one started is dropped, as if it never happened; when it comes later (a long stroke, then a pinch), the stroke is
//! kept and ends there. Either way the tool gets nothing more until every finger has lifted, the lift's own release
//! included, so the end of a pinch never clicks. An open lasso outline stays open (`lasso_ui::pinched`).
//!
//! With the virtual mouse on, the fingers do not drive the tool directly (`virtual_mouse.rs` handles two fingers).

use crate::PhotocraftApp;
use crate::canvas::ToolEvent;
use crate::state::Tool;

/// A second finger within this many seconds of the first makes a pinch from the start: the first finger's stroke goes.
pub const QUICK_SECS: f64 = 0.35;

#[derive(Clone, Copy, Debug, Default)]
struct Guard {
    /// When the first finger of the touches came down.
    since: Option<f64>,
    /// Two fingers have been down since; the tool gets nothing until they have all lifted.
    latched: bool,
}

fn guard_id() -> egui::Id {
    egui::Id::unique("pc-touch-guard")
}

/// Whether the tool must get nothing this frame, because two fingers are (or were, until every finger has lifted) on the
/// screen. Drops or ends what the first finger started, once, as the second lands.
pub fn pinching(app: &mut PhotocraftApp, ctx: &egui::Context) -> bool {
    if !app.phone_layout || app.virtual_mouse.is_on() {
        return false;
    }
    let (touching, multi, now) = ctx.input(|i| (i.any_touches(), i.multi_touch().is_some(), i.time));
    let mut g: Guard = ctx.data(|d| d.get_temp(guard_id())).unwrap_or_default();
    if !touching {
        // The frame the fingers lift is still the pinch's: its release must not reach the tool as a click.
        let held = g.latched;
        ctx.data_mut(|d| d.insert_temp(guard_id(), Guard::default()));
        return held;
    }
    let since = *g.since.get_or_insert(now);
    if multi && !g.latched {
        g.latched = true;
        drop_or_end(app, now - since <= QUICK_SECS);
    }
    ctx.data_mut(|d| d.insert_temp(guard_id(), g));
    g.latched
}

/// What the first finger started, as the second lands: dropped when it had only just started (`early`), ended where it
/// is otherwise.
fn drop_or_end(app: &mut PhotocraftApp, early: bool) {
    app.press_stroke = false;
    let Some(d) = app.drag.as_ref() else { return };
    if d.tool == Tool::Lasso && d.lasso.is_some() {
        crate::lasso_ui::pinched(app, early);
        return;
    }
    let (tool, last) = (d.tool, d.points.last().map_or(d.start, |p| [p[0], p[1]]));
    if early && (crate::canvas::strokes_live(tool) || selects(tool)) {
        app.drag = None;
        app.live_stroke = None;
        return;
    }
    crate::canvas::tool_event(app, ToolEvent::Up { x: last[0], y: last[1] }, egui::Modifiers::NONE);
}

/// The tools whose drag only draws an outline, so dropping it leaves nothing behind.
fn selects(tool: Tool) -> bool {
    matches!(tool, Tool::RectMarquee | Tool::EllipseMarquee | Tool::Lasso)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_quick_second_finger_drops_the_stroke_and_a_late_one_keeps_it() {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
        app.run("file.new", serde_json::json!({"width": 120, "height": 60})).unwrap();
        app.ui.tool = Tool::Brush;
        let m = egui::Modifiers::NONE;
        let steps = |app: &PhotocraftApp| app.session.documents()[0].history.past_len();
        let before = steps(&app);
        crate::canvas::tool_event(&mut app, ToolEvent::Down { x: 10.0, y: 30.0, pressure: 1.0 }, m);
        crate::canvas::tool_event(&mut app, ToolEvent::Move { x: 20.0, y: 30.0, pressure: 1.0 }, m);
        drop_or_end(&mut app, true);
        assert!(app.drag.is_none() && app.live_stroke.is_none());
        assert_eq!(steps(&app), before, "the dropped stroke left no history step");
        crate::canvas::tool_event(&mut app, ToolEvent::Down { x: 10.0, y: 30.0, pressure: 1.0 }, m);
        for x in [30.0, 50.0, 70.0] {
            crate::canvas::tool_event(&mut app, ToolEvent::Move { x, y: 30.0, pressure: 1.0 }, m);
        }
        drop_or_end(&mut app, false);
        assert!(app.drag.is_none());
        assert_eq!(steps(&app), before + 1, "the long stroke is kept, as one step");
    }

    #[test]
    fn off_the_phone_nothing_is_guarded() {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
        let ctx = egui::Context::default();
        assert!(!pinching(&mut app, &ctx));
    }
}
