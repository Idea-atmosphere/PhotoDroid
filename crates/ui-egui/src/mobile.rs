//! The phone layout: the same editor, arranged for a narrow touch screen.
//!
//! On a phone the desktop chrome (menu bar, options bar, tool column, status bar, dock) leaves
//! almost no room for the picture. This layout keeps the same panels in other places:
//!
//! - a top bar: the menus button, the document's name and status line, undo, redo, the toggles of
//!   the virtual mouse and the virtual keyboard, and the panels button;
//! - at the bottom, when the keyboard is on, the virtual keyboard ([`crate::virtual_input`]); then
//!   the options of the active tool, and under them a strip of every tool, with 44 pt touch targets
//!   (a button that holds several tools lists them on a long press). The virtual mouse is a pointer
//!   on the canvas, which a finger moves ([`crate::virtual_mouse`]);
//! - above those, when the panels button is on, a sheet with the dock's groups (Layers, History,
//!   Colour, Properties, Navigator);
//! - the menus, as a sheet over the picture: one menu to a page (a back button returns), with a search
//!   field that lists the matching commands.
//!
//! The canvas takes the room that is left. Each part draws the same code as the desktop, and the
//! behaviour is the same, except where a touch screen needs other gestures on this layout: a press
//! held down does a double-click's job ([`held_or_double`]), and a row or swatch under a finger
//! takes clicks only, so that a swipe scrolls the list ([`touch_screen`]). The layout is used when
//! the shell asks for it ([`PhotocraftApp::phone_layout`], set on Android) and the window's shorter
//! side is under [`PHONE_MAX_SIDE`]. Desktop and web windows keep their chrome and their mouse.

use egui::{Align, Frame, Layout, Margin, RichText, Sense, Stroke, Vec2, vec2};
use serde_json::json;

use crate::dock::Group;
use crate::theme::{self, Tokens};
use crate::{PhotocraftApp, i18n, icons, menus, notices, panels, virtual_input};

/// A window whose shorter side is under this many points gets the phone layout.
pub const PHONE_MAX_SIDE: f32 = 600.0;
/// A touch target: 44 points, the smallest size the platforms recommend.
const TOUCH: f32 = 44.0;
/// The top bar's height.
pub(crate) const TOP_BAR: f32 = 48.0;
/// The options row's height.
const OPTIONS_ROW: f32 = 44.0;
/// The tool strip's height: a touch target and its margins.
const TOOL_STRIP: f32 = 56.0;

/// The panel groups the sheet offers: the group, its icon and its name.
const SHEET_GROUPS: [(Group, &str, &str); 5] = [
    (Group::Layers, "layers", "Layers"),
    (Group::History, "clock", "History"),
    (Group::Color, "palette", "Color & Swatches"),
    (Group::Properties, "sliders-horizontal", "Properties"),
    (Group::Navigator, "navigation", "Navigator"),
];

/// Does a window of this size get the phone layout? Its shorter side must be under
/// [`PHONE_MAX_SIDE`], in either orientation.
pub fn fits_phone(size: Vec2) -> bool {
    size.x.min(size.y) < PHONE_MAX_SIDE
}

/// Is the phone layout in use for this frame?
pub fn active(app: &PhotocraftApp, ctx: &egui::Context) -> bool {
    // The whole window, system bars included: the threshold is the window's size, not the room
    // left between the bars.
    app.phone_layout && fits_phone(ctx.viewport_rect().size())
}

/// The key of the flag that says whether the touch gestures are on this frame (see [`set_gestures`]).
fn gestures_key() -> egui::Id {
    egui::Id::unique("photocraft-phone-gestures")
}

/// Records whether the touch gestures are on this frame: `lib.rs` sets it to the phone layout each
/// frame. The gestures below apply only then, so the desktop, the web and a tablet's desktop chrome
/// keep the mouse's behaviour, even on a touch screen.
pub fn set_gestures(ctx: &egui::Context, on: bool) {
    ctx.data_mut(|d| d.insert_temp(gestures_key(), on));
}

/// Are the touch gestures on this frame? See [`set_gestures`].
pub fn gestures_on(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<bool>(gestures_key())).unwrap_or(false)
}

/// Is a finger on the phone's screen? egui knows it has a touch screen once it has had a touch, and
/// keeps the knowledge. On one, the rows, eyes and swatches take clicks only ([`row_sense`]): a drag
/// there would take the swipe that scrolls the list, reorder the row, sweep the visibility of the
/// eyes it passes over, or drop the swatch.
pub fn touch_screen(ctx: &egui::Context) -> bool {
    gestures_on(ctx) && ctx.input(|i| i.has_touch_screen())
}

/// The sense of a list row, an eye or a swatch: a drag where a mouse has one (it reorders, moves or
/// drops the row), a click alone on a touch screen. There a finger's drag takes the press before the
/// list's scroll can, so a swipe over the rows would not scroll the list.
pub fn row_sense(ctx: &egui::Context) -> Sense {
    if touch_screen(ctx) { Sense::click() } else { Sense::click_and_drag() }
}

/// Does this response ask for its second-tap action? With a mouse that is a double-click. On a touch
/// screen the second tap often comes too late to count as one (egui allows 0.3 s between the two),
/// so a press held down does the same job instead. egui reports a long press only for touches, so a
/// mouse keeps the double-click alone. A widget with a context menu keeps the double-click too, as a
/// long press there opens the menu.
pub fn held_or_double(ctx: &egui::Context, resp: &egui::Response) -> bool {
    resp.double_clicked() || (resp.long_touched() && gestures_on(ctx))
}

/// Draw the phone chrome. It goes where the desktop chrome goes, before the canvas.
pub fn chrome(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    if photocraft_doc::diag::enabled() {
        photocraft_doc::cpu::note_governor_state();
        let vp = ui.ctx().viewport_rect();
        let cr = ui.ctx().content_rect();
        let insets = ui.ctx().input(|i| i.safe_area_insets()).0;
        let ppp = ui.ctx().pixels_per_point();
        let key = ((vp.width().round() as u64) << 48)
            ^ ((vp.height().round() as u64) << 32)
            ^ ((cr.width().round() as u64) << 16)
            ^ (cr.height().round() as u64)
            ^ ((insets.bottom.round() as u64) << 8)
            ^ (insets.top.round() as u64);
        photocraft_doc::diag::note_changed(0x5549_4348, key, || {
            format!(
                "UI chrome layout: viewport=[{:.1},{:.1}..{:.1},{:.1}] ({:.1}x{:.1}) content=[{:.1},{:.1}..{:.1},{:.1}] ({:.1}x{:.1}) safe_insets=(t={:.1},b={:.1},l={:.1},r={:.1}) ppp={ppp:.2} rtl={}",
                vp.left(),
                vp.top(),
                vp.right(),
                vp.bottom(),
                vp.width(),
                vp.height(),
                cr.left(),
                cr.top(),
                cr.right(),
                cr.bottom(),
                cr.width(),
                cr.height(),
                insets.top,
                insets.bottom,
                insets.left,
                insets.right,
                crate::direction::is_rtl()
            )
        });
    }
    // Bottom panels stack upwards in the order they are added: the virtual keyboard (when it is on)
    // sits at the foot of the screen, the tool strip above it, the options above that, and the sheet
    // above those. The keys are drawn again above a dialog or the menu, which dim the screen.
    let covered = !app.ui.dialogs.is_empty() || is_set(ui.ctx(), menu_open_id());
    app.virtual_mouse.set_covered(covered);
    virtual_input::panel(app, ui, covered);
    tool_strip(app, ui);
    options(app, ui);
    // Held sideways the sheet goes to the right of the picture, under the top bar (see `side_sheet`);
    // upright it goes above the options, as wide as the screen.
    let side = side_sheet(ui.ctx().content_rect().size());
    if !side {
        sheet(app, ui);
    }
    top_bar(app, ui);
    if side {
        sheet(app, ui);
    }
    // The floating bars over the canvas: the open lasso outline's, and the virtual mouse's own buttons.
    if !covered {
        crate::lasso_ui::phone_bar(app, ui.ctx());
        mouse_bar(app, ui.ctx());
    }
    menu(app, ui.ctx());
    app.virtual_mouse.overlay(ui.ctx());
}

/// Sanitizes raw touch and pointer events on the phone before `egui::Context::begin_pass` runs:
/// 1) `egui-winit` emits `Event::PointerGone` on `TouchPhase::Cancel` without releasing the primary
///    pointer button (`PointerButton { pressed: false }`), which leaves `primary_down` stuck in `egui`
///    whenever Android cancels a touch near the navigation bar or screen edge.
/// 2) When a touch release (`End`/`Cancel`/`PointerButton { pressed: false }`) and a new touch press
///    (`Start`/`PointerButton { pressed: true }`) arrive in the same frame, ensures `Event::PointerGone`
///    separates them and logs the event transition when diagnostic logging is enabled.
pub fn sanitize_touch_events(ctx: &egui::Context, raw_input: &mut egui::RawInput) {
    if raw_input.events.is_empty() {
        return;
    }
    let diag_on = photocraft_doc::diag::enabled();
    let (had_primary_down, mods) = ctx.input(|i| (i.pointer.primary_down(), i.modifiers));
    let mut primary_down = had_primary_down;
    let mut saw_release_this_frame = false;
    let mut saw_gone_since_release = false;
    let mut out = Vec::with_capacity(raw_input.events.len() + 4);

    for event in raw_input.events.drain(..) {
        match event {
            egui::Event::Touch { device_id, id, phase, pos, force } => {
                match phase {
                    egui::TouchPhase::Start => {
                        if saw_release_this_frame && !saw_gone_since_release {
                            out.push(egui::Event::PointerGone);
                            saw_gone_since_release = true;
                            if diag_on {
                                photocraft_doc::diag::note(|| {
                                    format!("touch sanitize: inserted PointerGone before same-frame Touch::Start id={} pos=({:.1},{:.1})", id.0, pos.x, pos.y)
                                });
                            }
                        }
                        if diag_on {
                            photocraft_doc::diag::note(|| format!("touch start: id={} pos=({:.1},{:.1})", id.0, pos.x, pos.y));
                        }
                        out.push(egui::Event::Touch { device_id, id, phase, pos, force });
                    }
                    egui::TouchPhase::Move => {
                        out.push(egui::Event::Touch { device_id, id, phase, pos, force });
                    }
                    egui::TouchPhase::End => {
                        saw_release_this_frame = true;
                        saw_gone_since_release = false;
                        if diag_on {
                            photocraft_doc::diag::note(|| format!("touch end: id={} pos=({:.1},{:.1})", id.0, pos.x, pos.y));
                        }
                        out.push(egui::Event::Touch { device_id, id, phase, pos, force });
                    }
                    egui::TouchPhase::Cancel => {
                        saw_release_this_frame = true;
                        saw_gone_since_release = false;
                        if diag_on {
                            photocraft_doc::diag::note(|| {
                                format!("touch cancel: id={} pos=({:.1},{:.1}) primary_down={primary_down}", id.0, pos.x, pos.y)
                            });
                        }
                        out.push(egui::Event::Touch { device_id, id, phase, pos, force });
                        if primary_down {
                            out.push(egui::Event::PointerButton {
                                pos,
                                button: egui::PointerButton::Primary,
                                pressed: false,
                                modifiers: mods,
                            });
                            primary_down = false;
                        }
                    }
                }
            }
            egui::Event::PointerButton { pos, button, pressed, modifiers } => {
                if button == egui::PointerButton::Primary {
                    if pressed {
                        if saw_release_this_frame && !saw_gone_since_release {
                            out.push(egui::Event::PointerGone);
                            saw_gone_since_release = true;
                        }
                        primary_down = true;
                    } else {
                        primary_down = false;
                        saw_release_this_frame = true;
                        saw_gone_since_release = false;
                    }
                }
                out.push(egui::Event::PointerButton { pos, button, pressed, modifiers });
            }
            egui::Event::PointerGone => {
                saw_gone_since_release = true;
                out.push(egui::Event::PointerGone);
            }
            other => out.push(other),
        }
    }

    raw_input.events = out;
}

