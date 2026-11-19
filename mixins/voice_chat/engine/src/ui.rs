//! The voice chat menu (V): the connection, the controls, the hotkeys and the
//! groups, drawn on the designer's own grey panels (shipped in the icon
//! resource pack). It lays itself out from a [`View`] of the voice chat each
//! frame and keeps only what is typed and focused. Every label is localised
//! through [`crate::i18n`].
use crate::Settings;
use crate::i18n::{fmt, tr};
use crate::net::Mode;

/// What a hook draws, in GUI pixels. `Sprite` blits part of a texture (a
/// designer panel, an icon), `Fill` draws solid rectangles and `Text` labels.
#[derive(Clone, Debug, PartialEq)]
pub enum Draw {
    Fill([f32; 4], u32),
    Text(String, [f32; 2], u32, bool),
    /// Part `src` (`[x, y, w, h]` in the sprite's own pixels) of sprite
    /// `name`, stretched into `dst` (GUI pixels). Maps to `native_sprite`.
    Sprite {
        name: &'static str,
        src: [f32; 4],
        dst: [f32; 4],
    },
}

// The designer's panels (their opaque content is the top-left WxH of each
// 256x256 sheet). The main menu is nine-sliced up to 224x72 so the status
// line fits; the sub-screens stretch a panel to the room they need, nine-
// sliced so the bevel and corner rivets stay crisp.
const PANEL_MAIN: (&str, f32, f32) = ("voice_chat:gui/gui_voicechat", 195.0, 76.0);
const MAIN_SIZE: (f32, f32) = (224.0, 72.0);
const MAIN_BORDER: f32 = 8.0;
const PANEL_SUB: (&str, f32, f32) = ("voice_chat:gui/gui_voicechat_settings", 248.0, 198.0);
const SUB_BORDER: f32 = 8.0;

// The designer's activation meter: a 12-segment VU strip (green through red)
// in the corner of a 256x256 sheet. The strip's own background is (26, 26, 26).
const TRACK: &str = "voice_chat:gui/voice_activation_slider";
const SLIDER_HANDLE: &str = "gui/sprites/widget/slider_handle";
const SLIDER_HANDLE_HOVER: &str = "gui/sprites/widget/slider_handle_highlighted";
const VU_BG: u32 = 0xFF1A_1A1A;
const VU_FRAME: u32 = 0xFF10_1014;

// The 16x16 icon glyphs in the resource pack.
const IC_MIC_ON: &str = "voice_chat:gui/icon_microphone";
const IC_MIC_OFF: &str = "voice_chat:gui/icon_mute";
const IC_SPK_ON: &str = "voice_chat:gui/icon_speaker_on";
const IC_SPK_OFF: &str = "voice_chat:gui/icon_speaker_off";
const IC_PTT: &str = "voice_chat:gui/icon_ptt";
const IC_VOICE: &str = "voice_chat:gui/icon_voice";
const IC_SETTINGS: &str = "voice_chat:gui/icon_settings";
const IC_GROUPS: &str = "voice_chat:gui/icon_group";
const IC_VOLUMES: &str = "voice_chat:gui/icon_adjust_volumes";
const IC_PLAYER_VOLUME: &str = "voice_chat:gui/icon_player_volume";
const IC_LOCK: &str = "voice_chat:gui/icon_lock";
const IC_JOIN: &str = "voice_chat:gui/icon_join";
const IC_LEAVE: &str = "voice_chat:gui/icon_leave";

// Readable ink on the designer's light-grey panel (no drop shadow).
const INK: u32 = 0x3F_3F3F;
const INK_SOFT: u32 = 0x55_5555;
const INK_FAINT: u32 = 0x6F_6F6F;
const TITLE_INK: u32 = 0x3F_3F3F;

const CARD_OUT: u32 = 0xFFA0_A0A0;
const CARD_BG: u32 = 0x10FF_FFFF;
const CARD_HEAD: u32 = 0x1000_0000;
const CARD_LINE: u32 = 0xFFA0_A0A0;
const CARD_LIGHT: u32 = 0x80FF_FFFF;
const KEY_BG: u32 = 0xFFD6_D6D6;
const KEY_OUT: u32 = 0xFF55_5555;

// The design book's button palette: black outline, grey face, white/dark bevel.
const BTN_OUT: u32 = 0xFF00_0000;
const BTN_FACE: u32 = 0xFF8B_8B8B;
const BTN_FACE_HOVER: u32 = 0xFF7D_8FC9;
const BTN_TOP: u32 = 0xFFAA_AAAA;
const BTN_BOT: u32 = 0xFF55_5555;
const BTN_INK: u32 = 0xFFFF_FFFF;
const BTN_ON: u32 = 0xFF4A_B64E;
const BTN_OFF: u32 = 0xFFC2_4234;

// A sunken text field.
const FIELD_OUT: u32 = 0xFF10_1014;
const FIELD_BG: u32 = 0xFF22_2226;
const FIELD_FOCUS: u32 = 0xFFE6_E6F0;
const FIELD_INK: u32 = 0xFFFF_FFFF;
const FIELD_HINT: u32 = 0xFF7A_7A82;

/// A bright HUD colour darkened to read on the light-grey panel.
fn ink(rgb: u32) -> u32 {
    let ch = |sh: u32| (((rgb >> sh) & 0xFF) as f32 * 0.52) as u32;
    (ch(16) << 16) | (ch(8) << 8) | ch(0)
}

/// Which screen of the menu is showing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Screen {
    #[default]
    Main,
    Settings,
    Groups,
    Volumes,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Mute,
    Disable,
    Mode,
    Volume(i32),
    PlayerVolume(u128, i32),
    Gain(i32),
    /// Pointer on the level meter; the fraction along its handle track
    /// (0.0 = quiet, 1.0 = loud).
    ThresholdAt(f32),
    /// Open or close the input/output device list under its picker.
    OpenDrop {
        input: bool,
    },
    /// Pick a device from the open list; `None` is the OS default.
    PickDevice {
        input: bool,
        name: Option<String>,
    },
    /// Scroll the open device list.
    DropScroll(i32),
    Join(u128),
    JoinWithPassword,
    CancelJoin,
    Leave,
    Focus(Field),
    Kind,
    Create,
    ShowCreate,
    CancelCreate,
    Close,
    /// Navigate to another screen, handled inside the menu (not by the session).
    Open(Screen),
    Back,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Name,
    Password,
    JoinPassword,
}

/// A device list open under a picker.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dropdown {
    pub input: bool,
    pub scroll: usize,
}

