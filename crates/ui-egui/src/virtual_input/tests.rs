use super::*;
use egui::{Event, Key, Modifiers};

/// The key of a page that shows `label`.
fn key(lang: Lang, page: Page, label: &str) -> KeyDef {
    rows(lang, page).into_iter().flatten().find(|k| k.label == label).unwrap_or_else(|| panic!("no key {label:?}"))
}

/// The key of a page that shows the icon `icon`.
fn icon_key(lang: Lang, page: Page, icon: &str) -> KeyDef {
    rows(lang, page).into_iter().flatten().find(|k| k.icon == Some(icon)).unwrap_or_else(|| panic!("no icon key {icon:?}"))
}

/// The key of a page that holds `m`.
fn modifier_key(lang: Lang, page: Page, m: Modifier) -> KeyDef {
    rows(lang, page).into_iter().flatten().find(|k| k.action == Action::Hold(m)).unwrap_or_else(|| panic!("no {m:?} key"))
}

/// One event as a short text: `down Q shift`, `up Q`, `text ض`.
fn describe(e: &Event) -> String {
    fn mods(m: &Modifiers) -> String {
        [(m.ctrl, " ctrl"), (m.alt, " alt"), (m.shift, " shift")].iter().filter(|(on, _)| *on).map(|(_, name)| *name).collect()
    }
    match e {
        Event::Key { key, pressed, modifiers, .. } => format!("{} {key:?}{}", if *pressed { "down" } else { "up" }, mods(modifiers)),
        Event::Text(text) => format!("text {text}"),
        other => format!("other {other:?}"),
    }
}

/// Presses `key` at `now` and says what it queued.
fn tap(vi: &mut VirtualInput, key: &KeyDef, now: f64) -> Vec<String> {
    vi.press(key, now);
    vi.take_events().iter().map(describe).collect()
}

#[test]
fn every_page_has_five_rows_with_the_modifiers_and_the_keys_a_shortcut_needs() {
    for lang in [Lang::En, Lang::Fa] {
        for page in [Page::Letters, Page::Numbers] {
            let r = rows(lang, page);
            assert_eq!(r.len(), 5, "{lang:?} {page:?}");
            assert!(r.iter().all(|row| row.iter().map(|k| k.units).sum::<f32>() > 0.0), "{lang:?} {page:?}: an empty row");
            let codes: Vec<Key> = r.iter().flatten().filter_map(|k| k.code).collect();
            let wanted = [Key::Escape, Key::Tab, Key::Home, Key::End, Key::ArrowLeft, Key::ArrowUp];
            for want in wanted.into_iter().chain([Key::ArrowDown, Key::ArrowRight, Key::Delete, Key::Enter, Key::Backspace]) {
                assert!(codes.contains(&want), "{lang:?} {page:?} lacks {want:?}");
            }
            for m in [Modifier::Ctrl, Modifier::Alt] {
                assert_eq!(modifier_key(lang, page, m).action, Action::Hold(m));
            }
        }
        assert!(rows(lang, Page::Letters).iter().flatten().any(|k| k.action == Action::Hold(Modifier::Shift)), "{lang:?}: no Shift");
    }
}

#[test]
fn the_keys_of_a_row_share_its_width_less_the_gaps_between_them() {
    for lang in [Lang::En, Lang::Fa] {
        for page in [Page::Letters, Page::Numbers] {
            for row in rows(lang, page) {
                let widths = key_widths(&row, 411.0);
                assert_eq!(widths.len(), row.len());
                let total = widths.iter().sum::<f32>() + KEY_GAP * (row.len() - 1) as f32;
                assert!((total - 411.0).abs() < 0.01, "{lang:?} {page:?}: a row is {total} wide");
            }
        }
    }
}

#[test]
fn the_panel_is_as_tall_as_its_rows_and_their_gaps_and_padding() {
    let n = rows(Lang::En, Page::Letters).len() as f32;
    assert_eq!(PANEL_H, n * KEY_H + (n - 1.0) * KEY_GAP + 2.0 * PAD);
}

#[test]
fn an_english_key_types_its_letter_between_its_key_events() {
    let mut vi = VirtualInput::default();
    assert_eq!(tap(&mut vi, &key(Lang::En, Page::Letters, "q"), 0.0), ["down Q", "text q", "up Q"]);
}

