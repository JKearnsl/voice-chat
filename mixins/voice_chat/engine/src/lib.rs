//! Voice Chat: a client for the Simple Voice Chat protocol (an
//! unofficial one, not affiliated with Simple Voice Chat or its author), speaking it so a Horizon player talks with players on the
//! Java mod on any server that has the voice chat plugin or mod.
//!
//! The game thread calls [`tick`], [`payload`], [`key`], [`command`] and the
//! HUD and settings functions; the voice runs on threads of its own.
pub mod audio;
pub mod buf;
pub mod crypt;
pub mod i18n;
pub mod mix;
pub mod net;
pub mod opus;
pub mod proto;
pub mod ui;

use i18n::{fmt, tr};

// The pure-Rust Opus codec (vendored `opus-rs`, BSD-3-Clause), inlined as a
// module because Horizon's mod build forbids path/extra dependencies. See
// `src/opus_rs/COPYING`.
mod opus_rs;
mod session;

use audio::{AudioIo, Devices, MicQueue};
use mix::{Listener, Mixer};
use net::{Conn, IDLE_LEVEL_DB, Mode, Redirect, Shared, Status};
use proto::{COMPAT_27, Group, PlayerState, Secret};
use session::Session;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex, MutexGuard};
use std::time::{Duration, Instant};

pub const ID: &str = "voice_chat";
pub const NAME: &str = "Voice Chat";
pub const VERSION: &str = "1.0.0";
pub const DESCRIPTION: &str = "Talk with players who use the Simple Voice Chat mod, on servers that have it (unofficial). \
V opens the menu: controls, hotkeys and groups. Push to talk or voice activation.";

pub const KEY_PTT: &str = "push_to_talk";
pub const KEY_WHISPER: &str = "whisper";
pub const KEY_MUTE: &str = "mute";
pub const KEY_DISABLE: &str = "disable";
pub const DEFAULT_PTT: &str = "key.keyboard.caps.lock";
pub const DEFAULT_WHISPER: &str = "key.keyboard.unknown";
pub const DEFAULT_MUTE: &str = "key.keyboard.j";
pub const DEFAULT_DISABLE: &str = "key.keyboard.k";
pub const KEY_MENU: &str = "menu";
pub const DEFAULT_MENU: &str = "key.keyboard.v";

/// The mod's keys: their name, what they do, and their default.
pub const HOTKEYS: [(&str, &str, &str); 5] = [
    (KEY_MENU, "Open this menu", DEFAULT_MENU),
    (KEY_PTT, "Push to talk", DEFAULT_PTT),
    (KEY_WHISPER, "Whisper", DEFAULT_WHISPER),
    (KEY_MUTE, "Mute microphone", DEFAULT_MUTE),
    (KEY_DISABLE, "Voice chat off / on", DEFAULT_DISABLE),
];

/// A player's eyes above their feet (`Player.getEyeHeight`, standing).
pub const EYE_HEIGHT: f64 = 1.62;

/// The game's side of a tick in a server's world.
pub struct World<'a> {
    /// The address the player joined, as typed (`host` or `host:port`).
    pub address: &'a str,
    pub listener: Option<Listener>,
    /// Other players by UUID, at their eyes.
    pub players: Vec<(u128, [f64; 3])>,
    pub redirect: Redirect,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    PushToTalk,
    Whisper,
    Mute,
    Disable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Protocol {
    Auto,
    V26,
    V27,
}

/// The settings page and what is stored of it.
#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub mode: Mode,
    pub muted: bool,
    pub disabled: bool,
    pub volume: u32,
    pub mic_gain: u32,
    pub threshold: i32,
    pub protocol: Protocol,
    pub hud: bool,
    pub input_device: Option<String>,
    pub output_device: Option<String>,
    pub player_volumes: HashMap<u128, u32>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            mode: Mode::PushToTalk,
            muted: false,
            disabled: false,
            volume: 100,
            mic_gain: 100,
            threshold: -50,
            protocol: Protocol::Auto,
            hud: true,
            input_device: None,
            output_device: None,
            player_volumes: HashMap::new(),
        }
    }
}

const VOLUMES: [u32; 9] = [0, 25, 50, 75, 100, 125, 150, 175, 200];
const GAINS: [u32; 8] = [50, 75, 100, 125, 150, 200, 250, 300];
const THRESHOLDS: [i32; 7] = [-70, -60, -50, -45, -40, -30, -20];

fn next<T: Copy + PartialEq>(list: &[T], v: T) -> T {
    let i = list
        .iter()
        .position(|x| *x == v)
        .map_or(0, |i| (i + 1) % list.len());
    list[i]
}

fn parse_player_volumes(value: Option<String>) -> HashMap<u128, u32> {
    value
        .unwrap_or_default()
        .split(',')
        .filter_map(|entry| {
            let (uuid, volume) = entry.split_once(':')?;
            let uuid = u128::from_str_radix(uuid, 16).ok()?;
            let volume = volume.parse::<u32>().ok()?.min(200);
            Some((uuid, volume))
        })
        .collect()
}

fn store_player_volumes(volumes: &HashMap<u128, u32>) -> String {
    let mut entries: Vec<_> = volumes.iter().collect();
    entries.sort_by_key(|(uuid, _)| **uuid);
    entries
        .into_iter()
        .map(|(uuid, volume)| format!("{uuid:032x}:{volume}"))
        .collect::<Vec<_>>()
        .join(",")
}

impl Settings {
    pub fn load(get: &dyn Fn(&str) -> Option<String>) -> Self {
        let d = Self::default();
        let num = |k: &str| get(k).and_then(|v| v.parse::<i64>().ok());
        Self {
            mode: if get("mode").as_deref() == Some("voice") {
                Mode::Voice
            } else {
                Mode::PushToTalk
            },
            muted: get("muted").as_deref() == Some("true"),
            disabled: get("disabled").as_deref() == Some("true"),
            volume: num("volume").map_or(d.volume, |v| v.clamp(0, 200) as u32),
            mic_gain: num("mic_gain").map_or(d.mic_gain, |v| v.clamp(10, 500) as u32),
            threshold: num("threshold").map_or(d.threshold, |v| v.clamp(-127, 0) as i32),
            protocol: match get("protocol").as_deref() {
                Some("2.6") => Protocol::V26,
                Some("2.7") => Protocol::V27,
                _ => Protocol::Auto,
            },
            hud: get("hud").as_deref() != Some("false"),
            input_device: get("input_device").filter(|v| !v.is_empty()),
            output_device: get("output_device").filter(|v| !v.is_empty()),
            player_volumes: parse_player_volumes(get("player_volumes")),
        }
    }

    pub fn stored(&self) -> Vec<(&'static str, String)> {
        vec![
            (
                "mode",
                if self.mode == Mode::Voice {
                    "voice"
                } else {
                    "ptt"
                }
                .into(),
            ),
            ("muted", self.muted.to_string()),
            ("disabled", self.disabled.to_string()),
            ("volume", self.volume.to_string()),
            ("mic_gain", self.mic_gain.to_string()),
            ("threshold", self.threshold.to_string()),
            (
                "protocol",
                match self.protocol {
                    Protocol::Auto => "auto",
                    Protocol::V26 => "2.6",
                    Protocol::V27 => "2.7",
                }
                .into(),
            ),
            ("hud", self.hud.to_string()),
            (
                "input_device",
                self.input_device.clone().unwrap_or_default(),
            ),
            (
                "output_device",
                self.output_device.clone().unwrap_or_default(),
            ),
            ("player_volumes", store_player_volumes(&self.player_volumes)),
        ]
    }

