//! Shortcut core: how a route is scored, when to switch, multipath
//! de-duplication, and the rule that keeps a relay from becoming an open
//! proxy. Plain std, shared by the desktop app and the relay.

use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

// ---------------------------------------------------------------------------
// Route scoring
// ---------------------------------------------------------------------------

/// Jitter costs more than ping: the game buffers for the worst case.
pub const JITTER_WEIGHT: f64 = 1.5;
/// Each 1% of loss feels like 10 ms more ping. Every lost packet is a stutter.
pub const LOSS_MS_PER_PERCENT: f64 = 10.0;
/// A probe this much slower than the median counts as a spike.
pub const SPIKE_MS: f64 = 50.0;

/// What one race round measured on one path.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stats {
    /// Median round trip, in ms.
    pub ping_ms: f64,
    /// Mean change between consecutive round trips, in ms.
    pub jitter_ms: f64,
    /// Share of probes that never came back, 0.0 to 1.0.
    pub loss: f64,
    /// Probes that were lost, or more than `SPIKE_MS` over the median.
    pub spikes: usize,
}

impl Stats {
    /// From one round of probes, in the order they were sent. `None` is a
    /// probe that never came back. `None` overall means the path is down.
    pub fn from_probes(rtts: &[Option<f64>]) -> Option<Stats> {
        let got: Vec<f64> = rtts.iter().flatten().copied().collect();
        if got.is_empty() {
            return None;
        }
        let mut sorted = got.clone();
        sorted.sort_by(f64::total_cmp);
        let mid = sorted.len() / 2;
        let ping_ms = if sorted.len() % 2 == 1 {
            sorted[mid]
        } else {
            (sorted[mid - 1] + sorted[mid]) / 2.0
        };
        let jitter_ms = if got.len() < 2 {
            0.0
        } else {
            got.windows(2).map(|w| (w[1] - w[0]).abs()).sum::<f64>() / (got.len() - 1) as f64
        };
        let lost = rtts.len() - got.len();
        let late = got.iter().filter(|&&r| r > ping_ms + SPIKE_MS).count();
        Some(Stats {
            ping_ms,
            jitter_ms,
            loss: lost as f64 / rtts.len() as f64,
            spikes: lost + late,
        })
    }

    /// One number to race on, in felt milliseconds. Lower wins.
    pub fn score(&self) -> f64 {
        self.ping_ms + JITTER_WEIGHT * self.jitter_ms + LOSS_MS_PER_PERCENT * self.loss * 100.0
    }
}

/// A challenger must beat the current route by this many ms...
pub const SWITCH_MARGIN_MS: f64 = 8.0;
/// ...or by this share of the current score, whichever is bigger...
pub const SWITCH_MARGIN_SHARE: f64 = 0.10;
/// ...for this many rounds in a row. Rounds run every 2 seconds.
pub const SWITCH_ROUNDS: u32 = 3;

/// Picks the route to use, round after round, without flapping.
#[derive(Clone, Debug)]
pub struct Switch {
    current: usize,
    challenger: Option<usize>,
    streak: u32,
}

impl Switch {
    pub fn new(start: usize) -> Self {
        Switch {
            current: start,
            challenger: None,
            streak: 0,
        }
    }

    pub fn current(&self) -> usize {
        self.current
    }

    /// Feeds one round: a score per route, `None` for a route that is down.
    /// Returns the route to use from now on.
    pub fn round(&mut self, scores: &[Option<f64>]) -> usize {
        let best = scores
            .iter()
            .enumerate()
            .filter_map(|(i, s)| s.map(|s| (i, s)))
            .min_by(|a, b| a.1.total_cmp(&b.1));
        let Some((best, best_score)) = best else {
            return self.current;
        };
        match scores.get(self.current).copied().flatten() {
            // The current route went down: move at once.
            None => self.move_to(best),
            Some(cur)
                if best != self.current
                    && cur - best_score >= SWITCH_MARGIN_MS.max(cur * SWITCH_MARGIN_SHARE) =>
            {
                if self.challenger == Some(best) {
                    self.streak += 1;
                } else {
                    self.challenger = Some(best);
                    self.streak = 1;
                }
                if self.streak >= SWITCH_ROUNDS {
                    self.move_to(best);
                }
            }
            _ => {
                self.challenger = None;
                self.streak = 0;
            }
        }
        self.current
    }

    fn move_to(&mut self, route: usize) {
        self.current = route;
        self.challenger = None;
        self.streak = 0;
    }
}

// ---------------------------------------------------------------------------
// Multipath
// ---------------------------------------------------------------------------

