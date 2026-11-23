//! Tests of the virtual mouse: the pointer's movement and easing, taps, double taps, the drag lock and its ends, the
//! right click, the pinch that leaves the pointer alone, the pauses that end a stroke, and the magnifier's state.

use super::*;
use egui::{Event, Modifiers, PointerButton, Pos2, Rect, TouchDeviceId, TouchId, TouchPhase, pos2};

/// The canvas: 400 by 800 points at the origin.
fn canvas() -> Rect {
    Rect::from_min_max(pos2(0.0, 0.0), pos2(400.0, 800.0))
}

fn on_canvas(p: Pos2) -> bool {
    canvas().contains(p)
}

fn touch(id: u64, phase: TouchPhase, pos: Pos2) -> Event {
    Event::Touch { device_id: TouchDeviceId(0), id: TouchId(id), phase, pos, force: None }
}

/// The primary button's press on the screen (not on the canvas) at `pos`.
fn screen_press(pos: Pos2) -> Event {
    Event::PointerButton { pos, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE }
}

/// One frame of input at `now` seconds, with no modifier key held.
fn frame(vm: &mut VirtualMouse, events: Vec<Event>, now: f64) -> Processed {
    vm.process(events, canvas(), now, Modifiers::NONE, on_canvas)
}

/// The button presses and releases in `events`, in order (`true` is a press).
fn clicks(events: &[Event]) -> Vec<(PointerButton, bool)> {
    events
        .iter()
        .filter_map(|e| match e {
            Event::PointerButton { button, pressed, .. } => Some((*button, *pressed)),
            _ => None,
        })
        .collect()
}

/// Where the pointer was last moved to in `events`.
fn pointer_at(events: &[Event]) -> Option<Pos2> {
    events.iter().rev().find_map(|e| match e {
        Event::PointerMoved(p) => Some(*p),
        _ => None,
    })
}

fn moves(events: &[Event]) -> usize {
    events.iter().filter(|e| matches!(e, Event::PointerMoved(_))).count()
}

/// A mouse on, with the pointer in the middle of the canvas (200, 400).
fn on() -> VirtualMouse {
    let mut vm = VirtualMouse::default();
    vm.toggle(canvas());
    vm
}

/// A tap at `pos`: a touch from `t` seconds that lifts 0.1 s later. The events of both frames.
fn tap(vm: &mut VirtualMouse, id: u64, pos: Pos2, t: f64) -> Vec<Event> {
    let mut events = frame(vm, vec![touch(id, TouchPhase::Start, pos)], t).events;
    events.extend(frame(vm, vec![touch(id, TouchPhase::End, pos)], t + 0.1).events);
    events
}

/// A drag that locks the button: a tap, then a second touch at 1.2 s that moves 15 points, then 25 more, and lifts.
/// Returns the mouse's events, which end with the lift; the lock is on afterwards.
fn locked_drag(vm: &mut VirtualMouse) -> Vec<Event> {
    let mut events = tap(vm, 1, pos2(150.0, 500.0), 1.0);
    events.extend(frame(vm, vec![touch(2, TouchPhase::Start, pos2(150.0, 500.0))], 1.2).events);
    events.extend(frame(vm, vec![touch(2, TouchPhase::Move, pos2(150.0, 515.0))], 1.22).events);
    events.extend(frame(vm, vec![touch(2, TouchPhase::Move, pos2(150.0, 540.0))], 1.24).events);
    events.extend(frame(vm, vec![touch(2, TouchPhase::End, pos2(150.0, 540.0))], 1.26).events);
    events
}

/// Frames without input until the pointer has settled on its target.
fn settle(vm: &mut VirtualMouse, now: &mut f64) {
    for _ in 0..90 {
        *now += 1.0 / 60.0;
        frame(vm, vec![], *now);
    }
}

#[test]
fn a_first_slide_moves_the_pointer_and_presses_nothing() {
    let mut vm = on();
    let start = vm.target.expect("the pointer is on");
    let mut now = 1.0;
    let mut events = frame(&mut vm, vec![touch(1, TouchPhase::Start, pos2(100.0, 600.0))], now).events;
    for i in 1..=20 {
        now += 1.0 / 60.0;
        events.extend(frame(&mut vm, vec![touch(1, TouchPhase::Move, pos2(100.0 + 3.0 * i as f32, 600.0))], now).events);
    }
    now += 1.0 / 60.0;
    events.extend(frame(&mut vm, vec![touch(1, TouchPhase::End, pos2(160.0, 600.0))], now).events);
    assert!(clicks(&events).is_empty(), "a slide presses nothing: {:?}", clicks(&events));
    assert!(vm.target.is_some_and(|t| t.x > start.x + 30.0), "the finger moved the pointer: {:?}", vm.target);
}

#[test]
fn a_slow_slide_presses_nothing_and_clicks_nothing_on_lift() {
    // The bug this replaces: a finger that hardly moved for a moment pressed, and then drew.
    let mut vm = on();
    let mut now = 1.0;
    let mut events = frame(&mut vm, vec![touch(1, TouchPhase::Start, pos2(100.0, 600.0))], now).events;
    for i in 1..=30 {
        now += 0.2;
        events.extend(frame(&mut vm, vec![touch(1, TouchPhase::Move, pos2(100.0 + i as f32, 600.0))], now).events);
    }
    now += 0.2;
    events.extend(frame(&mut vm, vec![touch(1, TouchPhase::End, pos2(130.0, 600.0))], now).events);
    assert!(clicks(&events).is_empty(), "a slow slide is neither a click nor a drag: {:?}", clicks(&events));
    assert!(!vm.held() && !vm.locked());
}

