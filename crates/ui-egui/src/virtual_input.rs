//! The on-screen keyboard: the phone's stand-in for a hardware keyboard, which a desktop app's shortcuts need.
//! It is a toggle in the phone's top bar.
//!
//! - The **keyboard** is a panel at the foot of the screen, in place of the system keyboard. It has the
//!   English (QWERTY) and Persian (ISIRI 2901) letters, a page of figures and symbols, and the keys a
//!   shortcut needs: Ctrl, Alt and Shift (a tap holds the modifier for the next key, two quick taps lock
//!   it until it is tapped again), Esc, Tab, Home, End, the arrows, Delete and Enter. A key sends the
//!   event its hardware key sends: the physical key with the modifiers held, and the text it types
//!   unless Ctrl or Alt is held.
//!
//! The events wait in a queue and go into the next frame's input (`raw_input_hook`), so every widget that
//! reads the keyboard gets them as it would get the hardware's. The canvas's pointer, which a finger moves,
//! is `virtual_mouse`.

use egui::{Align2, Event, Frame, Key, Margin, Modifiers, Rect, Sense, Stroke, vec2};

use crate::PhotocraftApp;
use crate::icons;
use crate::theme::{self, Tokens};

/// Two taps of a modifier this close together (seconds) lock it.
const LOCK_GAP: f64 = 0.45;
/// A key's height, the gap between keys and between rows, and the panel's padding above and below.
pub const KEY_H: f32 = 42.0;
const KEY_GAP: f32 = 4.0;
const PAD: f32 = 6.0;
/// The keyboard panel's height: five rows of keys, with their gaps and padding.
pub const PANEL_H: f32 = 5.0 * KEY_H + 4.0 * KEY_GAP + 2.0 * PAD;

/// The keyboard's language.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Lang {
    /// English: the QWERTY letters.
    #[default]
    En,
    /// Persian: the ISIRI 2901 letters, Persian figures and punctuation.
    Fa,
}

/// The keyboard's page: the letters, or the figures and symbols.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Page {
    #[default]
    Letters,
    Numbers,
}

/// A modifier key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Modifier {
    Ctrl,
    Alt,
    Shift,
}

/// How a modifier is held.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Hold {
    #[default]
    Off,
    /// For the next key pressed only.
    Once,
    /// Until it is tapped again.
    Locked,
}

/// What a key does when it is pressed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Sends its key, and types its text unless Ctrl or Alt is held.
    Type,
    /// Sends its key only (Esc, the arrows, Enter...).
    Press,
    /// Holds a modifier.
    Hold(Modifier),
    /// Switches between the letters and the figures.
    Page,
    /// Switches between English and Persian.
    Lang,
    /// Takes room only.
    Gap,
}

/// One key: what it shows, what it sends, and how wide it is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KeyDef {
    /// The text the key shows.
    pub label: &'static str,
    /// A Lucide icon the key shows instead of its label.
    pub icon: Option<&'static str>,
    /// A small text in the key's corner: the Latin letter under a Persian key, the layout of the language key.
    pub hint: &'static str,
    pub action: Action,
    /// The physical key the event names.
    pub code: Option<Key>,
    /// The text a typing key types without Shift.
    pub text: &'static str,
    /// The text it types with Shift (empty: the upper case of `text`).
    pub shifted: &'static str,
    /// The key sends Shift itself: a symbol that sits on a shifted key of the US layout.
    pub force_shift: bool,
    /// The key's width, in units of the row's width.
    pub units: f32,
}

fn typing(label: &'static str, code: Key, text: &'static str) -> KeyDef {
    KeyDef { label, icon: None, hint: "", action: Action::Type, code: Some(code), text, shifted: "", force_shift: false, units: 1.0 }
}

fn pressing(label: &'static str, icon: Option<&'static str>, code: Key, units: f32) -> KeyDef {
    KeyDef { label, icon, hint: "", action: Action::Press, code: Some(code), text: "", shifted: "", force_shift: false, units }
}