// The phone chrome's state lives in egui's memory: none of it is saved with the document.

fn sheet_open_id() -> egui::Id {
    egui::Id::unique("phone-sheet-open")
}

fn sheet_group_id() -> egui::Id {
    egui::Id::unique("phone-sheet-group")
}

fn menu_open_id() -> egui::Id {
    egui::Id::unique("phone-menu-open")
}

fn menu_query_id() -> egui::Id {
    egui::Id::unique("phone-menu-query")
}

fn is_set(ctx: &egui::Context, id: egui::Id) -> bool {
    ctx.data(|d| d.get_temp::<bool>(id)).unwrap_or(false)
}

fn set_flag(ctx: &egui::Context, id: egui::Id, on: bool) {
    ctx.data_mut(|d| d.insert_temp(id, on));
}

fn top_bar_rect_id() -> egui::Id {
    egui::Id::unique("phone-top-bar-rect")
}

/// The top bar's rectangle as it was last drawn (`None` before the first frame of the phone layout).
pub fn top_bar_rect(ctx: &egui::Context) -> Option<egui::Rect> {
    ctx.data(|d| d.get_temp::<egui::Rect>(top_bar_rect_id()))
}

/// The top bar: menus, the document's name, undo, redo, the virtual mouse and keyboard toggles, and the
/// panels button.
fn top_bar(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    let ctx = ui.ctx().clone();
    let (menu_open, sheet_open) = (is_set(&ctx, menu_open_id()), is_set(&ctx, sheet_open_id()));
    let (mouse_on, keys_on, lens_on) = (app.virtual_mouse.is_on(), app.virtual_input.keyboard_on(), app.virtual_mouse.magnifier_on());
    let title = app
        .session
        .active()
        .map(|d| format!("{}{}", d.doc.name, if d.is_dirty() { "  •" } else { "" }))
        .unwrap_or_else(|| "PhotoDroid".into());
    let status = app.ui.status.clone();
    let can_undo = menus::is_enabled(app, "edit.undo");
    let can_redo = menus::is_enabled(app, "edit.redo");
    let (mut toggle_menu, mut toggle_sheet) = (false, false);
    let (mut toggle_mouse, mut toggle_keys, mut toggle_lens) = (false, false, false);
    let mut run: Option<&str> = None;
    egui::Panel::top("phone-top")
        .exact_size(TOP_BAR)
        .frame(Frame::NONE.fill(t.chrome).inner_margin(Margin::symmetric(4, 0)))
        .show(ui, |ui| {
            // Remembered for the next frame's input: the virtual mouse keeps the bar's touches off the canvas.
            ctx.data_mut(|d| d.insert_temp(top_bar_rect_id(), ui.max_rect()));
            ui.horizontal_centered(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                toggle_menu = icons::button(ui, "align-justify", TOUCH, menu_open, tl!("Menu")).clicked();
                // The name, and the status line under it, take the room the buttons leave.
                // Undo, redo, the mouse, the magnifier, the keyboard and the panels.
                let buttons = 6.0;
                let name_w = (ui.available_width() - buttons * (TOUCH + 2.0)).max(0.0);
                ui.vertical(|ui| {
                    ui.set_width(name_w);
                    ui.add(egui::Label::new(RichText::new(title.as_str()).font(theme::medium(14.0)).color(t.text)).truncate());
                    if !status.is_empty() {
                        ui.add(egui::Label::new(RichText::new(status.as_str()).size(11.0).color(t.text_faint)).truncate());
                    }
                });
                let _ = ui.add_enabled_ui(can_undo, |ui| {
                    if icons::button(ui, "undo-2", TOUCH, false, tl!("Undo")).clicked() {
                        run = Some("edit.undo");
                    }
                });
                let _ = ui.add_enabled_ui(can_redo, |ui| {
                    if icons::button(ui, "redo-2", TOUCH, false, tl!("Redo")).clicked() {
                        run = Some("edit.redo");
                    }
                });
                toggle_mouse = toggle_button(ui, "mouse-pointer-2", mouse_on, tl!("Virtual mouse"));
                // The magnifier works with a finger too: a loupe in a corner shows what is under it.
                toggle_lens = toggle_button(ui, "search", lens_on, tl!("Magnifier"));
                toggle_keys = toggle_button(ui, "keyboard", keys_on, tl!("Virtual keyboard"));
                toggle_sheet = icons::button(ui, "layers", TOUCH, sheet_open, tl!("Panels")).clicked();
            });
        });
    if toggle_menu {
        set_flag(&ctx, menu_open_id(), !menu_open);
        // Whichever way the button goes, the sheet starts on the top menus.
        ctx.data_mut(|d| d.insert_temp(menu_path_id(), Vec::<String>::new()));
    }
    if toggle_sheet {
        set_flag(&ctx, sheet_open_id(), !sheet_open);
    }
    if toggle_mouse {
        let canvas = app.last_canvas_rect;
        app.virtual_mouse.toggle(canvas);
    }
    if toggle_lens {
        let canvas = app.last_canvas_rect;
        app.virtual_mouse.toggle_magnifier(canvas);
    }
    if toggle_keys {
        app.virtual_input.toggle_keyboard();
    }
    if let Some(id) = run {
        let _ = app.run(id, json!({}));
    }
}

/// The virtual mouse's own buttons, floating at the canvas's bottom right while the pointer is on: a right click where
/// the pointer is (aimed, unlike a two-finger tap), and the precise mode for fine work.
fn mouse_bar(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let canvas = app.last_canvas_rect;
    if !app.virtual_mouse.is_on() || !canvas.is_positive() {
        return;
    }
    let precise = app.virtual_mouse.precise_on();
    // The right click button is a switch for the menu it opens: pressed while a menu is open, it only closes that
    // menu. The press itself may close the menu before the button sees its click (a press outside a menu closes
    // it), so whether one was open is taken from the frame before the press.
    let state = egui::Id::unique("phone-mouse-bar-menu");
    let open_now = egui::Popup::is_any_open(ctx);
    let (open_before, mut open_at_press): (bool, bool) = ctx.data(|d| d.get_temp(state)).unwrap_or_default();
    if ctx.input(|i| i.pointer.any_pressed()) {
        open_at_press = open_before || open_now;
    }
    let (mut right, mut fine) = (false, false);
    egui::Area::new(egui::Id::unique("phone-mouse-bar"))
        .order(egui::Order::Foreground)
        .pivot(egui::Align2::RIGHT_BOTTOM)
        .fixed_pos(canvas.right_bottom() - vec2(10.0, 12.0))
        .show(ctx, |ui| {
            Frame::popup(ui.style()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    right = icons::button(ui, "ellipsis", TOUCH, false, "Right click").clicked();
                    fine = toggle_button(ui, "scan", precise, "Precise pointer");
                });
            });
        });
    if right {
        if open_at_press || egui::Popup::is_any_open(ctx) {
            egui::Popup::close_all(ctx);
        } else {
            app.virtual_mouse.right_click();
        }
        open_at_press = false;
    }
    // Read before the write: egui's context is locked inside `data_mut`, and asking it anything there hangs.
    let open_after = egui::Popup::is_any_open(ctx);
    ctx.data_mut(|d| d.insert_temp(state, (open_after, open_at_press)));
    if fine {
        app.virtual_mouse.toggle_precise();
    }
}

/// A top-bar button that is a switch: it shows whether it is on, to a screen reader too. Returns whether it
/// was pressed.
fn toggle_button(ui: &mut egui::Ui, icon: &str, on: bool, tooltip: &str) -> bool {
    let resp = icons::button(ui, icon, TOUCH, on, tooltip);
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, on, tooltip));
    resp.clicked()
}

/// Horizontal drag threshold (points) before a touch on a bottom strip engages strip scrolling.
/// Kept at 11 pt so a slight thumb drift (e.g. 8 pt) during a 0.35s press-and-hold still opens a
/// tool's flyout, while any real horizontal swipe smoothly scrolls the strip.
const STRIP_SCROLL_SLOP: f32 = 11.0;

#[derive(Clone, Copy, Debug, Default)]
struct StripScrollState {
    offset_x: f32,
    max_offset_x: f32,
    content_width: f32,
    viewport_width: f32,
    strip_rect: Option<egui::Rect>,
    pointer_in_strip: bool,
    dragging: bool,
    press_pos: Option<egui::Pos2>,
    last_pos: Option<egui::Pos2>,
    press_start_offset_x: f32,
    max_travel: f32,
    velocity_x: f32,
}