/// A group as the menu lists it.
#[derive(Clone, Debug, PartialEq)]
pub struct GroupRow {
    pub id: u128,
    pub name: String,
    pub has_password: bool,
    pub members: Vec<String>,
    pub ours: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VolumeRow {
    pub uuid: u128,
    pub name: String,
    pub volume: u32,
}

/// The voice chat as the menu shows it this frame.
pub struct View {
    pub status: (String, u32),
    pub settings: Settings,
    pub level_db: f32,
    pub connected: bool,
    pub groups_enabled: bool,
    pub groups: Vec<GroupRow>,
    /// The hotkeys: what each does and the key bound to it, already named.
    pub hotkeys: Vec<(String, String)>,
    /// Input and output device names for the settings screen.
    pub devices: (Vec<String>, Vec<String>),
    pub volume_sources: Vec<VolumeRow>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Widget {
    pub rect: [f32; 4],
    pub action: Action,
}

pub const NAME_MAX: usize = crate::proto::MAX_GROUP_NAME;
const PASSWORD_MAX: usize = crate::proto::MAX_GROUP_PASSWORD;
const ROW: f32 = 30.0;

/// The group kinds, as i18n keys.
const KIND_KEYS: [&str; 3] = ["kind_normal", "kind_open", "kind_isolated"];
const KIND_DESCRIPTION_KEYS: [&str; 3] =
    ["kind_normal_desc", "kind_open_desc", "kind_isolated_desc"];

#[derive(Default)]
pub struct Menu {
    pub open: bool,
    pub screen: Screen,
    close_requested: bool,
    pub name: String,
    pub password: String,
    pub kind: i16,
    pub focus: Option<Field>,
    /// The password group being joined, and what is typed for it.
    pub joining: Option<u128>,
    pub creating: bool,
    pub join_password: String,
    pub message: Option<(String, u32)>,
    scroll: usize,
    threshold_drag: bool,
    /// The device list open under a picker, if any.
    dropdown: Option<Dropdown>,
    /// Where each device picker's face was drawn last frame, so the dropdown
    /// knows where to hang.
    drop_anchor_in: Option<[f32; 4]>,
    drop_anchor_out: Option<[f32; 4]>,
    /// The character of the key that opened the menu, typed with it.
    skip_char: Option<char>,
    widgets: Vec<Widget>,
}

struct Out<'a> {
    draws: Vec<Draw>,
    widgets: Vec<Widget>,
    mouse: [f32; 2],
    measure: &'a dyn Fn(&str) -> f32,
    /// A tooltip to draw on top, set when the pointer is over a widget.
    tip: Option<String>,
}

fn inside(rect: [f32; 4], at: [f32; 2]) -> bool {
    at[0] >= rect[0] && at[0] < rect[0] + rect[2] && at[1] >= rect[1] && at[1] < rect[1] + rect[3]
}

fn threshold_fraction(rect: [f32; 4], at: [f32; 2]) -> f32 {
    ((at[0] - rect[0] - 4.0) / (rect[2] - 8.0)).clamp(0.0, 1.0)
}

impl Out<'_> {
    fn fill(&mut self, rect: [f32; 4], argb: u32) {
        self.draws.push(Draw::Fill(rect, argb));
    }
    /// Dark panel text, no drop shadow.
    fn text(&mut self, text: &str, at: [f32; 2], rgb: u32) {
        self.draws.push(Draw::Text(text.to_owned(), at, rgb, false));
    }
    /// Light text with a shadow, for buttons and fields.
    fn text_sh(&mut self, text: &str, at: [f32; 2], rgb: u32) {
        self.draws.push(Draw::Text(text.to_owned(), at, rgb, true));
    }
    fn width(&self, text: &str) -> f32 {
        (self.measure)(text)
    }
    fn sprite(&mut self, name: &'static str, src: [f32; 4], dst: [f32; 4]) {
        self.draws.push(Draw::Sprite { name, src, dst });
    }
    /// Nine-slice `name` into `dst`: corners stay 1:1, edges and centre
    /// stretch, so a panel keeps its bevel and rivets at any size.
    fn nine_slice(&mut self, name: &'static str, dst: [f32; 4], sprite: (f32, f32), b: f32) {
        let [x, y, w, h] = dst;
        let (sw, sh) = sprite;
        let dx = [x, x + b, x + w - b];
        let dw = [b, (w - 2.0 * b).max(0.0), b];
        let dy = [y, y + b, y + h - b];
        let dh = [b, (h - 2.0 * b).max(0.0), b];
        let sx = [0.0, b, sw - b];
        let sww = [b, sw - 2.0 * b, b];
        let sy = [0.0, b, sh - b];
        let shh = [b, sh - 2.0 * b, b];
        for r in 0..3 {
            for c in 0..3 {
                self.sprite(
                    name,
                    [sx[c], sy[r], sww[c], shh[r]],
                    [dx[c], dy[r], dw[c], dh[r]],
                );
            }
        }
    }
    /// The designer's main panel nine-sliced into `dst`; a grey backing shows
    /// first so the menu is never invisible if the pack is missing.
    fn panel_main(&mut self, dst: [f32; 4]) {
        let (name, w, h) = PANEL_MAIN;
        self.fill(dst, 0xFFC2_C2C2);
        let [x, y, dw, dh] = dst;
        self.fill([x, y, dw, 1.0], 0xFF1C_1C1C);
        self.fill([x, y + dh - 1.0, dw, 1.0], 0xFF1C_1C1C);
        self.fill([x, y, 1.0, dh], 0xFF1C_1C1C);
        self.fill([x + dw - 1.0, y, 1.0, dh], 0xFF1C_1C1C);
        self.nine_slice(name, dst, (w, h), MAIN_BORDER);
    }
    /// A designer panel stretched (nine-sliced) into `dst`, with a grey backing.
    fn panel_sub(&mut self, dst: [f32; 4]) {
        let (name, w, h) = PANEL_SUB;
        self.fill(dst, 0xFFC2_C2C2);
        let [x, y, dw, dh] = dst;
        self.fill([x, y, dw, 1.0], 0xFF1C_1C1C);
        self.fill([x, y + dh - 1.0, dw, 1.0], 0xFF1C_1C1C);
        self.fill([x, y, 1.0, dh], 0xFF1C_1C1C);
        self.fill([x + dw - 1.0, y, 1.0, dh], 0xFF1C_1C1C);
        self.nine_slice(name, dst, (w, h), SUB_BORDER);
    }

    /// A 24x24 icon button with a 16x16 glyph centred in its bevel.
    fn icon_square(
        &mut self,
        x: f32,
        y: f32,
        icon: &'static str,
        action: Action,
        on: bool,
        tip: &str,
    ) {
        let rect = [x, y, 24.0, 24.0];
        let hover = inside(rect, self.mouse);
        self.fill(rect, BTN_OUT);
        self.fill(
            [x + 1.0, y + 1.0, 22.0, 22.0],
            if hover { BTN_FACE_HOVER } else { BTN_FACE },
        );
        self.fill([x + 1.0, y + 1.0, 22.0, 1.0], BTN_TOP);
        self.fill([x + 1.0, y + 1.0, 1.0, 22.0], BTN_TOP);
        self.fill([x + 1.0, y + 22.0, 22.0, 1.0], BTN_BOT);
        self.sprite(icon, [0.0, 0.0, 16.0, 16.0], [x + 4.0, y + 4.0, 16.0, 16.0]);
        if !on {
            self.fill([x + 1.0, y + 1.0, 22.0, 22.0], 0x7020_2024);
        }
        if hover {
            self.tip = Some(tip.to_owned());
        }
        self.widgets.push(Widget { rect, action });
    }
    /// `text` cut to `max` pixels with an ellipsis.
    fn fit(&self, text: &str, max: f32) -> String {
        if self.width(text) <= max {
            return text.to_owned();
        }
        let ellipsis = self.width("...");
        let bounds: Vec<usize> = text
            .char_indices()
            .map(|(i, _)| i)
            .chain([text.len()])
            .collect();
        let (mut low, mut high) = (0, bounds.len() - 1);
        while low < high {
            let mid = (low + high).div_ceil(2);
            if self.width(&text[..bounds[mid]]) + ellipsis <= max {
                low = mid;
            } else {
                high = mid - 1;
            }
        }
        format!("{}...", &text[..bounds[low]])
    }
    /// A drawn Minecraft-style button: bevelled face, highlighted on hover,
    /// a thin accent for a toggle's state (green on, red off).
    fn button(&mut self, rect: [f32; 4], label: &str, action: Action, state: Option<bool>) {
        let [x, y, w, h] = rect;
        let hover = inside(rect, self.mouse);
        self.fill(rect, BTN_OUT);
        self.fill(
            [x + 1.0, y + 1.0, w - 2.0, h - 2.0],
            if hover { BTN_FACE_HOVER } else { BTN_FACE },
        );
        self.fill([x + 1.0, y + 1.0, w - 2.0, 1.0], BTN_TOP);
        self.fill([x + 1.0, y + h - 2.0, w - 2.0, 1.0], BTN_BOT);
        if let Some(on) = state {
            self.fill(
                [x + 2.0, y + h - 3.0, w - 4.0, 1.0],
                if on { BTN_ON } else { BTN_OFF },
            );
        }
        let label = self.fit(label, w - 6.0);
        let tw = self.width(&label);
        self.text_sh(&label, [x + (w - tw) / 2.0, y + (h - 8.0) / 2.0], BTN_INK);
        self.widgets.push(Widget { rect, action });
    }
    /// A text field showing `value` (dots for a password), focused or not.
    fn field(
        &mut self,
        rect: [f32; 4],
        value: &str,
        hint: &str,
        secret: bool,
        focus: bool,
        action: Action,
    ) {
        let [x, y, w, h] = rect;
        self.fill(rect, if focus { FIELD_FOCUS } else { FIELD_OUT });
        self.fill([x + 1.0, y + 1.0, w - 2.0, h - 2.0], FIELD_BG);
        let shown = if secret {
            "*".repeat(value.chars().count())
        } else {
            value.to_owned()
        };
        let is_hint = shown.is_empty() && !focus;
        let (mut text, rgb) = if is_hint {
            (hint.to_owned(), FIELD_HINT)
        } else {
            (shown, FIELD_INK)
        };
        if is_hint {
            // A placeholder clips on the right; typed text scrolls from the left
            // so the caret stays in view.
            text = self.fit(&text, w - 6.0);
        } else {
            while !text.is_empty() && self.width(&text) > w - 10.0 {
                text.remove(0);
            }
        }
        let caret = if focus { "_" } else { "" };
        self.text_sh(
            &format!("{text}{caret}"),
            [x + 3.0, y + (h - 8.0) / 2.0],
            rgb,
        );
        self.widgets.push(Widget { rect, action });
    }
    /// `label: [-] value [+]` across `w`, on the light panel.
    fn stepper(
        &mut self,
        at: [f32; 2],
        w: f32,
        label: &str,
        value: &str,
        down: Action,
        up: Action,
    ) {
        let [x, y] = at;
        self.text(label, [x, y + 2.0], INK);
        let right = x + w;
        self.button([right - 13.0, y, 13.0, 12.0], "+", up, None);
        let vw = 46.0;
        let vx = right - 15.0 - vw;
        let tw = self.width(value);
        self.text(value, [vx + (vw - tw) / 2.0, y + 2.0], INK_SOFT);
        self.button([vx - 15.0, y, 13.0, 12.0], "-", down, None);
    }
    /// A section heading on the panel.
    fn heading(&mut self, text: &str, x: f32, y: f32) {
        self.text(text, [x, y], TITLE_INK);
    }
    fn settings_card(&mut self, rect: [f32; 4], title: &str) {
        let [x, y, w, h] = rect;
        self.fill(rect, CARD_OUT);
        self.fill([x + 1.0, y + 1.0, w - 2.0, h - 2.0], CARD_BG);
        self.fill([x + 1.0, y + 1.0, w - 2.0, 14.0], CARD_HEAD);
        self.fill([x + 1.0, y + 1.0, w - 2.0, 1.0], CARD_LIGHT);
        self.fill([x + 1.0, y + 15.0, w - 2.0, 1.0], CARD_LINE);
        self.fill([x + 1.0, y + 16.0, w - 2.0, 1.0], CARD_LIGHT);
        self.text(title, [x + 6.0, y + 5.0], TITLE_INK);
    }