fn holding(m: Modifier, label: &'static str, units: f32) -> KeyDef {
    KeyDef { label, icon: None, hint: "", action: Action::Hold(m), code: None, text: "", shifted: "", force_shift: false, units }
}

fn switching(label: &'static str, icon: Option<&'static str>, hint: &'static str, action: Action, units: f32) -> KeyDef {
    KeyDef { label, icon, hint, action, code: None, text: "", shifted: "", force_shift: false, units }
}

fn gap(units: f32) -> KeyDef {
    KeyDef { label: "", icon: None, hint: "", action: Action::Gap, code: None, text: "", shifted: "", force_shift: false, units }
}

impl KeyDef {
    fn shifted(self, text: &'static str) -> Self {
        Self { shifted: text, ..self }
    }

    fn forced(self) -> Self {
        Self { force_shift: true, ..self }
    }

    fn hinted(self, hint: &'static str) -> Self {
        Self { hint, ..self }
    }

    fn wide(self, units: f32) -> Self {
        Self { units, ..self }
    }
}

/// The virtual keyboard's state, kept on the app (not saved with a document).
#[derive(Default)]
pub struct VirtualInput {
    keyboard: bool,
    lang: Lang,
    page: Page,
    ctrl: HoldState,
    alt: HoldState,
    shift: HoldState,
    /// The events for the next frame's input.
    queue: Vec<Event>,
    /// The system keyboard has been asked away for the current text focus (see [`VirtualInput::ime_request`]).
    ime_away_asked: bool,
}

#[derive(Clone, Copy, Debug, Default)]
struct HoldState {
    hold: Hold,
    /// When the modifier was last tapped, in seconds (egui's clock).
    last_tap: f64,
}

impl VirtualInput {
    pub fn keyboard_on(&self) -> bool {
        self.keyboard
    }

    /// Shows or hides the keyboard. Hiding it lets go of the modifiers.
    pub fn set_keyboard(&mut self, on: bool) {
        self.keyboard = on;
        if !on {
            self.ctrl = HoldState::default();
            self.alt = HoldState::default();
            self.shift = HoldState::default();
        }
    }

    pub fn toggle_keyboard(&mut self) {
        self.set_keyboard(!self.keyboard);
    }

    pub fn lang(&self) -> Lang {
        self.lang
    }

    pub fn page(&self) -> Page {
        self.page
    }

    pub fn hold(&self, m: Modifier) -> Hold {
        self.state(m).hold
    }