fn begin_strip_scroll(
    ui: &egui::Ui,
    strip_id: egui::Id,
    label: &'static str,
    diag_slot: u64,
    block_drag: bool,
) -> StripScrollState {
    let mut state = ui.data(|d| d.get_temp::<StripScrollState>(strip_id)).unwrap_or_default();
    let strip_rect = state.strip_rect.unwrap_or_else(|| ui.max_rect()).expand(4.0);
    let diag_on = photocraft_doc::diag::enabled();

    let (any_down, any_pressed, any_released, press_origin, interact_pos, raw_delta_x, dt, event_touch_pos, same_frame_restart) =
        ui.input(|i| {
            let mut last_touch_pos = None;
            let mut saw_end = false;
            let mut restart = false;
            for ev in &i.raw.events {
                match ev {
                    egui::Event::Touch { phase, pos, .. } => {
                        last_touch_pos = Some(*pos);
                        match phase {
                            egui::TouchPhase::End | egui::TouchPhase::Cancel => saw_end = true,
                            egui::TouchPhase::Start if saw_end => restart = true,
                            _ => {}
                        }
                    }
                    egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, .. } => {
                        last_touch_pos = Some(*pos);
                        if !*pressed {
                            saw_end = true;
                        } else if saw_end {
                            restart = true;
                        }
                    }
                    egui::Event::PointerMoved(pos) => {
                        last_touch_pos = Some(*pos);
                    }
                    _ => {}
                }
            }
            (
                i.pointer.any_down(),
                i.pointer.any_pressed(),
                i.pointer.any_released(),
                i.pointer.press_origin(),
                i.pointer.interact_pos().or_else(|| i.pointer.latest_pos()).or(last_touch_pos),
                i.pointer.delta().x,
                i.stable_dt.clamp(1.0 / 240.0, 0.1),
                last_touch_pos,
                restart,
            )
        });

    let origin_changed = match (press_origin, state.press_pos) {
        (Some(o), Some(prev_o)) => (o - prev_o).length() > 1.5,
        (Some(_), None) => true,
        _ => false,
    };
    let new_press = any_pressed || same_frame_restart || (any_down && (!state.pointer_in_strip || origin_changed));

    if !any_down || (any_released && !any_pressed && !same_frame_restart) {
        if state.pointer_in_strip {
            if diag_on {
                let off = state.offset_x;
                let max_off = state.max_offset_x;
                let travel = state.max_travel;
                let vel = state.velocity_x;
                photocraft_doc::diag::note(move || {
                    format!("{label} scroll touch released: offset_x={off:.1}/{max_off:.1} max_travel={travel:.1} coast_vel={vel:.1}")
                });
            }
            state.pointer_in_strip = false;
            state.dragging = false;
            state.press_pos = None;
            state.last_pos = None;
        }
        if state.velocity_x.abs() > 15.0 {
            let limit = if state.max_offset_x > 0.0 { state.max_offset_x } else { f32::MAX };
            let prev = state.offset_x;
            state.offset_x = (state.offset_x + state.velocity_x * dt).clamp(0.0, limit);
            state.velocity_x *= (-6.5 * dt).exp();
            if state.offset_x <= 0.0 || (state.max_offset_x > 0.0 && state.offset_x >= state.max_offset_x) || state.velocity_x.abs() <= 15.0 {
                state.velocity_x = 0.0;
            }
            if (state.offset_x - prev).abs() > 0.05 || state.velocity_x.abs() > 15.0 {
                ui.ctx().request_repaint();
            }
        } else {
            state.velocity_x = 0.0;
        }
    } else if new_press {
        let start_pos = press_origin.or(interact_pos).or(event_touch_pos);
        if !block_drag && start_pos.is_some_and(|p| strip_rect.contains(p)) {
            if diag_on && raw_delta_x.abs() > 16.0 {
                let kept = state.offset_x;
                photocraft_doc::diag::note(move || {
                    format!(
                        "{label} scroll: suppressed air-travel press delta raw_dx={raw_delta_x:.1} at {:?}, keeping offset_x={kept:.1}",
                        start_pos
                    )
                });
            }
            state.pointer_in_strip = true;
            state.dragging = false;
            state.press_pos = start_pos;
            state.last_pos = interact_pos.or(start_pos);
            state.press_start_offset_x = state.offset_x;
            state.max_travel = 0.0;
            state.velocity_x = 0.0;
            if diag_on {
                let off = state.offset_x;
                let max_off = state.max_offset_x;
                photocraft_doc::diag::note(move || {
                    format!("{label} scroll touch start: pos={start_pos:?} offset_x={off:.1}/{max_off:.1}")
                });
            }
        } else {
            state.pointer_in_strip = false;
            state.dragging = false;
            state.press_pos = None;
            state.last_pos = None;
            state.velocity_x = 0.0;
        }
    } else if any_down && state.pointer_in_strip {
        if block_drag {
            state.dragging = false;
            state.velocity_x = 0.0;
        } else if let Some(curr) = interact_pos {
            let origin = state.press_pos.unwrap_or(curr);
            let prev = state.last_pos.unwrap_or(curr);
            let step_dx = curr.x - prev.x;
            let total_dx = curr.x - origin.x;
            let travel = (curr - origin).length().max(total_dx.abs());
            state.max_travel = state.max_travel.max(travel);
            state.last_pos = Some(curr);
            let limit = if state.max_offset_x > 0.0 { state.max_offset_x } else { f32::MAX };

            if !state.dragging {
                if total_dx.abs() > STRIP_SCROLL_SLOP {
                    state.dragging = true;
                    let initial_dx = total_dx - STRIP_SCROLL_SLOP * total_dx.signum();
                    state.offset_x = (state.press_start_offset_x - initial_dx).clamp(0.0, limit);
                    state.velocity_x = (-initial_dx / dt).clamp(-2400.0, 2400.0) * 0.5;
                    if diag_on {
                        let off = state.offset_x;
                        let max_off = state.max_offset_x;
                        photocraft_doc::diag::note(move || {
                            format!("{label} scroll drag engaged: total_dx={total_dx:.1} offset_x={off:.1}/{max_off:.1}")
                        });
                    }
                    ui.ctx().request_repaint();
                }
            } else {
                if step_dx.abs() > 0.0 {
                    state.offset_x = (state.offset_x - step_dx).clamp(0.0, limit);
                    let inst_vel = (-step_dx / dt).clamp(-2400.0, 2400.0);
                    if step_dx.abs() > 0.4 {
                        if state.velocity_x == 0.0 || inst_vel.signum() == state.velocity_x.signum() || step_dx.abs() > 4.5 {
                            state.velocity_x = state.velocity_x * 0.4 + inst_vel * 0.6;
                        } else {
                            state.velocity_x *= 0.5;
                            if diag_on {
                                let kept_vel = state.velocity_x;
                                photocraft_doc::diag::note(move || {
                                    format!("{label} scroll: filtered thumb-lift rollback step_dx={step_dx:.2} inst_vel={inst_vel:.1} kept_vel={kept_vel:.1}")
                                });
                            }
                        }
                    } else {
                        state.velocity_x *= (-8.0 * dt).exp();
                    }
                } else {
                    state.velocity_x *= (-8.0 * dt).exp();
                }
                ui.ctx().request_repaint();
            }
        }
    }

    let _ = diag_slot;
    state
}

#[allow(clippy::too_many_arguments)]
fn end_strip_scroll(
    ui: &egui::Ui,
    strip_id: egui::Id,
    label: &'static str,
    diag_slot: u64,
    mut state: StripScrollState,
    inner_rect: egui::Rect,
    content_size: Vec2,
    scroll_area_offset_x: f32,
) {
    let prev_content_w = state.content_width;
    state.strip_rect = Some(inner_rect);
    state.viewport_width = inner_rect.width().max(0.0);
    state.content_width = content_size.x.max(state.viewport_width);
    let new_max = (content_size.x - inner_rect.width()).max(0.0);
    state.max_offset_x = new_max;

    if !state.pointer_in_strip && state.velocity_x.abs() <= 15.0 && (scroll_area_offset_x - state.offset_x).abs() > 0.5 {
        state.offset_x = scroll_area_offset_x;
    }
    state.offset_x = state.offset_x.clamp(0.0, new_max);

    if (state.content_width - prev_content_w).abs() > 1.0 && crate::direction::is_rtl() {
        ui.ctx().request_repaint();
    }

    ui.data_mut(|d| d.insert_temp(strip_id, state));

    if photocraft_doc::diag::enabled() {
        let key = ((state.offset_x.round() as i64 as u64 & 0xffff) << 48)
            ^ ((new_max.round() as i64 as u64 & 0xffff) << 32)
            ^ ((content_size.x.round() as i64 as u64 & 0xffff) << 16)
            ^ (inner_rect.width().round() as i64 as u64 & 0xfffe)
            ^ u64::from(state.dragging);
        let off = state.offset_x;
        let cw = content_size.x;
        let vw = inner_rect.width();
        let dragging = state.dragging;
        let vel = state.velocity_x;
        photocraft_doc::diag::note_changed(diag_slot, key, move || {
            format!(
                "{label} state: offset_x={off:.1}/{new_max:.1} content_w={cw:.1} view_w={vw:.1} rect=[{:.1},{:.1}..{:.1},{:.1}] dragging={dragging} vel={vel:.1}",
                inner_rect.left(),
                inner_rect.top(),
                inner_rect.right(),
                inner_rect.bottom()
            )
        });
    }
}