/// How many recent sequence numbers `Dedup` remembers.
pub const WINDOW: u32 = 1024;

/// Every packet travels two ways at once. The far end keeps the first copy of
/// each sequence number and drops the rest, including anything older than
/// the window (a copy that late is no use to a game anyway).
#[derive(Clone, Debug, Default)]
pub struct Dedup {
    top: Option<u32>,
    seen: [u64; (WINDOW / 64) as usize],
}

impl Dedup {
    pub fn new() -> Self {
        Self::default()
    }

    /// True for the first copy of `seq`. False for a duplicate, or a packet
    /// too old to tell. Sequence numbers may wrap around.
    pub fn accept(&mut self, seq: u32) -> bool {
        let Some(top) = self.top else {
            self.top = Some(seq);
            self.flip(seq, true);
            return true;
        };
        let ahead = seq.wrapping_sub(top);
        if ahead != 0 && ahead < u32::MAX / 2 {
            // Newer than anything so far: slide the window forward.
            if ahead >= WINDOW {
                self.seen = [0; (WINDOW / 64) as usize];
            } else {
                (1..=ahead).for_each(|s| self.flip(top.wrapping_add(s), false));
            }
            self.top = Some(seq);
            self.flip(seq, true);
            return true;
        }
        if top.wrapping_sub(seq) >= WINDOW || self.has(seq) {
            return false;
        }
        self.flip(seq, true);
        true
    }

    fn slot(seq: u32) -> (usize, u64) {
        let i = seq % WINDOW;
        ((i / 64) as usize, 1 << (i % 64))
    }

    fn has(&self, seq: u32) -> bool {
        let (w, bit) = Self::slot(seq);
        self.seen[w] & bit != 0
    }

    fn flip(&mut self, seq: u32, on: bool) {
        let (w, bit) = Self::slot(seq);
        if on {
            self.seen[w] |= bit
        } else {
            self.seen[w] &= !bit
        }
    }
}

// ---------------------------------------------------------------------------
// The relay rule
// ---------------------------------------------------------------------------

/// A session may forward this many bytes a second: 1 Mbit/s, several times
/// what a shooter sends, and useless for anything else.
pub const RATE_BYTES: f64 = 125_000.0;
/// Short bursts allowed on top, in bytes.
pub const BURST_BYTES: f64 = 32_000.0;
/// Each packet also pays for its IP and UDP headers, so floods of tiny
/// packets hit the limit too.
pub const HEADER_BYTES: usize = 28;
/// Ports a relay never sends to, even above 1024: services that answer a
/// small request with a big reply and get abused for floods.
pub const BLOCKED_PORTS: [u16; 8] = [1434, 1900, 3283, 3702, 5353, 5683, 10001, 11211];

/// Why a relay said no.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refused {
    /// Private, loopback, multicast or reserved address.
    NotPublic = 1,
    /// Below 1024, or a blocked service port.
    Port = 2,
    /// The session is already bound to a different address.
    OtherTarget = 3,
    /// Over the session's rate.
    OverRate = 4,
    /// The relay has no room for another session.
    Full = 5,
}

/// May a relay forward to `target`? Only to a public unicast address on a
/// port games use. Web traffic is TCP, which a relay never carries at all.
pub fn allowed_target(target: SocketAddr) -> Result<(), Refused> {
    if target.port() < 1024 || BLOCKED_PORTS.contains(&target.port()) {
        return Err(Refused::Port);
    }
    if !is_public(target.ip()) {
        return Err(Refused::NotPublic);
    }
    Ok(())
}

pub fn is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let [a, b, ..] = v4.octets();
            !(v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || v4.is_multicast()
                || v4.is_unspecified()
                || v4.is_documentation()
                || a == 0
                || a >= 240
                || (a == 100 && (64..128).contains(&b))
                || (a == 198 && (b == 18 || b == 19)))
        }
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => is_public(IpAddr::V4(v4)),
            None => {
                let [s0, s1, ..] = v6.segments();
                !(v6.is_loopback()
                    || v6.is_unspecified()
                    || v6.is_multicast()
                    || s0 & 0xfe00 == 0xfc00
                    || s0 & 0xffc0 == 0xfe80
                    || (s0 == 0x2001 && s1 == 0x0db8))
            }
        },
    }
}

/// One relay session: the single address its client registered, and a token
/// bucket shared by both directions.
#[derive(Clone, Debug)]
pub struct Session {
    target: SocketAddr,
    rate: f64,
    tokens: f64,
    last: Duration,
}