#[test]
fn a_tap_clicks_where_the_pointer_is() {
    let mut vm = on();
    let at = vm.pos.expect("the pointer is on");
    let mut events = Vec::new();
    events.extend(frame(&mut vm, vec![touch(1, TouchPhase::Start, pos2(150.0, 500.0))], 1.0).events);
    let lift = frame(&mut vm, vec![touch(1, TouchPhase::End, pos2(150.5, 500.0))], 1.1);
    assert_eq!(clicks(&lift.events), vec![(PointerButton::Primary, true), (PointerButton::Primary, false)]);
    assert!(lift.clicked, "a click is reported, so a held modifier is spent");
    let press_at = lift.events.iter().find_map(|e| match e {
        Event::PointerButton { pos, .. } => Some(*pos),
        _ => None,
    });
    assert_eq!(press_at, Some(at), "the click is where the pointer is");
    events.extend(lift.events);
    assert_eq!(clicks(&events).len(), 2);
}

#[test]
fn two_quick_taps_double_click_and_lock_without_moving() {
    let mut vm = on();
    let mut events = tap(&mut vm, 1, pos2(150.0, 500.0), 1.0);
    events.extend(frame(&mut vm, vec![touch(2, TouchPhase::Start, pos2(150.0, 500.0))], 1.2).events);
    events.extend(frame(&mut vm, vec![touch(2, TouchPhase::End, pos2(150.0, 500.0))], 1.3).events);
    let presses = clicks(&events).iter().filter(|(b, pressed)| *b == PointerButton::Primary && *pressed).count();
    assert_eq!(presses, 2, "two clicks in a row: {:?}", clicks(&events));
    assert!(vm.locked(), "a double click locks without having to move the finger");
    assert!(!vm.held(), "the click ends on lift; the next movement starts a held stroke");
}

#[test]
fn a_stationary_double_tap_enables_dragging_and_a_second_one_unlocks() {
    let mut vm = on();
    let point = pos2(150.0, 500.0);
    let first = tap(&mut vm, 1, point, 1.0);
    assert!(!vm.locked(), "one tap must not lock");
    assert_eq!(clicks(&first), vec![(PointerButton::Primary, true), (PointerButton::Primary, false)]);
    let second = tap(&mut vm, 2, point, 1.2);
    assert!(vm.locked());
    assert_eq!(clicks(&second), vec![(PointerButton::Primary, true), (PointerButton::Primary, false)]);

    let mut stroke = frame(&mut vm, vec![touch(3, TouchPhase::Start, point)], 2.0).events;
    assert!(!vm.held(), "touching alone does not draw");
    stroke.extend(frame(&mut vm, vec![touch(3, TouchPhase::Move, point + vec2(12.0, 0.0))], 2.02).events);
    assert!(vm.held(), "moving after the double tap starts a held stroke");
    stroke.extend(frame(&mut vm, vec![touch(3, TouchPhase::End, point + vec2(12.0, 0.0))], 2.1).events);
    assert_eq!(clicks(&stroke), vec![(PointerButton::Primary, true), (PointerButton::Primary, false)]);
    assert!(vm.locked() && !vm.held(), "lifting ends the stroke but keeps the lock");

    let mut unlock = tap(&mut vm, 4, point, 3.0);
    assert!(vm.locked(), "the first tap does not unlock");
    unlock.extend(tap(&mut vm, 5, point, 3.2));
    assert!(!vm.locked() && !vm.held());
    assert!(clicks(&unlock).is_empty(), "the unlocking taps do not draw or click");

    let mut after = frame(&mut vm, vec![touch(6, TouchPhase::Start, point)], 4.0).events;
    after.extend(frame(&mut vm, vec![touch(6, TouchPhase::Move, point + vec2(12.0, 0.0))], 4.02).events);
    after.extend(frame(&mut vm, vec![touch(6, TouchPhase::End, point + vec2(12.0, 0.0))], 4.1).events);
    assert!(clicks(&after).is_empty(), "sliding after unlock must not draw");
}

#[test]
fn a_cancelled_second_tap_does_not_enable_lock_or_leave_a_button_down() {
    let mut vm = on();
    let point = pos2(150.0, 500.0);
    tap(&mut vm, 1, point, 1.0);
    let mut events = frame(&mut vm, vec![touch(2, TouchPhase::Start, point)], 1.2).events;
    assert!(vm.held());
    events.extend(frame(&mut vm, vec![touch(2, TouchPhase::Cancel, point)], 1.25).events);
    assert!(!vm.locked() && !vm.held());
    assert_eq!(clicks(&events), vec![(PointerButton::Primary, true), (PointerButton::Primary, false)]);
}

