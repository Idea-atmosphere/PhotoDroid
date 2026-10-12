//! One lasso outline can alternate between freehand strokes and Alt-held straight segments:
//! with Alt held, each click adds a straight segment and a drag draws freehand. Releasing Alt
//! while the button is up closes the outline and makes the selection.
//! Selection intent is latched at the first press: Alt held then subtracts from an existing
//! selection; with no selection there is nothing to subtract, so it starts straight segments.
//! Alt pressed later changes drawing mode.
//!
//! On the phone a finger cannot stay down for a whole outline, so a lift does not close it: the outline stays open,
//! and the next touch goes on from where it stopped (a touch somewhere else joins it with a straight line, so taps
//! add straight segments). The outline closes when the finger comes back to its first point, or with the floating
//! bar's ✓; the bar's ↶ takes away the last piece (a drag or a tap), and ✕ drops the outline. Two fingers that pan or
//! zoom meanwhile leave it open (`pinched`).

use crate::{
    PhotocraftApp,
    canvas::{Drag, ToolEvent, ViewXform},
    state::Tool,
};
use egui::{Event, Modifiers, PointerButton};

#[derive(Clone, Debug)]
pub struct Lasso {
    pub cursor: [f64; 2],
    down: bool,
    polygonal: bool,
    document: Option<photocraft_doc::DocId>,
    /// The outline's length at the start of each piece added after the first (a drag or a tap): ↶ takes the last one
    /// away ([`undo_piece`]).
    marks: Vec<usize>,
}

/// On the phone a finger this near (points) to the outline's first point closes it.
const PHONE_CLOSE: f64 = 22.0;

/// Whether the outline stays open across lifts (the phone's way, see the module's notes).
fn keeps_open(app: &PhotocraftApp) -> bool {
    app.phone_layout
}

pub fn active(app: &PhotocraftApp) -> bool {
    app.drag.as_ref().is_some_and(|d| d.tool == Tool::Lasso && d.lasso.is_some())
}

/// Between polygonal clicks, Backspace/right-click retracts the last fixed vertex.
/// Do not affect a freehand stroke whose primary button is still held.
pub fn waiting_for_vertex(app: &PhotocraftApp) -> bool {
    app.ui.tool == Tool::Lasso
        && app
            .drag
            .as_ref()
            .is_some_and(|d| d.tool == Tool::Lasso && d.lasso.as_ref().is_some_and(|l| !l.down && l.document == app.session.active().map(|st| st.doc.id)))
}

pub fn undo_last_vertex(app: &mut PhotocraftApp) -> bool {
    cancel_stale(app);
    if !waiting_for_vertex(app) {
        return false;
    }
    if app.drag.as_ref().is_some_and(|d| d.points.len() <= 1) {
        app.drag = None;
    } else if let Some(d) = app.drag.as_mut() {
        d.points.pop();
    }
    true
}

/// A lasso drag that moves the selection (started inside it, `canvas::selection_drag_kind`):
/// the canvas's own selection-move path handles it, this module only feeds it the events.
fn moving_selection(app: &PhotocraftApp) -> bool {
    app.drag.as_ref().is_some_and(|d| d.tool == Tool::Lasso && d.sel_move.is_some())
}

pub fn cancel_stale(app: &mut PhotocraftApp) {
    if app
        .drag
        .as_ref()
        .and_then(|d| d.lasso.as_ref())
        .is_some_and(|lasso| app.ui.tool != Tool::Lasso || lasso.document != app.session.active().map(|st| st.doc.id))
    {
        app.drag = None;
    }
}

fn append(d: &mut Drag, p: [f64; 2]) {
    if d.points.last().is_none_or(|last| last[0] != p[0] || last[1] != p[1]) {
        d.points.push([p[0], p[1], 1.0]);
    }
}

fn modifiers(app: &mut PhotocraftApp, mods: Modifiers) {
    let Some(d) = app.drag.as_mut().filter(|d| d.tool == Tool::Lasso) else { return };
    d.track(mods);
    // Alt held before the initial press still means subtract, until released and pressed again.
    let polygonal = mods.alt && (!d.modifiers.alt || d.released.alt);
    // Alt released between straight segments (button up): the outline is done.
    let close = d.lasso.as_ref().is_some_and(|l| l.polygonal && !polygonal && !l.down);
    let anchor = d.lasso.as_ref().and_then(|l| (l.polygonal && !polygonal && l.down).then_some(l.cursor));
    if let Some(p) = anchor {
        append(d, p);
    }
    if let Some(lasso) = &mut d.lasso {
        lasso.polygonal = polygonal;
    }
    if close {
        commit(app);
    }
}