impl Session {
    /// `now` is time since the relay started.
    pub fn open(target: SocketAddr, rate: f64, now: Duration) -> Result<Session, Refused> {
        allowed_target(target)?;
        Ok(Session {
            target,
            rate,
            tokens: BURST_BYTES,
            last: now,
        })
    }

    pub fn target(&self) -> SocketAddr {
        self.target
    }

    /// May `len` bytes go to `to` now? Only the registered target, within the rate.
    pub fn check(&mut self, to: SocketAddr, len: usize, now: Duration) -> Result<(), Refused> {
        if to != self.target {
            return Err(Refused::OtherTarget);
        }
        self.charge(len, now)
    }

    /// Takes `len` bytes from the bucket, or refuses if it would run dry.
    pub fn charge(&mut self, len: usize, now: Duration) -> Result<(), Refused> {
        let dt = now.saturating_sub(self.last).as_secs_f64();
        self.last = self.last.max(now);
        self.tokens = (self.tokens + dt * self.rate).min(BURST_BYTES);
        let cost = (len + HEADER_BYTES) as f64;
        if self.tokens < cost {
            return Err(Refused::OverRate);
        }
        self.tokens -= cost;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Wire format
// ---------------------------------------------------------------------------

/// The relay protocol. Every datagram starts with a kind byte.
///
/// | Kind | Body | Notes |
/// |---|---|---|
/// | 1 hello | cookie `u64` | Sent with a zero cookie; the relay answers with the cookie for your address. Same size both ways, so it can't amplify. |
/// | 2 open | cookie, session `u64`, mode `u8`, target | Binds a session to one address. A second open from a new address joins as another path. |
/// | 3 data | cookie, session, seq `u32`, payload | The cookie is 0 on the way back. |
/// | 4 probe | up to 64 bytes | Echoed unchanged. The route race times these. |
/// | 5 refused | session, reason `u8` | |
///
/// Targets are a family byte (4 or 6), the address and a big-endian port.
pub mod wire {
    use super::Refused;
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

    pub const HELLO: u8 = 1;
    pub const OPEN: u8 = 2;
    pub const DATA: u8 = 3;
    pub const PROBE: u8 = 4;
    pub const REFUSED: u8 = 5;
    /// Bytes in front of the payload in a data frame.
    pub const DATA_HEADER: usize = 21;

    /// What a relay does with a session's packets.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Mode {
        /// Last relay: unwrap the payload and send it to the game server.
        Exit = 0,
        /// Middle relay: pass the whole frame on to the next relay.
        Hop = 1,
    }

    #[derive(Debug, PartialEq)]
    pub enum Frame<'a> {
        Hello {
            cookie: u64,
        },
        Open {
            cookie: u64,
            session: u64,
            mode: Mode,
            target: SocketAddr,
        },
        Data {
            cookie: u64,
            session: u64,
            seq: u32,
            payload: &'a [u8],
        },
        Probe,
        Refused {
            session: u64,
            reason: u8,
        },
    }

    fn u64_at(b: &[u8], at: usize) -> Option<u64> {
        Some(u64::from_be_bytes(b.get(at..at + 8)?.try_into().ok()?))
    }

