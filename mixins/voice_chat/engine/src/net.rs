//! The voice connection: a UDP socket to the server's voice port, as
//! `ClientVoicechatConnection` runs it (authenticate, then check the
//! connection, then keep it alive), and the mic thread that sends what the
//! player says.
use crate::audio::MicQueue;
use crate::mix::{Frame, Listener, Mixer, spatial};
use crate::opus::{Decoder, Encoder, FRAME};
use crate::proto::{In, Link, Out, Secret, Sound, SoundKind};
use std::collections::HashMap;
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Where a server address leads (its SRV record): the game's own resolver.
pub type Redirect = fn(&str, u16) -> (String, u16);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    PushToTalk,
    Voice,
}

/// What the player chose, read by the threads.
#[derive(Clone, Debug)]
pub struct Controls {
    pub mode: Mode,
    pub ptt: bool,
    pub whisper: bool,
    pub muted: bool,
    pub disabled: bool,
    pub threshold_db: f32,
    pub mic_gain: f32,
    pub volume: f32,
    pub player_volumes: HashMap<u128, f32>,
}

#[derive(Clone, Copy)]
struct MicControls {
    mode: Mode,
    ptt: bool,
    whisper: bool,
    muted: bool,
    disabled: bool,
    threshold_db: f32,
    mic_gain: f32,
}

impl Controls {
    fn mic(&self) -> MicControls {
        MicControls {
            mode: self.mode,
            ptt: self.ptt,
            whisper: self.whisper,
            muted: self.muted,
            disabled: self.disabled,
            threshold_db: self.threshold_db,
            mic_gain: self.mic_gain,
        }
    }
}