    /// The events queued for the next frame's input.
    pub fn take_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.queue)
    }

    /// The events queued and not yet taken.
    pub fn queued(&self) -> &[Event] {
        &self.queue
    }

    /// What to ask of the system keyboard this frame, given whether a text field has the focus. While the
    /// on-screen keyboard is on, the system one is asked away (`Some(false)`) once for each focus, so that the
    /// field types with the on-screen one. If the on-screen keyboard goes while a field still has the focus,
    /// the system one is asked back (`Some(true)`).
    pub fn ime_request(&mut self, typing: bool) -> Option<bool> {
        if self.keyboard && typing {
            return (!std::mem::replace(&mut self.ime_away_asked, true)).then_some(false);
        }
        let was_away = std::mem::replace(&mut self.ime_away_asked, false);
        (was_away && typing).then_some(true)
    }

    /// A key was pressed (and released): queues its events, or changes the modifier, page or language it
    /// switches. A key press spends a modifier held for one key only.
    pub fn press(&mut self, key: &KeyDef, now: f64) {
        match key.action {
            Action::Hold(m) => self.tap(m, now),
            Action::Page => {
                self.page = match self.page {
                    Page::Letters => Page::Numbers,
                    Page::Numbers => Page::Letters,
                }
            }
            Action::Lang => {
                self.lang = match self.lang {
                    Lang::En => Lang::Fa,
                    Lang::Fa => Lang::En,
                }
            }
            Action::Gap => {}
            Action::Type | Action::Press => {
                let events = self.key_events(key);
                self.queue.extend(events);
                self.spend_once();
            }
        }
    }

    fn state(&self, m: Modifier) -> &HoldState {
        match m {
            Modifier::Ctrl => &self.ctrl,
            Modifier::Alt => &self.alt,
            Modifier::Shift => &self.shift,
        }
    }

    fn state_mut(&mut self, m: Modifier) -> &mut HoldState {
        match m {
            Modifier::Ctrl => &mut self.ctrl,
            Modifier::Alt => &mut self.alt,
            Modifier::Shift => &mut self.shift,
        }
    }

    /// The modifiers the next key carries: every one that is held.
    pub(crate) fn modifiers(&self) -> Modifiers {
        let ctrl = self.hold(Modifier::Ctrl) != Hold::Off;
        Modifiers {
            alt: self.hold(Modifier::Alt) != Hold::Off,
            ctrl,
            shift: self.hold(Modifier::Shift) != Hold::Off,
            mac_cmd: false,
            command: ctrl,
        }
    }

    /// Off → held for one key → (a second tap soon after) locked → off.
    fn tap(&mut self, m: Modifier, now: f64) {
        let s = self.state_mut(m);
        let quick = now - s.last_tap <= LOCK_GAP;
        s.hold = match s.hold {
            Hold::Off => Hold::Once,
            Hold::Once if quick => Hold::Locked,
            Hold::Once | Hold::Locked => Hold::Off,
        };
        s.last_tap = now;
    }

    pub(crate) fn spend_once(&mut self) {
        for m in [Modifier::Ctrl, Modifier::Alt, Modifier::Shift] {
            let s = self.state_mut(m);
            if s.hold == Hold::Once {
                s.hold = Hold::Off;
            }
        }
    }

    fn key_events(&self, key: &KeyDef) -> Vec<Event> {
        let Some(code) = key.code else { return Vec::new() };
        let mut modifiers = self.modifiers();
        modifiers.shift |= key.force_shift;
        let text = if key.action == Action::Type && !modifiers.ctrl && !modifiers.alt { typed_text(key, modifiers.shift) } else { String::new() };
        let press = |pressed: bool| Event::Key { key: code, physical_key: Some(code), pressed, repeat: false, modifiers };
        let mut events = vec![press(true)];
        if !text.is_empty() {
            events.push(Event::Text(text));
        }
        events.push(press(false));
        events
    }
}

/// The text a typing key types, with or without Shift.
pub fn typed_text(key: &KeyDef, shift: bool) -> String {
    match (shift, key.shifted) {
        (false, _) => key.text.to_string(),
        (true, "") => key.text.to_uppercase(),
        (true, shifted) => shifted.to_string(),
    }
}

/// The height the keyboard takes from the bottom of the screen (none while it is off): dialogs and the
/// menu keep clear of it.
pub fn lift(app: &PhotocraftApp) -> f32 {
    if app.virtual_input.keyboard_on() { PANEL_H } else { 0.0 }
}

/// Each frame: the system keyboard is asked away from (or back to) a text field, and the keyboard's Persian
/// letters get their font once it is on.
pub fn logic(app: &mut PhotocraftApp, ctx: &egui::Context) {
    if let Some(show) = app.virtual_input.ime_request(ctx.egui_wants_keyboard_input())
        && let Some(ask) = app.services.soft_keyboard.as_mut()
    {
        ask(show);
    }
    if app.virtual_input.keyboard_on() {
        load_script_font(ctx);
    }
}

