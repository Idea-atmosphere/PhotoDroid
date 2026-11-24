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

/// A dedicated, touch-sized handle above the panel's tabs. It is separate from both the
/// scrollable contents and egui's narrow resize edge, so finger drags do not scroll the list.
pub(crate) fn grip(ui: &mut Ui) -> egui::Response {
    let (row, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 44.0), egui::Sense::hover());
    let hit = Rect::from_center_size(row.center(), egui::vec2(row.width().min(140.0), 44.0));
    let response = ui.interact(hit, ui.scope_id().with("phone-sheet-resize-grip"), egui::Sense::drag())
        .on_hover_cursor(egui::CursorIcon::ResizeVertical);
    let color = if response.dragged() { ui.visuals().strong_text_color() } else { ui.visuals().weak_text_color() };
    let mark = Rect::from_center_size(hit.center(), egui::vec2(hit.width().min(52.0), 5.0));
    ui.painter().rect_filled(mark, 2.5, color);
    response
}

#[derive(Clone, Copy)]
struct GripDrag {
    origin_y: f32,
    height: f32,
}

fn grip_drag_id(ui: &Ui) -> Id {
    panel_id(ui).with("phone-sheet-grip-drag")
}

/// Resize from a fixed starting height and the pointer's total movement. The panel is updated for
/// the next frame; using the starting height avoids drift as the handle follows the finger.
pub(crate) fn resize_from_grip(ui: &Ui, grip: &egui::Response, rect: Rect, min: f32, max: f32, available: f32) {
    let key = grip_drag_id(ui);
    if grip.drag_started() {
        if let Some(origin) = ui.input(|i| i.pointer.press_origin()) {
            ui.data_mut(|d| d.insert_temp(key, GripDrag { origin_y: origin.y, height: rect.height() }));
        }
    }
    if grip.dragged() || grip.drag_stopped() {
        let drag = ui.data(|d| d.get_temp::<GripDrag>(key));
        let pointer = grip.interact_pointer_pos().or_else(|| ui.input(|i| i.pointer.latest_pos()));
        if let (Some(drag), Some(pointer)) = (drag, pointer) {
            let available = available.max(0.0);
            let height = (drag.height - (pointer.y - drag.origin_y)).clamp(min.min(available), max.min(available).max(min.min(available)));
            let mut next = rect;
            next.min.y = next.max.y - height;
            ui.data_mut(|d| d.insert_persisted(panel_id(ui), PanelState { outer_rect: next }));
            ui.ctx().request_repaint();
        }
    }
    if grip.drag_stopped() {
        ui.data_mut(|d| d.remove::<GripDrag>(key));
    }
}