/// The strip of every tool, sideways-scrolling, Photoshop's order with a divider between sections.
/// A button that holds several tools shows the one last used, and lists the others on a long press.
fn tool_strip(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    let sections = panels::visible_sections(&app.session.prefs().toolbar.hidden);
    let flyout_id = egui::Id::unique("tool-flyout");
    let held_id = flyout_id.with("held");
    let pressed_slot_id = flyout_id.with("pressed-slot");
    if ui.input(|i| i.pointer.any_pressed()) {
        ui.data_mut(|d| {
            d.remove::<egui::Id>(held_id);
            d.remove::<(egui::Id, egui::Pos2)>(pressed_slot_id);
        });
    } else if ui.input(|i| !i.pointer.primary_down() && !i.pointer.any_released()) {
        ui.data_mut(|d| d.remove::<(egui::Id, egui::Pos2)>(pressed_slot_id));
    }
    let total_slots: usize = sections.iter().map(|s| s.len()).sum();
    let total_seps = sections.len().saturating_sub(1);
    let estimated_tools_w = total_slots as f32 * (TOUCH + 2.0) + total_seps as f32 * 9.0 + 8.0;

    egui::Panel::bottom("phone-tools")
        .exact_size(TOOL_STRIP)
        .frame(Frame::NONE.fill(t.chrome).inner_margin(Margin::symmetric(4, 6)))
        .show(ui, |ui| {
            let r = ui.max_rect();
            ui.painter().line_segment([r.left_top(), r.right_top()], Stroke::new(1.0, t.separator));
            let hold_flyout_active = ui.data(|d| d.get_temp::<egui::Id>(held_id)).is_some()
                && ui.data(|d| d.get_temp::<(egui::Id, egui::Rect)>(flyout_id)).is_some();
            let strip_scroll_id = egui::Id::unique("phone-tools-strip-scroll");
            let strip_state = begin_strip_scroll(ui, strip_scroll_id, "tool_strip", 0x5453_5452, hold_flyout_active);
            let strip_dragging = strip_state.dragging;
            if strip_dragging {
                ui.data_mut(|d| {
                    d.remove::<egui::Id>(held_id);
                    d.remove::<(egui::Id, egui::Pos2)>(pressed_slot_id);
                });
            }
            let visible = ui.available_width();
            let min_row_w = strip_state.content_width.max(visible).max(estimated_tools_w);
            // Controlled horizontal scroll: StripScrollState tracks finger drags directly so multi-swipe
            // right-to-left scrolling never bounces back on air-travel deltas or thumb-lift jitter.
            let scroll_out = egui::ScrollArea::horizontal()
                .id_salt("phone-tools-scroll")
                .auto_shrink([false, false])
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                .scroll_source(egui::scroll_area::ScrollSource::MOUSE_WHEEL)
                .horizontal_scroll_offset(strip_state.offset_x)
                .show(ui, |ui| {
                    crate::direction::horizontal_centered(ui, |ui| {
                        if crate::direction::is_rtl() {
                            ui.set_min_width(min_row_w);
                        }
                        ui.spacing_mut().item_spacing.x = 2.0;
                        // The same slot keys as the desktop toolbar, so both remember the last tool used.
                        for (si, section) in sections.iter().enumerate() {
                            if si > 0 {
                                ui.add_space(3.0);
                                let (rect, _) = ui.allocate_exact_size(vec2(1.0, TOUCH - 8.0), Sense::hover());
                                ui.painter().line_segment([rect.center_top(), rect.center_bottom()], Stroke::new(1.0, t.separator));
                                ui.add_space(3.0);
                            }
                            for (slot_index, slot) in section.iter() {
                                let key = egui::Id::unique(("tool-slot", *slot_index));
                                let tool = panels::slot_tool(ui, app.ui.tool, slot, key);
                                let selected = slot.contains(&app.ui.tool);
                                // Suppress hover tooltips on the phone strip so holding a tool button opens its flyout
                                // without a competing tooltip popup.
                                let resp = icons::button_with_icon_size_tip(
                                    ui,
                                    icons::tool_icon(tool),
                                    TOUCH,
                                    (TOUCH * 0.52).round(),
                                    selected,
                                    tl!(tool.label()),
                                    false,
                                );
                                if slot.len() > 1 {
                                    // A corner mark: this button holds several tools.
                                    let r = resp.rect;
                                    let corner = vec![
                                        r.right_bottom() + vec2(-2.0, -2.0),
                                        r.right_bottom() + vec2(-8.0, -2.0),
                                        r.right_bottom() + vec2(-2.0, -8.0),
                                    ];
                                    ui.painter().add(egui::Shape::convex_polygon(corner, t.text_faint, Stroke::NONE));
                                }
                                if !strip_dragging
                                    && ui.input(|i| i.pointer.any_pressed() && i.pointer.press_origin().is_some_and(|p| resp.rect.contains(p)))
                                    && let Some(origin) = ui.input(|i| i.pointer.press_origin())
                                {
                                    ui.data_mut(|d| d.insert_temp(pressed_slot_id, (key, origin)));
                                }
                                let open_here = ui.data(|d| d.get_temp::<(egui::Id, egui::Rect)>(flyout_id)).is_some_and(|(k, _)| k == key);
                                let pressed_slot = ui.data(|d| d.get_temp::<(egui::Id, egui::Pos2)>(pressed_slot_id));
                                let latest_pos = ui.input(|i| i.pointer.latest_pos());
                                let still_on_slot = !strip_dragging
                                    && pressed_slot.is_some_and(|(k, origin)| {
                                        k == key
                                            && latest_pos
                                                .or(Some(origin))
                                                .is_some_and(|pos| open_here || (resp.rect.expand(6.0).contains(pos) && (pos.x - origin.x).abs() <= 14.0))
                                    });
                                if !still_on_slot && pressed_slot.is_some_and(|(k, _)| k == key) {
                                    ui.data_mut(|d| d.remove::<(egui::Id, egui::Pos2)>(pressed_slot_id));
                                }
                                let down_here = !strip_dragging && (resp.is_pointer_button_down_on() || (still_on_slot && ui.input(|i| i.pointer.primary_down())));
                                let held_for = down_here.then(|| ui.input(|i| i.pointer.press_start_time().map(|t0| i.time - t0))).flatten();
                                if slot.len() > 1
                                    && let Some(seconds) = held_for
                                    && seconds < 0.35
                                {
                                    ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(0.35 - seconds));
                                }
                                let long_press = !strip_dragging && (held_for.is_some_and(|seconds| seconds >= 0.35) || resp.long_touched());
                                let held_key = ui.data(|d| d.get_temp::<egui::Id>(held_id));
                                let released_here = !strip_dragging
                                    && still_on_slot
                                    && ui.input(|i| i.pointer.any_released() && i.pointer.interact_pos().is_some_and(|p| resp.rect.expand(6.0).contains(p)));
                                let tapped = !strip_dragging && (resp.clicked() || released_here) && held_key != Some(key);
                                let reclicked = !strip_dragging && open_here && (resp.secondary_clicked() || tapped);
                                if reclicked {
                                    ui.data_mut(|d| d.remove::<(egui::Id, egui::Rect)>(flyout_id));
                                    photocraft_doc::diag::note(|| format!("tool_strip flyout closed by reclick on slot={slot_index}"));
                                } else if !strip_dragging && !open_here && slot.len() > 1 && (resp.secondary_clicked() || long_press) {
                                    ui.data_mut(|d| {
                                        d.insert_temp(flyout_id, (key, resp.rect));
                                        if long_press {
                                            d.insert_temp(held_id, key);
                                        }
                                    });
                                    crate::tool_memory::select(app, tool);
                                    photocraft_doc::diag::note(|| {
                                        format!("tool_strip flyout opened: slot={slot_index} tool={:?} rect={:?}", tool, resp.rect)
                                    });
                                    ui.ctx().request_repaint();
                                } else if !open_here && tapped {
                                    crate::tool_memory::select(app, tool);
                                    photocraft_doc::diag::note(|| format!("tool_strip tool selected: slot={slot_index} tool={:?}", tool));
                                }
                                if ui.data(|d| d.get_temp::<(egui::Id, egui::Rect)>(flyout_id)).is_some_and(|(k, _)| k == key) && slot.len() > 1 {
                                    let label_w = slot
                                        .iter()
                                        .map(|it| {
                                            ui.painter()
                                                .layout_no_wrap(tl!(it.label()).to_string(), egui::FontId::proportional(14.0), t.text)
                                                .size()
                                                .x
                                        })
                                        .fold(0.0f32, f32::max);
                                    let fw = (label_w + 56.0).max(210.0);
                                    let screen = ui.ctx().content_rect();
                                    let min_x = screen.left() + 6.0;
                                    let max_x = (screen.right() - fw - 6.0).max(min_x);
                                    let x = if crate::direction::is_rtl() {
                                        (resp.rect.right() - fw).clamp(min_x, max_x)
                                    } else {
                                        resp.rect.left().clamp(min_x, max_x)
                                    };
                                    let pos = egui::pos2(x, resp.rect.top() - 6.0);
                                    let area = egui::Area::new(key.with("flyout"))
                                        .order(egui::Order::Foreground)
                                        .pivot(egui::Align2::LEFT_BOTTOM)
                                        .fixed_pos(pos)
                                        .show(ui.ctx(), |ui| {
                                            Frame::popup(ui.style()).show(ui, |ui| {
                                                ui.spacing_mut().item_spacing.y = 2.0;
                                                for &item in slot.iter() {
                                                    let (r, ir) = ui.allocate_exact_size(vec2(fw, 40.0), Sense::click());
                                                    ir.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, item == tool, tl!(item.label())));
                                                    let pointer_over_row =
                                                        ui.input(|i| i.pointer.primary_down() && i.pointer.interact_pos().is_some_and(|p| r.contains(p)));
                                                    if pointer_over_row {
                                                        ui.ctx().request_repaint();
                                                    }
                                                    if item == tool {
                                                        ui.painter().rect_filled(r, t.radius_sm, t.accent_soft);
                                                        ui.painter().rect_stroke(r, t.radius_sm, Stroke::new(1.0, t.accent_border), egui::StrokeKind::Inside);
                                                    } else if ir.hovered() || pointer_over_row {
                                                        ui.painter().rect_filled(r, t.radius_sm, t.hover);
                                                    }
                                                    let fg = if item == tool { t.accent_text } else { t.text };
                                                    let icon_tint = if item == tool { t.accent_text } else { t.icon };
                                                    if item == tool {
                                                        ui.painter().rect_filled(
                                                            egui::Rect::from_center_size(egui::pos2(r.left() + 10.0, r.center().y), vec2(5.0, 5.0)),
                                                            1.0,
                                                            fg,
                                                        );
                                                    }
                                                    icons::paint(
                                                        ui,
                                                        egui::Rect::from_center_size(egui::pos2(r.left() + 28.0, r.center().y), vec2(20.0, 20.0)),
                                                        icons::tool_icon(item),
                                                        16.0,
                                                        icon_tint,
                                                    );
                                                    ui.painter().text(
                                                        egui::pos2(r.left() + 46.0, r.center().y),
                                                        egui::Align2::LEFT_CENTER,
                                                        tl!(item.label()),
                                                        egui::FontId::proportional(14.0),
                                                        fg,
                                                    );
                                                    let held_here = ui.data(|d| d.get_temp::<egui::Id>(held_id)) == Some(key);
                                                    let released_on_row = ui.input(|i| {
                                                        i.pointer.any_released()
                                                            && i.pointer.interact_pos().is_some_and(|p| r.contains(p))
                                                            && (i.pointer.press_origin().is_some_and(|p| r.contains(p)) || held_here)
                                                    });
                                                    if ir.clicked() || released_on_row {
                                                        crate::tool_memory::select(app, item);
                                                        photocraft_doc::diag::note(|| {
                                                            format!("tool_strip flyout item picked: slot={slot_index} item={:?}", item)
                                                        });
                                                        ui.data_mut(|d| {
                                                            d.remove::<(egui::Id, egui::Rect)>(flyout_id);
                                                            d.remove::<egui::Id>(held_id);
                                                            d.remove::<(egui::Id, egui::Pos2)>(pressed_slot_id);
                                                        });
                                                    }
                                                }
                                            });
                                        });
                                    let pressed_outside = ui.input(|i| {
                                        i.pointer.any_pressed()
                                            && i.pointer.press_origin().is_some_and(|p| !area.response.rect.contains(p) && !resp.rect.contains(p))
                                    });
                                    if pressed_outside || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                                        ui.data_mut(|d| d.remove::<(egui::Id, egui::Rect)>(flyout_id));
                                        photocraft_doc::diag::note(|| format!("tool_strip flyout dismissed: slot={slot_index}"));
                                    }
                                }
                            }
                        }
                        if ui.input(|i| i.pointer.any_released()) {
                            ui.data_mut(|d| {
                                d.remove::<egui::Id>(held_id);
                                d.remove::<(egui::Id, egui::Pos2)>(pressed_slot_id);
                            });
                        }
                    });
                });
            end_strip_scroll(
                ui,
                strip_scroll_id,
                "tool_strip",
                0x5453_5452,
                strip_state,
                scroll_out.inner_rect,
                scroll_out.content_size,
                scroll_out.state.unclamped_offset().x,
            );
        });
}

/// The active tool's options, in a row as wide as the screen. The row scrolls sideways, and its
/// minimum width is the screen's, so a control pinned to its right end (a transform's Commit, say)
/// stays on screen while wider option rows scroll smoothly end-to-end.
fn options(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    egui::Panel::bottom("phone-options")
        .exact_size(OPTIONS_ROW)
        .frame(Frame::NONE.fill(t.chrome).inner_margin(Margin::symmetric(6, 0)))
        .show(ui, |ui| {
            // The desktop bar's separators: the strip is drawn between two rules.
            let r = ui.max_rect();
            ui.painter().line_segment([r.left_top(), r.right_top()], Stroke::new(1.0, t.separator));
            ui.painter().line_segment([r.left_bottom(), r.right_bottom()], Stroke::new(1.0, t.separator));
            let strip_scroll_id = egui::Id::unique("phone-options-strip-scroll");
            let strip_state = begin_strip_scroll(ui, strip_scroll_id, "options_strip", 0x4f50_5453, false);
            let strip_dragging = strip_state.dragging;
            let visible = ui.available_width();
            let min_row_w = strip_state.content_width.max(visible);
            let scroll_out = egui::ScrollArea::horizontal()
                .id_salt("phone-options-scroll")
                .auto_shrink([false, false])
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                .scroll_source(egui::scroll_area::ScrollSource::MOUSE_WHEEL)
                .horizontal_scroll_offset(strip_state.offset_x)
                .show(ui, |ui| {
                    ui.set_min_width(min_row_w);
                    if strip_dragging {
                        ui.disable();
                        ui.set_opacity(1.0);
                    }
                    panels::options_strip(app, ui);
                });
            end_strip_scroll(
                ui,
                strip_scroll_id,
                "options_strip",
                0x4f50_5453,
                strip_state,
                scroll_out.inner_rect,
                scroll_out.content_size,
                scroll_out.state.unclamped_offset().x,
            );
        });
}