#[test]
fn shift_capitalises_the_next_key_only() {
    let mut vi = VirtualInput::default();
    let shift = modifier_key(Lang::En, Page::Letters, Modifier::Shift);
    assert!(tap(&mut vi, &shift, 0.0).is_empty(), "a modifier sends no event");
    assert_eq!(vi.hold(Modifier::Shift), Hold::Once);
    assert_eq!(tap(&mut vi, &key(Lang::En, Page::Letters, "q"), 1.0), ["down Q shift", "text Q", "up Q shift"]);
    assert_eq!(vi.hold(Modifier::Shift), Hold::Off);
    assert_eq!(tap(&mut vi, &key(Lang::En, Page::Letters, "w"), 1.1), ["down W", "text w", "up W"]);
}

#[test]
fn ctrl_sends_the_key_without_its_text_and_is_spent() {
    let mut vi = VirtualInput::default();
    tap(&mut vi, &modifier_key(Lang::En, Page::Letters, Modifier::Ctrl), 0.0);
    assert_eq!(tap(&mut vi, &key(Lang::En, Page::Letters, "z"), 0.5), ["down Z ctrl", "up Z ctrl"]);
    assert_eq!(tap(&mut vi, &key(Lang::En, Page::Letters, "x"), 0.6), ["down X", "text x", "up X"]);
}

#[test]
fn two_quick_taps_lock_a_modifier_until_it_is_tapped_again() {
    let mut vi = VirtualInput::default();
    let ctrl = modifier_key(Lang::En, Page::Letters, Modifier::Ctrl);
    tap(&mut vi, &ctrl, 0.0);
    tap(&mut vi, &ctrl, 0.2);
    assert_eq!(vi.hold(Modifier::Ctrl), Hold::Locked);
    assert_eq!(tap(&mut vi, &key(Lang::En, Page::Letters, "z"), 0.5), ["down Z ctrl", "up Z ctrl"]);
    assert_eq!(tap(&mut vi, &key(Lang::En, Page::Letters, "x"), 0.6), ["down X ctrl", "up X ctrl"], "a lock outlasts a key");
    tap(&mut vi, &ctrl, 1.0);
    assert_eq!(vi.hold(Modifier::Ctrl), Hold::Off);
    assert_eq!(tap(&mut vi, &key(Lang::En, Page::Letters, "c"), 1.1), ["down C", "text c", "up C"]);
}

#[test]
fn a_slow_second_tap_does_not_lock_a_modifier() {
    let mut vi = VirtualInput::default();
    let alt = modifier_key(Lang::En, Page::Letters, Modifier::Alt);
    tap(&mut vi, &alt, 0.0);
    tap(&mut vi, &alt, 2.0);
    assert_eq!(vi.hold(Modifier::Alt), Hold::Off);
}

#[test]
fn a_persian_key_types_its_letter_and_sends_the_latin_key_it_sits_on() {
    let mut vi = VirtualInput::default();
    assert_eq!(tap(&mut vi, &key(Lang::Fa, Page::Letters, "ض"), 0.0), ["down Q", "text ض", "up Q"]);
}

#[test]
fn shift_on_the_persian_comma_types_the_persian_comma() {
    let mut vi = VirtualInput::default();
    tap(&mut vi, &modifier_key(Lang::Fa, Page::Letters, Modifier::Shift), 0.0);
    assert_eq!(tap(&mut vi, &key(Lang::Fa, Page::Letters, "و"), 0.1), ["down Comma shift", "text ،", "up Comma shift"]);
}

#[test]
fn shift_and_space_in_persian_types_the_zero_width_non_joiner() {
    let mut vi = VirtualInput::default();
    tap(&mut vi, &modifier_key(Lang::Fa, Page::Letters, Modifier::Shift), 0.0);
    assert_eq!(tap(&mut vi, &key(Lang::Fa, Page::Letters, "Space"), 0.1), ["down Space shift", "text \u{200C}", "up Space shift"]);
}

#[test]
fn ctrl_on_a_persian_key_sends_its_latin_key_for_a_shortcut() {
    let mut vi = VirtualInput::default();
    tap(&mut vi, &modifier_key(Lang::Fa, Page::Letters, Modifier::Ctrl), 0.0);
    assert_eq!(tap(&mut vi, &key(Lang::Fa, Page::Letters, "ض"), 0.1), ["down Q ctrl", "up Q ctrl"]);
}

