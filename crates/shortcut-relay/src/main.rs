//! shortcut-relay: forwards one game's UDP packets for Shortcut players.
//!
//! A session only ever sends to the one address it registered (a game
//! server, or the next relay), only to a public address, and only at game
//! rate. `shortcut_core::Session` is the rule, `shortcut_core::wire` the frames.
//! It keeps no logs of who connects.
//!
//! Usage: shortcut-relay [config]        (default: /etc/shortcut-relay.conf)

use shortcut_core::wire::{self, Frame, Mode};
use shortcut_core::{Dedup, RATE_BYTES, Refused, Session};
use std::collections::HashMap;
use std::hash::{BuildHasher, RandomState};
use std::io::{self, ErrorKind};
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, UdpSocket};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// A session with no packets from its player for this long is closed.
const SESSION_IDLE: Duration = Duration::from_secs(30);
/// Replies go to every address the player's packets arrived from this recently.
const PATH_IDLE: Duration = Duration::from_secs(5);
const MAX_PATHS: usize = 3;

#[derive(Clone, Debug)]
struct Config {
    listen: SocketAddr,
    name: String,
    max_sessions: usize,
    rate_kbit: f64,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            listen: (Ipv4Addr::UNSPECIFIED, 7400).into(),
            name: "relay".into(),
            max_sessions: 64,
            rate_kbit: RATE_BYTES * 8.0 / 1000.0,
        }
    }
}

/// `key = value` lines, `#` comments.
fn parse_config(text: &str) -> Result<Config, String> {
    let mut c = Config::default();
    for (n, raw) in text.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let (key, value) = line
            .split_once('=')
            .ok_or_else(|| format!("line {}: expected `key = value`", n + 1))?;
        let (key, value) = (key.trim(), value.trim().trim_matches('"'));
        let bad = || format!("line {}: bad value for `{key}`", n + 1);
        match key {
            "listen" => c.listen = value.parse().map_err(|_| bad())?,
            "name" => c.name = value.to_string(),
            "max_sessions" => c.max_sessions = value.parse().map_err(|_| bad())?,
            "rate_kbit" => c.rate_kbit = value.parse().map_err(|_| bad())?,
            _ => return Err(format!("line {}: unknown key `{key}`", n + 1)),
        }
    }
    Ok(c)
}

/// What both directions of a session share.
struct Shared {
    rule: Session,
    paths: Vec<(SocketAddr, Instant)>,
    /// Hop mode: our cookie at the next relay, once it has answered.
    next_cookie: u64,
    /// Exit mode: sequence numbers for replies, so the player can drop duplicates.
    seq: u32,
    closed: bool,
}

impl Shared {
    fn touch(&mut self, from: SocketAddr, now: Instant) {
        if let Some(p) = self.paths.iter_mut().find(|p| p.0 == from) {
            p.1 = now;
            return;
        }
        self.paths.retain(|p| now - p.1 < PATH_IDLE);
        if self.paths.len() < MAX_PATHS {
            self.paths.push((from, now));
        }
    }
}

struct Sess {
    mode: Mode,
    up: UdpSocket,
    dedup: Dedup,
    shared: Arc<Mutex<Shared>>,
    last: Instant,
}

struct Relay {
    cfg: Config,
    sock: UdpSocket,
    keys: RandomState,
    sessions: HashMap<u64, Sess>,
    start: Instant,
    swept: Instant,
}

impl Relay {
    fn bind(cfg: Config) -> io::Result<Relay> {
        let sock = UdpSocket::bind(cfg.listen)?;
        sock.set_read_timeout(Some(Duration::from_secs(1)))?;
        let now = Instant::now();
        Ok(Relay {
            cfg,
            sock,
            keys: RandomState::new(),
            sessions: HashMap::new(),
            start: now,
            swept: now,
        })
    }

    /// Only someone who receives at `addr` can know this. It stops anyone
    /// from opening a session in a spoofed player's name.
    fn cookie(&self, addr: SocketAddr) -> u64 {
        self.keys.hash_one(addr) | 1
    }

    fn run(&mut self) -> io::Result<()> {
        let mut buf = [0u8; 2048];
        loop {
            match self.sock.recv_from(&mut buf) {
                Ok((n, from)) => self.handle(from, &buf[..n]),
                Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
                Err(e) => return Err(e),
            }
            if self.swept.elapsed() > Duration::from_secs(1) {
                self.sweep(Instant::now());
            }
        }
    }