    /// The settings page's buttons, in order (localised).
    pub fn buttons(&self) -> Vec<String> {
        let on_off = |b: bool| tr(if b { "on" } else { "off" });
        vec![
            format!(
                "{}: {}",
                tr("activation"),
                tr(if self.mode == Mode::Voice {
                    "voice_activation"
                } else {
                    "push_to_talk"
                })
            ),
            format!(
                "{}: {}",
                tr("microphone"),
                tr(if self.muted { "muted" } else { "on" })
            ),
            format!("{}: {}", tr("voice_chat"), on_off(!self.disabled)),
            format!("{}: {}%", tr("voice_volume"), self.volume),
            format!("{}: {}%", tr("mic_gain"), self.mic_gain),
            format!("{}: {} dB", tr("activation_level"), self.threshold),
            format!(
                "{}: {}",
                tr("protocol"),
                match self.protocol {
                    Protocol::Auto => tr("automatic"),
                    Protocol::V26 => "2.6.x".into(),
                    Protocol::V27 => "2.7".into(),
                }
            ),
            format!("{}: {}", tr("status_on_screen"), on_off(self.hud)),
        ]
    }

    pub fn click(&mut self, button: usize) {
        match button {
            0 => {
                self.mode = if self.mode == Mode::Voice {
                    Mode::PushToTalk
                } else {
                    Mode::Voice
                }
            }
            1 => self.muted = !self.muted,
            2 => self.disabled = !self.disabled,
            3 => self.volume = next(&VOLUMES, self.volume),
            4 => self.mic_gain = next(&GAINS, self.mic_gain),
            5 => self.threshold = next(&THRESHOLDS, self.threshold),
            6 => {
                self.protocol = match self.protocol {
                    Protocol::Auto => Protocol::V26,
                    Protocol::V26 => Protocol::V27,
                    Protocol::V27 => Protocol::Auto,
                }
            }
            7 => self.hud = !self.hud,
            _ => {}
        }
    }
}

struct Voice {
    settings: Settings,
    loaded: bool,
    session: Option<Session>,
    /// Custom payloads for the server, sent at the tick.
    outbox: Vec<(String, Vec<u8>)>,
    /// Settings to write back, and chat lines for the player.
    store: Vec<(String, String)>,
    chat: Vec<String>,
    shared: Arc<Shared>,
    mixer: Arc<Mutex<Mixer>>,
    mic: Arc<MicQueue>,
    devices: Arc<Mutex<Devices>>,
    redirect: Option<Redirect>,
    menu: ui::Menu,
    ui_click: bool,
    /// Device names for the settings screen, refreshed while it is open.
    device_list: Option<(Vec<String>, Vec<String>)>,
    devices_scanned: Option<Instant>,
}

fn voice() -> MutexGuard<'static, Voice> {
    static VOICE: LazyLock<Mutex<Voice>> = LazyLock::new(|| {
        Mutex::new(Voice {
            settings: Settings::default(),
            loaded: false,
            session: None,
            outbox: Vec::new(),
            store: Vec::new(),
            chat: Vec::new(),
            shared: Arc::default(),
            mixer: Arc::default(),
            mic: Arc::default(),
            devices: Arc::new(Mutex::new(Devices::default())),
            redirect: None,
            menu: ui::Menu::default(),
            ui_click: false,
            device_list: None,
            devices_scanned: None,
        })
    });
    VOICE.lock().unwrap_or_else(|e| e.into_inner())
}

static SOUND_CARD: AtomicBool = AtomicBool::new(true);

/// For tools and tests: leave the sound card alone, and feed [`mic_queue`]
/// and read [`mixer`] instead.
pub fn use_sound_card(on: bool) {
    SOUND_CARD.store(on, Ordering::Relaxed);
}

pub fn mic_queue() -> Arc<MicQueue> {
    voice().mic.clone()
}

pub fn mixer() -> Arc<Mutex<Mixer>> {
    voice().mixer.clone()
}

/// The settings key a server's remembered version is kept under.
fn compat_key(address: &str) -> String {
    let clean: String = address
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!("compat.{clean}")
}

impl Voice {
    fn apply_controls(&self) {
        let s = &self.settings;
        let mut c = self.shared.controls.lock().unwrap();
        c.mode = s.mode;
        c.muted = s.muted;
        c.disabled = s.disabled;
        c.volume = s.volume as f32 / 100.0;
        c.mic_gain = s.mic_gain as f32 / 100.0;
        c.threshold_db = s.threshold as f32;
        c.player_volumes = s
            .player_volumes
            .iter()
            .map(|(uuid, volume)| (*uuid, *volume as f32 / 100.0))
            .collect();
    }

    fn save(&mut self) {
        self.apply_controls();
        let stored = self.settings.stored();
        self.store
            .extend(stored.into_iter().map(|(k, v)| (k.to_owned(), v)));
    }

    fn set_threshold(&mut self, frac: f32) {
        self.settings.threshold = (-80.0 + frac.clamp(0.0, 1.0) * 80.0).round() as i32;
        self.apply_controls();
    }

    /// Applies the selected devices on the audio worker, closing its current
    /// streams before opening replacements.
    fn restart_audio(&mut self) {
        self.device_list = None;
        self.mic.clear();
        if !SOUND_CARD.load(Ordering::Relaxed) {
            return;
        }
        let Some(session) = &mut self.session else {
            return;
        };
        let input = self.settings.input_device.clone();
        let output = self.settings.output_device.clone();
        if let Some(audio) = &session.audio
            && audio.restart(input.clone(), output.clone()).is_ok()
        {
            return;
        }
        session.audio = match AudioIo::start(
            self.mixer.clone(),
            self.mic.clone(),
            self.devices.clone(),
            input,
            output,
        ) {
            Ok(audio) => Some(audio),
            Err(e) => {
                *self.devices.lock().unwrap() = Devices {
                    input: Err(e.clone()),
                    output: Err(e.clone()),
                };
                None
            }
        };
    }

    fn send(&mut self, (channel, data): (&str, Vec<u8>)) {
        self.outbox.push((channel.to_owned(), data));
    }

    /// Tells the server whether the player muted or turned voice chat off.
    fn send_state(&mut self) {
        let Some(compat) = self
            .session
            .as_ref()
            .filter(|s| s.secret.is_some())
            .map(Session::compat)
        else {
            return;
        };
        let (muted, disabled) = (self.settings.muted, self.settings.disabled);
        self.send(proto::update_state(compat, muted, disabled));
    }

    fn leave(&mut self) {
        self.shared.connection.fetch_add(1, Ordering::Relaxed);
        self.session = None;
        self.mic.clear();
        self.shared.transmitting.store(false, Ordering::Relaxed);
        self.shared
            .mic_level
            .store(IDLE_LEVEL_DB.to_bits(), Ordering::Relaxed);
        self.shared.talking.lock().unwrap().clear();
        self.shared.activity.lock().unwrap().clear();
        self.shared.sources.lock().unwrap().clear();
        *self.shared.status.lock().unwrap() = Status::Idle;
        *self.shared.mic_error.lock().unwrap() = None;
        self.mixer.lock().unwrap().clear();
    }

    fn on_secret(&mut self, data: &[u8]) {
        let Ok(secret) = Secret::parse(data) else {
            self.chat
                .push("Voice chat: the server sent a secret this version can't read".into());
            return;
        };
        let Some(redirect) = self.redirect else {
            return;
        };
        let Some(session) = self.session.as_mut() else {
            return;
        };
        if session.remembered != Some(secret.compat) {
            session.remembered = Some(secret.compat);
            self.store
                .push((compat_key(&session.address), secret.compat.to_string()));
        }
        // A new secret replaces the connection.
        session.conn = None;
        let server = match proto::split_host_port(&session.address) {
            Some((host, port)) => (host, port.unwrap_or(25565)),
            None => (session.address.clone(), 25565),
        };
        session.secret = Some(secret.clone());
        session.conn = match Conn::start(
            secret,
            server,
            redirect,
            self.shared.clone(),
            self.mixer.clone(),
            self.mic.clone(),
        ) {
            Ok(conn) => Some(conn),
            Err(e) => {
                self.chat.push(format!("Voice chat can't start: {e}"));
                None
            }
        };
        if session.audio.is_none() {
            self.restart_audio();
        }
        self.send_state();
    }

