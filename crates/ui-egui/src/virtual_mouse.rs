//! The virtual mouse: a pointer on the phone's canvas, that a finger moves the way a touchpad's does, and the buttons
//! of a mouse made of the screen's touches.
//!
//! - A finger that slides moves the pointer by the distance it slides, not to where the finger is, so a finger that
//!   is placed again does not jump the pointer. The pointer eases to where the finger puts it, so it glides and does
//!   not step. A slide presses nothing, however slowly it goes.
//! - A tap (a finger that lifts within [`TAP_SECS`] without moving more than [`TAP_SLOP`]) clicks where the pointer
//!   was when the finger touched down, so the finger's small wobble during the tap does not nudge the click. A finger
//!   that rests and then lifts clicks nothing.
//! - Two taps in a row ([`DOUBLE_TAP_SECS`] apart) are a double click. A second touch that moves after a tap is a
//!   drag, and it starts the drag lock: the button stays held across lifts, so a finger that lifts and moves again
//!   goes on dragging (a stroke goes on), until another double tap ends the lock. While the lock is on, a single tap
//!   clicks nothing.
//! - A lift or a pause (a second finger, a press on the screen, a dialog) lets the button go, so the stroke ends
//!   where the pointer was, and the next movement starts a new stroke there. The view's change never carries a stroke
//!   across it. Each stroke is an undo step.
//! - Two fingers that tap together (neither moves) are a right click. Two fingers that move pinch or pan the canvas,
//!   as they do without the pointer, and the pointer stays where it is.
//! - The magnifier (a toggle in the top bar) draws a loupe in a corner of the screen while a finger works: the canvas
//!   around the pointer (or, with the pointer off, around the finger), enlarged, so it is clear what is being touched.
//!   It moves to the other corner when the work comes under it. It works with the pointer off too.
//! - The pointer's own buttons float over the canvas while it is on: a right click where the pointer is (no two-finger
//!   tap to aim), and the precise mode, which slows the pointer down for fine work.
//! - A locked drag that reaches the canvas's edge pans the picture (`edge_push`), so a stroke or an outline can go on
//!   past what the screen shows. A padlock beside the pointer shows that the lock is on.
//!
//! A touch off the canvas, or on a floating window over it, goes to the screen as it is, and so does a press that
//! starts there; while such a press is held, the pointer draws nothing and sends no moves. The pointer's presses and
//! moves replace the first finger's pointer events (which egui makes where the finger is), so the canvas gets what a
//! mouse would send. The events are queued into the frame's input (`raw_input_hook`).

use egui::{Color32, Event, Id, LayerId, Modifiers, Order, Pos2, PointerButton, Rect, Stroke, TextureId, TouchId, TouchPhase, Vec2, pos2, vec2};

use crate::icons;
use crate::theme::Tokens;

/// A finger that moves no further than this (points) from where it touched down is still: a tap.
pub const TAP_SLOP: f32 = 8.0;
/// A touch held longer than this (seconds) is no tap, however still it was: a finger that rests and lifts clicks nothing.
pub const TAP_SECS: f64 = 0.3;
/// A touch that starts within this many seconds of a tap's release is the second touch of a double tap.
pub const DOUBLE_TAP_SECS: f64 = 0.35;
/// The pointer's gain (points of pointer per point of finger): [`GAIN_MIN`] for a very slow finger (fine positioning),
/// 1 for a slow one, [`GAIN_MAX`] for a fast one.
const GAIN_MIN: f32 = 0.7;
const GAIN_MAX: f32 = 1.8;
/// The finger's speed (points a second) below which the gain is [`GAIN_MIN`], where it is back at 1 and starts to rise,
/// and where it is at [`GAIN_MAX`].
const PRECISE_BELOW: f32 = 40.0;
const ACCEL_FROM: f32 = 220.0;
const ACCEL_TO: f32 = 1400.0;
/// The finger's speed is smoothed with this time constant (seconds), so that the gain does not jump with one frame's
/// uneven touch samples.
const SPEED_SECS: f32 = 0.05;
/// The pointer eases to where the finger puts it, with this time constant (seconds).
const EASE_SECS: f64 = 0.04;
/// The pointer is at its target within this many points.
const SETTLED: f32 = 0.05;
/// A locked drag presses the button once the finger has moved this far (points) since it touched down.
const RESUME_PRESS: f32 = 0.5;
/// The loupe's side (points), and how much it enlarges the canvas.
pub const LOUPE: f32 = 144.0;
pub const MAGNIFY: f32 = 2.5;
/// The loupe's margin from the screen's corner, and the room it keeps from the work it shows.
const LOUPE_MARGIN: f32 = 8.0;
const LOUPE_CLEAR: f32 = 24.0;
/// The pointer's gain in the precise mode, on top of the usual gain.
const PRECISE_GAIN: f32 = 0.35;
/// A locked drag within this many points of the canvas's edge pans the picture, at up to [`EDGE_SPEED`] points a second.
const EDGE: f32 = 28.0;
const EDGE_SPEED: f32 = 700.0;