#[test]
fn a_double_tap_and_a_drag_lock_the_button_until_the_next_double_tap() {
    let mut vm = on();
    let events = locked_drag(&mut vm);
    assert!(vm.locked(), "the drag past the slop locks it");
    assert!(!vm.held(), "lifting lets the button go, and the lock stays");
    let seen: Vec<bool> = clicks(&events).iter().map(|(_, pressed)| *pressed).collect();
    assert_eq!(seen, vec![true, false, true, false], "a click, then the drag's press and release: {:?}", clicks(&events));
}

#[test]
fn the_lock_goes_on_dragging_after_a_lift_when_the_finger_moves_again() {
    let mut vm = on();
    locked_drag(&mut vm);
    let mut events = frame(&mut vm, vec![touch(3, TouchPhase::Start, pos2(120.0, 500.0))], 2.0).events;
    assert!(!vm.held(), "a touch on its own presses nothing while the lock is on");
    events.extend(frame(&mut vm, vec![touch(3, TouchPhase::Move, pos2(120.0, 520.0))], 2.02).events);
    assert!(vm.held(), "the next movement holds the button again, where the pointer is");
    events.extend(frame(&mut vm, vec![touch(3, TouchPhase::End, pos2(120.0, 520.0))], 2.1).events);
    assert!(!vm.held() && vm.locked());
    let seen: Vec<bool> = clicks(&events).iter().map(|(_, pressed)| *pressed).collect();
    assert_eq!(seen, vec![true, false], "{:?}", clicks(&events));
}

#[test]
fn a_double_tap_while_the_lock_is_on_ends_it_and_clicks_nothing() {
    let mut vm = on();
    locked_drag(&mut vm);
    assert!(vm.locked());
    let mut events = tap(&mut vm, 3, pos2(120.0, 500.0), 2.0);
    assert!(vm.locked(), "a single tap leaves the lock on");
    events.extend(tap(&mut vm, 4, pos2(120.0, 500.0), 2.2));
    assert!(!vm.locked(), "the second tap of a double tap ends it");
    assert!(clicks(&events).is_empty(), "taps with the lock on click nothing: {:?}", clicks(&events));
}

#[test]
fn two_fingers_pause_the_drag_and_keep_the_lock_and_the_pointer() {
    let mut vm = on();
    // The drag's finger stays down while the second finger lands, as a pinch needs it.
    let mut events = tap(&mut vm, 1, pos2(150.0, 500.0), 1.0);
    events.extend(frame(&mut vm, vec![touch(2, TouchPhase::Start, pos2(150.0, 500.0))], 1.2).events);
    events.extend(frame(&mut vm, vec![touch(2, TouchPhase::Move, pos2(150.0, 515.0))], 1.22).events);
    assert!(vm.locked() && vm.held(), "the drag holds the button");
    events.extend(frame(&mut vm, vec![touch(5, TouchPhase::Start, pos2(150.0, 500.0))], 1.25).events);
    assert!(!vm.held(), "a second finger lets the button go, so the stroke ends");
    assert!(vm.locked(), "the lock stays");
    let before = vm.target;
    for i in 1..=10 {
        let x = 3.0 * i as f32;
        let pair = vec![touch(2, TouchPhase::Move, pos2(150.0 - x, 515.0)), touch(5, TouchPhase::Move, pos2(150.0 + x, 500.0))];
        events.extend(frame(&mut vm, pair, 1.25 + 0.02 * i as f64).events);
    }
    assert_eq!(vm.target, before, "the pinch or the pan does not move the pointer");
    assert!(!vm.held(), "and it presses nothing");
    events.extend(frame(&mut vm, vec![touch(2, TouchPhase::End, pos2(120.0, 515.0)), touch(5, TouchPhase::End, pos2(180.0, 500.0))], 1.6).events);
    assert!(vm.locked() && !vm.held());
    let seen: Vec<bool> = clicks(&events).iter().map(|(_, pressed)| *pressed).collect();
    assert_eq!(seen, vec![true, false, true, false], "the pinch adds no click: {:?}", clicks(&events));
}

#[test]
fn a_press_on_the_screen_lets_go_of_a_locked_drag_first_and_keeps_the_lock() {
    let mut vm = on();
    locked_drag(&mut vm);
    let mut events = frame(&mut vm, vec![touch(7, TouchPhase::Start, pos2(120.0, 500.0))], 2.0).events;
    events.extend(frame(&mut vm, vec![touch(7, TouchPhase::Move, pos2(120.0, 530.0))], 2.02).events);
    assert!(vm.held());
    let r = frame(&mut vm, vec![screen_press(pos2(200.0, 900.0))], 2.1);
    let seen = clicks(&r.events);
    assert_eq!(seen, vec![(PointerButton::Primary, false), (PointerButton::Primary, true)], "the release comes before the press: {seen:?}");
    assert!(!vm.held() && vm.locked(), "the lock stays for the next stroke");
}

#[test]
fn while_a_press_on_the_screen_lasts_the_pointer_sends_no_moves() {
    let mut vm = on();
    let mut now = 1.0;
    frame(&mut vm, vec![touch(1, TouchPhase::Start, pos2(100.0, 600.0))], now);
    now += 1.0 / 60.0;
    frame(&mut vm, vec![touch(1, TouchPhase::Move, pos2(160.0, 600.0))], now);
    now += 1.0 / 60.0;
    frame(&mut vm, vec![touch(1, TouchPhase::End, pos2(160.0, 600.0))], now);
    frame(&mut vm, vec![screen_press(pos2(200.0, 900.0))], now + 0.01);
    assert!(vm.wants_frames(), "the pointer still has somewhere to ease to");
    let mut later = Vec::new();
    for _ in 0..10 {
        now += 1.0 / 60.0;
        later.extend(frame(&mut vm, vec![], now).events);
    }
    assert_eq!(moves(&later), 0, "the pointer sends no moves over a press on the screen");
}