/// Loads the system's Arabic-script font, once, for the Persian keys (Android). egui's own fonts have no
/// Persian letters, and the CJK fallback looks only at CJK text.
fn load_script_font(ctx: &egui::Context) {
    let id = egui::Id::unique("virtual-keyboard-script-font");
    if ctx.data(|d| d.get_temp::<bool>(id)).unwrap_or(false) {
        return;
    }
    ctx.data_mut(|d| d.insert_temp(id, true));
    if let Some(bytes) = script_font_bytes() {
        crate::cjk_fonts::add_to_all_families(ctx, "system-arabic".to_string(), egui::FontData::from_owned(bytes));
        ctx.request_repaint();
    }
}

#[cfg(target_os = "android")]
fn script_font_bytes() -> Option<Vec<u8>> {
    const FILES: [&str; 4] = ["NotoNaskhArabic-Regular.ttf", "NotoNaskhArabicUI-Regular.ttf", "NotoSansArabic-Regular.ttf", "DroidSansArabic.ttf"];
    FILES.iter().find_map(|file| {
        let path = std::path::Path::new("/system/fonts").join(file);
        let meta = std::fs::metadata(&path).ok()?;
        if !meta.is_file() || meta.len() == 0 || meta.len() > crate::cjk_fonts::MAX_FONT_BYTES {
            return None;
        }
        std::fs::read(&path).ok()
    })
}

#[cfg(not(target_os = "android"))]
fn script_font_bytes() -> Option<Vec<u8>> {
    None
}

// The layouts. English: QWERTY. Persian: ISIRI 2901, the key's letter under each Latin key, with the
// letter it types with Shift (empty: the same letter).

const EN_ROW1: &[(Key, &str)] = &[
    (Key::Q, "q"),
    (Key::W, "w"),
    (Key::E, "e"),
    (Key::R, "r"),
    (Key::T, "t"),
    (Key::Y, "y"),
    (Key::U, "u"),
    (Key::I, "i"),
    (Key::O, "o"),
    (Key::P, "p"),
];
const EN_ROW2: &[(Key, &str)] = &[
    (Key::A, "a"),
    (Key::S, "s"),
    (Key::D, "d"),
    (Key::F, "f"),
    (Key::G, "g"),
    (Key::H, "h"),
    (Key::J, "j"),
    (Key::K, "k"),
    (Key::L, "l"),
];
const EN_ROW3: &[(Key, &str)] = &[(Key::Z, "z"), (Key::X, "x"), (Key::C, "c"), (Key::V, "v"), (Key::B, "b"), (Key::N, "n"), (Key::M, "m")];