/// The panels sheet: one dock group at a time, chosen with the icons along its top.
fn sheet(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    if !is_set(&ctx, sheet_open_id()) {
        return;
    }
    let t = Tokens::get(&ctx);
    let group = ctx.data(|d| d.get_temp::<Group>(sheet_group_id())).unwrap_or(Group::Layers);
    let screen = ctx.content_rect().size();
    let (mut pick, mut close) = (None, false);
    let panel = if side_sheet(screen) {
        egui::Panel::right("phone-sheet-side")
            .resizable(true)
            // Room at least for the row of group icons and the hide button.
            .default_size((screen.x * 0.42).max(300.0))
            .size_range(290.0..=(screen.x * 0.6).max(300.0))
    } else {
        egui::Panel::bottom("phone-sheet")
            .resizable(true)
            .default_size((screen.y * 0.4).max(220.0))
            .size_range(220.0..=(screen.y * 0.85).max(260.0))
    };
    panel
        .frame(Frame::NONE.fill(t.dock).inner_margin(Margin::same(6)))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                for (g, icon, name) in SHEET_GROUPS {
                    if icons::button(ui, icon, TOUCH, group == g, tl!(name)).clicked() {
                        pick = Some(g);
                    }
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if icons::button(ui, "chevron-down", TOUCH, false, tl!("Hide panels")).clicked() {
                        close = true;
                    }
                });
            });
            crate::dock::show(app, ui, &[group], panels::dock_body);
        });
    if close {
        set_flag(&ctx, sheet_open_id(), false);
    }
    if let Some(g) = pick {
        ctx.data_mut(|d| d.insert_temp(sheet_group_id(), g));
    }
}

/// Does the panels sheet go beside the picture? It does on a phone held sideways: there a sheet above
/// the options would leave the picture a strip, and the screen's width has room for both.
pub fn side_sheet(screen: Vec2) -> bool {
    screen.x > screen.y * 1.3 && screen.x >= 560.0
}

/// The menu sheet's widest: on a phone held sideways a row across the whole screen is hard to read.
const MENU_MAX_WIDTH: f32 = 560.0;

/// The menu sheet's rows: a finger's target, with room for a second line under the label.
const MENU_ROW: f32 = 52.0;

/// The menus, as a sheet over the picture, one menu to a page: the top menus first, then a menu's own
/// commands and its submenus, and each submenu on a page of its own. A back button returns to the page
/// before. A search lists the matching commands instead, each with the menus it lives in. A command runs
/// and closes the sheet.
fn menu(app: &mut PhotocraftApp, ctx: &egui::Context) {
    if !is_set(ctx, menu_open_id()) {
        return;
    }
    let lang = i18n::current();
    let items = menus::menu_items(app);
    let mut query: String = ctx.data(|d| d.get_temp::<String>(menu_query_id())).unwrap_or_default();
    let mut path: Vec<String> = ctx.data(|d| d.get_temp::<Vec<String>>(menu_path_id())).unwrap_or_default();
    let mut picked: Option<String> = None;
    let mut close = false;
    let screen = ctx.content_rect();
    let viewport = ctx.viewport_rect();
    let nav_bottom_inset = (viewport.bottom() - screen.bottom()).max(0.0);
    let t = Tokens::get(ctx);
    let sheet = egui::Id::unique("phone-menu");
    // At the foot of the screen and as wide as it, so that a thumb reaches every row; above the
    // virtual keyboard and any system bottom navigation bar inset.
    let lift = virtual_input::lift(app) + nav_bottom_inset;
    let area = egui::Modal::default_area(sheet).anchor(egui::Align2::CENTER_BOTTOM, vec2(0.0, -8.0 - lift));
    let modal = egui::Modal::new(sheet).area(area).show(ctx, |ui| {
        ui.set_width(dialog_room(screen.size()).min(MENU_MAX_WIDTH));
        ui.set_max_height((screen.height() * 0.8 - lift).max(260.0));
        let searching = !query.trim().is_empty();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            if !searching && !path.is_empty() && back_button(ui, tl!("Back")) {
                path.pop();
            }
            let title = if searching {
                tl!("Search").to_string()
            } else if let Some(name) = path.last() {
                i18n::tr(lang, name).to_string()
            } else {
                tl!("Menu").to_string()
            };
            ui.vertical(|ui| {
                ui.label(RichText::new(title).font(theme::semibold(15.0)).color(t.text));
                // Where the page is, under its name (File › Export), so a deep page never loses its place.
                if !searching && path.len() > 1 {
                    let trail: Vec<&str> = path[..path.len() - 1].iter().map(|p| i18n::tr(lang, p)).collect();
                    ui.add(egui::Label::new(RichText::new(trail.join(" › ")).size(11.0).color(t.text_faint)).truncate());
                }
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if icons::button(ui, "x", TOUCH, false, tl!("Close menu")).clicked() {
                    close = true;
                }
            });
        });
        let field = ui.available_width();
        ui.add_sized(vec2(field, TOUCH), egui::TextEdit::singleline(&mut query).hint_text(tl!("Search commands")));
        ui.separator();
        // Each page keeps its own scroll: a page opened from far down a long menu starts at its top.
        let page_id = if searching { vec!["\u{1}search".to_string()] } else { path.clone() };
        egui::ScrollArea::vertical().id_salt(("phone-menu-page", page_id)).auto_shrink([false, true]).show(ui, |ui| {
            let needle = query.trim();
            if needle.is_empty() {
                // The page as the desktop menu has it: commands and submenus in their order, with the separators
                // between groups. What this build cannot run is left out, and a submenu with nothing it can run too.
                let mut gap = true;
                for entry in page_entries(&items, &path) {
                    match entry {
                        PageEntry::Separator => {
                            if !gap {
                                ui.add_space(4.0);
                                let r = ui.allocate_exact_size(vec2(ui.available_width(), 6.0), Sense::hover()).0;
                                ui.painter().line_segment([r.left_center(), r.right_center()], Stroke::new(2.0, t.separator));
                                ui.add_space(4.0);
                                gap = true;
                            }
                        }
                        PageEntry::Command(item) => {
                            command_row(ui, item, lang, &mut picked, "");
                            gap = false;
                        }
                        PageEntry::Submenu { name, enabled } => {
                            if menu_row(ui, i18n::tr(lang, &name), "", false, enabled, true).clicked() && enabled {
                                path.push(name.clone());
                            }
                            gap = false;
                        }
                    }
                }
                if path.is_empty() {
                    menu_footer_controls(app, ui);
                }
            } else {
                let hits = menus::search_items(&items, needle, lang);
                if hits.is_empty() {
                    ui.add_space(12.0);
                    ui.label(RichText::new(tl!("No commands match")).color(t.text_faint));
                }
                for item in hits {
                    let trail: Vec<&str> = item.path.iter().map(|p| i18n::tr(lang, p)).collect();
                    command_row(ui, item, lang, &mut picked, &trail.join(" › "));
                }
            }
        });
    });
    // The back key (Escape) goes back a page; on the top page it closes the menus.
    let escape = ctx.input(|i| i.key_pressed(egui::Key::Escape));
    let back = !close && modal.should_close() && escape && !path.is_empty() && query.trim().is_empty();
    if back {
        path.pop();
    }
    ctx.data_mut(|d| d.insert_temp(menu_query_id(), query));
    ctx.data_mut(|d| d.insert_temp(menu_path_id(), path));
    if let Some(cmd) = picked {
        close = true;
        if let Err(e) = menus::invoke(app, ctx, &cmd, menu_params(&cmd)) {
            let _ = notices::post(app, "PhotoDroid", vec![e], true, None);
        }
    }
    if close || (modal.should_close() && !back) {
        set_flag(ctx, menu_open_id(), false);
        ctx.data_mut(|d| d.insert_temp(menu_path_id(), Vec::<String>::new()));
    }
}

/// The menu sheet's page: the menus it has opened, outermost first (empty: the top menus).
fn menu_path_id() -> egui::Id {
    egui::Id::unique("phone-menu-path")
}

/// What one page of the menu sheet lists: the commands whose menu path is exactly the page's, and the
/// submenus directly under it.
struct MenuPage<'a> {
    #[cfg_attr(not(test), allow(dead_code))]
    commands: Vec<&'a menus::MenuItem>,
    submenus: Vec<String>,
}

/// The page at the menu path `at` (empty: the top menus). The top menus come in the order of
/// [`menus::TOP_MENUS`]; a submenu's come in the order its commands do.
fn menu_page<'a>(items: &'a [menus::MenuItem], at: &[String]) -> MenuPage<'a> {
    let commands: Vec<&menus::MenuItem> = items.iter().filter(|i| i.path.as_slice() == at).collect();
    let mut submenus: Vec<String> = Vec::new();
    if at.is_empty() {
        for top in menus::TOP_MENUS {
            if items.iter().any(|i| i.path.first().map(String::as_str) == Some(top)) {
                submenus.push(top.to_string());
            }
        }
    } else {
        for item in items.iter().filter(|i| i.path.len() > at.len() && i.path.starts_with(at)) {
            let name = &item.path[at.len()];
            if !submenus.contains(name) {
                submenus.push(name.clone());
            }
        }
    }
    MenuPage { commands, submenus }
}

/// One line of a menu page, in the desktop menu's order.
enum PageEntry<'a> {
    Command(&'a menus::MenuItem),
    /// A submenu, enabled while something under it can run.
    Submenu {
        name: String,
        enabled: bool,
    },
    Separator,
}

/// The page at `at` as the phone shows it: commands, submenus (where their first item is) and separators, in order.
/// Commands this build does not have are left out, and so are submenus with none it has; separators are not doubled,
/// and none starts or ends the page. The top page lists the top menus.
fn page_entries<'a>(items: &'a [menus::MenuItem], at: &[String]) -> Vec<PageEntry<'a>> {
    let live = |i: &menus::MenuItem| i.label != "---" && menus::is_live(&i.id);
    // Whether the submenu `name` holds anything this build has, and anything that can run now.
    let state = |name: &str| {
        let mut has = (false, false);
        for i in items.iter().filter(|i| i.path.len() > at.len() && i.path.starts_with(at) && i.path[at.len()] == name) {
            if live(i) {
                has = (true, has.1 || i.enabled);
            }
        }
        has
    };
    let mut out: Vec<PageEntry<'a>> = Vec::new();
    if at.is_empty() {
        for name in menu_page(items, at).submenus {
            let (shown, enabled) = state(&name);
            if shown {
                out.push(PageEntry::Submenu { name, enabled });
            }
        }
        return out;
    }
    let mut seen: Vec<&str> = Vec::new();
    for item in items.iter().filter(|i| i.path.starts_with(at)) {
        if item.path.len() == at.len() {
            if item.label == "---" {
                if !matches!(out.last(), None | Some(PageEntry::Separator)) {
                    out.push(PageEntry::Separator);
                }
            } else if live(item) {
                out.push(PageEntry::Command(item));
            }
        } else {
            let name = item.path[at.len()].as_str();
            if seen.contains(&name) {
                continue;
            }
            seen.push(name);
            let (shown, enabled) = state(name);
            if shown {
                out.push(PageEntry::Submenu { name: name.to_string(), enabled });
            }
        }
    }
    while matches!(out.last(), Some(PageEntry::Separator)) {
        out.pop();
    }
    out
}