    fn on_payload(&mut self, channel: &str, data: &[u8]) {
        if channel == "voicechat:secret" {
            self.on_secret(data);
            return;
        }
        let Some(session) = self.session.as_mut() else {
            return;
        };
        let compat = session.compat();
        match channel {
            "voicechat:states" => {
                if let Ok(states) = PlayerState::parse_all(data, compat) {
                    session.set_states(states);
                }
            }
            "voicechat:state" => {
                if let Ok(s) = PlayerState::parse(data, compat) {
                    session.insert_state(s);
                }
            }
            "voicechat:remove_state" => {
                if let Ok(id) = proto::parse_uuid(data) {
                    session.remove_state(id);
                }
            }
            "voicechat:add_group" => {
                if let Ok(g) = Group::parse(data) {
                    session.groups.insert(g.id, g);
                }
            }
            "voicechat:remove_group" => {
                if let Ok(id) = proto::parse_uuid(data) {
                    session.groups.remove(&id);
                }
            }
            "voicechat:joined_group" => {
                if let Ok((group, wrong_password)) = proto::parse_joined_group(data) {
                    if wrong_password {
                        self.menu.say(tr("wrong_password"), true);
                    } else {
                        session.group = group;
                        let name = group
                            .and_then(|g| session.groups.get(&g))
                            .map(|g| g.name.clone());
                        let said = match &name {
                            Some(n) => fmt("joined_named", &[n]),
                            None if group.is_some() => tr("joined_group"),
                            None => tr("left_group"),
                        };
                        self.menu.say(said, false);
                    }
                }
            }
            _ => {}
        }
    }

    fn tick(&mut self, world: World, get: &dyn Fn(&str) -> Option<String>) {
        if !self.loaded {
            self.loaded = true;
            self.settings = Settings::load(get);
            self.apply_controls();
        }
        self.redirect = Some(world.redirect);
        if self
            .session
            .as_ref()
            .is_some_and(|s| s.address != world.address)
        {
            self.leave();
        }
        if self.session.is_none() {
            let remembered = get(&compat_key(world.address)).and_then(|v| v.parse().ok());
            self.session = Some(Session::new(world.address, remembered));
            self.send(("minecraft:register", proto::register_payload()));
        }
        if let Some(l) = world.listener {
            *self.shared.listener.lock().unwrap() = l;
        }
        *self.shared.players.lock().unwrap() = world.players.into_iter().collect();

        let protocol = self.settings.protocol;
        let session = self.session.as_mut().expect("made above");
        let now = Instant::now();
        if self.shared.transmitting.load(Ordering::Relaxed)
            && let Some(secret) = &session.secret
        {
            let whisper = self.shared.controls.lock().unwrap().whisper;
            self.shared
                .talking
                .lock()
                .unwrap()
                .insert(secret.player, (now, whisper));
        }
        if session.secret.is_none() && now >= session.next_ask {
            let plan = session.plan(protocol);
            if let Some(&compat) = plan.get(session.asked.len()) {
                session.asked.push(compat);
                session.next_ask = now + Duration::from_secs(3);
                self.send(proto::request_secret(compat));
            }
        }
        // The connection timed out: ask again, as the mod reconnects.
        let session = self.session.as_mut().expect("made above");
        let failed = matches!(*self.shared.status.lock().unwrap(), Status::Failed(_));
        if failed && session.secret.is_some() && now - session.joined > Duration::from_secs(5) {
            let compat = session.compat();
            session.secret = None;
            session.conn = None;
            session.asked.clear();
            session.next_ask = now + Duration::from_secs(5);
            session.remembered = Some(compat);
        }
    }
}

/// A tick in a server's world (`world`), or out of one (None). Returns the
/// custom payloads to send the server and the settings to store.
pub fn tick(world: Option<World>, get: &dyn Fn(&str) -> Option<String>) -> Tick {
    let mut v = voice();
    match world {
        Some(w) => v.tick(w, get),
        None => {
            if v.session.is_some() {
                v.leave();
            }
        }
    }
    Tick {
        payloads: std::mem::take(&mut v.outbox),
        store: std::mem::take(&mut v.store),
        chat: std::mem::take(&mut v.chat),
    }
}

#[derive(Default)]
pub struct Tick {
    pub payloads: Vec<(String, Vec<u8>)>,
    pub store: Vec<(String, String)>,
    pub chat: Vec<String>,
}

/// A custom payload from the server: true when it was the voice chat's.
pub fn payload(channel: &str, data: &[u8]) -> bool {
    if !channel.starts_with("voicechat:") {
        return false;
    }
    voice().on_payload(channel, data);
    true
}

/// Applies a voice chat key.
pub fn key(which: Key, down: bool) {
    let mut v = voice();
    match which {
        Key::PushToTalk => v.shared.controls.lock().unwrap().ptt = down,
        Key::Whisper => v.shared.controls.lock().unwrap().whisper = down,
        Key::Mute if down => {
            v.settings.muted = !v.settings.muted;
            v.save();
            v.send_state();
        }
        Key::Disable if down => {
            v.settings.disabled = !v.settings.disabled;
            v.save();
            v.send_state();
            if v.settings.disabled {
                v.mixer.lock().unwrap().clear();
            }
        }
        _ => {}
    }
}

pub fn settings_buttons() -> Vec<String> {
    voice().settings.buttons()
}

pub fn settings_click(button: usize) {
    voice().click(button);
}

impl Voice {
    fn click(&mut self, button: usize) {
        let before = (self.settings.muted, self.settings.disabled);
        self.settings.click(button);
        self.save();
        if before != (self.settings.muted, self.settings.disabled) {
            self.send_state();
        }
    }

    fn status_line(&self) -> (String, u32) {
        let Some(s) = self.session.as_ref() else {
            return (tr("not_on_server"), GREY);
        };
        match &s.secret {
            None if s.asked.len() >= s.plan(self.settings.protocol).len() => {
                (tr("no_voice_server"), GREY)
            }
            None => (tr("asking_server"), YELLOW),
            Some(secret) => {
                let version = if secret.compat >= COMPAT_27 {
                    "2.7"
                } else {
                    "2.6.x"
                };
                match &*self.shared.status.lock().unwrap() {
                    Status::Connected => (fmt("connected_proto", &[version]), GREEN),
                    Status::Failed(e) => (e.clone(), RED),
                    _ => (tr("connecting"), YELLOW),
                }
            }
        }
    }

    fn volume_sources(&self) -> Vec<ui::VolumeRow> {
        let mut names: HashMap<u128, String> = self
            .session
            .as_ref()
            .map(|s| {
                s.states
                    .values()
                    .filter(|p| !p.disconnected)
                    .map(|p| (p.uuid, p.name.clone()))
                    .collect()
            })
            .unwrap_or_default();
        let sources = self.shared.sources.lock().unwrap();
        for uuid in sources.keys().chain(self.settings.player_volumes.keys()) {
            names
                .entry(*uuid)
                .or_insert_with(|| fmt("audio_player", &[&format!("{:08x}", uuid & 0xFFFF_FFFF)]));
        }
        drop(sources);
        let mut rows: Vec<_> = names
            .into_iter()
            .map(|(uuid, name)| ui::VolumeRow {
                uuid,
                name,
                volume: self
                    .settings
                    .player_volumes
                    .get(&uuid)
                    .copied()
                    .unwrap_or(100),
            })
            .collect();
        rows.sort_by(|a, b| {
            a.name
                .to_lowercase()
                .cmp(&b.name.to_lowercase())
                .then(a.uuid.cmp(&b.uuid))
        });
        rows
    }