    fn key_cap(&mut self, right: f32, y: f32, max_w: f32, label: &str, bound: bool) -> f32 {
        let w = (self.width(label) + 10.0).clamp(26.0, max_w);
        let x = right - w;
        self.fill([x, y - 2.0, w, 12.0], KEY_OUT);
        self.fill(
            [x + 1.0, y - 1.0, w - 2.0, 10.0],
            if bound { KEY_BG } else { CARD_BG },
        );
        let label = self.fit(label, w - 6.0);
        let tw = self.width(&label);
        self.text(
            &label,
            [x + (w - tw) / 2.0, y],
            if bound { INK } else { INK_FAINT },
        );
        w
    }
    /// `label  [current device ▾]` — the face opens the device list below it.
    fn device_picker(&mut self, x: f32, y: f32, w: f32, label: &str, name: &str, open: Action) {
        self.text(label, [x, y + 3.0], INK);
        let nx = x + 88.0;
        let nw = w - 88.0;
        let rect = [nx, y, nw, 14.0];
        let hover = inside(rect, self.mouse);
        self.fill(rect, BTN_OUT);
        self.fill(
            [nx + 1.0, y + 1.0, nw - 2.0, 12.0],
            if hover { BTN_FACE_HOVER } else { BTN_FACE },
        );
        self.fill([nx + 1.0, y + 1.0, nw - 2.0, 1.0], BTN_TOP);
        self.fill([nx + 1.0, y + 12.0, nw - 2.0, 1.0], BTN_BOT);
        self.text_sh(&self.fit(name, nw - 16.0), [nx + 3.0, y + 3.0], BTN_INK);
        self.text_sh("\u{25BC}", [nx + nw - 10.0, y + 3.0], BTN_INK);
        self.widgets.push(Widget { rect, action: open });
    }
    /// The full VU strip sits under a smooth dark veil driven by the live
    /// microphone level. The vanilla slider handle marks the threshold.
    fn vu_meter(&mut self, x: f32, y: f32, w: f32, level_db: f32, threshold: i32, dragging: bool) {
        let (dx, dy, dw, dh) = (x + 1.0, y + 1.0, w - 2.0, 12.0);
        let rect = [x, y, w, 14.0];
        self.fill(rect, VU_FRAME);
        self.fill([dx, dy, dw, dh], VU_BG);
        self.sprite(TRACK, [0.0, 0.0, 226.0, 18.0], [dx, dy, dw, dh]);

        let level = ((level_db + 80.0) / 80.0).clamp(0.0, 1.0);
        let vx = dx + level * dw;
        if vx < dx + dw {
            self.fill([vx, dy, dx + dw - vx, dh], 0x8010_1014);
        }

        let thr = ((threshold + 80) as f32 / 80.0).clamp(0.0, 1.0);
        let kx = x + thr * (w - 8.0);
        let hover = dragging || inside(rect, self.mouse);
        let handle_y = y - 3.0;
        self.fill([kx, handle_y, 8.0, 20.0], BTN_OUT);
        self.fill(
            [kx + 1.0, handle_y + 1.0, 6.0, 18.0],
            if hover { BTN_FACE_HOVER } else { BTN_FACE },
        );
        self.sprite(
            if hover {
                SLIDER_HANDLE_HOVER
            } else {
                SLIDER_HANDLE
            },
            [0.0, 0.0, 8.0, 20.0],
            [kx, handle_y, 8.0, 20.0],
        );
        self.widgets.push(Widget {
            rect,
            action: Action::ThresholdAt(0.0),
        });
    }
    /// A small ▲/▼ button at the side of an open device list.
    fn drop_arrow(&mut self, x: f32, y: f32, glyph: &str, action: Action) {
        let rect = [x, y, 9.0, 9.0];
        let hover = inside(rect, self.mouse);
        self.fill(rect, BTN_OUT);
        self.fill(
            [x + 1.0, y + 1.0, 7.0, 7.0],
            if hover { BTN_FACE_HOVER } else { BTN_FACE },
        );
        self.text_sh(glyph, [x + 2.0, y + 1.0], BTN_INK);
        self.widgets.push(Widget { rect, action });
    }
}

impl Menu {
    pub fn open(&mut self, opened_by: Option<char>) {
        self.open = true;
        self.screen = Screen::Main;
        self.skip_char = opened_by;
        self.message = None;
    }

    pub fn reset(&mut self) {
        self.open = false;
        self.screen = Screen::Main;
        self.focus = None;
        self.joining = None;
        self.creating = false;
        self.join_password.clear();
        self.dropdown = None;
        self.threshold_drag = false;
        self.skip_char = None;
        self.widgets.clear();
    }

    pub fn close(&mut self) {
        self.reset();
        self.close_requested = true;
    }

    pub fn take_close(&mut self) -> bool {
        std::mem::take(&mut self.close_requested)
    }

    /// The game's screen is (`ours`) or isn't the menu: forget a menu the
    /// game closed.
    pub fn sync_open(&mut self, ours: bool) {
        if !ours && self.open {
            self.reset();
        }
    }

    /// The action under the pointer, from the last frame drawn.
    pub fn hit(&self, at: [f32; 2]) -> Option<Action> {
        self.widgets
            .iter()
            .rev()
            .find(|w| inside(w.rect, at))
            .map(|w| match &w.action {
                Action::ThresholdAt(_) => Action::ThresholdAt(threshold_fraction(w.rect, at)),
                other => other.clone(),
            })
    }

    /// Where the widget doing `action` was drawn last.
    #[cfg(test)]
    pub fn hit_rect(&self, action: &Action) -> Option<[f32; 4]> {
        self.widgets
            .iter()
            .find(|w| &w.action == action)
            .map(|w| w.rect)
    }

    pub fn drag_threshold(&self, at: [f32; 2]) -> Option<Action> {
        if !self.threshold_drag {
            return None;
        }
        let rect = self
            .widgets
            .iter()
            .find(|w| matches!(w.action, Action::ThresholdAt(_)))?
            .rect;
        Some(Action::ThresholdAt(threshold_fraction(rect, at)))
    }

    pub fn end_threshold_drag(&mut self) -> bool {
        std::mem::take(&mut self.threshold_drag)
    }

    fn field_mut(&mut self, f: Field) -> &mut String {
        match f {
            Field::Name => &mut self.name,
            Field::Password => &mut self.password,
            Field::JoinPassword => &mut self.join_password,
        }
    }

    pub fn char_typed(&mut self, c: char) {
        if let Some(skip) = self.skip_char.take()
            && skip.eq_ignore_ascii_case(&c)
        {
            return;
        }
        let Some(f) = self.focus else { return };
        let max = if f == Field::Name {
            NAME_MAX
        } else {
            PASSWORD_MAX
        };
        let field = self.field_mut(f);
        if !c.is_control() && field.chars().count() < max && !(f != Field::Name && c == ' ') {
            field.push(c);
        }
    }

    /// Editing keys. Returns the action Enter asks for, if any.
    pub fn key(&mut self, key: &str, shift: bool) -> Option<Action> {
        self.skip_char = None;
        match key {
            "key.keyboard.escape" => {
                if self.dropdown.is_some() {
                    self.dropdown = None;
                    return None;
                }
                if self.joining.is_some() {
                    return Some(Action::CancelJoin);
                }
                if self.focus.is_some() {
                    self.focus = None;
                    return None;
                }
                if self.creating {
                    self.creating = false;
                    return None;
                }
                // Escape on a sub-screen steps back to the main menu.
                if self.screen != Screen::Main {
                    self.screen = Screen::Main;
                    return None;
                }
                return Some(Action::Close);
            }
            "key.keyboard.backspace" => {
                if let Some(f) = self.focus {
                    self.field_mut(f).pop();
                }
            }
            "key.keyboard.tab" => {
                self.focus = match (self.focus, shift) {
                    (Some(Field::JoinPassword), _) => Some(Field::JoinPassword),
                    (Some(Field::Name), false) | (Some(Field::Password), true) => {
                        Some(Field::Password)
                    }
                    _ => Some(Field::Name),
                };
                if self.joining.is_some() {
                    self.focus = Some(Field::JoinPassword);
                }
            }
            "key.keyboard.enter" | "key.keyboard.keypad.enter" => match self.focus {
                Some(Field::JoinPassword) => return Some(Action::JoinWithPassword),
                Some(_) => return Some(Action::Create),
                None => {}
            },
            "key.keyboard.up" => self.scroll = self.scroll.saturating_sub(1),
            "key.keyboard.down" => self.scroll += 1,
            _ => {}
        }
        None
    }