/// A finger on the canvas: where it touched down, where it is, and the furthest it has been from the first place.
#[derive(Clone, Copy, Debug)]
struct Finger {
    id: TouchId,
    start: Pos2,
    last: Pos2,
    travel: f32,
}

/// The fingers from the first touch on the canvas until the last one lifts, and what they did.
#[derive(Clone, Copy, Debug)]
struct Gesture {
    /// How many fingers touched the canvas in it.
    fingers: u8,
    /// The furthest any finger has moved from where it touched down. A drag makes a gesture no tap.
    travel: f32,
    /// The first finger is the second touch of a double tap.
    armed: bool,
    /// When the first finger touched down.
    started: f64,
    /// Where the fingers had put the pointer when the first finger touched down: a tap clicks there.
    anchor: Option<Pos2>,
}

/// What a frame's events became (see [`VirtualMouse::process`]).
#[derive(Debug)]
pub struct Processed {
    /// The frame's events.
    pub events: Vec<Event>,
    /// Whether the pointer clicked in it (a press, which a held modifier key is spent by).
    pub clicked: bool,
}

/// The canvas picture as it was drawn this frame: its texture, where it is drawn, and whether it is mirrored. The
/// loupe samples it.
#[derive(Clone, Copy, Debug)]
struct CanvasView {
    tex: TextureId,
    img: Rect,
    flip: bool,
}

/// The time, the canvas, and the modifiers of one frame's processing.
#[derive(Clone, Copy)]
struct Step {
    now: f64,
    canvas: Rect,
    mods: Modifiers,
}

#[derive(Debug, Default)]
pub struct VirtualMouse {
    on: bool,
    magnifier: bool,
    covered: bool,
    /// Where the pointer is drawn: it eases towards `target`.
    pos: Option<Pos2>,
    /// Where the fingers have put the pointer.
    target: Option<Pos2>,
    /// The drag lock: from a double tap's drag until the next double tap, the button stays held across lifts.
    locked: bool,
    /// The button is down, as this mouse has sent it.
    down: bool,
    /// The first finger on the canvas: it moves the pointer, unless a second finger is down.
    pad: Option<Finger>,
    /// A second finger on the canvas: a pinch, a pan, or the second finger of a right click.
    second: Option<Finger>,
    gesture: Option<Gesture>,
    /// When the last tap was released, until the next touch.
    last_tap: Option<f64>,
    /// When the last frame was processed, for the finger's speed and the easing.
    last_frame: Option<f64>,
    /// The first finger's speed (points a second), smoothed over a few frames (see [`SPEED_SECS`]).
    speed: f32,
    /// Events queued for the next frame (a button let go when the mouse went off or the screen was covered).
    queued: Vec<Event>,
    /// The precise mode: the pointer moves at [`PRECISE_GAIN`] of its usual speed.
    precise: bool,
    /// A locked drag is at the canvas's edge, panning the picture (see [`VirtualMouse::edge_push`]).
    pushing: bool,
    /// Whether egui's primary press started on the canvas (`Some(true)`) or on the screen (`Some(false)`); `None` while
    /// no press is down.
    press: Option<bool>,
}