#[test]
fn figures_follow_the_language_and_symbols_are_sent_with_shift() {
    let mut vi = VirtualInput::default();
    assert_eq!(tap(&mut vi, &key(Lang::Fa, Page::Numbers, "۱"), 0.0), ["down Num1", "text ۱", "up Num1"]);
    assert_eq!(tap(&mut vi, &key(Lang::Fa, Page::Numbers, "؟"), 0.1), ["down Slash shift", "text ؟", "up Slash shift"]);
    assert_eq!(tap(&mut vi, &key(Lang::En, Page::Numbers, "@"), 0.2), ["down Num2 shift", "text @", "up Num2 shift"]);
}

#[test]
fn arrows_escape_and_tab_send_their_keys_without_text() {
    let mut vi = VirtualInput::default();
    assert_eq!(tap(&mut vi, &icon_key(Lang::En, Page::Letters, "arrow-left"), 0.0), ["down ArrowLeft", "up ArrowLeft"]);
    assert_eq!(tap(&mut vi, &key(Lang::En, Page::Letters, "Esc"), 0.1), ["down Escape", "up Escape"]);
    assert_eq!(tap(&mut vi, &key(Lang::En, Page::Letters, "Tab"), 0.2), ["down Tab", "up Tab"]);
}

#[test]
fn the_page_and_language_keys_switch_the_layout_and_type_nothing() {
    let mut vi = VirtualInput::default();
    assert!(tap(&mut vi, &key(Lang::En, Page::Letters, "123"), 0.0).is_empty());
    assert_eq!(vi.page(), Page::Numbers);
    assert!(tap(&mut vi, &key(Lang::En, Page::Numbers, "abc"), 0.1).is_empty());
    assert_eq!(vi.page(), Page::Letters);
    assert!(tap(&mut vi, &icon_key(Lang::En, Page::Letters, "languages"), 0.2).is_empty());
    assert_eq!(vi.lang(), Lang::Fa);
    assert!(tap(&mut vi, &icon_key(Lang::Fa, Page::Letters, "languages"), 0.3).is_empty());
    assert_eq!(vi.lang(), Lang::En);
}

#[test]
fn a_gap_takes_room_and_sends_nothing() {
    let mut vi = VirtualInput::default();
    let gap = rows(Lang::En, Page::Letters).into_iter().flatten().find(|k| k.action == Action::Gap).unwrap_or_else(|| panic!("no gap"));
    assert!(tap(&mut vi, &gap, 0.0).is_empty());
}

#[test]
fn typed_text_is_the_shifted_text_when_shift_is_held() {
    let q = key(Lang::En, Page::Letters, "q");
    assert_eq!(typed_text(&q, false), "q");
    assert_eq!(typed_text(&q, true), "Q");
    assert_eq!(typed_text(&key(Lang::En, Page::Numbers, "1"), true), "!");
}

#[test]
fn the_system_keyboard_is_asked_away_once_per_text_focus() {
    let mut vi = VirtualInput::default();
    assert_eq!(vi.ime_request(true), None, "not while the on-screen keyboard is off");
    vi.set_keyboard(true);
    assert_eq!(vi.ime_request(true), Some(false));
    assert_eq!(vi.ime_request(true), None, "the same focus is not asked again");
    assert_eq!(vi.ime_request(false), None, "the focus has gone");
    assert_eq!(vi.ime_request(true), Some(false), "a new focus is asked");
}

#[test]
fn the_system_keyboard_comes_back_if_the_on_screen_one_goes_during_a_focus() {
    let mut vi = VirtualInput::default();
    vi.set_keyboard(true);
    assert_eq!(vi.ime_request(true), Some(false));
    vi.set_keyboard(false);
    assert_eq!(vi.ime_request(true), Some(true));
    assert_eq!(vi.ime_request(true), None);
    // Turned off with no focus, nothing is asked.
    vi.set_keyboard(true);
    assert_eq!(vi.ime_request(false), None);
    vi.set_keyboard(false);
    assert_eq!(vi.ime_request(false), None);
}

#[test]
fn turning_the_keyboard_off_lets_go_of_the_modifiers() {
    let mut vi = VirtualInput::default();
    vi.set_keyboard(true);
    tap(&mut vi, &modifier_key(Lang::En, Page::Letters, Modifier::Ctrl), 0.0);
    vi.set_keyboard(false);
    assert_eq!(vi.hold(Modifier::Ctrl), Hold::Off);
}

#[test]
fn the_keyboard_lifts_the_dialogs_and_the_menu_only_while_it_is_on() {
    let mut app = crate::PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    assert_eq!(lift(&app), 0.0);
    app.virtual_input.set_keyboard(true);
    assert_eq!(lift(&app), PANEL_H);
}