/// A Persian key: the key, its Latin letter (the hint), the letter, and the letter typed with Shift.
type FaKey = (Key, &'static str, &'static str, &'static str);

const FA_ROW1: &[FaKey] = &[
    (Key::Q, "q", "ض", ""),
    (Key::W, "w", "ص", ""),
    (Key::E, "e", "ث", ""),
    (Key::R, "r", "ق", ""),
    (Key::T, "t", "ف", ""),
    (Key::Y, "y", "غ", ""),
    (Key::U, "u", "ع", ""),
    (Key::I, "i", "ه", ""),
    (Key::O, "o", "خ", ""),
    (Key::P, "p", "ح", ""),
    (Key::OpenBracket, "[", "ج", "{"),
    (Key::CloseBracket, "]", "چ", "}"),
];
const FA_ROW2: &[FaKey] = &[
    (Key::A, "a", "ش", ""),
    (Key::S, "s", "س", ""),
    (Key::D, "d", "ی", ""),
    (Key::F, "f", "ب", ""),
    (Key::G, "g", "ل", ""),
    (Key::H, "h", "ا", ""),
    (Key::J, "j", "ت", ""),
    (Key::K, "k", "ن", ""),
    (Key::L, "l", "م", ""),
    (Key::Semicolon, ";", "ک", "؛"),
    (Key::Quote, "'", "گ", "\""),
];
const FA_ROW3: &[FaKey] = &[
    (Key::Z, "z", "ظ", ""),
    (Key::X, "x", "ط", ""),
    (Key::C, "c", "ز", ""),
    (Key::V, "v", "ر", ""),
    (Key::B, "b", "ذ", ""),
    (Key::N, "n", "د", ""),
    (Key::M, "m", "ئ", ""),
    (Key::Comma, ",", "و", "،"),
    (Key::Period, ".", ".", ">"),
    (Key::Slash, "/", "/", "؟"),
];

/// A figure key: the key, the figure it types, and the symbol it types with Shift.
type DigitKey = (Key, &'static str, &'static str);

const EN_DIGITS: &[DigitKey] = &[
    (Key::Num1, "1", "!"),
    (Key::Num2, "2", "@"),
    (Key::Num3, "3", "#"),
    (Key::Num4, "4", "$"),
    (Key::Num5, "5", "%"),
    (Key::Num6, "6", "^"),
    (Key::Num7, "7", "&"),
    (Key::Num8, "8", "*"),
    (Key::Num9, "9", "("),
    (Key::Num0, "0", ")"),
];
const FA_DIGITS: &[DigitKey] = &[
    (Key::Num1, "۱", "!"),
    (Key::Num2, "۲", "@"),
    (Key::Num3, "۳", "#"),
    (Key::Num4, "۴", "$"),
    (Key::Num5, "۵", "%"),
    (Key::Num6, "۶", "^"),
    (Key::Num7, "۷", "&"),
    (Key::Num8, "۸", "*"),
    (Key::Num9, "۹", "("),
    (Key::Num0, "۰", ")"),
];

fn en_row(row: &[(Key, &'static str)]) -> Vec<KeyDef> {
    row.iter().map(|&(code, letter)| typing(letter, code, letter)).collect()
}

fn fa_row(row: &[FaKey]) -> Vec<KeyDef> {
    row.iter().map(|&(code, hint, letter, shifted)| typing(letter, code, letter).hinted(hint).shifted(shifted)).collect()
}

fn utility_row() -> Vec<KeyDef> {
    vec![
        pressing("Esc", None, Key::Escape, 1.0),
        pressing("Tab", None, Key::Tab, 1.0),
        holding(Modifier::Ctrl, "Ctrl", 1.0),
        holding(Modifier::Alt, "Alt", 1.0),
        pressing("Home", None, Key::Home, 1.0),
        pressing("", Some("arrow-left"), Key::ArrowLeft, 1.0),
        pressing("", Some("arrow-up"), Key::ArrowUp, 1.0),
        pressing("", Some("arrow-down"), Key::ArrowDown, 1.0),
        pressing("", Some("arrow-right"), Key::ArrowRight, 1.0),
        pressing("End", None, Key::End, 1.0),
    ]
}

fn letter_rows(lang: Lang) -> Vec<Vec<KeyDef>> {
    let delete = pressing("", Some("delete"), Key::Backspace, 1.5);
    match lang {
        Lang::En => {
            let mut second = vec![gap(0.5)];
            second.extend(en_row(EN_ROW2));
            second.push(gap(0.5));
            let mut third = vec![holding(Modifier::Shift, "Shift", 1.5)];
            third.extend(en_row(EN_ROW3));
            third.push(delete);
            vec![en_row(EN_ROW1), second, third]
        }
        Lang::Fa => {
            let mut third = vec![holding(Modifier::Shift, "Shift", 1.5)];
            third.extend(fa_row(FA_ROW3));
            third.push(delete);
            vec![fa_row(FA_ROW1), fa_row(FA_ROW2), third]
        }
    }
}

fn number_rows(lang: Lang) -> Vec<Vec<KeyDef>> {
    let (digits, comma, question, semicolon) = match lang {
        Lang::En => (EN_DIGITS, ",", "?", ";"),
        Lang::Fa => (FA_DIGITS, "،", "؟", "؛"),
    };
    let figures: Vec<KeyDef> = digits.iter().map(|&(code, label, shifted)| typing(label, code, label).shifted(shifted)).collect();
    let symbols = vec![
        typing("-", Key::Minus, "-"),
        typing("/", Key::Slash, "/"),
        typing(":", Key::Semicolon, ":").forced(),
        typing(semicolon, Key::Semicolon, semicolon),
        typing("(", Key::Num9, "(").forced(),
        typing(")", Key::Num0, ")").forced(),
        typing("$", Key::Num4, "$").forced(),
        typing("&", Key::Num7, "&").forced(),
        typing("@", Key::Num2, "@").forced(),
        typing("\"", Key::Quote, "\"").forced(),
    ];
    let more = vec![
        typing(".", Key::Period, "."),
        typing(comma, Key::Comma, comma),
        typing(question, Key::Slash, question).forced(),
        typing("!", Key::Num1, "!").forced(),
        typing("=", Key::Equals, "="),
        typing("+", Key::Equals, "+").forced(),
        typing("'", Key::Quote, "'"),
        typing("_", Key::Minus, "_").forced(),
        pressing("", Some("delete"), Key::Backspace, 2.0),
    ];
    vec![figures, symbols, more]
}

fn bottom_row(lang: Lang, page: Page) -> Vec<KeyDef> {
    let page_label = match page {
        Page::Letters => "123",
        Page::Numbers => "abc",
    };
    let lang_hint = match lang {
        Lang::En => "EN",
        Lang::Fa => "FA",
    };
    let space_shifted = match lang {
        Lang::En => "",
        // Shift + Space is the zero-width non-joiner, which Persian writing uses inside words.
        Lang::Fa => "\u{200C}",
    };
    vec![
        switching(page_label, None, "", Action::Page, 1.5),
        switching("", Some("languages"), lang_hint, Action::Lang, 1.0),
        typing("Space", Key::Space, " ").shifted(space_shifted).wide(4.0),
        typing(".", Key::Period, "."),
        pressing("Del", None, Key::Delete, 1.0),
        pressing("", Some("corner-down-left"), Key::Enter, 1.5),
    ]
}

/// The keyboard's rows, top to bottom, for a language and page: the keys' order is the order they are laid
/// out in, left to right.
pub fn rows(lang: Lang, page: Page) -> Vec<Vec<KeyDef>> {
    let mut rows = vec![utility_row()];
    rows.extend(match page {
        Page::Letters => letter_rows(lang),
        Page::Numbers => number_rows(lang),
    });
    rows.push(bottom_row(lang, page));
    rows
}

/// The widths of a row's keys when the row is `avail` points wide: the keys share the width the gaps between
/// them leave, in proportion to their units.
pub fn key_widths(row: &[KeyDef], avail: f32) -> Vec<f32> {
    let units: f32 = row.iter().map(|k| k.units).sum();
    let gaps = KEY_GAP * row.len().saturating_sub(1) as f32;
    let unit = if units > 0.0 { ((avail - gaps) / units).max(0.0) } else { 0.0 };
    row.iter().map(|k| unit * k.units).collect()
}

/// The label a key shows: while Shift is held, the letter or symbol it types with Shift.
fn shown_label(key: &KeyDef, shift: bool) -> String {
    if !shift || key.action != Action::Type || key.label.chars().count() != 1 {
        return key.label.to_string();
    }
    if key.shifted.is_empty() { key.label.to_uppercase() } else { key.shifted.to_string() }
}

/// What a screen reader calls a key: its label, or its icon's name.
fn key_name(key: &KeyDef) -> &'static str {
    if key.label.is_empty() { key.icon.unwrap_or("key") } else { key.label }
}

/// The keyboard's keys, row by row. Returns the key that was clicked, if one was.
fn body(app: &PhotocraftApp, ui: &mut egui::Ui) -> Option<KeyDef> {
    let t = Tokens::get(ui.ctx());
    let vi = &app.virtual_input;
    let avail = ui.available_width();
    let shift = vi.hold(Modifier::Shift) != Hold::Off;
    let mut picked = None;
    ui.spacing_mut().item_spacing = vec2(KEY_GAP, KEY_GAP);
    for row in rows(vi.lang, vi.page) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = KEY_GAP;
            for (key, w) in row.iter().zip(key_widths(&row, avail)) {
                if key.action == Action::Gap {
                    let _ = ui.allocate_exact_size(vec2(w, KEY_H), Sense::hover());
                    continue;
                }
                let (rect, resp) = ui.allocate_exact_size(vec2(w, KEY_H), Sense::click());
                resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, key_name(key)));
                let label = shown_label(key, shift);
                paint_key(ui, rect, &resp, key, vi, &label, &t);
                if resp.clicked() {
                    picked = Some(*key);
                }
            }
        });
    }
    picked
}