    /// Whether typing goes into a field, so other keys (the menu key) type.
    pub fn typing(&self) -> bool {
        self.focus.is_some()
    }

    pub fn scroll_by(&mut self, steps: i32) {
        self.scroll = (self.scroll as i32 - steps).max(0) as usize;
    }

    pub fn draw(
        &mut self,
        view: &View,
        size: [f32; 2],
        mouse: [f32; 2],
        measure: &dyn Fn(&str) -> f32,
    ) -> Vec<Draw> {
        let mut out = Out {
            draws: Vec::new(),
            widgets: Vec::new(),
            mouse,
            measure,
            tip: None,
        };
        out.fill([0.0, 0.0, size[0], size[1]], 0x8000_0000);
        match self.screen {
            Screen::Main => self.draw_main(&mut out, view, size),
            Screen::Settings | Screen::Groups | Screen::Volumes => {
                self.draw_sub(&mut out, view, size)
            }
        }
        if let Some(tip) = out.tip.take() {
            draw_tooltip(&mut out, &tip, size);
        }
        self.widgets = out.widgets;
        out.draws
    }

    fn draw_main(&self, out: &mut Out<'_>, view: &View, size: [f32; 2]) {
        let (w, h) = MAIN_SIZE;
        let x0 = ((size[0] - w) / 2.0).round();
        let y0 = ((size[1] - h) / 2.0).round();
        out.panel_main([x0, y0, w, h]);

        let title = tr("voice_chat");
        out.text(
            &title,
            [x0 + (w - out.width(&title)) / 2.0, y0 + 8.0],
            TITLE_INK,
        );

        let (status, rgb) = &view.status;
        let st = out.fit(status, w - 16.0);
        out.text(&st, [x0 + (w - out.width(&st)) / 2.0, y0 + 22.0], ink(*rgb));

        let s = &view.settings;
        let in_group = view.groups.iter().any(|g| g.ours);
        let n = 6.0;
        let gap = 4.0;
        let row_w = n * 24.0 + (n - 1.0) * gap;
        let sx = x0 + (w - row_w) / 2.0;
        let iy = y0 + 38.0;
        let at = |i: f32| sx + i * (24.0 + gap);
        out.icon_square(
            at(0.0),
            iy,
            if s.muted { IC_MIC_OFF } else { IC_MIC_ON },
            Action::Mute,
            !s.muted,
            &tr(if s.muted { "tip_mic_off" } else { "tip_mic_on" }),
        );
        out.icon_square(
            at(1.0),
            iy,
            if s.disabled { IC_SPK_OFF } else { IC_SPK_ON },
            Action::Disable,
            !s.disabled,
            &tr(if s.disabled {
                "tip_voice_off"
            } else {
                "tip_voice_on"
            }),
        );
        let voice = s.mode == Mode::Voice;
        out.icon_square(
            at(2.0),
            iy,
            if voice { IC_VOICE } else { IC_PTT },
            Action::Mode,
            true,
            &tr(if voice {
                "tip_mode_voice"
            } else {
                "tip_mode_ptt"
            }),
        );
        out.icon_square(
            at(3.0),
            iy,
            IC_VOLUMES,
            Action::Open(Screen::Volumes),
            true,
            &tr("player_volumes"),
        );
        out.icon_square(
            at(4.0),
            iy,
            IC_SETTINGS,
            Action::Open(Screen::Settings),
            true,
            &tr("settings"),
        );
        out.icon_square(
            at(5.0),
            iy,
            IC_GROUPS,
            Action::Open(Screen::Groups),
            in_group,
            &tr("groups"),
        );
    }

    fn draw_sub(&mut self, out: &mut Out<'_>, view: &View, size: [f32; 2]) {
        let settings = self.screen == Screen::Settings;
        let compact = settings && size[1] < 270.0;
        let narrow = matches!(self.screen, Screen::Settings | Screen::Volumes);
        let w = (size[0] - 16.0)
            .min(if narrow { 320.0 } else { 340.0 })
            .max(if narrow { 220.0 } else { 240.0 });
        let h = if settings {
            if compact { 235.0 } else { 263.0 }
        } else {
            (size[1] - 16.0).clamp(216.0, 240.0)
        };
        let x0 = ((size[0] - w) / 2.0).round();
        let y0 = ((size[1] - h) / 2.0).round();
        out.panel_sub([x0, y0, w, h]);

        let pad = 10.0;
        let title = tr(match self.screen {
            Screen::Settings => "settings",
            Screen::Groups => "groups",
            Screen::Volumes => "player_volumes",
            Screen::Main => "voice_chat",
        });
        out.heading(&title, x0 + pad, y0 + 7.0);
        out.button(
            [x0 + w - pad - 48.0, y0 + 5.0, 48.0, 14.0],
            &tr("back"),
            Action::Back,
            None,
        );

        let cx = x0 + pad;
        let cw = w - pad * 2.0;
        out.fill([cx, y0 + 20.0, cw, 1.0], CARD_LINE);
        out.fill([cx, y0 + 21.0, cw, 1.0], CARD_LIGHT);
        match self.screen {
            Screen::Settings => {
                self.draw_controls(
                    out,
                    view,
                    cx,
                    y0 + if compact { 22.0 } else { 23.0 },
                    cw,
                    compact,
                );
                self.draw_dropdown(out, view);
            }
            Screen::Groups => self.draw_groups(out, view, cx, y0 + 24.0, cw, y0 + h - 8.0),
            Screen::Volumes => self.draw_volumes(out, view, cx, y0 + 24.0, cw, y0 + h - 8.0),
            Screen::Main => {}
        }
    }

    fn draw_controls(
        &mut self,
        out: &mut Out<'_>,
        view: &View,
        x: f32,
        top: f32,
        w: f32,
        compact: bool,
    ) {
        let s = &view.settings;
        let (devices_h, levels_h, hotkeys_h, gap) = if compact {
            (53.0, 76.0, 71.0, 5.0)
        } else {
            (55.0, 80.0, 93.0, 3.0)
        };
        let ix = x + 8.0;
        let iw = w - 16.0;
        let mut y = top;

        out.settings_card([x, y, w, devices_h], &tr("devices"));
        let input_y = y + if compact { 20.0 } else { 21.0 };
        let input = s
            .input_device
            .clone()
            .unwrap_or_else(|| tr("default_device"));
        out.device_picker(
            ix,
            input_y,
            iw,
            &tr("microphone"),
            &input,
            Action::OpenDrop { input: true },
        );
        self.drop_anchor_in = out.widgets.last().map(|wd| wd.rect);
        let output_y = y + if compact { 36.0 } else { 38.0 };
        let output = s
            .output_device
            .clone()
            .unwrap_or_else(|| tr("default_device"));
        out.device_picker(
            ix,
            output_y,
            iw,
            &tr("speakers"),
            &output,
            Action::OpenDrop { input: false },
        );
        self.drop_anchor_out = out.widgets.last().map(|wd| wd.rect);
        y += devices_h + gap;

        out.settings_card([x, y, w, levels_h], &tr("levels"));
        let volume_y = y + if compact { 20.0 } else { 21.0 };
        out.stepper(
            [ix, volume_y],
            iw,
            &tr("voice_volume"),
            &format!("{}%", s.volume),
            Action::Volume(-25),
            Action::Volume(25),
        );
        let gain_y = y + if compact { 34.0 } else { 36.0 };
        out.stepper(
            [ix, gain_y],
            iw,
            &tr("mic_gain"),
            &format!("{}%", s.mic_gain),
            Action::Gain(-25),
            Action::Gain(25),
        );
        let activation_y = y + if compact { 49.0 } else { 52.0 };
        out.text(&tr("activation_level"), [ix, activation_y], INK);
        let value = format!("{} dB", s.threshold);
        out.text(
            &value,
            [ix + iw - out.width(&value), activation_y],
            INK_SOFT,
        );
        let meter_y = y + if compact { 59.0 } else { 63.0 };
        out.vu_meter(
            ix,
            meter_y,
            iw,
            view.level_db,
            s.threshold,
            self.threshold_drag,
        );
        y += levels_h + gap;

        out.settings_card([x, y, w, hotkeys_h], &tr("hotkeys"));
        if compact {
            let column_gap = 6.0;
            let column_w = (iw - column_gap) / 2.0;
            for (i, (what, key)) in view.hotkeys.iter().enumerate() {
                let column = i / 3;
                let row = i % 3;
                let rx = ix + column as f32 * (column_w + column_gap);
                let ry = y + 21.0 + row as f32 * 12.0;
                let bound = *key != tr("not_bound") && key != "not bound";
                let key_w = out.key_cap(rx + column_w, ry, column_w * 0.55, key, bound);
                out.text(
                    &out.fit(&hotkey_desc(what), column_w - key_w - 6.0),
                    [rx, ry],
                    INK_SOFT,
                );
            }
        } else {
            for (i, (what, key)) in view.hotkeys.iter().enumerate() {
                let ry = y + 21.0 + i as f32 * 12.0;
                let bound = *key != tr("not_bound") && key != "not bound";
                let key_w = out.key_cap(ix + iw, ry, 90.0, key, bound);
                out.text(
                    &out.fit(&hotkey_desc(what), iw - key_w - 8.0),
                    [ix, ry],
                    INK_SOFT,
                );
            }
        }
        let hint_y = y + if compact { 60.0 } else { 82.0 };
        out.text(&out.fit(&tr("change_keys"), iw), [ix, hint_y], INK_FAINT);
    }