    pub fn parse(b: &[u8]) -> Option<Frame<'_>> {
        match *b.first()? {
            HELLO if b.len() == 9 => Some(Frame::Hello {
                cookie: u64_at(b, 1)?,
            }),
            OPEN => {
                let mode = match *b.get(17)? {
                    0 => Mode::Exit,
                    1 => Mode::Hop,
                    _ => return None,
                };
                let ip: IpAddr = match (*b.get(18)?, b.len()) {
                    (4, 25) => Ipv4Addr::from(<[u8; 4]>::try_from(&b[19..23]).ok()?).into(),
                    (6, 37) => Ipv6Addr::from(<[u8; 16]>::try_from(&b[19..35]).ok()?).into(),
                    _ => return None,
                };
                let port = u16::from_be_bytes([b[b.len() - 2], b[b.len() - 1]]);
                Some(Frame::Open {
                    cookie: u64_at(b, 1)?,
                    session: u64_at(b, 9)?,
                    mode,
                    target: SocketAddr::new(ip, port),
                })
            }
            DATA if b.len() >= DATA_HEADER => Some(Frame::Data {
                cookie: u64_at(b, 1)?,
                session: u64_at(b, 9)?,
                seq: u32::from_be_bytes(b[17..21].try_into().ok()?),
                payload: &b[DATA_HEADER..],
            }),
            PROBE if b.len() <= 64 => Some(Frame::Probe),
            REFUSED if b.len() == 10 => Some(Frame::Refused {
                session: u64_at(b, 1)?,
                reason: b[9],
            }),
            _ => None,
        }
    }

    pub fn hello(cookie: u64) -> Vec<u8> {
        let mut f = vec![HELLO];
        f.extend(cookie.to_be_bytes());
        f
    }

    pub fn open(cookie: u64, session: u64, mode: Mode, target: SocketAddr) -> Vec<u8> {
        let mut f = vec![OPEN];
        f.extend(cookie.to_be_bytes());
        f.extend(session.to_be_bytes());
        f.push(mode as u8);
        match target.ip() {
            IpAddr::V4(ip) => {
                f.push(4);
                f.extend(ip.octets());
            }
            IpAddr::V6(ip) => {
                f.push(6);
                f.extend(ip.octets());
            }
        }
        f.extend(target.port().to_be_bytes());
        f
    }

    pub fn data(cookie: u64, session: u64, seq: u32, payload: &[u8]) -> Vec<u8> {
        let mut f = Vec::with_capacity(DATA_HEADER + payload.len());
        f.push(DATA);
        f.extend(cookie.to_be_bytes());
        f.extend(session.to_be_bytes());
        f.extend(seq.to_be_bytes());
        f.extend_from_slice(payload);
        f
    }

    /// A middle relay swaps in its own cookie before passing a frame on.
    pub fn set_cookie(frame: &mut [u8], cookie: u64) {
        frame[1..9].copy_from_slice(&cookie.to_be_bytes());
    }

    pub fn refused(session: u64, why: Refused) -> Vec<u8> {
        let mut f = vec![REFUSED];
        f.extend(session.to_be_bytes());
        f.push(why as u8);
        f
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stats_from_probes() {
        let s =
            Stats::from_probes(&[Some(46.0), Some(48.0), Some(45.0), None, Some(140.0)]).unwrap();
        assert_eq!(s.ping_ms, 47.0);
        assert_eq!(s.loss, 0.2);
        assert_eq!(s.spikes, 2); // one lost, one 93 ms over the median
        assert!((s.jitter_ms - (2.0 + 3.0 + 95.0) / 3.0).abs() < 1e-9);
        assert_eq!(Stats::from_probes(&[None, None]), None);
    }

    #[test]
    fn jitter_and_loss_cost_more_than_ping() {
        let direct = Stats {
            ping_ms: 74.0,
            jitter_ms: 18.0,
            loss: 0.021,
            spikes: 11,
        };
        let relay = Stats {
            ping_ms: 46.0,
            jitter_ms: 3.0,
            loss: 0.0,
            spikes: 0,
        };
        assert!((direct.score() - 122.0).abs() < 1e-9);
        assert!((relay.score() - 50.5).abs() < 1e-9);
        let low_ping_lossy = Stats {
            ping_ms: 40.0,
            jitter_ms: 4.0,
            loss: 0.03,
            spikes: 9,
        };
        assert!(relay.score() < low_ping_lossy.score());
    }

    #[test]
    fn switches_only_after_three_clear_rounds() {
        let mut sw = Switch::new(0);
        let round = [Some(122.0), Some(50.5), Some(55.0)];
        assert_eq!(sw.round(&round), 0);
        assert_eq!(sw.round(&round), 0);
        assert_eq!(sw.round(&round), 1);
        // A 3 ms lead is noise: hold.
        for _ in 0..10 {
            assert_eq!(sw.round(&[Some(122.0), Some(50.5), Some(47.5)]), 1);
        }
    }

    #[test]
    fn a_lead_must_hold_in_a_row() {
        let mut sw = Switch::new(0);
        let ahead = [Some(80.0), Some(60.0)];
        let level = [Some(80.0), Some(78.0)];
        for _ in 0..5 {
            sw.round(&ahead);
            sw.round(&ahead);
            sw.round(&level);
        }
        assert_eq!(sw.current(), 0);
    }

    #[test]
    fn moves_at_once_when_the_route_goes_down() {
        let mut sw = Switch::new(1);
        assert_eq!(sw.round(&[Some(122.0), None, Some(55.0)]), 2);
        assert_eq!(sw.round(&[None, None, None]), 2);
    }

    #[test]
    fn dedup_keeps_the_first_copy() {
        let mut d = Dedup::new();
        assert!(d.accept(48211));
        assert!(!d.accept(48211));
        assert!(d.accept(48213));
        assert!(d.accept(48212)); // late but new
        assert!(!d.accept(48212));
        assert!(!d.accept(48213));
    }

    #[test]
    fn dedup_window_and_wraparound() {
        let mut d = Dedup::new();
        assert!(d.accept(5000));
        assert!(!d.accept(5000 - WINDOW)); // too old
        assert!(d.accept(5000 + 3 * WINDOW)); // big jump clears the window
        assert!(d.accept(5000 + 3 * WINDOW - 1));
        let mut w = Dedup::new();
        assert!(w.accept(u32::MAX - 1));
        assert!(w.accept(u32::MAX));
        assert!(w.accept(0));
        assert!(w.accept(1));
        assert!(!w.accept(u32::MAX));
        assert!(!w.accept(0));
    }

    #[test]
    fn multipath_survives_loss_on_one_path() {
        let mut d = Dedup::new();
        let mut delivered = 0;
        for seq in 0..1000u32 {
            let a = seq % 7 != 0; // path A loses every 7th packet
            let b = seq % 11 != 0; // path B loses every 11th
            let kept = [a, b]
                .iter()
                .filter(|&&arrived| arrived && d.accept(seq))
                .count();
            assert!(kept <= 1);
            delivered += kept;
        }
        assert_eq!(delivered, 1000 - 13); // only packets lost on both paths are gone
    }

    #[test]
    fn relay_targets() {
        let ok = |s: &str| allowed_target(s.parse().unwrap());
        assert_eq!(ok("45.10.20.30:7120"), Ok(()));
        assert_eq!(ok("[2a01:4f8::1]:27015"), Ok(()));
        assert_eq!(ok("192.168.1.20:7000"), Err(Refused::NotPublic));
        assert_eq!(ok("127.0.0.1:7000"), Err(Refused::NotPublic));
        assert_eq!(ok("100.64.3.1:7000"), Err(Refused::NotPublic));
        assert_eq!(ok("224.0.0.251:7000"), Err(Refused::NotPublic));
        assert_eq!(ok("[fd00::1]:7000"), Err(Refused::NotPublic));
        assert_eq!(ok("[::ffff:10.0.0.1]:7000"), Err(Refused::NotPublic));
        assert_eq!(ok("45.10.20.30:53"), Err(Refused::Port));
        assert_eq!(ok("45.10.20.30:443"), Err(Refused::Port));
        assert_eq!(ok("45.10.20.30:11211"), Err(Refused::Port));
    }

    #[test]
    fn session_sends_to_one_target_at_game_rate() {
        let t0 = Duration::ZERO;
        let server: SocketAddr = "45.10.20.30:7120".parse().unwrap();
        let mut s = Session::open(server, RATE_BYTES, t0).unwrap();
        assert_eq!(
            s.check("45.10.20.31:7120".parse().unwrap(), 100, t0),
            Err(Refused::OtherTarget)
        );
        // A shooter at 128 packets a second, 200 bytes each, runs all day.
        for i in 0..1280u32 {
            assert_eq!(
                s.check(server, 200, Duration::from_micros(7812 * i as u64)),
                Ok(())
            );
        }
        // A flood runs dry within the burst.
        let flood = (0..1000)
            .filter(|_| s.check(server, 1200, Duration::from_secs(10)).is_ok())
            .count();
        assert!(flood < 30);
        assert_eq!(s.check(server, 1200, Duration::from_secs(11)), Ok(()));
    }

    #[test]
    fn wire_round_trip() {
        use wire::*;
        let target: SocketAddr = "45.10.20.30:7120".parse().unwrap();
        assert_eq!(
            parse(&open(7, 42, Mode::Hop, target)),
            Some(Frame::Open {
                cookie: 7,
                session: 42,
                mode: Mode::Hop,
                target
            })
        );
        let v6: SocketAddr = "[2a01:4f8::1]:27015".parse().unwrap();
        assert_eq!(
            parse(&open(7, 42, Mode::Exit, v6)),
            Some(Frame::Open {
                cookie: 7,
                session: 42,
                mode: Mode::Exit,
                target: v6
            })
        );
        let mut d = data(7, 42, 48211, b"tick");
        assert_eq!(
            parse(&d),
            Some(Frame::Data {
                cookie: 7,
                session: 42,
                seq: 48211,
                payload: b"tick"
            })
        );
        set_cookie(&mut d, 9);
        assert!(matches!(parse(&d), Some(Frame::Data { cookie: 9, .. })));
        assert_eq!(hello(0).len(), hello(u64::MAX).len());
        assert_eq!(
            parse(&refused(42, Refused::Port)),
            Some(Frame::Refused {
                session: 42,
                reason: 2
            })
        );
        assert_eq!(parse(&[PROBE; 65]), None);
        assert_eq!(parse(&[OPEN, 1, 2]), None);
    }
}