/// Takes away the outline's last piece (the last drag or tap), or the whole outline when only its first is left.
/// Returns whether there was an outline.
pub fn undo_piece(app: &mut PhotocraftApp) -> bool {
    cancel_stale(app);
    if !active(app) {
        return false;
    }
    let Some(d) = app.drag.as_mut() else { return false };
    let mark = d.lasso.as_mut().and_then(|l| l.marks.pop());
    match mark {
        Some(n) if n >= 1 => {
            d.points.truncate(n);
            let last = d.points.last().map_or(d.start, |p| [p[0], p[1]]);
            if let Some(l) = d.lasso.as_mut() {
                l.cursor = last;
                l.down = false;
            }
        }
        _ => app.drag = None,
    }
    true
}

/// Drops the outline without making a selection.
pub fn cancel(app: &mut PhotocraftApp) {
    if active(app) {
        app.drag = None;
    }
}

/// Two fingers came down while the outline is open (they pan or zoom the view). The piece the first finger was
/// drawing goes, if it had only just started (`early`: the finger was a pinch's first one); a longer one stays. Either
/// way the outline stays open, for the next touch to go on.
pub fn pinched(app: &mut PhotocraftApp, early: bool) {
    if !active(app) {
        return;
    }
    let down = app.drag.as_ref().and_then(|d| d.lasso.as_ref()).is_some_and(|l| l.down);
    if !down {
        return;
    }
    if early {
        undo_piece(app);
    }
    if let Some(l) = app.drag.as_mut().and_then(|d| d.lasso.as_mut()) {
        l.down = false;
    }
}

/// The phone's floating bar while an outline is open: ✓ closes it, ↶ takes away the last piece, ✕ drops it. At the
/// foot of the canvas, where a thumb reaches.
pub fn phone_bar(app: &mut PhotocraftApp, ctx: &egui::Context) {
    cancel_stale(app);
    if !keeps_open(app) || !active(app) || app.ui.tool != Tool::Lasso {
        return;
    }
    let canvas = app.last_canvas_rect;
    if !canvas.is_positive() {
        return;
    }
    let mut action = None;
    egui::Area::new(egui::Id::new("phone-lasso-bar"))
        .order(egui::Order::Foreground)
        .pivot(egui::Align2::CENTER_BOTTOM)
        .fixed_pos(canvas.center_bottom() - egui::vec2(0.0, 12.0))
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    if crate::icons::button(ui, "check", 44.0, false, "OK").clicked() {
                        action = Some(0);
                    }
                    if crate::icons::button(ui, "undo-2", 44.0, false, "Undo").clicked() {
                        action = Some(1);
                    }
                    if crate::icons::button(ui, "x", 44.0, false, "Cancel").clicked() {
                        action = Some(2);
                    }
                });
            });
        });
    match action {
        Some(0) => commit(app),
        Some(1) => {
            undo_piece(app);
        }
        Some(2) => cancel(app),
        _ => {}
    }
}

pub fn commit(app: &mut PhotocraftApp) {
    cancel_stale(app);
    if active(app)
        && let Some(d) = app.drag.take()
    {
        crate::canvas::finish_gesture(app, d);
    }
}