    /// The open device list: a dark popup under its picker with the OS
    /// default and the devices, the current one marked, scrolling when there
    /// are many.
    fn draw_dropdown(&self, out: &mut Out<'_>, view: &View) {
        let Some(dd) = self.dropdown else { return };
        let anchor = if dd.input {
            self.drop_anchor_in
        } else {
            self.drop_anchor_out
        };
        let Some([ax, ay, aw, _]) = anchor else {
            return;
        };
        let mut entries: Vec<Option<String>> = vec![None];
        entries.extend(
            (if dd.input {
                &view.devices.0
            } else {
                &view.devices.1
            })
            .iter()
            .cloned()
            .map(Some),
        );
        let current = if dd.input {
            view.settings.input_device.as_deref()
        } else {
            view.settings.output_device.as_deref()
        };
        const VISIBLE: usize = 6;
        let scroll = dd.scroll.min(entries.len().saturating_sub(VISIBLE));
        let rows = entries.len().min(VISIBLE);
        let h = rows as f32 * 14.0 + 2.0;
        let (x, y, w) = (ax, ay + 14.0, aw);
        out.fill([x - 1.0, y - 1.0, w + 2.0, h + 2.0], 0xFF10_1014);
        out.fill([x, y, w, h], 0xF01E_1E22);
        for (i, name) in entries.iter().skip(scroll).take(VISIBLE).enumerate() {
            let ry = y + 1.0 + i as f32 * 14.0;
            let row = [x, ry, w - 10.0, 14.0];
            let label = name.clone().unwrap_or_else(|| tr("default_device"));
            let is_current = match name {
                None => current.is_none(),
                Some(n) => current == Some(n.as_str()),
            };
            if inside(row, out.mouse) {
                out.fill(row, 0xFF3E_3E46);
            }
            if is_current {
                out.fill([x, ry, 2.0, 14.0], 0xFF4A_B64E);
            }
            out.text_sh(&out.fit(&label, w - 22.0), [x + 4.0, ry + 3.0], 0xFFFFFFFF);
            out.widgets.push(Widget {
                rect: row,
                action: Action::PickDevice {
                    input: dd.input,
                    name: name.clone(),
                },
            });
        }
        if entries.len() > VISIBLE {
            out.drop_arrow(x + w - 10.0, y + 1.0, "\u{25B2}", Action::DropScroll(-1));
            out.drop_arrow(
                x + w - 10.0,
                y + h - 10.0,
                "\u{25BC}",
                Action::DropScroll(1),
            );
        }
    }

    fn draw_volumes(&self, out: &mut Out<'_>, view: &View, x: f32, top: f32, w: f32, bottom: f32) {
        out.text(&out.fit(&tr("volumes_hint"), w), [x, top + 2.0], INK_SOFT);
        let y = top + 14.0;
        let list_h = bottom - y;
        out.fill([x, y, w, list_h], 0xFFA0_A0A0);
        out.fill([x + 1.0, y + 1.0, w - 2.0, list_h - 2.0], 0xF018_181C);
        let fits = (((list_h - 2.0) / 26.0).floor().max(1.0)) as usize;
        let scroll = self
            .scroll
            .min(view.volume_sources.len().saturating_sub(fits));
        if view.volume_sources.is_empty() {
            out.sprite(
                IC_VOLUMES,
                [0.0, 0.0, 16.0, 16.0],
                [x + (w - 24.0) / 2.0, y + list_h / 2.0 - 22.0, 24.0, 24.0],
            );
            let empty = out.fit(&tr("no_audio_sources"), w - 16.0);
            out.text_sh(
                &empty,
                [x + (w - out.width(&empty)) / 2.0, y + list_h / 2.0 + 10.0],
                0xAAAAAA,
            );
        }
        for (i, source) in view
            .volume_sources
            .iter()
            .skip(scroll)
            .take(fits)
            .enumerate()
        {
            let ry = y + 1.0 + i as f32 * 26.0;
            let row = [x + 1.0, ry, w - 2.0, 26.0];
            if inside(row, out.mouse) {
                out.fill(row, 0x303A_3A44);
            }
            out.fill([x + 1.0, ry + 25.0, w - 2.0, 1.0], 0x20FF_FFFF);
            out.sprite(
                IC_PLAYER_VOLUME,
                [0.0, 0.0, 16.0, 16.0],
                [x + 7.0, ry + 7.0, 12.0, 12.0],
            );
            out.text_sh(
                &out.fit(&source.name, w - 108.0),
                [x + 25.0, ry + 9.0],
                0xFFFFFF,
            );
            let right = x + w - 6.0;
            out.button(
                [right - 13.0, ry + 7.0, 13.0, 12.0],
                "+",
                Action::PlayerVolume(source.uuid, 10),
                None,
            );
            let value = format!("{}%", source.volume);
            let vw = 38.0;
            let vx = right - 13.0 - vw;
            let tw = out.width(&value);
            out.text_sh(&value, [vx + (vw - tw) / 2.0, ry + 9.0], 0xE0E0E0);
            out.button(
                [vx - 15.0, ry + 7.0, 13.0, 12.0],
                "-",
                Action::PlayerVolume(source.uuid, -10),
                None,
            );
        }
    }

    fn draw_groups(&self, out: &mut Out<'_>, view: &View, x: f32, top: f32, w: f32, bottom: f32) {
        if self.creating {
            self.draw_group_create(out, x, top, w, bottom);
            return;
        }
        if !view.connected || !view.groups_enabled {
            let why = if view.connected {
                tr("groups_off_server")
            } else {
                tr("not_connected")
            };
            out.fill([x, top, w, 50.0], CARD_OUT);
            out.fill([x + 1.0, top + 1.0, w - 2.0, 48.0], CARD_BG);
            out.sprite(
                IC_GROUPS,
                [0.0, 0.0, 16.0, 16.0],
                [x + 10.0, top + 17.0, 16.0, 16.0],
            );
            out.text(&out.fit(&why, w - 48.0), [x + 34.0, top + 21.0], INK_SOFT);
            return;
        }

        let mut y = top;
        if let Some(ours) = view.groups.iter().find(|g| g.ours) {
            out.fill([x, y, w, 30.0], CARD_OUT);
            out.fill([x + 1.0, y + 1.0, w - 2.0, 28.0], CARD_BG);
            out.sprite(
                IC_GROUPS,
                [0.0, 0.0, 16.0, 16.0],
                [x + 7.0, y + 7.0, 16.0, 16.0],
            );
            let label = out.fit(&fmt("in_group", &[&ours.name]), w - 94.0);
            out.text(&label, [x + 29.0, y + 4.0], ink(0x55FF55));
            let members = if ours.members.is_empty() {
                tr("nobody")
            } else {
                ours.members.join(", ")
            };
            out.text(&out.fit(&members, w - 94.0), [x + 29.0, y + 15.0], INK_SOFT);
            out.icon_square(
                x + w - 30.0,
                y + 3.0,
                IC_LEAVE,
                Action::Leave,
                true,
                &tr("leave"),
            );
            y += 36.0;
        }

        out.heading(&tr("available_groups"), x, y + 2.0);
        y += 14.0;

        let create_y = bottom - 16.0;
        let list_bottom = create_y - if self.message.is_some() { 14.0 } else { 4.0 };
        let list_h = (list_bottom - y).max(ROW + 2.0);
        out.fill([x, y, w, list_h], 0xFFA0_A0A0);
        out.fill([x + 1.0, y + 1.0, w - 2.0, list_h - 2.0], 0xF018_181C);
        let fits = (((list_h - 2.0) / ROW).floor().max(1.0)) as usize;
        let scroll = self.scroll.min(view.groups.len().saturating_sub(fits));

        if view.groups.is_empty() {
            out.sprite(
                IC_GROUPS,
                [0.0, 0.0, 16.0, 16.0],
                [x + (w - 24.0) / 2.0, y + list_h / 2.0 - 22.0, 24.0, 24.0],
            );
            let empty = out.fit(&tr("no_groups"), w - 16.0);
            out.text_sh(
                &empty,
                [x + (w - out.width(&empty)) / 2.0, y + list_h / 2.0 + 10.0],
                0xAAAAAA,
            );
        }

        for (i, g) in view.groups.iter().skip(scroll).take(fits).enumerate() {
            let ry = y + 1.0 + i as f32 * ROW;
            let row = [x + 1.0, ry, w - 2.0, ROW];
            if g.ours {
                out.fill(row, 0x403F_7F46);
            } else if inside(row, out.mouse) {
                out.fill(row, 0x303A_3A44);
            }
            out.fill([x + 1.0, ry + ROW - 1.0, w - 2.0, 1.0], 0x20FF_FFFF);
            out.sprite(
                if g.has_password { IC_LOCK } else { IC_GROUPS },
                [0.0, 0.0, 16.0, 16.0],
                [x + 7.0, ry + 7.0, 16.0, 16.0],
            );
            if self.joining == Some(g.id) {
                let title = out.fit(&fmt("password_for", &[&g.name]), w - 34.0);
                out.text_sh(&title, [x + 28.0, ry + 3.0], 0xFFFFFF);
                let fw = w - 138.0;
                out.field(
                    [x + 28.0, ry + 15.0, fw, 12.0],
                    &self.join_password,
                    &tr("password"),
                    true,
                    self.focus == Some(Field::JoinPassword),
                    Action::Focus(Field::JoinPassword),
                );
                out.button(
                    [x + w - 104.0, ry + 14.0, 62.0, 14.0],
                    &tr("cancel"),
                    Action::CancelJoin,
                    None,
                );
                out.icon_square(
                    x + w - 30.0,
                    ry + 3.0,
                    IC_JOIN,
                    Action::JoinWithPassword,
                    true,
                    &tr("join"),
                );
            } else {
                let name_colour = if g.ours { 0x55FF55 } else { 0xFFFFFF };
                out.text_sh(
                    &out.fit(&g.name, w - 88.0),
                    [x + 28.0, ry + 3.0],
                    name_colour,
                );
                let members = if g.members.is_empty() {
                    tr("nobody")
                } else {
                    g.members.join(", ")
                };
                out.text_sh(
                    &out.fit(&members, w - 88.0),
                    [x + 28.0, ry + 15.0],
                    0xAAAAAA,
                );
                if !g.ours {
                    out.icon_square(
                        x + w - 30.0,
                        ry + 3.0,
                        IC_JOIN,
                        Action::Join(g.id),
                        true,
                        &tr("join"),
                    );
                }
            }
        }

        if let Some((message, rgb)) = &self.message {
            out.text(&out.fit(message, w), [x, list_bottom + 4.0], ink(*rgb));
        }
        out.button(
            [x, create_y, w, 16.0],
            &tr("new_group"),
            Action::ShowCreate,
            None,
        );
    }