#[test]
fn turning_the_mouse_off_lets_go_of_a_locked_drag_and_ends_the_lock() {
    let mut vm = on();
    locked_drag(&mut vm);
    let mut events = frame(&mut vm, vec![touch(7, TouchPhase::Start, pos2(120.0, 500.0))], 2.0).events;
    events.extend(frame(&mut vm, vec![touch(7, TouchPhase::Move, pos2(120.0, 530.0))], 2.02).events);
    assert!(vm.held());
    vm.toggle(canvas());
    let r = frame(&mut vm, vec![], 2.1);
    assert!(clicks(&r.events).contains(&(PointerButton::Primary, false)), "the button is let go: {:?}", clicks(&r.events));
    assert!(!vm.is_on() && !vm.held() && !vm.locked());
}

#[test]
fn two_fingers_that_tap_together_are_a_right_click() {
    let mut vm = on();
    let mut events = frame(&mut vm, vec![touch(1, TouchPhase::Start, pos2(150.0, 500.0)), touch(2, TouchPhase::Start, pos2(250.0, 500.0))], 1.0).events;
    events.extend(frame(&mut vm, vec![touch(1, TouchPhase::End, pos2(150.0, 500.0)), touch(2, TouchPhase::End, pos2(250.0, 500.0))], 1.1).events);
    assert_eq!(clicks(&events), vec![(PointerButton::Secondary, true), (PointerButton::Secondary, false)]);
}

#[test]
fn two_fingers_that_move_pinch_without_clicking_or_moving_the_pointer() {
    let mut vm = on();
    let before = vm.target;
    let mut events = frame(&mut vm, vec![touch(1, TouchPhase::Start, pos2(150.0, 500.0)), touch(2, TouchPhase::Start, pos2(250.0, 500.0))], 1.0).events;
    for i in 1..=10 {
        let x = 3.0 * i as f32;
        let pair = vec![touch(1, TouchPhase::Move, pos2(150.0 - x, 500.0)), touch(2, TouchPhase::Move, pos2(250.0 + x, 500.0))];
        events.extend(frame(&mut vm, pair, 1.0 + 0.02 * i as f64).events);
    }
    events.extend(frame(&mut vm, vec![touch(1, TouchPhase::End, pos2(120.0, 500.0)), touch(2, TouchPhase::End, pos2(280.0, 500.0))], 1.3).events);
    assert!(clicks(&events).is_empty(), "a pinch is no click: {:?}", clicks(&events));
    assert_eq!(vm.target, before, "a pinch leaves the pointer where it was");
}

#[test]
fn a_touch_off_the_canvas_goes_to_the_screen_as_it_is() {
    let mut vm = on();
    let event = touch(1, TouchPhase::Start, pos2(100.0, 900.0));
    let r = frame(&mut vm, vec![event.clone()], 1.0);
    assert!(r.events.contains(&event), "the touch passes through");
    assert!(clicks(&r.events).is_empty(), "and presses nothing");
}

#[test]
fn the_pointer_stays_on_the_canvas() {
    let mut vm = on();
    vm.pos = Some(pos2(395.0, 400.0));
    vm.target = Some(pos2(395.0, 400.0));
    let mut now = 1.0;
    frame(&mut vm, vec![touch(1, TouchPhase::Start, pos2(100.0, 400.0))], now);
    for i in 1..=30 {
        now += 1.0 / 60.0;
        frame(&mut vm, vec![touch(1, TouchPhase::Move, pos2(100.0 + 10.0 * i as f32, 400.0))], now);
    }
    settle(&mut vm, &mut now);
    let p = vm.pos.expect("the pointer is on");
    assert!((p.x - 400.0).abs() < 1e-3, "the pointer stops at the canvas's edge: {p:?}");
}

#[test]
fn the_pointer_eases_to_where_the_finger_puts_it_and_settles() {
    let mut vm = on();
    let mut now = 1.0;
    frame(&mut vm, vec![touch(1, TouchPhase::Start, pos2(100.0, 600.0))], now);
    now += 1.0 / 60.0;
    frame(&mut vm, vec![touch(1, TouchPhase::Move, pos2(160.0, 600.0))], now);
    assert!(vm.wants_frames(), "the pointer is still easing");
    assert_ne!(vm.pos, vm.target, "it has not arrived in one frame");
    settle(&mut vm, &mut now);
    assert!(!vm.wants_frames());
    assert_eq!(vm.pos, vm.target, "it settles on its target");
}