impl VirtualMouse {
    pub fn is_on(&self) -> bool {
        self.on
    }

    /// Whether the magnifier's loupe is on (it shows the pointer's place, so the pointer is on with it).
    pub fn magnifier_on(&self) -> bool {
        self.magnifier
    }

    /// Whether the precise mode is on (the pointer moves slower, for fine work).
    pub fn precise_on(&self) -> bool {
        self.precise
    }

    pub fn toggle_precise(&mut self) {
        self.precise = !self.precise;
    }

    /// A right click where the pointer is, in the next frame: the floating button's, aimed with the pointer rather than
    /// with two fingers.
    pub fn right_click(&mut self) {
        if !self.on {
            return;
        }
        let Some(p) = self.pos else { return };
        if let Some(e) = self.release_now() {
            self.queued.push(e);
        }
        self.queued.push(Event::PointerMoved(p));
        click(p, PointerButton::Secondary, Modifiers::NONE, &mut self.queued);
    }

    /// Whether the drag lock is on: the button stays held across lifts, until a double tap turns the lock off.
    pub fn locked(&self) -> bool {
        self.locked
    }

    /// Whether the button is down now.
    pub fn held(&self) -> bool {
        self.down
    }

    /// Where the pointer is shown: while it is on and no dialog or menu covers the screen.
    fn shown_at(&self) -> Option<Pos2> {
        if self.on && !self.covered { self.pos } else { None }
    }

    /// Turns the pointer on (in the middle of `canvas`, unless it is still on the canvas where it was), or off. The
    /// magnifier stays as it is: with the pointer off it follows the finger.
    pub fn toggle(&mut self, canvas: Rect) {
        if self.on {
            self.stop();
            self.on = false;
            self.pushing = false;
        } else {
            self.on = true;
            // Before the canvas is laid out there is no place for the pointer yet.
            if canvas.is_positive() && !self.pos.is_some_and(|p| canvas.contains(p)) {
                let center = canvas.center();
                self.pos = Some(center);
                self.target = Some(center);
            }
        }
    }

    /// The magnifier: a loupe in a corner of the screen, around the pointer or the finger. It needs no pointer.
    pub fn toggle_magnifier(&mut self, _canvas: Rect) {
        self.magnifier = !self.magnifier;
    }

    /// Whether a dialog or the menu covers the screen. Covering ends everything the fingers were doing (the drag lock
    /// too): a held button is let go.
    pub fn set_covered(&mut self, covered: bool) {
        if covered && !self.covered {
            self.stop();
        }
        self.covered = covered;
    }

    /// Whether a frame should come soon, though no input does: the pointer is still easing to its target.
    pub fn wants_frames(&self) -> bool {
        self.on && !self.covered && (self.pushing || self.pos.zip(self.target).is_some_and(|(p, t)| (t - p).length() > SETTLED))
    }

    /// The picture's pan this frame (points a second, screen axes: where the view should move) while a held drag is at
    /// the edge of `canvas`: the nearer the edge, the faster. Zero otherwise.
    pub fn edge_push(&mut self, canvas: Rect) -> Vec2 {
        let push = match self.shown_at() {
            Some(p) if self.down && canvas.is_positive() && canvas.contains(p) => edge_velocity(p, canvas),
            _ => Vec2::ZERO,
        };
        self.pushing = push != Vec2::ZERO;
        push
    }

    /// Ends what the fingers were doing: a held button is let go (in the next frame), and the drag lock goes.
    fn stop(&mut self) {
        if let Some(e) = self.release_now() {
            self.queued.push(e);
        }
        self.locked = false;
        self.gesture = None;
        self.pad = None;
        self.second = None;
        self.last_tap = None;
        self.target = self.pos;
    }

    /// The button's release, for a place outside a frame's processing; `None` if no button is down.
    fn release_now(&mut self) -> Option<Event> {
        if !self.down {
            return None;
        }
        self.down = false;
        self.pos.map(|p| button(p, PointerButton::Primary, false, Modifiers::NONE))
    }