fn paint_key(ui: &egui::Ui, rect: Rect, resp: &egui::Response, key: &KeyDef, vi: &VirtualInput, label: &str, t: &Tokens) {
    let (fill, border, ink) = match key.action {
        Action::Hold(m) => match vi.hold(m) {
            Hold::Off => (t.card, t.card_border, t.text),
            Hold::Once => (t.accent_soft, t.accent_border, t.text),
            Hold::Locked => (t.accent, t.accent, t.accent_text),
        },
        Action::Page | Action::Lang => (t.field, t.field_border, t.text_dim),
        _ => (t.card, t.card_border, t.text),
    };
    let fill = if resp.is_pointer_button_down_on() { t.pressed } else { fill };
    let painter = ui.painter();
    painter.rect_filled(rect, t.radius_sm, fill);
    painter.rect_stroke(rect, t.radius_sm, Stroke::new(1.0, border), egui::StrokeKind::Inside);
    if let Some(icon) = key.icon {
        icons::paint(ui, rect, icon, 20.0, ink);
    }
    if !label.is_empty() {
        let size = match key.action {
            Action::Type if label.chars().count() <= 2 => 18.0,
            Action::Type | Action::Page | Action::Lang => 13.0,
            _ => 12.0,
        };
        painter.text(rect.center(), Align2::CENTER_CENTER, label, theme::medium(size), ink);
    }
    if !key.hint.is_empty() {
        painter.text(rect.left_top() + vec2(5.0, 3.0), Align2::LEFT_TOP, key.hint, egui::FontId::proportional(9.0), t.text_faint);
    }
}