#[test]
fn placing_a_finger_again_does_not_jump_the_pointer() {
    let mut vm = on();
    let mut now = 1.0;
    frame(&mut vm, vec![touch(1, TouchPhase::Start, pos2(100.0, 600.0))], now);
    for i in 1..=10 {
        now += 1.0 / 60.0;
        frame(&mut vm, vec![touch(1, TouchPhase::Move, pos2(100.0 + 4.0 * i as f32, 600.0))], now);
    }
    now += 0.1;
    frame(&mut vm, vec![touch(1, TouchPhase::End, pos2(140.0, 600.0))], now);
    settle(&mut vm, &mut now);
    let before = vm.pos;
    now += 0.5;
    let r = frame(&mut vm, vec![touch(2, TouchPhase::Start, pos2(340.0, 150.0))], now);
    assert_eq!(pointer_at(&r.events), before, "placing a finger somewhere else does not move the pointer");
    assert!(clicks(&r.events).is_empty());
}

#[test]
fn a_slide_that_comes_after_the_double_tap_window_does_not_lock() {
    let mut vm = on();
    let mut events = tap(&mut vm, 1, pos2(150.0, 500.0), 1.0);
    events.extend(frame(&mut vm, vec![touch(2, TouchPhase::Start, pos2(150.0, 500.0))], 1.7).events);
    events.extend(frame(&mut vm, vec![touch(2, TouchPhase::Move, pos2(150.0, 540.0))], 1.8).events);
    events.extend(frame(&mut vm, vec![touch(2, TouchPhase::End, pos2(150.0, 540.0))], 1.9).events);
    assert!(!vm.locked(), "too late to be a double tap");
    assert_eq!(clicks(&events), vec![(PointerButton::Primary, true), (PointerButton::Primary, false)], "only the first tap clicked");
}

#[test]
fn a_cancelled_touch_clicks_nothing() {
    let mut vm = on();
    frame(&mut vm, vec![touch(1, TouchPhase::Start, pos2(150.0, 500.0))], 1.0);
    let r = frame(&mut vm, vec![touch(1, TouchPhase::Cancel, pos2(150.0, 500.0))], 1.05);
    assert!(clicks(&r.events).is_empty(), "a cancelled touch is no click");
}

#[test]
fn the_magnifier_works_without_the_pointer_and_outlasts_it() {
    let mut vm = VirtualMouse::default();
    vm.toggle_magnifier(canvas());
    assert!(!vm.is_on() && vm.magnifier_on(), "the loupe follows the finger with the pointer off");
    vm.toggle(canvas());
    vm.toggle(canvas());
    assert!(!vm.is_on() && vm.magnifier_on(), "turning the pointer off leaves the loupe on");
    vm.toggle_magnifier(canvas());
    assert!(!vm.magnifier_on());
}

#[test]
fn loupe_marker_is_white_on_dark_and_black_on_light() {
    assert_eq!(loupe_marker_color([0.0, 0.0, 0.0, 1.0]), Color32::WHITE);
    assert_eq!(loupe_marker_color([1.0, 1.0, 1.0, 1.0]), Color32::BLACK);
    assert_eq!(loupe_marker_color([0.1, 0.2, 0.1, 1.0]), Color32::WHITE);
    assert_eq!(loupe_marker_color([0.9, 0.8, 0.9, 1.0]), Color32::BLACK);
    assert_eq!(loupe_marker_color([0.0, 0.0, 0.0, 0.0]), Color32::BLACK);
    assert_eq!(loupe_marker_color([f32::NAN, 0.0, 0.0, 1.0]), Color32::BLACK);
}

#[test]
fn the_loupe_stays_on_the_virtual_pointer_after_lift_until_switched_off() {
    let ctx = egui::Context::default();
    let mut vm = on();
    vm.toggle_magnifier(canvas());
    let p = vm.pos.expect("the pointer has a position");
    frame(&mut vm, vec![touch(1, TouchPhase::Start, pos2(150.0, 500.0))], 1.0);
    frame(&mut vm, vec![touch(1, TouchPhase::End, pos2(150.0, 500.0))], 1.1);
    assert_eq!(vm.active_loupe_point(&ctx, canvas()), Some(p));
    vm.toggle_magnifier(canvas());
    assert_eq!(vm.active_loupe_point(&ctx, canvas()), None);
}

#[test]
fn the_loupe_keeps_the_last_canvas_touch_after_lift_with_the_pointer_off() {
    let ctx = egui::Context::default();
    let mut vm = VirtualMouse::default();
    vm.toggle_magnifier(canvas());
    let p = pos2(150.0, 500.0);
    let input = egui::RawInput { screen_rect: Some(canvas()), events: vec![
        touch(1, TouchPhase::Start, p), Event::PointerMoved(p),
        Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE },
    ], ..Default::default() };
    let mut output = ctx.run_ui(input, |ui| assert_eq!(vm.active_loupe_point(ui.ctx(), canvas()), Some(p)));
    output.textures_delta.clear();
    let input = egui::RawInput { screen_rect: Some(canvas()), events: vec![
        touch(1, TouchPhase::End, p),
        Event::PointerButton { pos: p, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE },
        Event::PointerGone,
    ], ..Default::default() };
    let mut output = ctx.run_ui(input, |ui| assert_eq!(vm.active_loupe_point(ui.ctx(), canvas()), Some(p)));
    output.textures_delta.clear();
    vm.toggle_magnifier(canvas());
    assert_eq!(vm.loupe_last_touch, None, "turning off the loupe clears the saved touch");
}