/// One row of the menu sheet, a full-width touch target: the label, a line under it (a search result's
/// menus), a tick where the command is checked, and a chevron where the row opens a page.
fn menu_row(ui: &mut egui::Ui, label: &str, detail: &str, checked: bool, enabled: bool, submenu: bool) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), MENU_ROW), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, label));
    if enabled && resp.hovered() {
        ui.painter().rect_filled(rect, t.radius_sm, t.hover);
    }
    let ink = if enabled { t.text } else { t.text_faint };
    let mut x = rect.left() + 14.0;
    if checked {
        let tick = egui::Rect::from_center_size(egui::pos2(x + 8.0, rect.center().y), vec2(16.0, 16.0));
        icons::paint(ui, tick, "check", 16.0, t.accent);
        x += 26.0;
    }
    let painter = ui.painter();
    if detail.is_empty() {
        painter.text(egui::pos2(x, rect.center().y), egui::Align2::LEFT_CENTER, label, theme::medium(15.0), ink);
    } else {
        painter.text(egui::pos2(x, rect.top() + 17.0), egui::Align2::LEFT_CENTER, label, theme::medium(14.0), ink);
        let font = egui::FontId::proportional(11.5);
        painter.text(egui::pos2(x, rect.top() + 37.0), egui::Align2::LEFT_CENTER, detail, font, t.text_faint);
    }
    if submenu {
        chevron(painter, egui::pos2(rect.right() - 18.0, rect.center().y), true, t.text_dim);
    }
    painter.line_segment([rect.left_bottom(), rect.right_bottom()], Stroke::new(1.0, t.separator));
    resp
}

/// A chevron pointing forward (a submenu's row) or back (a page's back button).
fn chevron(painter: &egui::Painter, at: egui::Pos2, forward: bool, color: egui::Color32) {
    let s = if forward { 1.0 } else { -1.0 };
    let stroke = Stroke::new(1.6, color);
    painter.line_segment([at + vec2(-4.0 * s, -6.0), at + vec2(4.0 * s, 0.0)], stroke);
    painter.line_segment([at + vec2(4.0 * s, 0.0), at + vec2(-4.0 * s, 6.0)], stroke);
}

/// A page's back button: a touch target with a chevron pointing back. True when it is pressed.
fn back_button(ui: &mut egui::Ui, tooltip: &str) -> bool {
    let t = Tokens::get(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(vec2(TOUCH, TOUCH), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(rect, t.radius_sm, t.hover);
    }
    chevron(ui.painter(), rect.center(), false, t.text);
    resp.on_hover_text(tooltip).clicked()
}

/// A command's row in the menu sheet. A command the sheet cannot run is shown dimmed and does nothing.
fn command_row(ui: &mut egui::Ui, item: &menus::MenuItem, lang: i18n::Lang, picked: &mut Option<String>, detail: &str) {
    let enabled = item.enabled;
    let label = i18n::tr(lang, &item.label);
    if menu_row(ui, label, detail, item.checked == Some(true), enabled, false).clicked() && enabled {
        *picked = Some(item.id.clone());
    }
}

/// The width a dialog may take on the phone: the screen less a margin on each side, and at least 200 points.
pub fn dialog_room(screen: Vec2) -> f32 {
    (screen.x - 24.0).max(200.0)
}

/// A dialog's width: `desired`, or the room the phone's screen leaves it when that is less. Off the phone
/// (`room` is `None`) it is `desired`.
pub fn fit_width(desired: f32, room: Option<f32>) -> f32 {
    room.map_or(desired, |r| desired.min(r))
}

/// The width of a floating window (a panel, a small dialog of its own) that wants `desired`: on the
/// phone layout no wider than the screen less a margin on each side and the window's frame, so that
/// it does not run off the screen; elsewhere `desired`.
pub fn window_width(ctx: &egui::Context, desired: f32) -> f32 {
    if gestures_on(ctx) { fit_width(desired, Some(window_room(ctx.content_rect().size()))) } else { desired }
}

/// The room a floating window's content has on the phone: the screen less a margin and a frame on each
/// side, and at least 160 points.
fn window_room(screen: Vec2) -> f32 {
    (screen.x - 48.0).max(160.0)
}

/// The parameters the phone's menu passes a command. Its Type options (Anti-Alias, Orientation,
/// OpenType) change every type layer of the document, as a PSD's texts are meant to share them;
/// the desktop menus pass nothing and change the active layer only.
fn menu_params(id: &str) -> serde_json::Value {
    if id.starts_with("type.antiAlias.") || id.starts_with("type.orientation.") || id.starts_with("type.openType.") {
        json!({"allTextLayers": true})
    } else {
        json!({})
    }
}

/// Footer section at the bottom of the top-level hamburger menu:
/// 1) Debug toggle (starts saving `photocraft-log.txt` when on, deletes `.txt` log files when off).
/// 2) Active CPU core selector (`1..=max_cores`).
fn menu_footer_controls(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    ui.add_space(4.0);
    let sep = ui.allocate_exact_size(vec2(ui.available_width(), 6.0), Sense::hover()).0;
    ui.painter().line_segment([sep.left_center(), sep.right_center()], Stroke::new(2.0, t.separator));
    ui.add_space(4.0);

    // 1) Debug (.txt log) toggle row.
    let debug_on = photocraft_doc::diag::enabled();
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), MENU_ROW), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, debug_on, "Debug"));
    if resp.hovered() {
        ui.painter().rect_filled(rect, t.radius_sm, t.hover);
    }
    let detail = if debug_on {
        "Saving .txt logs in Downloads/PhotoCraft"
    } else {
        "Off — .txt log files removed"
    };
    let x = rect.left() + 14.0;
    ui.painter().text(egui::pos2(x, rect.top() + 17.0), egui::Align2::LEFT_CENTER, "Debug", theme::medium(14.0), t.text);
    ui.painter().text(egui::pos2(x, rect.top() + 37.0), egui::Align2::LEFT_CENTER, detail, egui::FontId::proportional(11.5), t.text_faint);
    let badge = egui::Rect::from_center_size(egui::pos2(rect.right() - 38.0, rect.center().y), vec2(52.0, 26.0));
    let (bg, border, fg, state_text) = if debug_on {
        (t.accent_soft, t.accent_border, t.accent_text, "ON")
    } else {
        (t.field, t.field_border, t.text_dim, "OFF")
    };
    ui.painter().rect_filled(badge, t.radius_sm, bg);
    ui.painter().rect_stroke(badge, t.radius_sm, Stroke::new(1.0, border), egui::StrokeKind::Inside);
    ui.painter().text(badge.center(), egui::Align2::CENTER_CENTER, state_text, theme::semibold(12.0), fg);
    ui.painter().line_segment([rect.left_bottom(), rect.right_bottom()], Stroke::new(1.0, t.separator));
    if resp.clicked() {
        menus::set_diagnostic_debug(app, !debug_on);
    }

    // 2) Active CPU core count row and quick-select pills.
    photocraft_doc::cpu::note_governor_state();
    let max_cores = photocraft_doc::cpu::max_cores();
    let active_cores = photocraft_doc::cpu::active_cores();
    let (cpu_rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), MENU_ROW), Sense::hover());
    let cores_detail = format!("{active_cores} / {max_cores} active cores");
    ui.painter().text(egui::pos2(cpu_rect.left() + 14.0, cpu_rect.top() + 17.0), egui::Align2::LEFT_CENTER, "CPU Cores", theme::medium(14.0), t.text);
    ui.painter().text(
        egui::pos2(cpu_rect.left() + 14.0, cpu_rect.top() + 37.0),
        egui::Align2::LEFT_CENTER,
        &cores_detail,
        egui::FontId::proportional(11.5),
        t.text_faint,
    );
    let can_dec = active_cores > 1;
    let can_inc = active_cores < max_cores;
    let dec_rect = egui::Rect::from_center_size(egui::pos2(cpu_rect.right() - 102.0, cpu_rect.center().y), vec2(34.0, 34.0));
    let count_center = egui::pos2(cpu_rect.right() - 62.0, cpu_rect.center().y);
    let inc_rect = egui::Rect::from_center_size(egui::pos2(cpu_rect.right() - 24.0, cpu_rect.center().y), vec2(34.0, 34.0));

    let dec_resp = ui.interact(dec_rect, ui.scope_id().with("cpu-dec"), Sense::click());
    dec_resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, can_dec, "Decrease CPU cores"));
    ui.painter().rect_filled(dec_rect, t.radius_sm, if can_dec && dec_resp.hovered() { t.hover } else { t.field });
    ui.painter().rect_stroke(dec_rect, t.radius_sm, Stroke::new(1.0, t.field_border), egui::StrokeKind::Inside);
    ui.painter().text(dec_rect.center(), egui::Align2::CENTER_CENTER, "−", theme::semibold(16.0), if can_dec { t.text } else { t.text_faint });
    if can_dec && dec_resp.clicked() {
        menus::set_active_cpu_cores(app, active_cores - 1);
    }

    ui.painter().text(count_center, egui::Align2::CENTER_CENTER, format!("{active_cores}"), theme::semibold(14.0), t.text);

    let inc_resp = ui.interact(inc_rect, ui.scope_id().with("cpu-inc"), Sense::click());
    inc_resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, can_inc, "Increase CPU cores"));
    ui.painter().rect_filled(inc_rect, t.radius_sm, if can_inc && inc_resp.hovered() { t.hover } else { t.field });
    ui.painter().rect_stroke(inc_rect, t.radius_sm, Stroke::new(1.0, t.field_border), egui::StrokeKind::Inside);
    ui.painter().text(inc_rect.center(), egui::Align2::CENTER_CENTER, "+", theme::semibold(16.0), if can_inc { t.text } else { t.text_faint });
    if can_inc && inc_resp.clicked() {
        menus::set_active_cpu_cores(app, active_cores + 1);
    }

    if max_cores > 1 {
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
            ui.add_space(10.0);
            for core in 1..=max_cores {
                let selected = core == active_cores;
                let (pill_rect, pill_resp) = ui.allocate_exact_size(vec2(34.0, 30.0), Sense::click());
                let label = format!("CPU {core}");
                pill_resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, selected, &label));
                let (pbg, pborder, pfg) = if selected {
                    (t.accent_soft, t.accent_border, t.accent_text)
                } else if pill_resp.hovered() {
                    (t.hover, t.field_border, t.text)
                } else {
                    (t.field, t.field_border, t.text_dim)
                };
                ui.painter().rect_filled(pill_rect, t.radius_sm, pbg);
                ui.painter().rect_stroke(pill_rect, t.radius_sm, Stroke::new(1.0, pborder), egui::StrokeKind::Inside);
                ui.painter().text(pill_rect.center(), egui::Align2::CENTER_CENTER, format!("{core}"), theme::medium(13.0), pfg);
                if pill_resp.clicked() {
                    menus::set_active_cpu_cores(app, core);
                }
            }
        });
        ui.add_space(6.0);
    }
    let bottom = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover()).0;
    ui.painter().line_segment([bottom.left_center(), bottom.right_center()], Stroke::new(1.0, t.separator));
}


#[cfg(test)]
mod tests {
    use super::*;

    /// Whether the phone's fonts are bound yet: the first frame of a harness runs before `setup_context`
    /// takes effect, so a test draws nothing until they are (as the app draws nothing until its fonts are ready).
    fn fonts_bound(ctx: &egui::Context) -> bool {
        ctx.fonts(|f| f.families().contains(&egui::FontFamily::Name("medium".into())))
    }