    /// The frame's events, from the screen. The pointer takes the presses and moves on the canvas, which is where
    /// `on_canvas` says (not a window over it), and adds its own. `now` is the time in seconds; `mods` are the
    /// modifiers the keyboard holds, which the pointer's clicks carry.
    pub fn process(&mut self, events: Vec<Event>, canvas: Rect, now: f64, mods: Modifiers, on_canvas: impl Fn(Pos2) -> bool) -> Processed {
        let mut out = std::mem::take(&mut self.queued);
        if !self.on || self.covered || !canvas.is_positive() {
            out.extend(events);
            return Processed { events: out, clicked: false };
        }
        self.pos = self.pos.map(|p| within(p, canvas));
        self.target = self.target.map(|t| within(t, canvas));
        let step = Step { now, canvas, mods };
        let mut injected = Vec::new();
        let mut moved = Vec2::ZERO;
        // A press on the screen this frame (a button, a strip, a tab): the pointer draws nothing while it lasts.
        let mut screen = false;
        for event in events {
            match event {
                Event::Touch { id, phase, pos, .. } => {
                    moved += self.touch(step, id, phase, pos, &on_canvas, &mut injected);
                    out.push(event);
                }
                Event::PointerButton { pos, button: PointerButton::Primary, pressed, .. } => {
                    if pressed {
                        let here = on_canvas(pos);
                        self.press = Some(here);
                        if here {
                            continue;
                        }
                        // The button lets go before the press on the screen, so that the press is not dragged along.
                        screen = true;
                        self.release(step, &mut out);
                    } else if self.press.take() == Some(true) {
                        continue;
                    } else {
                        screen = true;
                    }
                    out.push(event);
                }
                Event::PointerMoved(pos) => {
                    let taken = match self.press {
                        Some(here) => here,
                        None => on_canvas(pos),
                    };
                    if !taken {
                        out.push(event);
                    }
                }
                // Egui reports the pointer gone when a touch lifts, and the press on the screen ends with it.
                Event::PointerGone => {
                    if self.press == Some(false) {
                        self.press = None;
                        screen = true;
                    }
                    if screen {
                        out.push(event);
                    }
                }
                other => out.push(other),
            }
        }
        self.advance(step, moved, &mut injected);
        // Kept in place every frame, so that the canvas keeps the pointer as hovered. Not while a press on the screen
        // is using the pointer: then the pointer sends no moves at all, so that the press is not dragged along.
        let quiet = screen || self.press == Some(false);
        if quiet {
            injected.retain(|e| !matches!(e, Event::PointerMoved(_)));
        } else if let Some(p) = self.pos
            && !injected.iter().any(|e| matches!(e, Event::PointerMoved(_)))
        {
            injected.insert(0, Event::PointerMoved(p));
        }
        let clicked = injected.iter().any(|e| matches!(e, Event::PointerButton { pressed: true, .. }));
        out.extend(injected);
        Processed { events: out, clicked }
    }

    /// One touch's event. Returns how far the first finger moved, which the pointer follows (see [`VirtualMouse::advance`]).
    fn touch(&mut self, step: Step, id: TouchId, phase: TouchPhase, pos: Pos2, on_canvas: &impl Fn(Pos2) -> bool, injected: &mut Vec<Event>) -> Vec2 {
        match phase {
            TouchPhase::Start => self.start(step, id, pos, on_canvas, injected),
            TouchPhase::Move => return self.move_finger(step, id, pos, injected),
            TouchPhase::End => self.end(step, id, pos, true, injected),
            TouchPhase::Cancel => self.end(step, id, pos, false, injected),
        }
        Vec2::ZERO
    }