#[test]
fn edge_pan_is_gentle_and_capped_even_at_a_corner() {
    let r = canvas();
    assert_eq!(edge_velocity(r.center(), r), Vec2::ZERO);
    assert!(edge_velocity(pos2(r.right() - 1.0, r.center().y), r).length() <= EDGE_SPEED);
    assert!(edge_velocity(r.right_bottom(), r).length() <= EDGE_SPEED + 0.001);
    assert!(EDGE_SPEED <= 90.0, "the former 700 pt/s was far too fast");
    let near_start = edge_velocity(pos2(r.right() - EDGE + 1.0, r.center().y), r).length();
    let near_edge = edge_velocity(pos2(r.right() - 1.0, r.center().y), r).length();
    assert!(near_start < near_edge && near_start < 1.0, "acceleration starts gently");
}

#[test]
fn the_loupe_keeps_out_of_the_way_of_the_work() {
    let screen = Rect::from_min_max(pos2(0.0, 0.0), pos2(400.0, 800.0));
    let far = loupe_rect(screen, 48.0, pos2(300.0, 600.0));
    assert_eq!(far.min, pos2(8.0, 56.0), "the top-left corner, under the top bar");
    let under = loupe_rect(screen, 48.0, pos2(60.0, 100.0));
    assert!(under.left() > 200.0 && !under.expand(LOUPE_CLEAR).contains(pos2(60.0, 100.0)), "it moves to the other corner: {under:?}");
}

#[test]
fn the_floating_right_click_clicks_where_the_pointer_is() {
    let mut vm = on();
    let at = vm.pos.expect("the pointer is on");
    vm.right_click();
    let r = frame(&mut vm, vec![], 1.0);
    assert_eq!(clicks(&r.events), vec![(PointerButton::Secondary, true), (PointerButton::Secondary, false)]);
    let press_at = r.events.iter().find_map(|e| match e {
        Event::PointerButton { pos, .. } => Some(*pos),
        _ => None,
    });
    assert_eq!(press_at, Some(at));
}

#[test]
fn the_precise_mode_slows_the_pointer() {
    let slide = |precise: bool| {
        let mut vm = on();
        if precise {
            vm.toggle_precise();
        }
        let start = vm.target.expect("on").x;
        let mut now = 1.0;
        frame(&mut vm, vec![touch(1, TouchPhase::Start, pos2(100.0, 600.0))], now);
        for i in 1..=20 {
            now += 1.0 / 60.0;
            frame(&mut vm, vec![touch(1, TouchPhase::Move, pos2(100.0 + 3.0 * i as f32, 600.0))], now);
        }
        vm.target.expect("on").x - start
    };
    let (normal, precise) = (slide(false), slide(true));
    assert!(precise < normal * 0.5 && precise > 0.0, "{precise} vs {normal}");
}

#[test]
fn a_held_drag_at_the_edge_pans_the_picture_and_nowhere_else() {
    let mut vm = on();
    locked_drag(&mut vm);
    frame(&mut vm, vec![touch(7, TouchPhase::Start, pos2(120.0, 500.0))], 2.0);
    frame(&mut vm, vec![touch(7, TouchPhase::Move, pos2(120.0, 530.0))], 2.02);
    assert!(vm.held());
    assert_eq!(vm.edge_push(canvas()), Vec2::ZERO, "in the middle nothing pans");
    vm.pos = Some(pos2(398.0, 400.0));
    let push = vm.edge_push(canvas());
    assert!(push.x > 0.0 && push.y == 0.0, "at the right edge the picture pans right: {push:?}");
    assert!(vm.wants_frames(), "and keeps the frames coming");
}

/// The top bar sits above the canvas (y < 0 here), so a touch on it is the screen's, not the pointer's.
#[test]
fn a_finger_on_the_top_bar_is_not_the_pointers() {
    let mut vm = on();
    let bar = pos2(200.0, -20.0);
    let out = vm.process(vec![touch(1, TouchPhase::Start, bar), screen_press(bar)], canvas(), 0.0, Modifiers::NONE, on_canvas);
    assert!(!out.clicked, "the top bar's press is not a pointer click");
    assert!(out.events.iter().any(|e| matches!(e, Event::Touch { .. })), "the touch goes to the screen");
    assert_eq!(clicks(&out.events), vec![(PointerButton::Primary, true)], "the screen's press goes through as it is");

    let lift = vm.process(
        vec![
            touch(1, TouchPhase::End, bar),
            Event::PointerButton { pos: bar, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE },
            Event::PointerGone,
        ],
        canvas(),
        0.1,
        Modifiers::NONE,
        on_canvas,
    );
    assert!(lift.events.iter().any(|e| matches!(e, Event::PointerGone)), "lifting a screen touch forwards PointerGone");
    assert_eq!(moves(&lift.events), 0, "no virtual pointer move is injected on the screen release frame");
}