    #[test]
    fn the_top_bar_toggles_the_virtual_keyboard_and_the_virtual_mouse() {
        use egui_kittest::{Harness, kittest::Queryable};
        let app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        let mut h = Harness::builder()
            .with_size(vec2(411.0, 300.0))
            .build_ui_state(|ui, app: &mut PhotocraftApp| if fonts_bound(ui.ctx()) { top_bar(app, ui) }, app);
        PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::Studio);
        h.run_steps(4);
        h.get_by_label("Virtual keyboard").click();
        h.run_steps(2);
        assert!(h.state().virtual_input.keyboard_on());
        assert!(!h.state().virtual_mouse.is_on());
        h.get_by_label("Virtual mouse").click();
        h.run_steps(2);
        assert!(h.state().virtual_mouse.is_on());
        h.get_by_label("Virtual keyboard").click();
        h.run_steps(2);
        assert!(!h.state().virtual_input.keyboard_on());
        assert!(h.state().virtual_mouse.is_on(), "each toggle is its own");
    }

    #[test]
    fn the_top_bar_magnifier_turns_the_loupe_on_with_the_pointer() {
        use egui_kittest::{Harness, kittest::Queryable};
        let app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        let mut h = Harness::builder()
            .with_size(vec2(411.0, 300.0))
            .build_ui_state(|ui, app: &mut PhotocraftApp| if fonts_bound(ui.ctx()) { top_bar(app, ui) }, app);
        PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::Studio);
        h.run_steps(4);
        h.get_by_label("Virtual mouse").click();
        h.run_steps(2);
        assert!(h.state().virtual_mouse.is_on());
        h.get_by_label("Magnifier").click();
        h.run_steps(2);
        assert!(h.state().virtual_mouse.magnifier_on(), "the top bar's magnifier turns the loupe on");
        h.get_by_label("Magnifier").click();
        h.run_steps(2);
        assert!(!h.state().virtual_mouse.magnifier_on(), "the second tap turns the loupe off");
        assert!(h.state().virtual_mouse.is_on(), "and the pointer stays on");
    }

    #[test]
    fn a_key_of_the_virtual_keyboard_queues_its_text_for_the_next_frame() {
        use egui_kittest::{Harness, kittest::Queryable};
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.virtual_input.set_keyboard(true);
        let mut h = Harness::builder()
            .with_size(vec2(411.0, 600.0))
            .build_ui_state(|ui, app: &mut PhotocraftApp| {
                if fonts_bound(ui.ctx()) {
                    virtual_input::panel(app, ui, false)
                }
            }, app);
        PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::Studio);
        h.run_steps(4);
        h.get_by_label("q").click();
        h.run_steps(2);
        let typed: Vec<String> =
            h.state().virtual_input.queued().iter().filter_map(|e| if let egui::Event::Text(t) = e { Some(t.clone()) } else { None }).collect();
        assert_eq!(typed, ["q"]);
    }

    /// Held sideways the panels go beside the picture; upright, and on a square screen, under it.
    #[test]
    fn the_panels_sheet_goes_beside_the_picture_only_when_the_phone_is_sideways() {
        assert!(side_sheet(vec2(891.0, 411.0)));
        assert!(!side_sheet(vec2(411.0, 891.0)));
        assert!(!side_sheet(vec2(520.0, 500.0)));
        assert!(!side_sheet(vec2(500.0, 300.0)), "too narrow for the sheet and a picture beside it");
    }

    #[test]
    fn a_narrow_window_is_a_phone_in_either_orientation() {
        assert!(fits_phone(vec2(411.0, 891.0)));
        assert!(fits_phone(vec2(891.0, 411.0)));
        // Tablets and desktop windows keep the desktop chrome.
        assert!(!fits_phone(vec2(800.0, 1280.0)));
        assert!(!fits_phone(vec2(1440.0, 900.0)));
    }

    fn a_finger_lands() -> egui::Event {
        egui::Event::Touch { device_id: egui::TouchDeviceId(0), id: egui::TouchId(0), phase: egui::TouchPhase::Start, pos: egui::pos2(1.0, 1.0), force: None }
    }

    /// One frame of `ctx`, with the touch gestures on or off as the phone layout sets them. A test has
    /// no renderer for the textures the frame makes, so they are cleared: epaint panics in a debug
    /// build when they are dropped unapplied.
    fn frame(ctx: &egui::Context, phone: bool, input: egui::RawInput, mut body: impl FnMut(&mut egui::Ui)) {
        let mut out = ctx.run_ui(input, |ui| {
            set_gestures(ui.ctx(), phone);
            body(ui);
        });
        out.textures_delta.clear();
    }

    /// egui learns of a touch screen from its first touch and keeps the knowledge, so a row's sense
    /// does not flip between frames while a finger is down or lifted.
    #[test]
    fn a_touch_screen_is_known_from_its_first_touch_on_and_stays_known() {
        let ctx = egui::Context::default();
        let mut seen = Vec::new();
        frame(&ctx, true, egui::RawInput::default(), |ui| seen.push(touch_screen(ui.ctx())));
        frame(&ctx, true, egui::RawInput { events: vec![a_finger_lands()], ..Default::default() }, |ui| seen.push(touch_screen(ui.ctx())));
        frame(&ctx, true, egui::RawInput::default(), |ui| seen.push(touch_screen(ui.ctx())));
        assert_eq!(seen, [false, true, true]);
    }

    /// On the phone a row takes a drag only where there is no touch screen: a finger's drag is the swipe
    /// that scrolls the list, so the row takes clicks.
    #[test]
    fn a_row_takes_a_drag_only_where_there_is_no_touch_screen() {
        let ctx = egui::Context::default();
        let mut senses = Vec::new();
        frame(&ctx, true, egui::RawInput::default(), |ui| senses.push(row_sense(ui.ctx()) == Sense::click_and_drag()));
        frame(&ctx, true, egui::RawInput { events: vec![a_finger_lands()], ..Default::default() }, |ui| senses.push(row_sense(ui.ctx()) == Sense::click()));
        frame(&ctx, true, egui::RawInput::default(), |ui| senses.push(row_sense(ui.ctx()) == Sense::click()));
        assert_eq!(senses, [true, true, true]);
    }

    /// Off the phone layout (the desktop, the web, a tablet's desktop chrome) a touch screen keeps the
    /// mouse's drags, even after a touch: the gestures are the phone's only.
    #[test]
    fn off_the_phone_layout_a_touch_screen_keeps_the_mouse_behaviour() {
        let ctx = egui::Context::default();
        let mut seen = Vec::new();
        let mut state = |ui: &mut egui::Ui| seen.push((touch_screen(ui.ctx()), row_sense(ui.ctx()) == Sense::click_and_drag()));
        frame(&ctx, false, egui::RawInput { events: vec![a_finger_lands()], ..Default::default() }, &mut state);
        frame(&ctx, false, egui::RawInput::default(), &mut state);
        assert_eq!(seen, [(false, true), (false, true)]);
    }

    /// The phone's menu sends the Type options to every type layer, and the other commands nothing.
    #[test]
    fn the_phone_menu_sends_the_type_options_to_every_type_layer_and_nothing_else() {
        assert_eq!(menu_params("type.antiAlias.strong"), json!({"allTextLayers": true}));
        assert_eq!(menu_params("type.orientation.vertical"), json!({"allTextLayers": true}));
        assert_eq!(menu_params("type.openType.oldstyle"), json!({"allTextLayers": true}));
        assert_eq!(menu_params("type.createWorkPath"), json!({}));
        assert_eq!(menu_params("type.panels.characterStyles"), json!({}));
        assert_eq!(menu_params("layer.flatten"), json!({}));
    }

    fn menu_item(id: &str, path: &[&str]) -> menus::MenuItem {
        menus::MenuItem {
            id: id.into(),
            label: id.into(),
            path: path.iter().map(|p| p.to_string()).collect(),
            shortcut: None,
            enabled: true,
            checked: None,
            color: None,
        }
    }

    fn ids<'a>(items: &[&'a menus::MenuItem]) -> Vec<&'a str> {
        items.iter().map(|i| i.id.as_str()).collect()
    }

    /// The menu sheet pages through the menus one at a time: the top menus, then a menu's own commands
    /// and its submenus, each submenu on a page of its own.
    #[test]
    fn the_menu_sheet_pages_through_the_menus_and_their_submenus() {
        let items = vec![
            menu_item("file.open", &["File"]),
            menu_item("file.export.png", &["File", "Export"]),
            menu_item("file.export.jpg", &["File", "Export"]),
            menu_item("type.antiAlias.sharp", &["Type", "Anti-Alias"]),
            menu_item("type.createWorkPath", &["Type"]),
            menu_item("view.zoomIn", &["View"]),
        ];
        let root = menu_page(&items, &Vec::<String>::new());
        assert!(root.commands.is_empty());
        assert_eq!(root.submenus, ["File", "Type", "View"]);
        let file = menu_page(&items, &["File".to_string()]);
        assert_eq!(ids(&file.commands), ["file.open"]);
        assert_eq!(file.submenus, ["Export"]);
        let export = menu_page(&items, &["File".to_string(), "Export".to_string()]);
        assert_eq!(ids(&export.commands), ["file.export.png", "file.export.jpg"]);
        assert!(export.submenus.is_empty());
        let typ = menu_page(&items, &["Type".to_string()]);
        assert_eq!(ids(&typ.commands), ["type.createWorkPath"]);
        assert_eq!(typ.submenus, ["Anti-Alias"]);
    }

    /// A page lists commands and submenus in the menu's own order, with single separators between the groups, and
    /// leaves out what this build cannot run (and a submenu of nothing else).
    #[test]
    fn a_menu_page_keeps_the_menus_order_and_leaves_out_what_cannot_run() {
        let items = vec![
            menu_item("---", &["File"]),
            menu_item("file.open", &["File"]),
            menu_item("---", &["File"]),
            menu_item("---", &["File"]),
            menu_item("file.notInThisBuild", &["File"]),
            menu_item("file.export.exportAs", &["File", "Export"]),
            menu_item("---", &["File"]),
            menu_item("file.automate.notInThisBuild", &["File", "Automate"]),
            menu_item("file.save", &["File"]),
            menu_item("---", &["File"]),
        ];
        let shown: Vec<String> = page_entries(&items, &["File".to_string()])
            .iter()
            .map(|e| match e {
                PageEntry::Command(i) => i.id.clone(),
                PageEntry::Submenu { name, .. } => format!("{name} ›"),
                PageEntry::Separator => "—".into(),
            })
            .collect();
        assert_eq!(shown, ["file.open", "—", "Export ›", "—", "file.save"]);
    }

    /// On the phone a dialog takes the room the screen leaves it; elsewhere it keeps its own width.
    #[test]
    fn a_dialog_fits_the_phone_screen_and_keeps_its_width_elsewhere() {
        let room = dialog_room(vec2(411.0, 891.0));
        assert_eq!(room, 387.0);
        assert_eq!(fit_width(780.0, Some(room)), 387.0);
        assert_eq!(fit_width(380.0, Some(room)), 380.0);
        assert_eq!(fit_width(800.0, None), 800.0);
        assert_eq!(dialog_room(vec2(120.0, 300.0)), 200.0);
    }

    /// On the phone a floating window keeps within the screen; elsewhere it keeps its width.
    #[test]
    fn a_floating_window_fits_the_phone_screen_and_keeps_its_width_elsewhere() {
        assert_eq!(window_room(vec2(411.0, 891.0)), 363.0);
        assert_eq!(window_room(vec2(100.0, 300.0)), 160.0);
        let ctx = egui::Context::default();
        let mut widths = Vec::new();
        frame(&ctx, true, egui::RawInput::default(), |ui| widths.push(window_width(ui.ctx(), 100_000.0) < 100_000.0));
        frame(&ctx, true, egui::RawInput::default(), |ui| widths.push(window_width(ui.ctx(), 120.0) == 120.0));
        frame(&ctx, false, egui::RawInput::default(), |ui| widths.push(window_width(ui.ctx(), 100_000.0) == 100_000.0));
        assert_eq!(widths, [true, true, true]);
    }

    #[test]
    fn long_press_tool_button_opens_flyout_and_picks_subtool() {
        use crate::state::Tool;
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.phone_layout = true;
        let ctx = egui::Context::default();
        PhotocraftApp::setup_context(&ctx, crate::theme::ThemeKind::Pro);
        let step = |app: &mut PhotocraftApp, time: f64, events: Vec<egui::Event>| {
            let input = egui::RawInput {
                time: Some(time),
                events,
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(411.0, 891.0))),
                ..Default::default()
            };
            frame(&ctx, true, input, |ui| tool_strip(app, ui));
        };
        step(&mut app, 0.0, vec![]);
        step(&mut app, 0.1, vec![]);
        let index = panels::TOOL_SECTIONS.iter().flat_map(|s| s.iter()).position(|slot| slot.contains(&Tool::Lasso)).unwrap();
        let mut buttons: Vec<egui::Rect> = ctx.viewport(|v| {
            v.prev_pass
                .widgets
                .layers()
                .flat_map(|(_, w)| w.iter())
                .filter(|w| w.rect.size() == Vec2::splat(TOUCH) && w.sense.senses_click())
                .map(|w| w.rect)
                .collect()
        });
        buttons.sort_by(|a, b| a.left().total_cmp(&b.left()));
        let at = buttons[index].center();
        let pointer = |pos, pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() };
        // Press and hold the Lasso slot (with slight thumb drift inside the button), then lift.
        step(&mut app, 1.0, vec![egui::Event::PointerMoved(at), pointer(at, true)]);
        step(&mut app, 1.2, vec![egui::Event::PointerMoved(at + vec2(8.0, 0.0))]);
        step(&mut app, 1.38, vec![]);
        let key = egui::Id::unique(("tool-slot", index));
        assert_eq!(ctx.data(|d| d.get_temp::<(egui::Id, egui::Rect)>(egui::Id::unique("tool-flyout"))).map(|(id, _)| id), Some(key));
        // Lifting the finger at the end of the long press keeps the flyout open.
        step(&mut app, 1.45, vec![pointer(at + vec2(8.0, 0.0), false)]);
        step(&mut app, 1.5, vec![]);
        assert_eq!(app.ui.tool, Tool::Lasso);
        assert_eq!(ctx.data(|d| d.get_temp::<(egui::Id, egui::Rect)>(egui::Id::unique("tool-flyout"))).map(|(id, _)| id), Some(key));
        // Tap the 3rd row (Magnetic Lasso Tool) in the flyout.
        let menu = ctx.memory(|m| m.area_rect(key.with("flyout"))).unwrap();
        let magnetic_row = egui::pos2(menu.center().x, menu.bottom() - 24.0);
        step(&mut app, 2.0, vec![egui::Event::PointerMoved(magnetic_row), pointer(magnetic_row, true)]);
        step(&mut app, 2.05, vec![pointer(magnetic_row, false)]);
        assert_eq!(app.ui.tool, Tool::MagneticLasso);
        assert!(ctx.data(|d| d.get_temp::<(egui::Id, egui::Rect)>(egui::Id::unique("tool-flyout"))).is_none());
    }

    #[test]
    fn the_menu_footer_toggles_debug_logs_and_sets_cpu_cores() {
        use egui_kittest::{Harness, kittest::Queryable};
        photocraft_doc::diag::set_enabled(false);
        photocraft_doc::diag::clear();
        photocraft_doc::cpu::reset_active_cores();

        let saved_files = std::rc::Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
        let saved_sink = saved_files.clone();
        let debug_states = std::rc::Rc::new(std::cell::RefCell::new(Vec::<bool>::new()));
        let debug_sink = debug_states.clone();
        let cpu_states = std::rc::Rc::new(std::cell::RefCell::new(Vec::<usize>::new()));
        let cpu_sink = cpu_states.clone();

        let svc = crate::Services {
            write: Some(Box::new(move |name: &str, _bytes: &[u8]| {
                saved_sink.borrow_mut().push(name.to_string());
                Ok(())
            })),
            set_debug_log: Some(Box::new(move |on: bool| {
                debug_sink.borrow_mut().push(on);
                if !on {
                    saved_files.borrow_mut().clear();
                }
                Ok(())
            })),
            set_cpu_cores: Some(Box::new(move |cores: usize| {
                cpu_sink.borrow_mut().push(cores);
                Ok(())
            })),
            ..Default::default()
        };
        let app = PhotocraftApp::new(photocraft_engine::Session::new(), svc);
        let mut h = Harness::builder()
            .with_size(vec2(411.0, 500.0))
            .build_ui_state(|ui, app: &mut PhotocraftApp| {
                if fonts_bound(ui.ctx()) {
                    menu_footer_controls(app, ui);
                }
            }, app);
        PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::Studio);
        h.run_steps(4);

        assert!(!photocraft_doc::diag::enabled(), "debug starts off");
        h.get_by_label("Debug").click();
        h.run_steps(2);
        assert!(photocraft_doc::diag::enabled(), "tapping Debug turns diagnostic logging on");
        assert_eq!(*debug_states.borrow(), vec![true]);

        h.get_by_label("Debug").click();
        h.run_steps(2);
        assert!(!photocraft_doc::diag::enabled(), "tapping Debug again turns diagnostic logging off");
        assert_eq!(*debug_states.borrow(), vec![true, false]);

        if photocraft_doc::cpu::max_cores() > 1 {
            h.get_by_label("CPU 1").click();
            h.run_steps(2);
            assert_eq!(photocraft_doc::cpu::active_cores(), 1);
            h.get_by_label("Increase CPU cores").click();
            h.run_steps(2);
            assert_eq!(photocraft_doc::cpu::active_cores(), 2);
            assert_eq!(*cpu_states.borrow(), vec![1, 2]);
        }
        photocraft_doc::cpu::reset_active_cores();
    }

    #[test]
    fn multi_swipe_right_to_left_scrolls_tool_strip_to_end_without_bounce_back() {
        for dir in [crate::direction::Dir::Ltr, crate::direction::Dir::Rtl] {
            let _dir = crate::direction::direction_scope(dir);
            let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
            app.phone_layout = true;
            let ctx = egui::Context::default();
            PhotocraftApp::setup_context(&ctx, crate::theme::ThemeKind::Pro);
            let step = |app: &mut PhotocraftApp, time: f64, mut events: Vec<egui::Event>| {
                let mut input = egui::RawInput {
                    time: Some(time),
                    events: std::mem::take(&mut events),
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(360.0, 800.0))),
                    ..Default::default()
                };
                sanitize_touch_events(&ctx, &mut input);
                frame(&ctx, true, input, |ui| tool_strip(app, ui));
            };
            step(&mut app, 0.0, vec![]);
            step(&mut app, 0.05, vec![]);

            let strip_id = egui::Id::unique("phone-tools-strip-scroll");
            let initial = ctx.data(|d| d.get_temp::<StripScrollState>(strip_id)).unwrap_or_default();
            assert!(initial.max_offset_x > 200.0, "tool strip overflows a 360pt phone screen: {:?}", initial);
            let y = initial.strip_rect.unwrap().center().y;
            let pointer = |pos, pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() };

            // First right-to-left swipe: 330 -> 50, with a +2.5pt thumb-peel wobble on release.
            step(&mut app, 0.10, vec![egui::Event::PointerMoved(egui::pos2(330.0, y)), pointer(egui::pos2(330.0, y), true)]);
            step(&mut app, 0.12, vec![egui::Event::PointerMoved(egui::pos2(240.0, y))]);
            step(&mut app, 0.14, vec![egui::Event::PointerMoved(egui::pos2(120.0, y))]);
            step(&mut app, 0.16, vec![egui::Event::PointerMoved(egui::pos2(50.0, y))]);
            step(&mut app, 0.18, vec![egui::Event::PointerMoved(egui::pos2(52.5, y)), pointer(egui::pos2(52.5, y), false)]);
            let after_swipe1 = ctx.data(|d| d.get_temp::<StripScrollState>(strip_id)).unwrap();
            assert!(after_swipe1.offset_x > 200.0, "first right-to-left swipe scrolled strip: {:?}", after_swipe1);

            // Second right-to-left swipe starting back on the right at x=330: must NOT bounce back!
            step(&mut app, 0.25, vec![egui::Event::PointerMoved(egui::pos2(330.0, y)), pointer(egui::pos2(330.0, y), true)]);
            let on_repress = ctx.data(|d| d.get_temp::<StripScrollState>(strip_id)).unwrap();
            assert!(
                on_repress.offset_x >= after_swipe1.offset_x - 1.0,
                "putting finger back on right side must not bounce offset_x back: before={} after={}",
                after_swipe1.offset_x,
                on_repress.offset_x
            );
            step(&mut app, 0.27, vec![egui::Event::PointerMoved(egui::pos2(200.0, y))]);
            step(&mut app, 0.29, vec![egui::Event::PointerMoved(egui::pos2(40.0, y))]);
            step(&mut app, 0.31, vec![pointer(egui::pos2(40.0, y), false)]);
            let after_swipe2 = ctx.data(|d| d.get_temp::<StripScrollState>(strip_id)).unwrap();
            assert!(after_swipe2.offset_x > after_swipe1.offset_x + 100.0, "second swipe advanced further: {:?}", after_swipe2);

            // Third right-to-left swipe reach the end of the strip.
            step(&mut app, 0.40, vec![egui::Event::PointerMoved(egui::pos2(330.0, y)), pointer(egui::pos2(330.0, y), true)]);
            step(&mut app, 0.42, vec![egui::Event::PointerMoved(egui::pos2(30.0, y))]);
            step(&mut app, 0.44, vec![pointer(egui::pos2(30.0, y), false)]);
            let final_state = ctx.data(|d| d.get_temp::<StripScrollState>(strip_id)).unwrap();
            assert!(
                (final_state.offset_x - final_state.max_offset_x).abs() < 1.0,
                "scrolled all the way to the end ({:?})",
                final_state
            );
        }
    }

    #[test]
    fn touch_cancel_injects_primary_button_release() {
        let ctx = egui::Context::default();
        let pos = egui::pos2(120.0, 760.0);
        let press_input = egui::RawInput {
            events: vec![
                egui::Event::Touch { device_id: egui::TouchDeviceId(0), id: egui::TouchId(1), phase: egui::TouchPhase::Start, pos, force: None },
                egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() },
            ],
            ..Default::default()
        };
        frame(&ctx, true, press_input, |_| {});
        assert!(ctx.input(|i| i.pointer.primary_down()));

        let mut cancel_input = egui::RawInput {
            events: vec![
                egui::Event::Touch { device_id: egui::TouchDeviceId(0), id: egui::TouchId(1), phase: egui::TouchPhase::Cancel, pos, force: None },
                egui::Event::PointerGone,
            ],
            ..Default::default()
        };
        sanitize_touch_events(&ctx, &mut cancel_input);
        frame(&ctx, true, cancel_input, |_| {});
        assert!(!ctx.input(|i| i.pointer.primary_down()), "TouchPhase::Cancel releases the primary button");
    }
}