    fn view(&self, hotkeys: Vec<(String, String)>) -> ui::View {
        let session = self.session.as_ref();
        let screen = self.menu.screen;
        let connected = session.is_some_and(|s| s.secret.is_some())
            && *self.shared.status.lock().unwrap() == Status::Connected;
        let mut groups: Vec<ui::GroupRow> = session
            .filter(|_| matches!(screen, ui::Screen::Main | ui::Screen::Groups))
            .map(|s| {
                let mut members = s.memberships();
                s.groups
                    .values()
                    .filter(|g| match screen {
                        ui::Screen::Main => s.group == Some(g.id),
                        _ => !g.hidden || s.group == Some(g.id),
                    })
                    .map(|g| ui::GroupRow {
                        id: g.id,
                        name: g.name.clone(),
                        has_password: g.has_password,
                        members: members.remove(&g.id).unwrap_or_default(),
                        ours: s.group == Some(g.id),
                    })
                    .collect()
            })
            .unwrap_or_default();
        groups.sort_by_key(|g| (!g.ours, g.name.to_lowercase()));
        ui::View {
            status: self.status_line(),
            settings: self.settings.clone(),
            level_db: if connected {
                self.shared.level()
            } else {
                IDLE_LEVEL_DB
            },
            connected,
            groups_enabled: session
                .and_then(|s| s.secret.as_ref())
                .is_some_and(|s| s.groups),
            groups,
            hotkeys: if screen == ui::Screen::Settings {
                hotkeys
            } else {
                Vec::new()
            },
            devices: if screen == ui::Screen::Settings {
                self.device_list.clone().unwrap_or_default()
            } else {
                Default::default()
            },
            volume_sources: if screen == ui::Screen::Volumes {
                self.volume_sources()
            } else {
                Vec::new()
            },
        }
    }

    /// What a click or Enter in the menu asks of the voice chat.
    fn menu_action(&mut self, action: ui::Action) {
        use ui::Action as A;
        let Some(action) = self.menu.act(action) else {
            return;
        };
        let group_name = |v: &Voice, id: u128| {
            v.session
                .as_ref()
                .and_then(|s| s.groups.get(&id))
                .map(|g| (g.name.clone(), g.has_password))
        };
        match action {
            A::Mode => self.click(0),
            A::Mute => self.click(1),
            A::Disable => self.click(2),
            A::Volume(d) => {
                self.settings.volume = (self.settings.volume as i32 + d).clamp(0, 200) as u32;
                self.save();
            }
            A::PlayerVolume(uuid, d) => {
                let volume = self.settings.player_volumes.entry(uuid).or_insert(100);
                *volume = (*volume as i32 + d).clamp(0, 200) as u32;
                self.save();
            }
            A::Gain(d) => {
                self.settings.mic_gain = (self.settings.mic_gain as i32 + d).clamp(25, 500) as u32;
                self.save();
            }
            A::PickDevice { input, name } => {
                // The dropdown hands back the picked device (None = the OS
                // default); re-open the streams on the new choice.
                if input {
                    self.settings.input_device = name;
                } else {
                    self.settings.output_device = name;
                }
                self.save();
                self.restart_audio();
            }
            A::ThresholdAt(frac) => {
                self.set_threshold(frac);
                self.save();
            }
            A::Join(id) => match group_name(self, id) {
                Some((_, true)) => {
                    self.menu.joining = Some(id);
                    self.menu.join_password.clear();
                    self.menu.focus = Some(ui::Field::JoinPassword);
                }
                Some((name, false)) => {
                    self.send(proto::join_group(id, None));
                    self.menu.say(fmt("joining", &[&name]), false);
                }
                None => {}
            },
            A::JoinWithPassword => {
                let Some(id) = self.menu.joining.take() else {
                    return;
                };
                let password = std::mem::take(&mut self.menu.join_password);
                self.menu.focus = None;
                if password.is_empty() {
                    self.menu.say(tr("type_password_first"), true);
                    self.menu.joining = Some(id);
                    self.menu.focus = Some(ui::Field::JoinPassword);
                    return;
                }
                let name = group_name(self, id).map(|(n, _)| n).unwrap_or_default();
                self.send(proto::join_group(id, Some(&password)));
                self.menu.say(fmt("joining", &[&name]), false);
            }
            A::Leave => {
                self.send(proto::leave_group());
                self.menu.say(tr("leaving"), false);
            }
            A::Create => {
                let name = self.menu.name.trim().to_owned();
                if name.is_empty() {
                    self.menu.say(tr("type_name_first"), true);
                    self.menu.focus = Some(ui::Field::Name);
                    return;
                }
                if self.session.as_ref().is_none_or(|s| s.secret.is_none()) {
                    self.menu.say(tr("not_connected"), true);
                    return;
                }
                let password = std::mem::take(&mut self.menu.password);
                let password = (!password.is_empty()).then_some(password);
                self.send(proto::create_group(
                    &name,
                    password.as_deref(),
                    self.menu.kind,
                ));
                self.menu.name.clear();
                self.menu.focus = None;
                self.menu.creating = false;
                self.menu.say(fmt("creating", &[&name]), false);
            }
            _ => {}
        }
    }
}

/// Opens the menu, opened by a key typing `opened_by`.
pub fn menu_open(opened_by: Option<char>) {
    voice().menu.open(opened_by);
}

pub fn menu_is_open() -> bool {
    voice().menu.open
}

/// The game's screen is (`ours`) or isn't the menu.
pub fn menu_sync(ours: bool) {
    voice().menu.sync_open(ours);
}

/// Whether the game should close the menu now (once).
pub fn menu_take_close() -> bool {
    voice().menu.take_close()
}

/// Draws the menu; `hotkeys` are what each key does and its key's name.
pub fn menu_draw(
    size: [f32; 2],
    mouse: [f32; 2],
    measure: &dyn Fn(&str) -> f32,
    hotkeys: Vec<(String, String)>,
) -> Vec<ui::Draw> {
    let mut v = voice();
    if v.menu.screen == ui::Screen::Settings
        && (v.device_list.is_none()
            || v.devices_scanned
                .is_none_or(|at| at.elapsed() >= Duration::from_secs(1)))
    {
        v.device_list = Some(audio::list_devices());
        v.devices_scanned = Some(Instant::now());
    }
    let view = v.view(hotkeys);
    v.menu.draw(&view, size, mouse, measure)
}

pub fn menu_click(at: [f32; 2]) {
    let mut v = voice();
    if let Some(action) = v.menu.hit(at) {
        if !matches!(&action, ui::Action::Focus(_)) {
            v.ui_click = true;
        }
        v.menu_action(action);
    }
}

pub fn take_ui_click() -> bool {
    std::mem::take(&mut voice().ui_click)
}

pub fn menu_drag(at: [f32; 2]) {
    let mut v = voice();
    if let Some(ui::Action::ThresholdAt(frac)) = v.menu.drag_threshold(at) {
        v.set_threshold(frac);
    }
}

pub fn menu_release() {
    let mut v = voice();
    if v.menu.end_threshold_drag() {
        v.save();
    }
}

/// A key on the menu: `menu_key` (the open key) closes it unless a field
/// takes the typing.
pub fn menu_key(key: &str, shift: bool, menu_key: &str) {
    let mut v = voice();
    if key == menu_key && !v.menu.typing() {
        v.menu.close();
        return;
    }
    if let Some(action) = v.menu.key(key, shift) {
        v.menu_action(action);
    }
}

pub fn menu_char(c: char) {
    voice().menu.char_typed(c);
}

pub fn menu_scroll(steps: f64) {
    voice().menu.scroll_by(steps.signum() as i32);
}

/// `key.keyboard.v` → `v`: the character a key types, for single letters
/// and digits.
pub fn key_char(key: &str) -> Option<char> {
    let name = key.strip_prefix("key.keyboard.")?;
    let mut chars = name.chars();
    let c = chars.next()?;
    chars.next().is_none().then_some(c)
}