/// The pointer may go up into the top bar, so its buttons can be pressed with it: its reach is the canvas plus the bar.
#[test]
fn the_pointer_can_reach_the_top_bar() {
    let mut vm = on();
    let reach = Rect::from_min_max(pos2(0.0, -48.0), pos2(400.0, 800.0));
    let bar = pos2(200.0, -20.0);
    vm.pos = Some(bar);
    vm.target = Some(bar);
    let out = vm.process(Vec::new(), reach, 0.0, Modifiers::NONE, on_canvas);
    assert_eq!(pointer_at(&out.events), Some(bar), "the pointer stays on the bar");
    let clamped = vm.process(Vec::new(), canvas(), 0.0, Modifiers::NONE, on_canvas);
    assert_eq!(pointer_at(&clamped.events), Some(pos2(200.0, 0.0)), "the canvas alone would keep it on the canvas");
}

#[test]
fn a_finger_that_rests_and_lifts_clicks_nothing() {
    let mut vm = on();
    frame(&mut vm, vec![touch(1, TouchPhase::Start, pos2(150.0, 500.0))], 1.0);
    let r = frame(&mut vm, vec![touch(1, TouchPhase::End, pos2(150.0, 500.0))], 1.0 + TAP_SECS + 0.2);
    assert!(clicks(&r.events).is_empty(), "a resting finger is no tap: {:?}", clicks(&r.events));
}

#[test]
fn a_tap_clicks_where_the_pointer_was_before_the_finger_wobbled() {
    let mut vm = on();
    let at = vm.pos.expect("the pointer is on");
    frame(&mut vm, vec![touch(1, TouchPhase::Start, pos2(150.0, 500.0))], 1.0);
    frame(&mut vm, vec![touch(1, TouchPhase::Move, pos2(155.0, 503.0))], 1.03);
    let lift = frame(&mut vm, vec![touch(1, TouchPhase::End, pos2(155.0, 503.0))], 1.1);
    let press_at = lift.events.iter().find_map(|e| match e {
        Event::PointerButton { pos, pressed: true, .. } => Some(*pos),
        _ => None,
    });
    assert_eq!(press_at, Some(at), "the wobble does not nudge the click");
    assert_eq!(vm.pos, Some(at));
}

#[test]
fn a_stroke_ends_where_the_finger_put_the_pointer() {
    let mut vm = on();
    tap(&mut vm, 1, pos2(150.0, 500.0), 1.0);
    frame(&mut vm, vec![touch(2, TouchPhase::Start, pos2(150.0, 500.0))], 1.2);
    frame(&mut vm, vec![touch(2, TouchPhase::Move, pos2(150.0, 540.0))], 1.21);
    frame(&mut vm, vec![touch(2, TouchPhase::Move, pos2(150.0, 600.0))], 1.22);
    let target = vm.target;
    let lift = frame(&mut vm, vec![touch(2, TouchPhase::End, pos2(150.0, 600.0))], 1.23);
    let release_at = lift.events.iter().find_map(|e| match e {
        Event::PointerButton { pos, pressed: false, .. } => Some(*pos),
        _ => None,
    });
    assert_eq!(release_at, target, "the stroke is not cut short by the easing");
}

#[test]
fn the_gain_rises_smoothly_from_fine_to_fast() {
    assert!((gain(0.0) - GAIN_MIN).abs() < 1e-6);
    assert!((gain(ACCEL_FROM) - 1.0).abs() < 1e-6);
    assert!((gain(ACCEL_TO) - GAIN_MAX).abs() < 1e-6);
    let mut last = gain(0.0);
    for i in 1..=2000 {
        let g = gain(i as f32);
        assert!(g >= last - 1e-6 && g - last < 0.05, "no step in the gain at {i}: {last} -> {g}");
        last = g;
    }
}

#[test]
fn loupe_replays_live_tool_shapes_with_clipping_and_discards_old_frames() {
    let ctx = egui::Context::default();
    let input = egui::RawInput { screen_rect: Some(canvas()), ..Default::default() };
    let point = pos2(200.0, 400.0);
    let mut output = ctx.run_ui(input, |ui| {
        let painter = ui.painter_at(canvas());
        let capture = begin_loupe_capture(&ctx, &painter, Some(point));
        // Two points of a live lasso path and a paint preview, plus one out-of-sample shape.
        painter.line_segment([pos2(190.0, 395.0), pos2(210.0, 405.0)], Stroke::new(1.0, Color32::RED));
        painter.circle_filled(point, 3.0, Color32::GREEN);
        painter.circle_filled(pos2(350.0, 700.0), 3.0, Color32::BLUE);
        finish_loupe_capture(&ctx, &painter, capture);
        let frame = ctx.data(|d| d.get_temp::<std::sync::Arc<LoupeFrame>>(loupe_frame_id())).expect("fresh frame");
        assert_eq!(frame.shapes.len(), 2, "only nearby canvas shapes are replayed");
        draw_loupe_frame(&ctx, &frame);
        begin_loupe_capture(&ctx, &painter, None);
        assert!(ctx.data(|d| d.get_temp::<std::sync::Arc<LoupeFrame>>(loupe_frame_id())).is_none(), "no stale frame after a lift");
    });
    let loupe = loupe_rect(canvas(), canvas().top(), point);
    let expected = [loupe.center() + vec2(-25.0, -12.5), loupe.center() + vec2(25.0, 12.5)];
    let replayed = output.shapes.iter().find_map(|item| match &item.shape {
        egui::Shape::LineSegment { points, stroke } if stroke.color == Color32::RED && stroke.width > 2.0 => Some((points, item.clip_rect)),
        _ => None,
    }).expect("lasso line is redrawn inside the loupe");
    assert_eq!(*replayed.0, expected);
    assert_eq!(replayed.1, loupe, "replayed canvas must not draw outside the loupe");
    output.textures_delta.clear();
}