/// Do not compare a newly opened portrait sheet with an old or landscape layout.
pub(crate) fn reset(ui: &Ui) {
    ui.data_mut(|d| {
        d.remove::<PreviousLayout>(previous_id(ui));
        d.remove::<GripDrag>(grip_drag_id(ui));
    });
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
            let max = 760.0 + if hidden { 100.0 } else { 0.0 };
            let available = ui.available_rect_before_wrap().height();
            let mut handle = None;
            let panel = Panel::bottom(PANEL_ID).resizable(true)
                .default_size(360.0 + if hidden { 100.0 } else { 0.0 })
                .size_range(220.0..=max)
                .frame(Frame::NONE)
                .show(ui, |ui| {
                    handle = Some(grip(ui));
                    let body = egui::ScrollArea::vertical().id_salt("test-layer-list").auto_shrink([false, false])
                        .show(ui, |ui| {
                            for n in 0..60 { ui.label(format!("Layer {n}")); }
                        });
                    ui.data_mut(|d| {
                        d.insert_temp(Id::unique("test-body"), body.inner_rect);
                        d.insert_temp(Id::unique("test-scroll"), body.state.clamped_offset().y);
                    });
                });
            finish(ui, hidden, panel.response.rect);
            let handle = handle.unwrap();
            ui.data_mut(|d| d.insert_temp(Id::unique("test-grip"), handle.rect));
            resize_from_grip(ui, &handle, panel.response.rect, 220.0, max, available);
            result = Some(panel.response.rect);
        });
        out.textures_delta.clear();
        result.unwrap()
    }

    fn close(a: f32, b: f32) {
        assert!((a - b).abs() <= 1.5, "expected {a} to be near {b}");
    }

    fn grip_rect(ctx: &Context) -> Rect {
        ctx.data(|d| d.get_temp(Id::unique("test-grip"))).unwrap()
    }

    fn drag_touch(ctx: &Context, from: egui::Pos2, to: egui::Pos2) {
        let touch = |phase, pos| egui::Event::Touch { device_id: egui::TouchDeviceId(1), id: egui::TouchId(1), phase, pos, force: None };
        let button = |pos, pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() };
        draw(ctx, true, 0.0, vec![touch(egui::TouchPhase::Start, from), egui::Event::PointerMoved(from), button(from, true)]);
        for step in 1..=5 {
            let pos = from + (to - from) * (step as f32 / 5.0);
            draw(ctx, true, 0.0, vec![touch(egui::TouchPhase::Move, pos), egui::Event::PointerMoved(pos)]);
        }
        draw(ctx, true, 0.0, vec![touch(egui::TouchPhase::End, to), button(to, false), egui::Event::PointerGone]);
        draw(ctx, true, 0.0, vec![]);
    }

    #[test]
    fn the_visible_touch_handle_resizes_up_and_down_without_drift() {
        let ctx = Context::default();
        let initial = draw(&ctx, true, 0.0, vec![]);
        let hit = grip_rect(&ctx);
        assert!(hit.width() >= 100.0 && hit.height() >= 44.0);
        drag_touch(&ctx, hit.center(), hit.center() - vec2(0.0, 100.0));
        let taller = draw(&ctx, true, 0.0, vec![]);
        close(taller.height(), initial.height() + 100.0);
        let hit = grip_rect(&ctx);
        drag_touch(&ctx, hit.center(), hit.center() + vec2(0.0, 80.0));
        let shorter = draw(&ctx, true, 0.0, vec![]);
        close(shorter.height(), taller.height() - 80.0);
        close(shorter.bottom(), initial.bottom());
        let shown = draw(&ctx, false, 0.0, vec![]);
        close(shown.top(), shorter.top());
        close(shorter.height() - shown.height(), 100.0);
    }

    #[test]
    fn handle_respects_height_limits_and_body_drags_do_not_resize() {
        let ctx = Context::default();
        let initial = draw(&ctx, true, 0.0, vec![]);
        let hit = grip_rect(&ctx);
        let body_rect: Rect = ctx.data(|d| d.get_temp(Id::unique("test-body"))).unwrap();
        let body = body_rect.center();
        // egui enables touch scrolling after learning that a touch screen is present.
        draw(&ctx, true, 0.0, vec![egui::Event::Touch { device_id: egui::TouchDeviceId(1), id: egui::TouchId(9), phase: egui::TouchPhase::Start, pos: body, force: None }]);
        draw(&ctx, true, 0.0, vec![egui::Event::Touch { device_id: egui::TouchDeviceId(1), id: egui::TouchId(9), phase: egui::TouchPhase::End, pos: body, force: None }]);
        draw(&ctx, true, 0.0, vec![]);
        drag_touch(&ctx, body, body - vec2(0.0, 100.0));
        close(draw(&ctx, true, 0.0, vec![]).height(), initial.height());
        let scroll: f32 = ctx.data(|d| d.get_temp(Id::unique("test-scroll"))).unwrap();
        assert!(scroll > 30.0, "touch on the list should still scroll, got {scroll}");
        drag_touch(&ctx, hit.center(), hit.center() + vec2(0.0, 500.0));
        close(draw(&ctx, true, 0.0, vec![]).height(), 220.0);
        let hit = grip_rect(&ctx);
        drag_touch(&ctx, hit.center(), hit.center() - vec2(0.0, 1000.0));
        close(draw(&ctx, true, 0.0, vec![]).height(), 860.0);
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