    fn handle(&mut self, from: SocketAddr, buf: &[u8]) {
        let now = Instant::now();
        match wire::parse(buf) {
            Some(Frame::Hello { .. }) => self.reply(&wire::hello(self.cookie(from)), from),
            Some(Frame::Probe) => self.reply(buf, from),
            Some(Frame::Open {
                cookie,
                session,
                mode,
                target,
            }) if cookie == self.cookie(from) => {
                if let Err(why) = self.open(session, mode, target, from, now) {
                    self.reply(&wire::refused(session, why), from);
                }
            }
            Some(Frame::Data {
                cookie,
                session,
                seq,
                payload,
            }) if cookie == self.cookie(from) => {
                self.forward(session, seq, payload, buf, from, now);
            }
            // Unknown, or the cookie is wrong: drop it without a word.
            _ => {}
        }
    }

    fn reply(&self, frame: &[u8], to: SocketAddr) {
        let _ = self.sock.send_to(frame, to);
    }

    fn open(
        &mut self,
        id: u64,
        mode: Mode,
        target: SocketAddr,
        from: SocketAddr,
        now: Instant,
    ) -> Result<(), Refused> {
        if let Some(s) = self.sessions.get_mut(&id) {
            let mut shared = s.shared.lock().unwrap();
            if shared.rule.target() != target {
                return Err(Refused::OtherTarget);
            }
            shared.touch(from, now);
            s.last = now;
            return Ok(());
        }
        if self.sessions.len() >= self.cfg.max_sessions {
            return Err(Refused::Full);
        }
        let rule = Session::open(target, self.cfg.rate_kbit * 125.0, now - self.start)?;
        let Ok((up, back, sock)) =
            upstream(target).and_then(|up| Ok((up.try_clone()?, up, self.sock.try_clone()?)))
        else {
            eprintln!("can't reach {target}");
            return Ok(());
        };
        if mode == Mode::Hop {
            let _ = up.send(&wire::hello(0));
        }
        let shared = Arc::new(Mutex::new(Shared {
            rule,
            paths: vec![(from, now)],
            next_cookie: 0,
            seq: 0,
            closed: false,
        }));
        let (s2, start) = (shared.clone(), self.start);
        thread::spawn(move || back_loop(id, mode, back, sock, s2, start));
        self.sessions.insert(
            id,
            Sess {
                mode,
                up,
                dedup: Dedup::new(),
                shared,
                last: now,
            },
        );
        eprintln!("session opened, {} active", self.sessions.len());
        Ok(())
    }

    fn forward(
        &mut self,
        id: u64,
        seq: u32,
        payload: &[u8],
        frame: &[u8],
        from: SocketAddr,
        now: Instant,
    ) {
        let elapsed = now - self.start;
        let Some(s) = self.sessions.get_mut(&id) else {
            return;
        };
        let next_cookie = {
            let mut shared = s.shared.lock().unwrap();
            shared.touch(from, now);
            if shared.rule.charge(frame.len(), elapsed).is_err() {
                return;
            }
            shared.next_cookie
        };
        s.last = now;
        let _ = match s.mode {
            // Keep the first copy of each packet. The game server sees one stream.
            Mode::Exit if s.dedup.accept(seq) => s.up.send(payload),
            Mode::Hop if next_cookie != 0 => {
                let mut f = frame.to_vec();
                wire::set_cookie(&mut f, next_cookie);
                s.up.send(&f)
            }
            _ => return,
        };
    }

    fn sweep(&mut self, now: Instant) {
        self.swept = now;
        let before = self.sessions.len();
        self.sessions.retain(|_, s| {
            let keep = now - s.last < SESSION_IDLE;
            if !keep {
                s.shared.lock().unwrap().closed = true;
            }
            keep
        });
        if self.sessions.len() < before {
            eprintln!("session closed, {} active", self.sessions.len());
        }
    }
}

/// Each session gets a socket of its own, connected to its one target, so
/// replies can only come from that address.
fn upstream(target: SocketAddr) -> io::Result<UdpSocket> {
    let any: SocketAddr = if target.is_ipv4() {
        (Ipv4Addr::UNSPECIFIED, 0).into()
    } else {
        (Ipv6Addr::UNSPECIFIED, 0).into()
    };
    let up = UdpSocket::bind(any)?;
    up.connect(target)?;
    Ok(up)
}