#[test]
fn gpu_loupe_tracks_the_pointer_through_a_rotated_view() {
    let params = crate::gpu_canvas::ViewParams {
        doc: 1, doc_size: [100, 100], zoom: 2.0, center: [30.0, 40.0], rotation: std::f32::consts::FRAC_PI_2,
        shadow: true, pixel_grid: false, view_key: 7, display: 0, output: 0, hdr: None, offset: [0.0, 0.0],
    };
    let canvas = Rect::from_min_size(Pos2::ZERO, vec2(400.0, 400.0));
    let lens = loupe_gpu_params(params, canvas, canvas.center() + vec2(20.0, 40.0));
    assert!((lens.center[0] - 50.0).abs() < 0.001);
    assert!((lens.center[1] - 30.0).abs() < 0.001);
    assert_eq!(lens.zoom, params.zoom * MAGNIFY);
    assert_ne!(lens.view_key, params.view_key);
    assert!(!lens.shadow);
}

#[test]
fn loupe_overlay_uses_the_current_canvas_pass() {
    let ctx = egui::Context::default();
    let point = pos2(200.0, 400.0);
    let mut vm = VirtualMouse::default();
    vm.on = true;
    vm.magnifier = true;
    vm.down = true;
    vm.pos = Some(point);
    let input = egui::RawInput { screen_rect: Some(canvas()), ..Default::default() };
    let mut output = ctx.run_ui(input, |ui| {
        let painter = ui.painter_at(canvas());
        let capture = begin_loupe_capture(&ctx, &painter, vm.active_loupe_point(&ctx, canvas()));
        painter.line_segment([point - vec2(10.0, 0.0), point + vec2(10.0, 0.0)], Stroke::new(1.0, Color32::RED));
        finish_loupe_capture(&ctx, &painter, capture);
        vm.overlay(&ctx);
    });
    assert!(output.shapes.iter().any(|item| matches!(&item.shape,
        egui::Shape::LineSegment { stroke, .. } if stroke.color == Color32::RED && stroke.width == MAGNIFY)),
        "overlay must display the new canvas shapes in the same pass");
    output.textures_delta.clear();
}

#[test]
fn the_loupe_draws_the_new_canvas_pass_after_the_finger_lifts() {
    let ctx = egui::Context::default();
    let mut vm = on();
    vm.toggle_magnifier(canvas());
    let point = vm.pos.expect("the pointer is on");
    frame(&mut vm, vec![touch(1, TouchPhase::Start, pos2(150.0, 500.0))], 1.0);
    frame(&mut vm, vec![touch(1, TouchPhase::End, pos2(150.0, 500.0))], 1.1);
    assert!(vm.pad.is_none());
    let input = egui::RawInput { screen_rect: Some(canvas()), ..Default::default() };
    let mut output = ctx.run_ui(input, |ui| {
        let painter = ui.painter_at(canvas());
        let capture = begin_loupe_capture(&ctx, &painter, vm.active_loupe_point(&ctx, canvas()));
        painter.line_segment([point - vec2(10.0, 0.0), point + vec2(10.0, 0.0)], Stroke::new(1.0, Color32::RED));
        finish_loupe_capture(&ctx, &painter, capture);
        vm.overlay(&ctx);
    });
    assert!(output.shapes.iter().any(|item| matches!(&item.shape,
        egui::Shape::LineSegment { stroke, .. } if stroke.color == Color32::RED && stroke.width == MAGNIFY)),
        "the last location still renders the fresh, magnified canvas pass");
    output.textures_delta.clear();
}

#[test]
fn loupe_draws_the_sampled_colour_and_opposite_halo() {
    for (pixel, foreground, halo) in [([0.0, 0.0, 0.0, 1.0], Color32::WHITE, Color32::BLACK),
                                      ([1.0, 1.0, 1.0, 1.0], Color32::BLACK, Color32::WHITE)] {
        let ctx = egui::Context::default();
        let point = canvas().center();
        let input = egui::RawInput { screen_rect: Some(canvas()), ..Default::default() };
        let mut output = ctx.run_ui(input, |ui| {
            let painter = ui.painter_at(canvas());
            let capture = begin_loupe_capture(&ctx, &painter, Some(point));
            finish_loupe_capture(&ctx, &painter, capture);
            set_loupe_marker_pixel(&ctx, Some(pixel));
            let frame = ctx.data(|d| d.get_temp::<std::sync::Arc<LoupeFrame>>(loupe_frame_id())).expect("frame");
            draw_loupe_frame(&ctx, &frame);
        });
        let loupe = loupe_rect(canvas(), canvas().top(), point);
        let strokes: Vec<_> = output.shapes.iter().filter_map(|item| match &item.shape {
            egui::Shape::Circle(c) if c.center == loupe.center() && c.radius == 5.0 => Some((c.stroke.width, c.stroke.color)),
            _ => None,
        }).collect();
        assert_eq!(strokes, vec![(4.0, halo), (2.0, foreground)]);
        output.textures_delta.clear();
    }
}