    fn draw_group_create(&self, out: &mut Out<'_>, x: f32, top: f32, w: f32, bottom: f32) {
        out.settings_card([x, top, w, 148.0], &tr("new_group"));
        let ix = x + 8.0;
        let iw = w - 16.0;
        out.text(&tr("name"), [ix, top + 24.0], INK_SOFT);
        out.field(
            [ix, top + 34.0, iw, 16.0],
            &self.name,
            &tr("group_name_hint"),
            false,
            self.focus == Some(Field::Name),
            Action::Focus(Field::Name),
        );
        out.text(&tr("password_optional"), [ix, top + 60.0], INK_SOFT);
        out.field(
            [ix, top + 70.0, iw, 16.0],
            &self.password,
            &tr("group_password_hint"),
            true,
            self.focus == Some(Field::Password),
            Action::Focus(Field::Password),
        );
        out.text(&tr("group_type"), [ix, top + 96.0], INK_SOFT);
        let kind_index = self.kind as usize % 3;
        let kind = tr(KIND_KEYS[kind_index]);
        out.button(
            [ix, top + 106.0, iw, 16.0],
            &format!("\u{2039} {} \u{203A}", fmt("type_label", &[&kind])),
            Action::Kind,
            None,
        );
        out.text(
            &out.fit(&tr(KIND_DESCRIPTION_KEYS[kind_index]), iw),
            [ix, top + 128.0],
            INK_SOFT,
        );
        if let Some((message, rgb)) = &self.message {
            out.text(&out.fit(message, w), [x, top + 158.0], ink(*rgb));
        }
        let half = (w - 6.0) / 2.0;
        out.button(
            [x, bottom - 16.0, half, 16.0],
            &tr("cancel"),
            Action::CancelCreate,
            None,
        );
        out.button(
            [x + half + 6.0, bottom - 16.0, half, 16.0],
            &tr("create"),
            Action::Create,
            None,
        );
    }

    /// What a click on the menu's own parts does; the rest is the voice
    /// chat's to do (returned).
    pub fn act(&mut self, action: Action) -> Option<Action> {
        // Anything but the device list's own parts closes an open list.
        if self.dropdown.is_some()
            && !matches!(action, Action::OpenDrop { .. } | Action::DropScroll(_))
        {
            self.dropdown = None;
        }
        match action {
            Action::Focus(f) => self.focus = Some(f),
            Action::ThresholdAt(frac) => {
                self.threshold_drag = true;
                return Some(Action::ThresholdAt(frac));
            }
            Action::Kind => self.kind = (self.kind + 1) % 3,
            Action::ShowCreate => {
                self.creating = true;
                self.focus = Some(Field::Name);
                self.message = None;
                self.scroll = 0;
            }
            Action::CancelCreate => {
                self.creating = false;
                self.focus = None;
                self.message = None;
            }
            Action::OpenDrop { input } => {
                self.dropdown = match self.dropdown {
                    Some(dd) if dd.input == input => None,
                    _ => Some(Dropdown { input, scroll: 0 }),
                };
            }
            Action::DropScroll(d) => {
                if let Some(dd) = &mut self.dropdown {
                    dd.scroll = (dd.scroll as i64 + d as i64).max(0) as usize;
                }
            }
            Action::CancelJoin => {
                self.joining = None;
                self.join_password.clear();
                self.focus = None;
            }
            // Navigation between the menu's screens stays inside the menu.
            Action::Open(screen) => {
                self.screen = screen;
                self.focus = None;
                self.creating = false;
                self.message = None;
                self.dropdown = None;
                self.scroll = 0;
            }
            Action::Back => {
                if self.creating {
                    self.creating = false;
                } else {
                    self.screen = Screen::Main;
                }
                self.focus = None;
                self.dropdown = None;
            }
            Action::Close => self.close(),
            other => {
                if !matches!(other, Action::JoinWithPassword | Action::Create) {
                    self.focus = None;
                }
                return Some(other);
            }
        }
        None
    }

    pub fn say(&mut self, text: impl Into<String>, error: bool) {
        self.message = Some((text.into(), if error { 0xFF5555 } else { 0x55FF55 }));
    }
}

/// A tooltip box drawn on top near the pointer, kept on screen.
fn draw_tooltip(out: &mut Out<'_>, text: &str, size: [f32; 2]) {
    let tw = out.width(text);
    let w = tw + 8.0;
    let h = 14.0;
    let mut x = out.mouse[0] + 12.0;
    let mut y = out.mouse[1] - 6.0;
    if x + w > size[0] {
        x = (out.mouse[0] - w - 6.0).max(2.0);
    }
    y = y.clamp(2.0, (size[1] - h - 2.0).max(2.0));
    out.fill([x - 1.0, y - 1.0, w + 2.0, h + 2.0], 0xF02B_2B36);
    out.fill([x, y, w, h], 0xF01A_1A20);
    out.text_sh(text, [x + 4.0, y + 3.0], 0xFFFFFF);
}

