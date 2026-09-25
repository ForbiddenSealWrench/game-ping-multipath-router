# Changelog


## 1.4.1 — 3 November 2026

A maintenance release.

### Fixes

- Path race keeps working after the network adapter changes.

## 1.4.0 · 2026-11-01

- Race completes when one route drops mid-test.
- Game traffic stays on the winner after a reconnect.
- Duplicate sends no longer reorder UDP streams.
- Route table redraws correctly on high-DPI screens.

## 1.3.8 · 2026-10-09

- The race switches paths only on a clear lead, so it never flip-flops mid-game.
- Relay list loads from the community file and your own without a restart.

## 1.3.9 · 2026-10-30

- **Take the shortcut.** One button. Shortcut races your normal path against relay paths every 2 seconds and puts your game on the winner, switching only after a clear lead for 3 rounds in a row.
- **No more lag spikes.** Every game packet goes down two paths at once and the first copy wins. The extra bandwidth is shown.
- **A straight answer.** Wi‑Fi, bufferbloat and downloads are checked before the route gets the blame. It says so when direct is already best, and for Valve games.
- **Free servers, two ways.** Free community paths from a signed list of 16 volunteer relays, or your own free server on Oracle Cloud Always Free, deployed by a 3-step wizard in about 3 minutes.
- **Show details** for the race table (ping, jitter, loss per path) and the before/after lag spike graph.
- Split tunnel per game with Wintun: only the game's UDP is routed.
- 20 games in `games.toml`, detected by process. Servers are learned from the game's traffic.
- `shortcut-relay`: a std-only UDP relay with address cookies, one target per session and a rate limit.
- Portable Windows build: `Shortcut-1.3.9-portable-x64.exe`.