    fn start(&mut self, step: Step, id: TouchId, pos: Pos2, on_canvas: &impl Fn(Pos2) -> bool, injected: &mut Vec<Event>) {
        if self.gesture.is_none() {
            // A gesture starts with a finger on the canvas, and not after a press on the screen over it.
            if self.press == Some(false) || !on_canvas(pos) {
                return;
            }
            let armed = self.last_tap.is_some_and(|t| step.now - t <= DOUBLE_TAP_SECS);
            self.last_tap = None;
            self.gesture = Some(Gesture { fingers: 1, travel: 0.0, armed, started: step.now, anchor: self.target });
            self.speed = 0.0;
            self.pad = Some(Finger { id, start: pos, last: pos, travel: 0.0 });
            // The second touch of a double tap presses at once, so a drag from here has no gap. While the lock is on,
            // a tap presses nothing: it only counts toward ending the lock.
            if armed && !self.locked {
                self.press_down(step, injected);
            }
            return;
        }
        // Another finger on the canvas makes a pinch, a pan or a right click. A finger on the screen is not the pointer's.
        if !on_canvas(pos) {
            return;
        }
        if let Some(g) = self.gesture.as_mut() {
            g.fingers = g.fingers.saturating_add(1);
            g.armed = false;
        }
        let finger = Finger { id, start: pos, last: pos, travel: 0.0 };
        if self.pad.is_none() {
            self.pad = Some(finger);
        } else if self.second.is_none() {
            self.second = Some(finger);
        }
        // The pointer lets go, so the stroke ends before the view moves under it.
        self.release(step, injected);
    }

    /// A finger moved. Returns the pointer's movement from it (none with two fingers down).
    fn move_finger(&mut self, step: Step, id: TouchId, pos: Pos2, injected: &mut Vec<Event>) -> Vec2 {
        if let Some(mut f) = self.pad
            && f.id == id
        {
            let moved = pos - f.last;
            f.last = pos;
            f.travel = f.travel.max(f.start.distance(pos));
            self.pad = Some(f);
            self.travelled(f.travel);
            if self.second.is_some() {
                // With two fingers the pointer stays where it is: the pinch or the pan does not move it.
                return Vec2::ZERO;
            }
            self.drag(step, f.travel, injected);
            return moved;
        }
        if let Some(mut f) = self.second
            && f.id == id
        {
            f.last = pos;
            f.travel = f.travel.max(f.start.distance(pos));
            self.second = Some(f);
            self.travelled(f.travel);
        }
        Vec2::ZERO
    }

    /// A single finger that has moved on the canvas. A double tap's touch that moves past the slop starts the drag lock;
    /// with the lock on, the button is held from the first real movement, where the pointer is before it moves.
    fn drag(&mut self, step: Step, travel: f32, injected: &mut Vec<Event>) {
        if self.gesture.is_some_and(|g| g.armed) && travel > TAP_SLOP {
            self.locked = true;
        }
        if self.locked && travel > RESUME_PRESS {
            self.press_down(step, injected);
        }
    }

    fn end(&mut self, step: Step, id: TouchId, pos: Pos2, ended: bool, injected: &mut Vec<Event>) {
        if let Some(f) = self.pad
            && f.id == id
        {
            self.travelled(f.start.distance(pos));
            self.pad = None;
            // A lift lets the button go, so the stroke ends here. The lock stays on. The stroke ends where the finger
            // put the pointer, not short of it where the easing has got to.
            if self.down {
                self.snap(injected);
            }
            self.release(step, injected);
        } else if let Some(f) = self.second
            && f.id == id
        {
            self.travelled(f.start.distance(pos));
            self.second = None;
        } else {
            return;
        }
        if self.pad.is_none() && self.second.is_none() {
            self.finish(step, ended, injected);
        }
    }

    /// Records how far a finger of the gesture has travelled.
    fn travelled(&mut self, travel: f32) {
        if let Some(g) = self.gesture.as_mut() {
            g.travel = g.travel.max(travel);
        }
    }