/// Replies from the target go back to the player on every live path.
fn back_loop(
    id: u64,
    mode: Mode,
    up: UdpSocket,
    sock: UdpSocket,
    shared: Arc<Mutex<Shared>>,
    start: Instant,
) {
    let _ = up.set_read_timeout(Some(Duration::from_secs(1)));
    let mut buf = [0u8; 2048];
    loop {
        let got = up.recv(&mut buf);
        let now = Instant::now();
        let mut s = shared.lock().unwrap();
        if s.closed {
            return;
        }
        let Ok(n) = got else { continue };
        let frame = match (mode, wire::parse(&buf[..n])) {
            (Mode::Hop, Some(Frame::Hello { cookie })) => {
                s.next_cookie = cookie;
                continue;
            }
            (Mode::Hop, Some(Frame::Data { .. })) => buf[..n].to_vec(),
            (Mode::Hop, _) => continue,
            (Mode::Exit, _) => {
                s.seq = s.seq.wrapping_add(1);
                wire::data(0, id, s.seq, &buf[..n])
            }
        };
        if s.rule.charge(frame.len(), now - start).is_err() {
            continue;
        }
        for &(path, seen) in &s.paths {
            if now - seen < PATH_IDLE {
                let _ = sock.send_to(&frame, path);
            }
        }
    }
}

fn main() {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/etc/shortcut-relay.conf".into());
    let cfg = match std::fs::read_to_string(&path) {
        Ok(text) => parse_config(&text),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(Config::default()),
        Err(e) => Err(format!("{path}: {e}")),
    }
    .unwrap_or_else(|e| {
        eprintln!("shortcut-relay: {e}");
        std::process::exit(2)
    });
    let mut relay = Relay::bind(cfg.clone()).unwrap_or_else(|e| {
        eprintln!("shortcut-relay: can't listen on {}: {e}", cfg.listen);
        std::process::exit(1)
    });
    eprintln!(
        "shortcut-relay {} on udp {}, {} kbit/s a session, {} sessions max",
        cfg.name, cfg.listen, cfg.rate_kbit, cfg.max_sessions
    );
    if let Err(e) = relay.run() {
        eprintln!("shortcut-relay: {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_its_config() {
        let c = parse_config("# my relay\nname = \"my-relay\"\nlisten = 0.0.0.0:7400\nrate_kbit = 800  # a session\n").unwrap();
        assert_eq!(
            (
                c.name.as_str(),
                c.listen.port(),
                c.rate_kbit,
                c.max_sessions
            ),
            ("my-relay", 7400, 800.0, 64)
        );
        assert!(parse_config("open_proxy = yes").is_err());
        assert!(parse_config("listen = everywhere").is_err());
    }

    /// Sends one frame from the player, lets the relay handle it, returns its answer.
    fn ask(relay: &mut Relay, player: &UdpSocket, frame: &[u8]) -> Option<Vec<u8>> {
        player
            .send_to(frame, relay.sock.local_addr().unwrap())
            .unwrap();
        let mut buf = [0u8; 2048];
        let (n, from) = relay.sock.recv_from(&mut buf).unwrap();
        relay.handle(from, &buf[..n]);
        player.recv(&mut buf).ok().map(|n| buf[..n].to_vec())
    }

    #[test]
    fn cookie_first_and_never_a_private_target() {
        let mut relay = Relay::bind(Config {
            listen: "127.0.0.1:0".parse().unwrap(),
            ..Config::default()
        })
        .unwrap();
        let player = UdpSocket::bind("127.0.0.1:0").unwrap();
        player
            .set_read_timeout(Some(Duration::from_millis(200)))
            .unwrap();
        let lan: SocketAddr = "192.168.1.20:7000".parse().unwrap();

        // Probes come straight back, so the race can time this relay.
        assert_eq!(
            ask(&mut relay, &player, &[wire::PROBE, 1, 2, 3]).as_deref(),
            Some(&[wire::PROBE, 1, 2, 3][..])
        );
        // No cookie, no session, and not a word back.
        assert_eq!(
            ask(&mut relay, &player, &wire::open(12345, 7, Mode::Exit, lan)),
            None
        );
        // The cookie answer is no bigger than the question.
        let hello = ask(&mut relay, &player, &wire::hello(0)).expect("a cookie");
        assert_eq!(hello.len(), wire::hello(0).len());
        let Some(Frame::Hello { cookie }) = wire::parse(&hello) else {
            panic!("not a hello")
        };
        // With the cookie, a home network or a DNS port is still refused.
        assert_eq!(
            ask(&mut relay, &player, &wire::open(cookie, 7, Mode::Exit, lan)),
            Some(wire::refused(7, Refused::NotPublic))
        );
        let dns: SocketAddr = "45.10.20.30:53".parse().unwrap();
        assert_eq!(
            ask(&mut relay, &player, &wire::open(cookie, 8, Mode::Exit, dns)),
            Some(wire::refused(8, Refused::Port))
        );
        // Data for a session that was never opened goes nowhere.
        assert_eq!(
            ask(&mut relay, &player, &wire::data(cookie, 9, 1, b"tick")),
            None
        );
        assert!(relay.sessions.is_empty());
    }
}