pub fn pointer(app: &mut PhotocraftApp, ev: ToolEvent, mods: Modifiers) -> bool {
    cancel_stale(app);
    if app.ui.tool != Tool::Lasso || moving_selection(app) {
        return false;
    }
    let p = match ev {
        ToolEvent::Down { x, y, .. } | ToolEvent::Move { x, y, .. } | ToolEvent::Up { x, y } => [x, y],
    };
    // A press inside the selection (no outline in progress) drags the selection instead.
    if matches!(ev, ToolEvent::Down { .. })
        && !active(app)
        && p.iter().all(|n| n.is_finite())
        && crate::canvas::selection_drag_kind(app, Tool::Lasso, p, mods).is_some()
    {
        return false;
    }
    if !p.iter().all(|n| n.is_finite()) {
        return true;
    }
    modifiers(app, mods);
    let phone = keeps_open(app);
    let zoom = app.point_zoom().max(0.01) as f64;
    match ev {
        ToolEvent::Down { .. } => {
            if active(app) {
                // A click on the first point, or on the last one again, closes the outline. On the phone only the
                // first point does (a finger put down again where it lifted goes on), from a finger's distance.
                let reach = (if phone { PHONE_CLOSE } else { 8.0 }) / zoom;
                let near = |q: [f64; 2]| (q[0] - p[0]).hypot(q[1] - p[1]) < reach;
                let close = app
                    .drag
                    .as_ref()
                    .is_some_and(|d| d.points.len() >= 3 && (near(d.start) || (!phone && d.points.last().is_some_and(|l| near([l[0], l[1]])))));
                if close {
                    commit(app);
                    return true;
                }
                if let Some(d) = &mut app.drag {
                    let piece = d.points.len();
                    append(d, p);
                    if let Some(lasso) = &mut d.lasso {
                        lasso.marks.push(piece);
                        lasso.cursor = p;
                        lasso.down = true;
                    }
                }
            } else {
                // Alt with no selection has nothing to subtract from: the outline starts with
                // straight segments, as a new selection.
                let nothing_selected = app.session.active().is_none_or(|st| st.doc.selection.is_none());
                let polygonal = mods.alt && nothing_selected;
                let intent = if polygonal { Modifiers { alt: false, ..mods } } else { mods };
                let mut d = Drag::new(Tool::Lasso, p, vec![[p[0], p[1], 1.0]], intent, false);
                d.lasso = Some(Lasso { cursor: p, down: true, polygonal, document: app.session.active().map(|st| st.doc.id), marks: Vec::new() });
                app.drag = Some(d);
            }
        }
        ToolEvent::Move { .. } | ToolEvent::Up { .. } => {
            let up = matches!(ev, ToolEvent::Up { .. });
            let Some(d) = app.drag.as_mut().filter(|d| d.tool == Tool::Lasso) else { return true };
            let Some(lasso) = &d.lasso else { return true };
            let (down, polygonal, previous) = (lasso.down, lasso.polygonal, lasso.cursor);
            if d.reposition {
                // Move relative to the rubber-band endpoint, not the last fixed vertex.
                let last = d.points.last().map_or(d.start, |p| [p[0], p[1]]);
                d.shift_to([last[0] + p[0] - previous[0], last[1] + p[1] - previous[1]]);
            } else if down {
                // While the button is down the outline follows the pointer (freehand), Alt or
                // not; between Alt clicks only the rubber band moves.
                append(d, p);
            }
            if let Some(lasso) = &mut d.lasso {
                lasso.cursor = p;
                if up {
                    lasso.down = false;
                }
            }
            // On the phone a lift leaves the outline open, unless it came back to the first point.
            let closes = !phone
                || (d.points.len() >= 8 && {
                    let reach = PHONE_CLOSE / zoom;
                    let from_start = |q: &[f64; 3]| (q[0] - d.start[0]).hypot(q[1] - d.start[1]);
                    (p[0] - d.start[0]).hypot(p[1] - d.start[1]) < reach && d.points.iter().map(from_start).fold(0.0, f64::max) > reach * 4.0
                });
            if up && down && !polygonal && closes {
                commit(app);
            }
        }
    }
    true
}

/// Read actual presses/releases, including clicks below egui's drag threshold. Hover moves
/// update only the rubber band; they must never become freehand vertices between Alt clicks.
pub fn canvas_input(app: &mut PhotocraftApp, ctx: &egui::Context, xf: &ViewXform, response: &egui::Response) {
    let events = ctx.input(|i| i.events.clone());
    let mut mods = app.drag.as_ref().map_or(ctx.input(|i| i.modifiers), |d| d.live);
    for ev in events {
        let tool_ev = match ev {
            Event::ModifiersChanged(m) => {
                mods = m;
                modifiers(app, crate::workspace_ui::sticky_mods(app, m));
                None
            }
            Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: m } => {
                mods = m;
                let p = xf.to_doc(pos);
                if pressed && response.contains_pointer() && response.rect.contains(pos) {
                    Some(ToolEvent::Down { x: p[0], y: p[1], pressure: 1.0 })
                } else if !pressed && (active(app) || moving_selection(app)) {
                    Some(ToolEvent::Up { x: p[0], y: p[1] })
                } else {
                    None
                }
            }
            Event::PointerMoved(pos) if active(app) || moving_selection(app) => {
                let p = xf.to_doc(pos);
                Some(ToolEvent::Move { x: p[0], y: p[1], pressure: 1.0 })
            }
            Event::WindowFocused(false) => {
                if active(app) || moving_selection(app) {
                    app.drag = None;
                }
                None
            }
            _ => None,
        };
        if let Some(ev) = tool_ev {
            crate::canvas::tool_event(app, ev, mods);
        }
    }
    if response.double_clicked() {
        commit(app);
    }
}

#[cfg(test)]
mod tests;