impl Default for Controls {
    fn default() -> Self {
        Self {
            mode: Mode::PushToTalk,
            ptt: false,
            whisper: false,
            muted: false,
            disabled: false,
            threshold_db: -50.0,
            mic_gain: 1.0,
            volume: 1.0,
            player_volumes: HashMap::new(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub enum Status {
    #[default]
    Idle,
    Connecting,
    Connected,
    Failed(String),
}

/// What the game thread and the voice threads share.
pub const IDLE_LEVEL_DB: f32 = -80.0;
const SOURCE_IDLE: Duration = Duration::from_secs(30);

#[derive(Clone, Copy)]
pub struct Activity {
    pub last: Instant,
    pub count: u32,
}

pub struct Shared {
    pub listener: Mutex<Listener>,
    /// Players' eye positions by UUID, this tick.
    pub players: Mutex<HashMap<u128, [f64; 3]>>,
    /// Who was heard last and when, and whether whispering.
    pub talking: Mutex<HashMap<u128, (Instant, bool)>>,
    pub activity: Mutex<HashMap<u128, Activity>>,
    pub controls: Mutex<Controls>,
    pub status: Mutex<Status>,
    pub mic_error: Mutex<Option<String>>,
    /// The microphone's last peak, dB as f32 bits.
    pub mic_level: AtomicU32,
    pub sources: Mutex<HashMap<u128, Instant>>,
    pub transmitting: AtomicBool,
    pub connection: AtomicU64,
}

impl Default for Shared {
    fn default() -> Self {
        Self {
            listener: Mutex::default(),
            players: Mutex::default(),
            talking: Mutex::default(),
            activity: Mutex::default(),
            controls: Mutex::default(),
            status: Mutex::default(),
            mic_error: Mutex::default(),
            mic_level: AtomicU32::new(IDLE_LEVEL_DB.to_bits()),
            sources: Mutex::default(),
            transmitting: AtomicBool::new(false),
            connection: AtomicU64::default(),
        }
    }
}

impl Shared {
    pub fn level(&self) -> f32 {
        f32::from_bits(self.mic_level.load(Ordering::Relaxed))
    }
    fn set_status(&self, s: Status) {
        *self.status.lock().unwrap() = s;
    }

    fn prune(&self, now: Instant) {
        self.talking
            .lock()
            .unwrap()
            .retain(|_, (at, _)| now - *at < Duration::from_secs(2));
        self.sources
            .lock()
            .unwrap()
            .retain(|_, at| now - *at < SOURCE_IDLE);
        self.activity
            .lock()
            .unwrap()
            .retain(|_, activity| now - activity.last < SOURCE_IDLE);
    }
}

pub struct Conn {
    shared: Arc<Shared>,
    generation: u64,
}

impl Conn {
    pub fn start(
        secret: Secret,
        server: (String, u16),
        redirect: Redirect,
        shared: Arc<Shared>,
        mixer: Arc<Mutex<Mixer>>,
        mic: Arc<MicQueue>,
    ) -> Result<Self, String> {
        let generation = shared.connection.fetch_add(1, Ordering::Relaxed) + 1;
        shared.set_status(Status::Connecting);
        let worker = shared.clone();
        std::thread::Builder::new()
            .name("voice_chat-net".into())
            .spawn(move || {
                let result = run(secret, server, redirect, &worker, &mixer, mic, generation);
                if worker.connection.load(Ordering::Relaxed) == generation {
                    mixer.lock().unwrap().clear();
                    worker.transmitting.store(false, Ordering::Relaxed);
                    worker.set_status(match result {
                        Ok(()) => Status::Idle,
                        Err(e) => Status::Failed(e),
                    });
                }
            })
            .map_err(|e| {
                let message = format!("starting network thread: {e}");
                shared.set_status(Status::Failed(message.clone()));
                message
            })?;
        Ok(Self { shared, generation })
    }
}

impl Drop for Conn {
    fn drop(&mut self) {
        if self.shared.connection.load(Ordering::Relaxed) == self.generation {
            self.shared.connection.fetch_add(1, Ordering::Relaxed);
        }
    }
}

fn resolve(host: &str, port: u16) -> Result<SocketAddr, String> {
    let addrs: Vec<SocketAddr> = (host, port)
        .to_socket_addrs()
        .map_err(|e| format!("unknown voice host {host}: {e}"))?
        .collect();
    addrs
        .iter()
        .find(|a| a.is_ipv4())
        .or(addrs.first())
        .copied()
        .ok_or_else(|| format!("unknown voice host {host}"))
}

fn run(
    secret: Secret,
    (server_host, server_port): (String, u16),
    redirect: Redirect,
    shared: &Arc<Shared>,
    mixer: &Arc<Mutex<Mixer>>,
    mic: Arc<MicQueue>,
    generation: u64,
) -> Result<(), String> {
    // The game connected to the SRV target, if any: so does the voice.
    let (game_host, _) = redirect(&server_host, server_port);
    let (host, port) = secret.voice_address(&game_host);
    let addr = resolve(&host, port)?;
    let socket = UdpSocket::bind(if addr.is_ipv4() {
        "0.0.0.0:0"
    } else {
        "[::]:0"
    })
    .map_err(|e| format!("no UDP socket: {e}"))?;
    socket
        .connect(addr)
        .map_err(|e| format!("voice server {addr}: {e}"))?;
    socket
        .set_read_timeout(Some(Duration::from_millis(50)))
        .map_err(|e| e.to_string())?;
    let keep_alive = Duration::from_millis(u64::from(secret.keep_alive) * 10);
    let own = secret.player;
    let link = Arc::new(Link::new(secret));
    let connected = Arc::new(AtomicBool::new(false));

    // The mic thread stops with this one, whatever ends it.
    let quit = Arc::new(AtomicBool::new(false));
    let mic_thread = {
        let (socket, link, shared, connected, quit) = (
            socket.try_clone().map_err(|e| e.to_string())?,
            link.clone(),
            shared.clone(),
            connected.clone(),
            quit.clone(),
        );
        std::thread::Builder::new()
            .name("voice_chat-mic".into())
            .spawn(move || mic_loop(&socket, &link, &shared, &mic, &quit, &connected))
            .map_err(|e| format!("starting microphone thread: {e}"))?
    };

    let send = |p: &Out| {
        let _ = socket.send(&link.write(p));
    };
    let mut authenticated = false;
    let mut last_try: Option<Instant> = None;
    let mut last_keep_alive = Instant::now();
    let mut channels: HashMap<u128, Channel> = HashMap::new();
    let mut buf = vec![0u8; 4096];
    let result = loop {
        if shared.connection.load(Ordering::Relaxed) != generation {
            break Ok(());
        }
        let now = Instant::now();
        if !connected.load(Ordering::Relaxed) {
            if last_try.is_none_or(|t| now - t >= Duration::from_secs(1)) {
                last_try = Some(now);
                send(if authenticated {
                    &Out::ConnectionCheck
                } else {
                    &Out::Authenticate
                });
            }
        } else if now - last_keep_alive > keep_alive {
            break Err("the voice server stopped answering".into());
        }
        shared.prune(now);
        channels.retain(|_, c| now - c.used < SOURCE_IDLE);
        let n = match socket.recv(&mut buf) {
            Ok(n) => n,
            // Timeouts, and ICMP's "unreachable" until the server is up.
            Err(_) => continue,
        };
        let Ok(Some(packet)) = link.read(&buf[..n]) else {
            continue;
        };
        match packet {
            In::AuthenticateAck { nonce } => {
                if !authenticated {
                    authenticated = true;
                    *link.nonce.lock().unwrap() = nonce;
                    last_try = None;
                }
            }
            In::ConnectionCheckAck => {
                if authenticated && !connected.swap(true, Ordering::Relaxed) {
                    shared.set_status(Status::Connected);
                    last_keep_alive = now;
                }
            }
            In::KeepAlive => {
                last_keep_alive = now;
                send(&Out::KeepAlive);
            }
            In::Ping { id, timestamp } => send(&Out::Ping { id, timestamp }),
            In::Sound(sound) => {
                if !shared.controls.lock().unwrap().disabled {
                    play(sound, own, shared, mixer, &mut channels);
                }
            }
        }
    };
    connected.store(false, Ordering::Relaxed);
    quit.store(true, Ordering::Relaxed);
    let _ = mic_thread.join();
    result
}

struct Channel {
    decoder: Decoder,
    last: i64,
    used: Instant,
}

/// `AudioChannel.run` and `writeToSpeaker`: decode in order, conceal what
/// was lost, and place the voice where its sender is.
fn play(
    s: Sound,
    own: u128,
    shared: &Shared,
    mixer: &Mutex<Mixer>,
    channels: &mut HashMap<u128, Channel>,
) {
    let now = Instant::now();
    shared.sources.lock().unwrap().insert(s.sender, now);
    let ch = match channels.entry(s.channel) {
        std::collections::hash_map::Entry::Occupied(e) => e.into_mut(),
        std::collections::hash_map::Entry::Vacant(e) => match Decoder::new() {
            Ok(decoder) => e.insert(Channel {
                decoder,
                last: -1,
                used: now,
            }),
            Err(_) => return,
        },
    };
    ch.used = now;
    if s.data.is_empty() {
        // The sender stopped talking.
        ch.last = -1;
        ch.decoder.reset();
        mixer.lock().unwrap().end(s.channel);
        shared.talking.lock().unwrap().remove(&s.sender);
        return;
    }
    let mut frames = Vec::new();
    if s.sequence < 0 {
        // A plugin's raw 16 bit little-endian PCM.
        let pcm: Vec<i16> = s
            .data
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| i16::from_le_bytes(*b))
            .collect();
        frames.extend(pcm.chunks(FRAME).map(|c| {
            let mut f = c.to_vec();
            f.resize(FRAME, 0);
            f
        }));
    } else {
        if ch.last >= 0 && s.sequence <= ch.last {
            return;
        }
        let mut lost = if ch.last >= 0 {
            s.sequence - ch.last - 1
        } else {
            0
        };
        if lost > 5 {
            ch.decoder.reset();
            lost = 0;
        }
        for i in 0..lost {
            frames.push(if i == lost - 1 {
                ch.decoder.decode(Some(&s.data), true)
            } else {
                ch.decoder.decode(None, false)
            });
        }
        frames.push(ch.decoder.decode(Some(&s.data), false));
        ch.last = s.sequence;
    }

    let (volume, listener) = {
        let c = shared.controls.lock().unwrap();
        let v = c.volume * c.player_volumes.get(&s.sender).copied().unwrap_or(1.0);
        (v, *shared.listener.lock().unwrap())
    };
    let gain = match s.kind {
        SoundKind::Group => Some([1.0, 1.0]),
        SoundKind::Entity { .. } if s.sender == own => Some([1.0, 1.0]),
        SoundKind::Entity { distance, .. } => {
            let at = shared.players.lock().unwrap().get(&s.sender).copied();
            at.and_then(|at| spatial(&listener, at, distance))
        }
        SoundKind::Location { at, distance } => spatial(&listener, at, distance),
    };
    let Some(gain) = gain else { return };
    let gain = gain.map(|g| g * volume);
    let mut m = mixer.lock().unwrap();
    for f in frames {
        m.push(
            s.channel,
            Frame {
                pcm: f.iter().map(|&v| f32::from(v) / 32768.0).collect(),
                gain,
            },
        );
    }
    drop(m);
    if gain.iter().any(|g| *g > 0.0) {
        let whispering = matches!(
            s.kind,
            SoundKind::Entity {
                whispering: true,
                ..
            }
        );
        shared
            .talking
            .lock()
            .unwrap()
            .insert(s.sender, (now, whispering));
        let mut activity = shared.activity.lock().unwrap();
        let entry = activity.entry(s.sender).or_insert(Activity {
            last: now,
            count: 0,
        });
        entry.last = now;
        entry.count = entry.count.saturating_add(1);
    }
}

/// `MicThread`: each 20 ms frame of the microphone, sent while the push to
/// talk key is held or the voice is loud enough, and a little after.
fn mic_loop(
    socket: &UdpSocket,
    link: &Link,
    shared: &Shared,
    mic: &MicQueue,
    stop: &AtomicBool,
    connected: &AtomicBool,
) {
    let mut encoder = match Encoder::new(link.secret.opus_application(), link.secret.mtu) {
        Ok(e) => e,
        Err(e) => {
            *shared.mic_error.lock().unwrap() = Some(e);
            return;
        }
    };
    let mut sequence: i64 = 0;
    let mut sending = false;
    let mut hold = 0u32;
    let send = |p: &Out| {
        let _ = socket.send(&link.write(p));
    };
    while !stop.load(Ordering::Relaxed) {
        let frame: Vec<f32> = {
            let mut q = mic.samples.lock().unwrap();
            if q.len() < FRAME {
                q = mic
                    .ready
                    .wait_timeout(q, Duration::from_millis(100))
                    .unwrap()
                    .0;
            }
            if q.len() < FRAME {
                drop(q);
                if sending {
                    send(&Out::Mic {
                        data: Vec::new(),
                        sequence,
                        whispering: false,
                    });
                    sequence += 1;
                    encoder.reset();
                    sending = false;
                    hold = 0;
                }
                shared.transmitting.store(false, Ordering::Relaxed);
                shared
                    .mic_level
                    .store(IDLE_LEVEL_DB.to_bits(), Ordering::Relaxed);
                continue;
            }
            q.drain(..FRAME).collect()
        };
        let c = shared.controls.lock().unwrap().mic();
        let mut pcm = [0i16; FRAME];
        let mut peak = 0f32;
        for (o, s) in pcm.iter_mut().zip(&frame) {
            let v = (s * c.mic_gain).clamp(-1.0, 1.0);
            peak = peak.max(v.abs());
            *o = (v * 32767.0) as i16;
        }
        let db = if peak > 0.0 {
            (20.0 * peak.log10()).max(-127.0)
        } else {
            -127.0
        };
        shared.mic_level.store(db.to_bits(), Ordering::Relaxed);

        let can = connected.load(Ordering::Relaxed) && !c.muted && !c.disabled;
        let mut talk = can
            && match c.mode {
                Mode::PushToTalk => c.ptt || c.whisper,
                Mode::Voice => db > c.threshold_db,
            };
        if talk {
            hold = if c.mode == Mode::PushToTalk { 5 } else { 25 };
        } else if can && hold > 0 {
            hold -= 1;
            talk = true;
        }
        if talk {
            if let Ok(data) = encoder.encode(&pcm) {
                send(&Out::Mic {
                    data,
                    sequence,
                    whispering: c.whisper,
                });
                sequence += 1;
                sending = true;
            }
        } else if sending {
            send(&Out::Mic {
                data: Vec::new(),
                sequence,
                whispering: false,
            });
            sequence += 1;
            encoder.reset();
            sending = false;
            hold = 0;
        }
        shared.transmitting.store(sending, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activity_state_expires_independently_of_end_packets() {
        let shared = Shared::default();
        let now = Instant::now();
        let old = now - SOURCE_IDLE - Duration::from_secs(1);
        shared.talking.lock().unwrap().insert(1, (old, false));
        shared.talking.lock().unwrap().insert(2, (now, false));
        shared.sources.lock().unwrap().insert(1, old);
        shared.sources.lock().unwrap().insert(2, now);
        shared.activity.lock().unwrap().insert(
            1,
            Activity {
                last: old,
                count: 1,
            },
        );
        shared.activity.lock().unwrap().insert(
            2,
            Activity {
                last: now,
                count: 1,
            },
        );

        shared.prune(now);

        assert!(!shared.talking.lock().unwrap().contains_key(&1));
        assert!(shared.talking.lock().unwrap().contains_key(&2));
        assert!(!shared.sources.lock().unwrap().contains_key(&1));
        assert!(shared.sources.lock().unwrap().contains_key(&2));
        assert!(!shared.activity.lock().unwrap().contains_key(&1));
        assert!(shared.activity.lock().unwrap().contains_key(&2));
    }
}