/// The localised description corresponding to a label in `HOTKEYS`.
fn hotkey_desc(what: &str) -> String {
    let key = match what {
        "Open this menu" => "hk_menu",
        "Push to talk" => "hk_ptt",
        "Whisper" => "hk_whisper",
        "Mute microphone" => "hk_mute",
        "Voice chat off / on" => "hk_disable",
        other => return other.to_owned(),
    };
    tr(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view() -> View {
        View {
            status: ("Connected".into(), 0x55FF55),
            settings: Settings::default(),
            level_db: -30.0,
            connected: true,
            groups_enabled: true,
            groups: vec![
                GroupRow {
                    id: 1,
                    name: "Builders".into(),
                    has_password: false,
                    members: vec!["Alex".into()],
                    ours: false,
                },
                GroupRow {
                    id: 2,
                    name: "Secret".into(),
                    has_password: true,
                    members: vec![],
                    ours: false,
                },
            ],
            hotkeys: vec![
                ("Open this menu".into(), "V".into()),
                ("Push to talk".into(), "Caps Lock".into()),
                ("Whisper".into(), "not bound".into()),
                ("Mute microphone".into(), "J".into()),
                ("Voice chat off / on".into(), "K".into()),
            ],
            devices: (vec!["USB Mic".into()], vec!["HDMI Out".into()]),
            volume_sources: vec![
                VolumeRow {
                    uuid: 10,
                    name: "Alex".into(),
                    volume: 100,
                },
                VolumeRow {
                    uuid: 11,
                    name: "Audio player · 000000ff".into(),
                    volume: 70,
                },
            ],
        }
    }

    fn texts(draws: &[Draw]) -> Vec<String> {
        draws
            .iter()
            .filter_map(|d| {
                if let Draw::Text(t, ..) = d {
                    Some(t.clone())
                } else {
                    None
                }
            })
            .collect()
    }

    fn centre(m: &Menu, action: &Action) -> [f32; 2] {
        let w = m.widgets.iter().find(|w| &w.action == action).unwrap();
        [w.rect[0] + w.rect[2] / 2.0, w.rect[1] + w.rect[3] / 2.0]
    }

    #[test]
    fn main_menu_and_sub_screens() {
        crate::i18n::set_language("en_us");
        let mut m = Menu::default();
        m.open(Some('v'));
        let measure = |t: &str| t.len() as f32 * 6.0;

        // Main menu: the designer's panel sprite, the title, the Settings /
        // Groups icon buttons, and no "Settings • Groups" hint line.
        let main = m.draw(&view(), [480.0, 270.0], [0.0, 0.0], &measure);
        assert!(
            main.iter()
                .any(|d| matches!(d, Draw::Sprite { name, .. } if *name == PANEL_MAIN.0))
        );
        assert!(texts(&main).iter().any(|x| x == "Voice Chat"));
        assert!(
            m.widgets
                .iter()
                .any(|w| w.action == Action::Open(Screen::Settings))
        );
        assert!(
            m.widgets
                .iter()
                .any(|w| w.action == Action::Open(Screen::Groups))
        );
        assert!(
            m.widgets
                .iter()
                .any(|w| w.action == Action::Open(Screen::Volumes))
        );
        assert!(
            !texts(&main).iter().any(|x| x.contains('\u{2022}')),
            "hint line must be gone: {:?}",
            texts(&main)
        );

        // Settings screen: the sub panel, the device sections, the VU strip
        // and the hotkeys. The strip reports the pointer's handle position.
        m.act(Action::Open(Screen::Settings));
        let sd = m.draw(&view(), [480.0, 270.0], [0.0, 0.0], &measure);
        assert!(
            sd.iter()
                .any(|d| matches!(d, Draw::Sprite { name, .. } if *name == PANEL_SUB.0))
        );
        let s = texts(&sd);
        for want in [
            "Devices",
            "Levels",
            "Microphone",
            "Speakers",
            "Default",
            "Hotkeys",
            "Push to talk",
            "Caps Lock",
            "not bound",
        ] {
            assert!(s.iter().any(|x| x == want), "{want} missing from {s:?}");
        }
        assert!(
            m.widgets
                .iter()
                .any(|w| w.action == Action::OpenDrop { input: true })
        );
        let track = m.hit_rect(&Action::ThresholdAt(0.0)).unwrap();
        assert_eq!(
            m.hit([track[0], track[1] + 5.0]),
            Some(Action::ThresholdAt(0.0))
        );
        assert_eq!(
            m.hit([track[0] + track[2] / 2.0, track[1] + 5.0]),
            Some(Action::ThresholdAt(0.5))
        );
        assert_eq!(
            m.hit([track[0] + track[2] - 1.0, track[1] + 5.0]),
            Some(Action::ThresholdAt(1.0))
        );

        // Groups screen: the groups, their members, and a Join hit area.
        m.act(Action::Open(Screen::Groups));
        let g = texts(&m.draw(&view(), [480.0, 270.0], [0.0, 0.0], &measure));
        for want in ["Builders", "Alex", "Secret"] {
            assert!(g.iter().any(|x| x == want), "{want} missing from {g:?}");
        }
        let join = centre(&m, &Action::Join(1));
        assert_eq!(m.hit(join), Some(Action::Join(1)));

        m.act(Action::Open(Screen::Volumes));
        let volumes = texts(&m.draw(&view(), [480.0, 270.0], [0.0, 0.0], &measure));
        for want in [
            "Player volumes",
            "Alex",
            "100%",
            "Audio player · 000000ff",
            "70%",
        ] {
            assert!(
                volumes.iter().any(|x| x == want),
                "{want} missing from {volumes:?}"
            );
        }
        let louder = Action::PlayerVolume(10, 10);
        assert_eq!(m.hit(centre(&m, &louder)), Some(louder));
        let mut many = view();
        for i in 2..14 {
            many.volume_sources.push(VolumeRow {
                uuid: i,
                name: format!("Source {i}"),
                volume: 100,
            });
        }
        m.scroll_by(-20);
        let end = texts(&m.draw(&many, [480.0, 270.0], [0.0, 0.0], &measure));
        assert!(
            end.iter().any(|t| t == "Source 13"),
            "volume list did not reach its end: {end:?}"
        );

        // Escape steps back to the main menu, then closes.
        assert_eq!(m.key("key.keyboard.escape", false), None);
        assert_eq!(m.screen, Screen::Main);
        assert_eq!(m.key("key.keyboard.escape", false), Some(Action::Close));
    }

    #[test]
    fn settings_cards_fit_regular_and_compact_screens() {
        crate::i18n::set_language("en_us");
        let measure = |t: &str| t.len() as f32 * 6.0;
        for (screen_h, panel_h, gap) in [(270.0, 263.0, 3.0), (240.0, 235.0, 5.0)] {
            let mut m = Menu::default();
            m.open(Some('v'));
            m.screen = Screen::Settings;
            let draws = m.draw(&view(), [480.0, screen_h], [0.0, 0.0], &measure);
            let mut cards: Vec<[f32; 4]> = draws
                .iter()
                .filter_map(|d| match d {
                    Draw::Fill(rect, colour) if *colour == CARD_OUT && rect[3] > 40.0 => {
                        Some(*rect)
                    }
                    _ => None,
                })
                .collect();
            cards.sort_by(|a, b| a[1].total_cmp(&b[1]));
            assert_eq!(cards.len(), 3);
            let panel_top = (screen_h - panel_h) / 2.0;
            let panel_bottom = panel_top + panel_h;
            assert!(cards[0][1] >= panel_top + 22.0);
            assert!(cards[2][1] + cards[2][3] <= panel_bottom);
            for pair in cards.windows(2) {
                assert_eq!(pair[1][1] - pair[0][1] - pair[0][3], gap);
            }
            for widget in &m.widgets {
                assert!(
                    widget.rect[1] >= panel_top && widget.rect[1] + widget.rect[3] <= panel_bottom
                );
            }
        }
    }

    #[test]
    fn group_list_and_create_form_fit_and_navigate() {
        crate::i18n::set_language("en_us");
        let mut m = Menu::default();
        m.open(Some('v'));
        m.screen = Screen::Groups;
        let measure = |t: &str| t.len() as f32 * 6.0;
        let mut listed = view();
        listed.groups.push(GroupRow {
            id: 100,
            name: "Alpha".into(),
            has_password: false,
            members: vec!["Me".into()],
            ours: true,
        });
        let list = m.draw(&listed, [480.0, 270.0], [0.0, 0.0], &measure);
        assert!(m.widgets.iter().any(|w| w.action == Action::ShowCreate));
        assert!(texts(&list).iter().any(|t| t == "Available groups"));
        assert!(
            list.iter()
                .any(|d| matches!(d, Draw::Sprite { name, .. } if *name == IC_JOIN))
        );
        assert!(
            list.iter()
                .any(|d| matches!(d, Draw::Sprite { name, .. } if *name == IC_LEAVE))
        );
        assert!(!texts(&list).iter().any(|t| t == "Join" || t == "Leave"));
        for widget in &m.widgets {
            assert!(widget.rect[1] >= 15.0 && widget.rect[1] + widget.rect[3] <= 255.0);
        }

        let mut many = listed;
        for i in 3..12 {
            many.groups.push(GroupRow {
                id: i,
                name: format!("Group {i}"),
                has_password: false,
                members: Vec::new(),
                ours: false,
            });
        }
        m.scroll_by(-20);
        let end = texts(&m.draw(&many, [480.0, 270.0], [0.0, 0.0], &measure));
        assert!(
            end.iter().any(|t| t == "Group 11"),
            "continuous scroll did not reach the end: {end:?}"
        );
        assert!(!end.iter().any(|t| t.contains(" of ")));

        m.act(Action::ShowCreate);
        assert!(m.creating && m.focus == Some(Field::Name));
        let form = m.draw(&view(), [480.0, 270.0], [0.0, 0.0], &measure);
        let form_text = texts(&form);
        for want in [
            "New group",
            "Group type",
            "name",
            "password (optional)",
            "No password",
            "Hear nearby players; they can't hear the group.",
        ] {
            assert!(
                form_text.iter().any(|t| t == want),
                "{want} missing from {form_text:?}"
            );
        }
        assert!(m.widgets.iter().any(|w| w.action == Action::Create));
        m.act(Action::Kind);
        let open = texts(&m.draw(&view(), [480.0, 270.0], [0.0, 0.0], &measure));
        assert!(
            open.iter()
                .any(|t| t == "Nearby players and the group hear each other.")
        );
        m.act(Action::Kind);
        let isolated = texts(&m.draw(&view(), [480.0, 270.0], [0.0, 0.0], &measure));
        assert!(
            isolated
                .iter()
                .any(|t| t == "Only group members hear each other.")
        );
        m.act(Action::Back);
        assert!(!m.creating && m.screen == Screen::Groups);
        m.act(Action::ShowCreate);
        m.act(Action::CancelCreate);
        assert!(!m.creating && m.focus.is_none());
    }

    #[test]
    fn vu_meter_layers_full_track_veil_and_vanilla_handle() {
        crate::i18n::set_language("en_us");
        let mut m = Menu::default();
        m.open(Some('v'));
        m.screen = Screen::Settings;
        let measure = |t: &str| t.len() as f32 * 6.0;
        let draws = m.draw(&view(), [480.0, 270.0], [0.0, 0.0], &measure);
        let (track_index, src, dst) = draws
            .iter()
            .enumerate()
            .find_map(|(i, d)| match d {
                Draw::Sprite { name, src, dst } if *name == TRACK => Some((i, *src, *dst)),
                _ => None,
            })
            .unwrap();
        assert_eq!(src, [0.0, 0.0, 226.0, 18.0]);
        let (veil_index, veil) = draws
            .iter()
            .enumerate()
            .find_map(|(i, d)| match d {
                Draw::Fill(rect, colour) if *colour == 0x8010_1014 => Some((i, *rect)),
                _ => None,
            })
            .unwrap();
        assert!(track_index < veil_index);
        assert!(veil[0] > dst[0] + dst[2] / 2.0 && veil[2] < dst[2]);
        assert!(draws.iter().enumerate().any(|(i, d)| i > veil_index
            && matches!(d, Draw::Sprite { name, .. } if *name == SLIDER_HANDLE)));

        let track = m.hit_rect(&Action::ThresholdAt(0.0)).unwrap();
        let hover = [track[0] + track[2] / 2.0, track[1] + 4.0];
        let hovered = m.draw(&view(), [480.0, 270.0], hover, &measure);
        assert!(
            hovered
                .iter()
                .any(|d| matches!(d, Draw::Sprite { name, .. } if *name == SLIDER_HANDLE_HOVER))
        );

        let mut idle_view = view();
        idle_view.level_db = crate::net::IDLE_LEVEL_DB;
        let idle = m.draw(&idle_view, [480.0, 270.0], [0.0, 0.0], &measure);
        let track = idle
            .iter()
            .find_map(|d| match d {
                Draw::Sprite { name, dst, .. } if *name == TRACK => Some(*dst),
                _ => None,
            })
            .unwrap();
        let veil = idle
            .iter()
            .find_map(|d| match d {
                Draw::Fill(rect, colour) if *colour == 0x8010_1014 => Some(*rect),
                _ => None,
            })
            .unwrap();
        assert_eq!(veil, track);
    }

    #[test]
    fn device_dropdown_lists_and_picks() {
        crate::i18n::set_language("en_us");
        let mut m = Menu::default();
        m.open(Some('v'));
        m.screen = Screen::Settings;
        let measure = |t: &str| t.len() as f32 * 6.0;
        let v = view();
        m.draw(&v, [480.0, 270.0], [-1.0, -1.0], &measure);
        // Opening the microphone picker shows the default and the devices.
        assert_eq!(m.act(Action::OpenDrop { input: true }), None);
        assert!(m.dropdown.is_some());
        let d = texts(&m.draw(&v, [480.0, 270.0], [-1.0, -1.0], &measure));
        assert!(d.iter().any(|x| x == "USB Mic"), "{d:?}");
        // Picking a device closes the list and hands the choice to the session.
        let pick = Action::PickDevice {
            input: true,
            name: Some("USB Mic".into()),
        };
        assert_eq!(m.hit(centre(&m, &pick)), Some(pick.clone()));
        assert_eq!(m.act(pick.clone()), Some(pick));
        assert!(m.dropdown.is_none());
        // Escape closes the list before it leaves the screen.
        m.act(Action::OpenDrop { input: true });
        assert_eq!(m.key("key.keyboard.escape", false), None);
        assert!(m.dropdown.is_none());
        assert_eq!(m.screen, Screen::Settings);
    }

    #[test]
    fn hovering_an_icon_shows_a_tooltip() {
        crate::i18n::set_language("en_us");
        let mut m = Menu::default();
        m.open(Some('v'));
        let measure = |t: &str| t.len() as f32 * 6.0;
        // Draw once to lay out the widgets, find the mute button, hover it.
        m.draw(&view(), [480.0, 270.0], [-1.0, -1.0], &measure);
        let mute = centre(&m, &Action::Mute);
        let draws = m.draw(&view(), [480.0, 270.0], mute, &measure);
        assert!(
            texts(&draws).iter().any(|t| t.contains("Microphone")),
            "tooltip missing: {:?}",
            texts(&draws)
        );
    }

    #[test]
    fn localises_the_menu() {
        crate::i18n::set_language("ru_ru");
        let mut m = Menu::default();
        m.open(Some('v'));
        let measure = |t: &str| t.len() as f32 * 6.0;
        let main = texts(&m.draw(&view(), [480.0, 270.0], [0.0, 0.0], &measure));
        assert!(main.iter().any(|x| x == "Голосовой чат"), "{main:?}");
        crate::i18n::set_language("en_us");
    }

    /// Writes each screen's draw list to JSON in the scratchpad for a visual
    /// preview (`DUMP_MENU=<dir> cargo test dump_menu_json`). Off by default.
    #[test]
    fn dump_menu_json() {
        let Ok(dir) = std::env::var("DUMP_MENU") else {
            return;
        };
        crate::i18n::set_language(&std::env::var("DUMP_LANG").unwrap_or_else(|_| "en_us".into()));
        let measure = |t: &str| t.chars().count() as f32 * 6.0;
        let mut v = view();
        v.groups.push(GroupRow {
            id: 3,
            name: "Alpha".into(),
            has_password: false,
            members: vec!["Me".into(), "Kim".into()],
            ours: true,
        });
        let json = |d: &Draw| -> String {
            match d {
                Draw::Fill(r, a) => format!(
                    "{{\"t\":\"fill\",\"r\":[{},{},{},{}],\"c\":{}}}",
                    r[0], r[1], r[2], r[3], a
                ),
                Draw::Text(s, p, c, sh) => format!(
                    "{{\"t\":\"text\",\"s\":{:?},\"p\":[{},{}],\"c\":{},\"sh\":{}}}",
                    s, p[0], p[1], c, sh
                ),
                Draw::Sprite { name, src, dst } => format!(
                    "{{\"t\":\"sprite\",\"n\":{:?},\"src\":[{},{},{},{}],\"dst\":[{},{},{},{}]}}",
                    name, src[0], src[1], src[2], src[3], dst[0], dst[1], dst[2], dst[3]
                ),
            }
        };
        for (file, screen, mouse, size) in [
            ("main", Screen::Main, [180.0f32, 139.0f32], [480.0, 270.0]),
            ("settings", Screen::Settings, [0.0, 0.0], [480.0, 270.0]),
            (
                "settings_compact",
                Screen::Settings,
                [0.0, 0.0],
                [480.0, 240.0],
            ),
            ("groups", Screen::Groups, [0.0, 0.0], [480.0, 270.0]),
            ("groups_create", Screen::Groups, [0.0, 0.0], [480.0, 270.0]),
            ("volumes", Screen::Volumes, [0.0, 0.0], [480.0, 270.0]),
        ] {
            let mut m = Menu::default();
            m.open(Some('v'));
            m.screen = screen;
            if file == "groups_create" {
                m.act(Action::ShowCreate);
                m.focus = None;
            }
            let draws = m.draw(&v, size, mouse, &measure);
            let body: Vec<String> = draws.iter().map(json).collect();
            std::fs::write(
                format!("{dir}/menu_{file}.json"),
                format!("[{}]", body.join(",")),
            )
            .unwrap();
        }
    }

    #[test]
    fn typing_a_new_group_and_creating_it() {
        crate::i18n::set_language("en_us");
        let mut m = Menu::default();
        m.open(Some('v'));
        m.char_typed('v');
        assert_eq!(m.act(Action::Focus(Field::Name)), None);
        for c in "Team A".chars() {
            m.char_typed(c);
        }
        m.key("key.keyboard.tab", false);
        for c in "p w".chars() {
            m.char_typed(c);
        }
        assert_eq!((m.name.as_str(), m.password.as_str()), ("Team A", "pw"));
        m.act(Action::Kind);
        assert_eq!(m.kind, 1);
        assert_eq!(m.key("key.keyboard.enter", false), Some(Action::Create));
        assert_eq!(m.key("key.keyboard.escape", false), None);
        assert_eq!(m.key("key.keyboard.escape", false), Some(Action::Close));
    }
}