    /// The gesture is over, as its last finger lifts. A tap clicks where the pointer is, unless the drag lock is on, when
    /// it may end the lock (the second tap of a double tap). Two fingers that tapped together make a right click.
    fn finish(&mut self, step: Step, ended: bool, injected: &mut Vec<Event>) {
        let Some(g) = self.gesture.take() else { return };
        let tap = ended && g.travel <= TAP_SLOP && step.now - g.started <= TAP_SECS;
        self.last_tap = None;
        if !tap {
            return;
        }
        match g.fingers {
            1 if g.armed => {
                // The second tap of a double tap. Unless the lock is on, its press and this lift were its click
                // (see `start`). With the lock on, it ends the lock and clicks nothing.
                if self.locked {
                    self.locked = false;
                }
            }
            1 => {
                if self.locked {
                    // A tap while the lock is on clicks nothing; it may be the first tap of the double tap that ends it.
                    self.last_tap = Some(step.now);
                } else {
                    self.return_to(g.anchor);
                    if let Some(p) = self.pos {
                        click(p, PointerButton::Primary, step.mods, injected);
                        self.last_tap = Some(step.now);
                    }
                }
            }
            2 => {
                self.return_to(g.anchor);
                if let Some(p) = self.pos {
                    click(p, PointerButton::Secondary, step.mods, injected);
                }
            }
            _ => {}
        }
    }

    /// Moves the pointer towards the first finger's target (the finger's movement this frame, scaled by its speed), and
    /// eases the pointer towards the target. The pointer stays on the canvas. A held button goes with it, as a drag's does.
    fn advance(&mut self, step: Step, moved: Vec2, injected: &mut Vec<Event>) {
        let dt = self.last_frame.map_or(1.0 / 60.0, |t| (step.now - t).max(0.0) as f32).clamp(1.0 / 240.0, 0.1);
        self.last_frame = Some(step.now);
        // The finger's speed, smoothed, so that one uneven frame does not make the pointer leap.
        if self.pad.is_some() {
            let k = 1.0 - (-dt / SPEED_SECS).exp();
            self.speed += (moved.length() / dt - self.speed) * k;
        }
        if moved != Vec2::ZERO
            && let Some(t) = self.target
        {
            let precise = if self.precise { PRECISE_GAIN } else { 1.0 };
            let next = t + moved * gain(self.speed) * precise;
            self.target = Some(within(next, step.canvas));
        }
        let (Some(p), Some(t)) = (self.pos, self.target) else { return };
        let ease = 1.0 - (-(dt as f64) / EASE_SECS).exp() as f32;
        let at = if (t - p).length() < SETTLED { t } else { p + (t - p) * ease };
        if at != p {
            self.pos = Some(at);
            injected.push(Event::PointerMoved(at));
        }
    }

    /// The pointer goes at once to its target (no easing), as a stroke's end must be where the finger put it.
    fn snap(&mut self, injected: &mut Vec<Event>) {
        if let (Some(p), Some(t)) = (self.pos, self.target)
            && p != t
        {
            self.pos = Some(t);
            injected.push(Event::PointerMoved(t));
        }
    }

    /// A tap's click goes where the pointer was when the finger touched down (`anchor`), undoing the finger's wobble.
    fn return_to(&mut self, anchor: Option<Pos2>) {
        if let Some(a) = anchor {
            self.pos = Some(a);
            self.target = Some(a);
        }
    }

    /// The button goes down where the pointer is (a stroke starts here), unless it is down already.
    fn press_down(&mut self, step: Step, injected: &mut Vec<Event>) {
        if self.down {
            return;
        }
        if let Some(p) = self.pos {
            self.down = true;
            injected.push(button(p, PointerButton::Primary, true, step.mods));
        }
    }

    /// The button lets go where the pointer is (a stroke ends here), if it is down.
    fn release(&mut self, step: Step, sink: &mut Vec<Event>) {
        if !self.down {
            return;
        }
        self.down = false;
        if let Some(p) = self.pos {
            sink.push(button(p, PointerButton::Primary, false, step.mods));
        }
    }