/// The keyboard's panel, at the foot of the screen: the first panel added, so the bottom-most. While a dialog
/// or the menu is open (`covered`), the keys are drawn again above their dimming, so that they take the taps.
pub fn panel(app: &mut PhotocraftApp, ui: &mut egui::Ui, covered: bool) {
    if !app.virtual_input.keyboard_on() {
        return;
    }
    let t = Tokens::get(ui.ctx());
    let now = ui.input(|i| i.time);
    let mut outer: Option<Rect> = None;
    let mut picked = None;
    egui::Panel::bottom("phone-keys")
        .exact_size(PANEL_H)
        .frame(Frame::NONE.fill(t.chrome).inner_margin(Margin::symmetric(4, 6)))
        .show(ui, |ui| {
            outer = Some(ui.max_rect().expand2(vec2(4.0, PAD)));
            picked = body(app, ui);
        });
    if let Some(r) = outer.filter(|_| covered) {
        let mut above = None;
        egui::Area::new(egui::Id::unique("virtual-keys-over-screen"))
            .order(egui::Order::Tooltip)
            .fixed_pos(r.min)
            .movable(false)
            .show(ui.ctx(), |ui| {
                ui.set_width(r.width());
                ui.set_height(r.height());
                Frame::NONE.fill(t.chrome).inner_margin(Margin::symmetric(4, 6)).show(ui, |ui| above = body(app, ui));
            });
        picked = picked.or(above);
    }
    if let Some(key) = picked {
        app.virtual_input.press(&key, now);
        ui.ctx().request_repaint();
    }
}

#[cfg(test)]
mod tests;