/// Loads the stored settings before the first world, for the settings page.
pub fn load(get: &dyn Fn(&str) -> Option<String>) {
    let mut v = voice();
    if !v.loaded {
        v.loaded = true;
        v.settings = Settings::load(get);
        v.apply_controls();
    }
}

/// Sets the player's language for the menu, HUD and chat, from the game's
/// `settings.language` (`en_us`, `ru_ru`, …). Call it on the game thread
/// before drawing; unknown languages fall back to English.
pub fn set_language(code: &str) {
    i18n::set_language(code);
}

/// Settings changed since the last call, to store.
pub fn take_store() -> Vec<(String, String)> {
    std::mem::take(&mut voice().store)
}

/// A line of the status the HUD shows: its text, colour, and an optional
/// icon sprite (`voice_chat:gui/icon_*`) drawn to its left, Simple Voice
/// Chat-style.
#[derive(Debug)]
pub struct Line {
    pub text: String,
    pub rgb: u32,
    pub icon: Option<&'static str>,
}

// The HUD status icons (drawn in assets/voice_chat/textures/gui/).
const IC_DISCONNECTED: &str = "voice_chat:gui/icon_disconnected";
const IC_DISABLED: &str = "voice_chat:gui/icon_speaker_off";
const IC_MUTE: &str = "voice_chat:gui/icon_mute";
const IC_TALKING: &str = "voice_chat:gui/icon_talking";
const IC_WHISPERING: &str = "voice_chat:gui/icon_whispering";
const IC_GROUP: &str = "voice_chat:gui/icon_group";
const IC_OTHER_TALKING: &str = "voice_chat:gui/icon_other_talking";
const IC_OTHER_WHISPERING: &str = "voice_chat:gui/icon_other_whispering";

const GREY: u32 = 0xAAAAAA;
const GREEN: u32 = 0x55FF55;
const AQUA: u32 = 0x55FFFF;
const RED: u32 = 0xFF5555;
const YELLOW: u32 = 0xFFFF55;

/// Connection, warnings and the active voice status, top line first.
pub fn hud() -> Vec<Line> {
    let v = voice();
    let Some(session) = v.session.as_ref().filter(|_| v.settings.hud) else {
        return Vec::new();
    };
    let mut lines = Vec::new();
    let line = |text: String, rgb| Line {
        text,
        rgb,
        icon: None,
    };
    let iline = |text: String, rgb, icon| Line {
        text,
        rgb,
        icon: Some(icon),
    };
    if session.secret.is_none() {
        // No voice chat here (yet): the status alone, with the disconnected
        // icon, so the HUD stays visible while asking.
        let (text, rgb) = v.status_line();
        lines.push(iline(text, rgb, IC_DISCONNECTED));
        return lines;
    }
    let status = v.shared.status.lock().unwrap().clone();
    match status {
        Status::Connected => {}
        Status::Failed(ref e) => {
            lines.push(iline(fmt("voice_chat_prefix", &[e]), RED, IC_DISCONNECTED))
        }
        _ => lines.push(iline(
            fmt("voice_chat_prefix", &[&tr("connecting")]),
            YELLOW,
            IC_DISCONNECTED,
        )),
    }
    if let Some(e) = v.shared.mic_error.lock().unwrap().as_ref() {
        lines.push(line(fmt("voice_chat_prefix", &[e]), RED));
    }
    if let Err(e) = &v.devices.lock().unwrap().input
        && e != "not open"
    {
        lines.push(line(fmt("mic_error", &[e]), RED));
    }
    if let Err(e) = &v.devices.lock().unwrap().output
        && e != "not open"
    {
        lines.push(line(fmt("spk_error", &[e]), RED));
    }
    if v.settings.disabled {
        lines.push(iline(tr("voice_chat_off"), RED, IC_DISABLED));
    } else if v.settings.muted {
        lines.push(iline(tr("microphone_muted"), RED, IC_MUTE));
    } else if v.shared.transmitting.load(Ordering::Relaxed) {
        let whisper = v.shared.controls.lock().unwrap().whisper;
        lines.push(if whisper {
            iline(tr("whispering"), AQUA, IC_WHISPERING)
        } else {
            iline(tr("talking"), GREEN, IC_TALKING)
        });
    }
    lines
}

/// The current group for the top-left roster header.
pub fn hud_group() -> Option<Line> {
    let v = voice();
    let session = v.session.as_ref().filter(|_| v.settings.hud)?;
    let group = session.group.and_then(|id| session.groups.get(&id))?;
    Some(Line {
        text: fmt("hud_group", &[&group.name]),
        rgb: GREEN,
        icon: Some(IC_GROUP),
    })
}

/// A group member, recent speaker, or muted player shown in the HUD roster.
#[derive(Debug)]
pub struct Head {
    pub uuid: u128,
    pub name: String,
    pub talking: bool,
    pub muted: bool,
}

/// Group members stay in the roster; outside speakers remain for two seconds.
/// Activity selects whom to keep only when `limit` cannot fit everyone.
pub fn hud_heads(limit: usize) -> Vec<Head> {
    let v = voice();
    let Some(session) = v
        .session
        .as_ref()
        .filter(|s| v.settings.hud && s.secret.is_some())
    else {
        return Vec::new();
    };
    let now = Instant::now();
    let talking = v.shared.talking.lock().unwrap();
    let mut heads = Vec::new();
    for (id, player) in &session.states {
        if player.disconnected {
            continue;
        }
        let last = talking.get(id).map(|(at, _)| *at);
        let speaking = last.is_some_and(|at| now - at < Duration::from_millis(300));
        let recent = last.is_some_and(|at| now - at < Duration::from_secs(2));
        let in_group = session
            .group
            .is_some_and(|group| player.group == Some(group));
        if in_group || recent || player.muted {
            heads.push(Head {
                uuid: *id,
                name: player.name.clone(),
                talking: speaking,
                muted: player.muted,
            });
        }
    }
    let by_name = |a: &Head, b: &Head| a.name.to_lowercase().cmp(&b.name.to_lowercase());
    heads.sort_by(by_name);
    if heads.len() > limit {
        let activity = v.shared.activity.lock().unwrap();
        heads.sort_by(|a, b| {
            activity
                .get(&b.uuid)
                .map_or(0, |a| a.count)
                .cmp(&activity.get(&a.uuid).map_or(0, |a| a.count))
                .then_with(|| by_name(a, b))
        });
        heads.truncate(limit);
        heads.sort_by(by_name);
    }
    heads
}

fn player_icon(
    session: &Session,
    talking: &HashMap<u128, (Instant, bool)>,
    now: Instant,
    id: u128,
    player: &PlayerState,
) -> Option<(&'static str, u32)> {
    if player.disconnected {
        Some((IC_DISCONNECTED, RED))
    } else if player.muted {
        Some((IC_MUTE, RED))
    } else if player.disabled {
        Some((IC_DISABLED, RED))
    } else if let Some(group) = player.group {
        session
            .groups
            .get(&group)
            .filter(|g| !g.hidden || session.group == Some(group))?;
        Some((
            IC_GROUP,
            if session.group == Some(group) {
                GREEN
            } else {
                YELLOW
            },
        ))
    } else {
        let (_, whispering) = talking
            .get(&id)
            .filter(|(at, _)| now - *at < Duration::from_secs(2))?;
        Some((
            if *whispering {
                IC_OTHER_WHISPERING
            } else {
                IC_OTHER_TALKING
            },
            if *whispering { AQUA } else { GREEN },
        ))
    }
}

/// The icon to draw inside a player's name tag, including team formatting.
pub fn name_tag_icon(text: &str) -> Option<&'static str> {
    let v = voice();
    let session = v
        .session
        .as_ref()
        .filter(|s| v.settings.hud && s.secret.is_some())?;
    let now = Instant::now();
    let talking = v.shared.talking.lock().unwrap();
    let (id, player) = session.player_for_name_tag(text)?;
    player_icon(session, &talking, now, id, player).map(|(icon, _)| icon)
}