    /// Draws the pointer (an arrow whose tip is the point, and a dot on the point: accent while the button is down or
    /// the drag lock is on, with a padlock beside it while the lock is on), and the loupe when the magnifier is on and a
    /// finger is working. The dot is painted last, so that the arrow's tip does not cover it.
    pub fn overlay(&self, ctx: &egui::Context) {
        if self.magnifier
            && !self.covered
            && let Some(p) = self.loupe_point(ctx)
            && let Some(view) = ctx.data(|d| d.get_temp::<CanvasView>(view_id()))
            && view.img.contains(p)
        {
            draw_loupe(ctx, p, view);
        }
        let Some(p) = self.shown_at() else { return };
        let t = Tokens::get(ctx);
        // The arrow's tip is at about 3.6 of its 24 units from the top-left corner.
        icons::cursor(ctx, "mouse-pointer-2", p, vec2(3.6, 3.6) / 24.0, 28.0);
        let dot = if self.locked || self.down { t.accent } else { Color32::WHITE };
        let painter = ctx.layer_painter(LayerId::new(Order::Debug, Id::unique("virtual-mouse-dot")));
        if self.locked {
            let badge = Rect::from_center_size(p + vec2(-14.0, 16.0), vec2(18.0, 18.0));
            painter.circle_filled(badge.center(), 10.0, Color32::from_black_alpha(170));
            // A padlock: a body, and a shackle (a half circle on two legs) over it.
            let c = badge.center();
            painter.rect_filled(Rect::from_center_size(c + vec2(0.0, 2.5), vec2(10.0, 7.0)), 1.5, t.accent);
            let shackle: Vec<Pos2> = (0..=8)
                .map(|k| {
                    let a = std::f32::consts::PI * (1.0 + k as f32 / 8.0);
                    c + vec2(3.0 * a.cos(), -1.0 + 3.0 * a.sin())
                })
                .collect();
            let mut path = vec![c + vec2(-3.0, 0.0)];
            path.extend(shackle);
            path.push(c + vec2(3.0, 0.0));
            painter.add(egui::Shape::line(path, Stroke::new(1.6, t.accent)));
        }
        painter.circle_filled(p, 3.0, dot);
        painter.circle_stroke(p, 3.0, Stroke::new(1.0, Color32::from_black_alpha(200)));
    }

    /// What the loupe shows: the pointer's place while a finger moves it or its button is down, or, with the pointer
    /// off, the finger on the canvas. Nothing while no finger works, so the loupe hides the picture only when it helps.
    fn loupe_point(&self, ctx: &egui::Context) -> Option<Pos2> {
        if self.on {
            return (self.pad.is_some() || self.down).then_some(self.pos).flatten();
        }
        ctx.input(|i| (i.any_touches() && i.pointer.primary_down()).then(|| i.pointer.latest_pos()).flatten())
    }
}

/// Records the canvas picture as it is drawn this frame, for the loupe. The canvas calls it where it draws the picture.
pub fn record_view(ctx: &egui::Context, tex: TextureId, img: Rect, flip: bool) {
    ctx.data_mut(|d| d.insert_temp(view_id(), CanvasView { tex, img, flip }));
}

/// Forgets the picture recorded before. A frame that draws the canvas some other way (a rotated view) has none for the
/// loupe, so the loupe must not sample the last one.
pub fn forget_view(ctx: &egui::Context) {
    ctx.data_mut(|d| d.remove::<CanvasView>(view_id()));
}

fn view_id() -> Id {
    Id::unique("virtual-mouse-canvas-view")
}

/// Where the loupe goes on `screen`: the top-left corner under the top bar (`top`), or the top-right one when the
/// point it shows is under or near the left one.
fn loupe_rect(screen: Rect, top: f32, p: Pos2) -> Rect {
    let y = (top + LOUPE_MARGIN).min((screen.bottom() - LOUPE).max(screen.top()));
    let left = Rect::from_min_size(pos2(screen.left() + LOUPE_MARGIN, y), vec2(LOUPE, LOUPE));
    if !left.expand(LOUPE_CLEAR).contains(p) {
        return left;
    }
    Rect::from_min_size(pos2((screen.right() - LOUPE_MARGIN - LOUPE).max(screen.left()), y), vec2(LOUPE, LOUPE))
}

