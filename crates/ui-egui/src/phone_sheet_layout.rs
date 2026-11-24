//! Keep the portrait phone sheet's top edge fixed when the tool strips change visibility.
//!
//! egui remembers a resizable panel's height, not its top edge. When the strips below it
//! disappear, the panel's bottom moves down; its remembered height must grow by the same amount.

use egui::{Id, Rect, Ui, containers::panel::PanelState};

pub(crate) const PANEL_ID: &str = "phone-sheet";

#[derive(Clone, Copy)]
struct PreviousLayout {
    tools_hidden: bool,
    rect: Rect,
}

fn panel_id(ui: &Ui) -> Id {
    ui.scope_id().with(PANEL_ID)
}

fn previous_id(ui: &Ui) -> Id {
    panel_id(ui).with("phone-sheet-previous-layout")
}

/// Adjust the remembered size only when the *drawn* strips change visibility. The caller passes
/// the state used to lay out this frame, which can differ from the flag set by a swipe mid-frame.
pub(crate) fn prepare(ui: &Ui, tools_hidden: bool) {
    let previous = ui.data(|d| d.get_temp::<PreviousLayout>(previous_id(ui)));
    if let Some(previous) = previous.filter(|p| p.tools_hidden != tools_hidden) {
        let bottom = ui.available_rect_before_wrap().bottom();
        let mut rect = previous.rect;
        rect.max.y = bottom;
        // A very small viewport or keyboard can temporarily leave less room than before; egui's
        // size range and available space will clamp this safely when it draws the panel.
        rect.min.y = rect.min.y.min(bottom);
        ui.data_mut(|d| d.insert_persisted(panel_id(ui), PanelState { outer_rect: rect }));
    }
}

/// Remember the actual rendered rectangle, including the latest manual resize.
pub(crate) fn finish(ui: &Ui, tools_hidden: bool, rect: Rect) {
    ui.data_mut(|d| d.insert_temp(previous_id(ui), PreviousLayout { tools_hidden, rect }));
}

/// Do not compare a newly opened portrait sheet with an old or landscape layout.
pub(crate) fn reset(ui: &Ui) {
    ui.data_mut(|d| d.remove::<PreviousLayout>(previous_id(ui)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{pos2, vec2, Context, Frame, Panel, RawInput};

    fn draw(ctx: &Context, hidden: bool, keyboard: f32, events: Vec<egui::Event>) -> Rect {
        let mut result = None;
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(411.0, 891.0))),
            events,
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| {
            if keyboard > 0.0 {
                Panel::bottom("keys").exact_size(keyboard).show(ui, |_| {});
            }
            if !hidden {
                Panel::bottom("tools").exact_size(56.0).show(ui, |_| {});
                Panel::bottom("options").exact_size(44.0).show(ui, |_| {});
            }
            prepare(ui, hidden);
            let panel = Panel::bottom(PANEL_ID).resizable(true)
                .default_size(360.0 + if hidden { 100.0 } else { 0.0 })
                .size_range(220.0..=(760.0 + if hidden { 100.0 } else { 0.0 }))
                .frame(Frame::NONE)
                .show(ui, |ui| ui.take_available_space());
            finish(ui, hidden, panel.response.rect);
            result = Some(panel.response.rect);
        });
        out.textures_delta.clear();
        result.unwrap()
    }

    fn close(a: f32, b: f32) {
        assert!((a - b).abs() <= 1.5, "expected {a} to be near {b}");
    }

    #[test]
    fn hiding_tools_expands_the_sheet_and_restoring_keeps_its_top_edge() {
        let ctx = Context::default();
        let initial = draw(&ctx, true, 0.0, vec![]);
        let shown = draw(&ctx, false, 0.0, vec![]);
        close(initial.top(), shown.top());
        close(initial.height() - shown.height(), 100.0);
        let hidden = draw(&ctx, true, 0.0, vec![]);
        close(hidden.top(), initial.top());
        close(hidden.height(), initial.height());
    }

    #[test]
    fn latest_resize_and_non_default_keyboard_space_are_respected() {
        let ctx = Context::default();
        let start = draw(&ctx, false, 80.0, vec![]);
        // Resize using the actual egui handle, including its interaction and release frames.
        let at = pos2(start.center().x, start.top());
        let moved = at - vec2(0.0, 80.0);
        let pointer = |pos, pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() };
        draw(&ctx, false, 80.0, vec![egui::Event::PointerMoved(at)]);
        draw(&ctx, false, 80.0, vec![pointer(at, true)]);
        draw(&ctx, false, 80.0, vec![egui::Event::PointerMoved(moved)]);
        let resized = draw(&ctx, false, 80.0, vec![pointer(moved, false)]);
        assert!(resized.height() > start.height() + 50.0, "actual panel handle must resize: {start:?} -> {resized:?}");
        let hidden = draw(&ctx, true, 80.0, vec![]);
        close(resized.top(), hidden.top());
        close(hidden.height() - resized.height(), 100.0);
        let shown = draw(&ctx, false, 80.0, vec![]);
        close(shown.top(), resized.top());
        close(shown.height(), resized.height());
    }

    #[test]
    fn a_large_manually_resized_sheet_can_use_the_freed_space() {
        let ctx = Context::default();
        let start = draw(&ctx, false, 0.0, vec![]);
        let at = pos2(start.center().x, start.top());
        let moved = pos2(at.x, 51.0);
        let pointer = |pos, pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() };
        draw(&ctx, false, 0.0, vec![egui::Event::PointerMoved(at)]);
        draw(&ctx, false, 0.0, vec![pointer(at, true)]);
        draw(&ctx, false, 0.0, vec![egui::Event::PointerMoved(moved)]);
        let resized = draw(&ctx, false, 0.0, vec![pointer(moved, false)]);
        assert!(resized.height() > 700.0);
        let hidden = draw(&ctx, true, 0.0, vec![]);
        close(hidden.top(), resized.top());
        close(hidden.height() - resized.height(), 100.0);
    }

    #[test]
    fn reset_does_not_apply_a_stale_transition_after_reopening() {
        let ctx = Context::default();
        let shown = draw(&ctx, false, 0.0, vec![]);
        let mut out = ctx.run_ui(RawInput::default(), |ui| reset(ui));
        out.textures_delta.clear();
        let reopened = draw(&ctx, true, 0.0, vec![]);
        close(reopened.height(), shown.height());
    }

    #[test]
    fn clamp_respects_available_room_and_does_not_repeat_adjustment() {
        let ctx = Context::default();
        let first = draw(&ctx, false, 400.0, vec![]);
        let hidden = draw(&ctx, true, 400.0, vec![]);
        assert!(hidden.top() >= 0.0 && hidden.bottom() <= 491.0);
        assert!(hidden.height() >= first.height());
        let repeated = draw(&ctx, true, 400.0, vec![]);
        close(hidden.height(), repeated.height());
    }
}