/// `key.keyboard.caps.lock` → `Caps Lock`.
pub fn key_label(key: &str) -> String {
    let name = key
        .strip_prefix("key.keyboard.")
        .or_else(|| key.strip_prefix("key.mouse."))
        .unwrap_or(key);
    if name == "unknown" {
        return tr("not_bound");
    }
    name.split('.')
        .map(|w| {
            let mut c = w.chars();
            c.next()
                .map_or_else(String::new, |f| f.to_uppercase().chain(c).collect())
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world(address: &str) -> World<'_> {
        World {
            address,
            listener: None,
            players: Vec::new(),
            redirect: |h, p| (h.to_owned(), p),
        }
    }

    /// The tests share the one voice chat.
    static SERIAL: Mutex<()> = Mutex::new(());

    #[test]
    fn asks_for_the_secret_with_both_versions() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let none = |_: &str| None;
        let t = tick(Some(world("play.example.net")), &none);
        assert_eq!(t.payloads[0].0, "minecraft:register");
        assert!(
            t.payloads
                .iter()
                .all(|(c, _)| c != "voicechat:request_secret")
        );
        voice().session.as_mut().unwrap().next_ask = Instant::now();
        let t = tick(Some(world("play.example.net")), &none);
        assert_eq!(
            t.payloads,
            vec![(
                "voicechat:request_secret".into(),
                20i32.to_be_bytes().to_vec()
            )]
        );
        voice().session.as_mut().unwrap().next_ask = Instant::now();
        let t = tick(Some(world("play.example.net")), &none);
        assert_eq!(t.payloads[0].1, 21i32.to_be_bytes().to_vec());
        assert!(!payload("minecraft:brand", b""));
        let t = tick(None, &none);
        assert!(t.payloads.is_empty());
        assert!(voice().session.is_none());
    }

    #[test]
    fn shows_other_players_groups() {
        use buf::Writer;
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let none = |_: &str| None;
        tick(None, &none);
        let mut w = world("groups.example.net");
        let positions = vec![(0xA, [0.0, 65.62, 10.0]), (0xB, [3.0, 65.62, 12.0])];
        w.players = positions.clone();
        tick(Some(w), &none);
        // As a 2.6 server sends them: a secret, the groups, then the states.
        let mut secret = Writer::new();
        secret.raw(&[1; 16]).i32(24454).uuid(0xC).u8(0).i32(1275);
        secret
            .raw(&48f64.to_be_bytes())
            .i32(1000)
            .bool(true)
            .utf("127.0.0.1")
            .bool(false);
        {
            let mut v = voice();
            let s = v.session.as_mut().unwrap();
            s.secret = Some(Secret::parse(&secret.finish()).unwrap());
        }
        for (id, name) in [(0x61u128, "Builders"), (0x62, "Miners")] {
            let mut g = Writer::new();
            g.uuid(id)
                .utf(name)
                .bool(false)
                .bool(true)
                .bool(false)
                .i16(0);
            assert!(payload("voicechat:add_group", &g.finish()));
        }
        let mut states = Writer::new();
        states.i32(3);
        for (id, name, group) in [
            (0xAu128, "Alex", 0x61u128),
            (0xB, "Bea", 0x62),
            (0xC, "Me", 0x61),
        ] {
            states
                .bool(false)
                .bool(false)
                .uuid(id)
                .utf(name)
                .bool(true)
                .uuid(group);
        }
        assert!(payload("voicechat:states", &states.finish()));
        let mut joined = Writer::new();
        joined.bool(true).uuid(0x61).bool(false);
        payload("voicechat:joined_group", &joined.finish());
        assert!(
            voice().chat.is_empty(),
            "group action produced a chat message"
        );

        assert_eq!(name_tag_icon("Alex"), Some(IC_GROUP));
        assert_eq!(name_tag_icon("[Builders] Bea"), Some(IC_GROUP));
        let hud: Vec<String> = hud().into_iter().map(|l| l.text).collect();
        assert!(
            !hud.contains(&"Group: Builders".to_owned()),
            "group stayed in bottom HUD: {hud:?}"
        );
        let group = hud_group().expect("current group header");
        assert_eq!(
            (group.text.as_str(), group.icon),
            ("Group: Builders", Some(IC_GROUP))
        );

        // The menu: a password group asks for its password, then joins.
        {
            let mut v = voice();
            let s = v.session.as_mut().unwrap();
            s.groups.get_mut(&0x62).unwrap().has_password = true;
        }
        *voice().shared.status.lock().unwrap() = Status::Connected;
        menu_open(Some('v'));
        let measure = |t: &str| t.len() as f32 * 6.0;
        let keys = vec![("Push to talk".to_owned(), key_label(DEFAULT_PTT))];
        let has = |d: &[ui::Draw], want: &str| {
            d.iter()
                .any(|d| matches!(d, ui::Draw::Text(t, ..) if t == want))
        };
        // The hotkeys are on the Settings screen.
        voice().menu.screen = ui::Screen::Settings;
        let sdraws = menu_draw([480.0, 270.0], [0.0, 0.0], &measure, keys.clone());
        assert!(has(&sdraws, "Caps Lock"));
        // The groups are on the Groups screen.
        voice().menu.screen = ui::Screen::Groups;
        let draws = menu_draw([480.0, 270.0], [0.0, 0.0], &measure, keys.clone());
        assert!(has(&draws, "Miners") && has(&draws, "Alex, Me"));
        let join = voice().menu.hit_rect(&ui::Action::Join(0x62)).unwrap();
        tick(Some(world("groups.example.net")), &none);
        menu_click([join[0] + 1.0, join[1] + 1.0]);
        assert_eq!(voice().menu.joining, Some(0x62));
        menu_char('v');
        for c in "hunter2".chars() {
            menu_char(c);
        }
        menu_key("key.keyboard.enter", false, DEFAULT_MENU);
        let t = tick(Some(world("groups.example.net")), &none);
        assert_eq!(
            t.payloads,
            vec![proto::join_group(0x62, Some("hunter2"))]
                .into_iter()
                .map(|(c, d)| (c.to_owned(), d))
                .collect::<Vec<_>>()
        );
        menu_key(DEFAULT_MENU, false, DEFAULT_MENU);
        assert!(menu_take_close() && !menu_is_open());
        tick(None, &none);
    }

    #[test]
    fn push_to_talk_toggle_and_hud_icons() {
        use buf::Writer;
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        i18n::set_language("en_us");
        let none = |_: &str| None;
        tick(None, &none);

        // Push to talk, held: sets the transmit intent down, then up. This is
        // what the key hook calls while the key is held and when it is let go.
        key(Key::PushToTalk, true);
        assert!(voice().shared.controls.lock().unwrap().ptt);
        key(Key::PushToTalk, false);
        assert!(!voice().shared.controls.lock().unwrap().ptt);

        // Mute changes the state without a chat notification.
        let before = voice().settings.muted;
        key(Key::Mute, true);
        assert_ne!(voice().settings.muted, before);
        assert!(voice().chat.is_empty());
        key(Key::Mute, true); // back to where it was

        // Before the secret arrives, the HUD already shows the asking status
        // with the disconnected icon (the server may not have the mod at all).
        tick(Some(world("hud.example.net")), &none);
        let pre: Vec<_> = hud().into_iter().map(|l| (l.text, l.icon)).collect();
        assert_eq!(
            pre,
            vec![("Asking the server...".to_owned(), Some(IC_DISCONNECTED))],
            "{pre:?}"
        );

        // A connected session: the HUD shows a status line with an icon on the
        // left (what the user sees outside the menu).
        let mut secret = Writer::new();
        secret.raw(&[1; 16]).i32(24454).uuid(0xC).u8(0).i32(1275);
        secret
            .raw(&48f64.to_be_bytes())
            .i32(1000)
            .bool(true)
            .utf("127.0.0.1")
            .bool(false);
        {
            let mut v = voice();
            v.session.as_mut().unwrap().secret = Some(Secret::parse(&secret.finish()).unwrap());
        }
        *voice().shared.status.lock().unwrap() = Status::Connected;
        let icons = || hud().into_iter().filter_map(|l| l.icon).collect::<Vec<_>>();
        // Connected but quiet: no idle microphone or mode rows.
        assert!(
            icons().is_empty(),
            "idle HUD should stay empty: {:?}",
            icons()
        );
        // Muted shows the mute icon; voice chat off shows the speaker-off icon.
        voice().settings.muted = true;
        assert!(icons().contains(&IC_MUTE), "{:?}", icons());
        voice().settings.muted = false;
        voice().settings.disabled = true;
        assert!(icons().contains(&IC_DISABLED), "{:?}", icons());
        voice().settings.disabled = false;
        voice().shared.transmitting.store(true, Ordering::Relaxed);
        assert!(
            icons().contains(&IC_TALKING),
            "active voice is missing: {:?}",
            icons()
        );
        voice().shared.controls.lock().unwrap().whisper = true;
        assert!(
            icons().contains(&IC_WHISPERING),
            "whispering is missing: {:?}",
            icons()
        );
        voice().shared.controls.lock().unwrap().whisper = false;
        voice().shared.transmitting.store(false, Ordering::Relaxed);
        tick(None, &none);
    }

    #[test]
    fn group_roster_stays_visible_and_prioritises_speakers() {
        use buf::Writer;
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let none = |_: &str| None;
        tick(None, &none);
        tick(Some(world("roster.example.net")), &none);
        let mut secret = Writer::new();
        secret.raw(&[1; 16]).i32(24454).uuid(0xC).u8(0).i32(1275);
        secret
            .raw(&48f64.to_be_bytes())
            .i32(1000)
            .bool(true)
            .utf("127.0.0.1")
            .bool(false);
        {
            let mut v = voice();
            let s = v.session.as_mut().unwrap();
            s.secret = Some(Secret::parse(&secret.finish()).unwrap());
            s.group = Some(0x77);
            for (id, name) in [(0xA, "A"), (0xB, "B"), (0xC, "C")] {
                s.insert_state(PlayerState {
                    uuid: id,
                    name: name.into(),
                    muted: false,
                    disabled: false,
                    disconnected: false,
                    group: Some(0x77),
                });
            }
        }
        assert_eq!(
            hud_heads(usize::MAX).len(),
            3,
            "inactive group members are missing"
        );
        let now = Instant::now();
        {
            let v = voice();
            let mut talking = v.shared.talking.lock().unwrap();
            talking.insert(0xA, (now - Duration::from_secs(3), false));
            talking.insert(0xB, (now, false));
            talking.insert(0xC, (now - Duration::from_secs(1), false));
        }
        for (id, count) in [(0xB, 2), (0xC, 5), (0xA, 1)] {
            voice()
                .shared
                .activity
                .lock()
                .unwrap()
                .insert(id, net::Activity { last: now, count });
        }
        let all = hud_heads(usize::MAX);
        let order: Vec<_> = all.iter().map(|h| h.uuid).collect();
        assert_eq!(
            order,
            vec![0xA, 0xB, 0xC],
            "speech must not reorder the roster"
        );
        let heads = hud_heads(2);
        let order: Vec<_> = heads.iter().map(|h| h.uuid).collect();
        assert_eq!(
            order,
            vec![0xB, 0xC],
            "overflow should keep the most active players"
        );
        assert!(heads[0].talking && !heads[1].talking);
        tick(None, &none);
    }

    #[test]
    fn speakers_without_groups_get_voice_icons() {
        use buf::Writer;
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let none = |_: &str| None;
        tick(None, &none);
        tick(Some(world("voice-label.example.net")), &none);
        let mut secret = Writer::new();
        secret.raw(&[1; 16]).i32(24454).uuid(0xC).u8(0).i32(1275);
        secret
            .raw(&48f64.to_be_bytes())
            .i32(1000)
            .bool(true)
            .utf("127.0.0.1")
            .bool(false);
        {
            let mut v = voice();
            v.settings.hud = true;
            let session = v.session.as_mut().unwrap();
            session.secret = Some(Secret::parse(&secret.finish()).unwrap());
            session.insert_state(PlayerState {
                uuid: 0xA,
                name: "Talker".into(),
                muted: false,
                disabled: false,
                disconnected: false,
                group: None,
            });
            v.shared
                .players
                .lock()
                .unwrap()
                .insert(0xA, [0.0, 65.62, 10.0]);
            v.shared
                .talking
                .lock()
                .unwrap()
                .insert(0xA, (Instant::now(), false));
        }
        assert_eq!(name_tag_icon("Talker"), Some(IC_OTHER_TALKING));
        assert_eq!(name_tag_icon("[Team] Talker"), Some(IC_OTHER_TALKING));
        assert_eq!(name_tag_icon("Talker2"), None);
        voice()
            .shared
            .talking
            .lock()
            .unwrap()
            .insert(0xA, (Instant::now(), true));
        assert_eq!(name_tag_icon("Talker"), Some(IC_OTHER_WHISPERING));
        voice()
            .shared
            .talking
            .lock()
            .unwrap()
            .insert(0xA, (Instant::now() - Duration::from_secs(3), false));
        assert_eq!(name_tag_icon("Talker"), None);
        for (muted, disabled, disconnected, icon) in [
            (true, false, false, IC_MUTE),
            (false, true, false, IC_DISABLED),
            (false, false, true, IC_DISCONNECTED),
        ] {
            {
                let mut v = voice();
                let player = v.session.as_mut().unwrap().states.get_mut(&0xA).unwrap();
                player.muted = muted;
                player.disabled = disabled;
                player.disconnected = disconnected;
            }
            assert_eq!(name_tag_icon("Talker"), Some(icon));
        }
        tick(None, &none);
    }

    #[test]
    fn hud_shows_no_voice_server_with_disconnected_icon() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        i18n::set_language("en_us");
        let none = |_: &str| None;
        tick(None, &none);
        tick(Some(world("silent.example.net")), &none);
        // Every ask has been tried and none answered: no voice chat here.
        {
            let mut v = voice();
            let plan_len = v.session.as_ref().unwrap().plan(v.settings.protocol).len();
            v.session.as_mut().unwrap().asked = vec![proto::COMPAT_26; plan_len];
        }
        let lines = hud();
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert_eq!(lines[0].text, "No voice chat on this server");
        assert_eq!(lines[0].rgb, GREY);
        assert_eq!(lines[0].icon, Some(IC_DISCONNECTED));
        tick(None, &none);
    }

    #[test]
    fn hud_heads_lists_talking_and_muted() {
        use buf::Writer;
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        i18n::set_language("en_us");
        let none = |_: &str| None;
        tick(None, &none);
        tick(Some(world("heads.example.net")), &none);
        let mut secret = Writer::new();
        secret.raw(&[1; 16]).i32(24454).uuid(0xC).u8(0).i32(1275);
        secret
            .raw(&48f64.to_be_bytes())
            .i32(1000)
            .bool(true)
            .utf("127.0.0.1")
            .bool(false);
        {
            let mut v = voice();
            v.session.as_mut().unwrap().secret = Some(Secret::parse(&secret.finish()).unwrap());
        }
        {
            let mut v = voice();
            let s = v.session.as_mut().unwrap();
            s.insert_state(PlayerState {
                uuid: 0xA,
                name: "Talker".into(),
                muted: false,
                disabled: false,
                disconnected: false,
                group: None,
            });
            s.insert_state(PlayerState {
                uuid: 0xB,
                name: "MutedGuy".into(),
                muted: true,
                disabled: false,
                disconnected: false,
                group: None,
            });
            s.insert_state(PlayerState {
                uuid: 0xC,
                name: "Quiet".into(),
                muted: false,
                disabled: false,
                disconnected: false,
                group: None,
            });
        }
        // Talker is talking now; MutedGuy has a muted microphone.
        voice()
            .shared
            .talking
            .lock()
            .unwrap()
            .insert(0xA, (Instant::now(), false));
        let heads = hud_heads(usize::MAX);
        assert_eq!(heads.len(), 2, "{heads:?}");
        assert!(
            heads
                .iter()
                .any(|h| h.name == "Talker" && h.talking && !h.muted)
        );
        assert!(
            heads
                .iter()
                .any(|h| h.name == "MutedGuy" && !h.talking && h.muted)
        );
        assert_eq!(
            heads.iter().find(|h| h.name == "Talker").map(|h| h.uuid),
            Some(0xA)
        );
        assert_eq!(
            heads.iter().find(|h| h.name == "MutedGuy").map(|h| h.uuid),
            Some(0xB)
        );
        voice()
            .shared
            .talking
            .lock()
            .unwrap()
            .insert(0xA, (Instant::now() - Duration::from_secs(1), false));
        let recent = hud_heads(usize::MAX);
        assert!(
            recent.iter().any(|h| h.uuid == 0xA && !h.talking),
            "recent speaker did not become inactive"
        );
        voice()
            .shared
            .talking
            .lock()
            .unwrap()
            .insert(0xA, (Instant::now() - Duration::from_secs(3), false));
        assert!(
            !hud_heads(usize::MAX).iter().any(|h| h.uuid == 0xA),
            "old speaker did not expire"
        );
        voice().shared.transmitting.store(true, Ordering::Relaxed);
        tick(Some(world("heads.example.net")), &none);
        voice().shared.transmitting.store(false, Ordering::Relaxed);
        assert!(
            hud_heads(usize::MAX).iter().any(|h| h.uuid == 0xC),
            "local transmission did not add the player's own head"
        );
        // Without a voice chat server there are no heads at all.
        voice().session.as_mut().unwrap().secret = None;
        assert!(hud_heads(usize::MAX).is_empty());
        tick(None, &none);
    }

    #[test]
    fn settings_round_trip_and_cycle() {
        let mut s = Settings::default();
        s.click(0);
        s.click(3);
        s.click(6);
        // Device choices survive the round trip (None stores as "").
        s.input_device = Some("USB Mic".into());
        s.output_device = Some("HDMI Out".into());
        s.player_volumes.insert(0xAB, 70);
        let stored: HashMap<_, _> = s.stored().into_iter().collect();
        let back = Settings::load(&|k| stored.get(k).cloned());
        assert_eq!(back, s);
        assert_eq!(
            (back.mode, back.volume, back.protocol),
            (Mode::Voice, 125, Protocol::V26)
        );
        // A None device loads back from its empty string.
        let mut bare = s.clone();
        bare.output_device = None;
        let stored: HashMap<_, _> = bare.stored().into_iter().collect();
        let back = Settings::load(&|k| stored.get(k).cloned());
        assert_eq!(back.output_device, None);
        assert_eq!(key_label("key.keyboard.caps.lock"), "Caps Lock");
        assert_eq!(
            compat_key("Play.Example.net:25565"),
            "compat.play.example.net_25565"
        );
    }

    #[test]
    fn player_volumes_apply_to_any_audio_source_and_persist() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        i18n::set_language("en_us");
        use_sound_card(false);
        {
            let mut v = voice();
            v.settings.player_volumes.clear();
            v.menu_action(ui::Action::PlayerVolume(0xAB, -10));
        }
        assert_eq!(voice().settings.player_volumes.get(&0xAB), Some(&90));
        assert_eq!(
            voice()
                .shared
                .controls
                .lock()
                .unwrap()
                .player_volumes
                .get(&0xAB),
            Some(&0.9)
        );
        let stored: HashMap<_, _> = take_store().into_iter().collect();
        assert_eq!(
            stored.get("player_volumes").map(String::as_str),
            Some("000000000000000000000000000000ab:90")
        );

        voice()
            .shared
            .sources
            .lock()
            .unwrap()
            .insert(0xAB, Instant::now());
        let rows = voice().volume_sources();
        assert!(
            rows.iter()
                .any(|r| r.uuid == 0xAB && r.name == "Audio player · 000000ab" && r.volume == 90)
        );
        {
            let mut v = voice();
            v.settings.player_volumes.clear();
            v.apply_controls();
            v.shared.sources.lock().unwrap().clear();
        }
        take_store();
        use_sound_card(true);
    }

    #[test]
    fn mic_level_is_silent_before_and_after_connection() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        assert_eq!(Shared::default().level(), IDLE_LEVEL_DB);
        tick(None, &|_: &str| None);
        tick(Some(world("idle.example.net")), &|_: &str| None);
        voice()
            .shared
            .mic_level
            .store((-20.0f32).to_bits(), Ordering::Relaxed);
        assert_eq!(voice().view(Vec::new()).level_db, IDLE_LEVEL_DB);
        tick(None, &|_: &str| None);
        assert_eq!(voice().shared.level(), IDLE_LEVEL_DB);
        assert_eq!(voice().view(Vec::new()).level_db, IDLE_LEVEL_DB);
    }

    #[test]
    fn threshold_slider_drags_live_and_saves_on_release() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        use_sound_card(false);
        tick(None, &|_: &str| None);
        menu_open(Some('v'));
        voice().menu.screen = ui::Screen::Settings;
        let measure = |t: &str| t.len() as f32 * 6.0;
        menu_draw([480.0, 270.0], [0.0, 0.0], &measure, Vec::new());
        let track = voice()
            .menu
            .hit_rect(&ui::Action::ThresholdAt(0.0))
            .unwrap();
        let centre = [track[0] + track[2] / 2.0, track[1] + 5.0];
        take_ui_click();
        menu_click(centre);
        assert!(take_ui_click());
        assert!(!take_ui_click());
        menu_click([0.0, 0.0]);
        assert!(!take_ui_click());
        assert_eq!(voice().settings.threshold, -40);
        take_store();

        let outside = [track[0] + track[2] + 20.0, track[1] + 5.0];
        menu_drag(outside);
        assert_eq!(voice().settings.threshold, 0);
        assert!(take_store().is_empty());
        let draws = menu_draw([480.0, 270.0], outside, &measure, Vec::new());
        assert!(draws.iter().any(|d| matches!(d, ui::Draw::Sprite { name, .. } if *name == "gui/sprites/widget/slider_handle_highlighted")));

        menu_release();
        let stored: HashMap<_, _> = take_store().into_iter().collect();
        assert_eq!(stored.get("threshold").map(String::as_str), Some("0"));
        menu_drag([track[0] - 20.0, track[1] + 5.0]);
        assert_eq!(voice().settings.threshold, 0);
        menu_release();
        assert!(take_store().is_empty());

        voice().set_threshold(0.375);
        voice().menu.reset();
        use_sound_card(true);
    }

    #[test]
    fn pick_device_and_meter_click_update_settings_offline() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        // No sound card: changing devices or the threshold must not touch
        // the audio.
        use_sound_card(false);
        {
            let mut v = voice();
            v.settings.input_device = None;
            v.menu_action(ui::Action::PickDevice {
                input: true,
                name: Some("USB Mic".into()),
            });
            v.menu_action(ui::Action::PickDevice {
                input: false,
                name: None,
            });
            // The meter spans -80..0 dB, quietest at the left.
            v.menu_action(ui::Action::ThresholdAt(0.5));
        }
        assert_eq!(voice().settings.input_device, Some("USB Mic".into()));
        assert_eq!(voice().settings.output_device, None);
        assert_eq!(voice().settings.threshold, -40);
        voice().menu.end_threshold_drag();
        use_sound_card(true);
    }
}