/// The loupe: the canvas around the point, enlarged by [`MAGNIFY`], in a corner of the screen ([`loupe_rect`]), with a
/// ring on the point's place. Where the sample reaches past the picture, the loupe stays white there.
fn draw_loupe(ctx: &egui::Context, p: Pos2, view: CanvasView) {
    let screen = ctx.viewport_rect();
    let top = crate::mobile::top_bar_rect(ctx).map_or(screen.top(), |r| r.bottom());
    let loupe = loupe_rect(screen, top, p);
    let painter = ctx.layer_painter(LayerId::new(Order::Debug, Id::unique("virtual-mouse-loupe")));
    painter.rect_filled(loupe, 8.0, Color32::WHITE);
    let side = LOUPE / MAGNIFY;
    let sample = Rect::from_center_size(p, vec2(side, side));
    let shown = sample.intersect(view.img);
    if shown.width() > 0.0 && shown.height() > 0.0 {
        let to_loupe = |x: Pos2| loupe.min + (x - sample.min) * MAGNIFY;
        let dst = Rect::from_min_max(to_loupe(shown.min), to_loupe(shown.max));
        let picture = |x: Pos2| vec2((x.x - view.img.left()) / view.img.width(), (x.y - view.img.top()) / view.img.height());
        let (a, b) = (picture(shown.min), picture(shown.max));
        // A mirrored picture is sampled mirrored, as the canvas draws it.
        let uv = if view.flip {
            Rect::from_min_max(pos2(1.0 - a.x, a.y), pos2(1.0 - b.x, b.y))
        } else {
            Rect::from_min_max(pos2(a.x, a.y), pos2(b.x, b.y))
        };
        painter.image(view.tex, dst, uv, Color32::WHITE);
    }
    painter.rect_stroke(loupe, 8.0, Stroke::new(2.0, Color32::from_gray(60)), egui::StrokeKind::Outside);
    painter.circle_stroke(loupe.center(), 5.0, Stroke::new(2.0, Color32::from_black_alpha(200)));
}

/// The pointer's gain at a finger speed (points a second): below 1 for a very slow finger, so the pointer can be put
/// on a pixel, and a mild acceleration for a fast one, so a fast slide travels further. Both ends ease in and out
/// (smoothstep), so the gain has no corner to feel.
fn gain(speed: f32) -> f32 {
    let smooth = |x: f32| {
        let x = x.clamp(0.0, 1.0);
        x * x * (3.0 - 2.0 * x)
    };
    let fine = smooth(speed / PRECISE_BELOW.max(1.0));
    let fast = smooth((speed - ACCEL_FROM) / (ACCEL_TO - ACCEL_FROM));
    GAIN_MIN + (1.0 - GAIN_MIN) * fine + (GAIN_MAX - 1.0) * fast
}

/// The pan for a pointer at `p` near the edge of `canvas`: towards the edge it is near, faster the nearer it is.
fn edge_velocity(p: Pos2, canvas: Rect) -> Vec2 {
    let near = |d: f32| ((EDGE - d) / EDGE).clamp(0.0, 1.0);
    let x = near(p.x - canvas.left()) * -1.0 + near(canvas.right() - p.x);
    let y = near(p.y - canvas.top()) * -1.0 + near(canvas.bottom() - p.y);
    vec2(x, y) * EDGE_SPEED
}

/// `p`, kept inside `r`. (Not `clamp`, which panics on an empty rectangle.)
fn within(p: Pos2, r: Rect) -> Pos2 {
    pos2(p.x.max(r.left()).min(r.right()), p.y.max(r.top()).min(r.bottom()))
}

fn button(pos: Pos2, which: PointerButton, pressed: bool, modifiers: Modifiers) -> Event {
    Event::PointerButton { pos, button: which, pressed, modifiers }
}

/// A press and its release at `pos`: a click, which the canvas takes as a click (and not as the start of a drag).
fn click(pos: Pos2, which: PointerButton, mods: Modifiers, injected: &mut Vec<Event>) {
    injected.push(button(pos, which, true, mods));
    injected.push(button(pos, which, false, mods));
}

#[cfg(test)]
mod tests;
