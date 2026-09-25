// Shortcut window UI. The Rust side finds the game, races the paths and moves
// the packets. This file draws what the player sees: one result, one sentence,
// one button, and the machinery behind "Show details". `window.shortcut`
// drives it.
(function () {
  'use strict';
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  const ease = (t) => 1 - Math.pow(1 - t, 3);
  const tween = (ms, fn) => new Promise((done) => {
    const t0 = performance.now();
    (function step(now) {
      const t = Math.min(1, Math.max(0, (now - t0) / ms));
      fn(ease(t));
      if (t < 1) requestAnimationFrame(step); else done();
    })(t0);
  });
  const $ = (sel, root = document) => root.querySelector(sel);
  const $$ = (sel, root = document) => [...root.querySelectorAll(sel)];
  const NS = 'http://www.w3.org/2000/svg';

  const icon = {
    min: '<svg viewBox="0 0 10 10"><path d="M0 5.5h10"/></svg>',
    max: '<svg viewBox="0 0 10 10"><rect x="0.5" y="0.5" width="9" height="9"/></svg>',
    close: '<svg viewBox="0 0 10 10"><path d="M0 0l10 10M10 0L0 10"/></svg>',
    go: '<svg viewBox="0 0 16 16"><path d="M2 12C3.5 6 8.5 3.5 13.5 5.5"/><path d="M10.2 3.4l3.3 2.1-2 3.3"/></svg>',
    check: '<svg viewBox="0 0 16 16"><path d="M3.5 8.5l3 3 6-7"/></svg>',
    gear: '<svg viewBox="0 0 16 16"><circle cx="8" cy="8" r="2.2"/><path d="M8 1.8v1.7M8 12.5v1.7M14.2 8h-1.7M3.5 8H1.8M12.4 3.6l-1.2 1.2M4.8 11.2l-1.2 1.2M12.4 12.4l-1.2-1.2M4.8 4.8L3.6 3.6"/></svg>',
    warn: '<svg viewBox="0 0 16 16"><path d="M8 2.2l6 11H2z"/><path d="M8 6.5v3M8 11.4v.1"/></svg>',
  };
  const arrow = '<svg class="arrow" viewBox="0 0 40 24"><path d="M3 12h32M26 4l9 8-9 8"/></svg>';
  const to = '<svg class="to" viewBox="0 0 12 12"><path d="M1.5 6h8.5M7 3l3 3-3 3"/></svg>';
  // The mark: an arc that jumps from you to the server.
  const mark = '<svg class="mark" viewBox="0 0 64 64"><path d="M15 40Q32 -1 49 40"/><circle cx="15" cy="41.5" r="6"/><circle class="end" cx="49" cy="41.5" r="6"/></svg>';
  const tile = (m) => `<span class="tile">${m}</span>`;

  // ---------- Map ---------------------------------------------------------
  // Land dots sampled from Natural Earth land (public domain) on a grid that
  // is even on screen at 50 degrees north. One bit per dot, row by row.
  const LAND = { lon0: -25.0, lat0: 64.0, dlat: 0.25, cols: 186, rows: 120, bits: 'A7/8AAAAAAAAAAB////8D/////9////AA/AAAAAAAAAAAAv///wP//////////AAAAAAAAAAAAAAv////AP//////////AAAAAAAAAAAAAAf///8A///////////AAAAAAAAAAAAAD////8B///////////AAAAAAAAAAAAAE////4B///////////AAAAAAAAAAAAAX////gB///////////AAAAAAAGAAAAB/////gB///////////AAAAAAAAAAAABr////gB///////////AAAAAAACAAAAB/////AA///////////AAAAAAAAAAAAB/////AA///////////AAAAAAAAAAAAAl////AB///////////AAAAAAAAAAAAB/////gB///////////AAAAAAAAAAQABf////wB//+////////AAAAAAAAAAQAA3////4Qv+A////////AAAAAAAAAAAAAP////8QPwAH///////AAAAAAAAAAAAA/////+AEAAf///////AAAAAAAAAAAAA//v//8AACD////////AAAAAAAAAAAAAv/v//4AA//////////AAAAAAAAAAAAAP/n//4AB//////////AAAAAAAAAAAAAf8H//gAJ//////////AAAAAAAAAAAAAf8H/8AAB//////////AAAAAAABHwAAAPwH/+AAd//////////AAAAAAADPgAAAHgD/+AAAf/////////AAAAAAACPAAAAAAD/+BAAf/////////AAAAAAAEPeAAAAAB/+GAQf/////////AAAAAAAFf+AAAABh/+GA8f/////////AAAAAAAEP+AAAAHg/+EB8//////////AAAAAAAAf8AAAALg//AB///////////AAAAAAAA38AAAAHgf8AD///////////AAAAAAAAv4AAAAfwf+AD///////////AAAAAAAAPwAAAAfgfAAD///////////AAAAAAAAj+AAAAfR+AAD///////////AAAAAAAAL/AAAAeHOAAB///////////AAAAAAAAT/AAAAO3IAAB///////////AAAAAAAfn/gAAAOyAgAB///////////AAAAAAAfnvgAAAGEAAAP///////////AAAAAAA/wfwAAAHAIA8f///////////AAAAAAA/wf4AAAPo8D+////////////AAAAAAH/iH8AAAHz8f/////////////AAAAAAD/AP8AAAD////////////////AAAAAAH/AP+AABv////////////////AAAAAAH/AP+AAf/////////////////AAAAAAB/h/+AA//////////////////AAAAAAD/i//wD//////////////////AAAAAAF/A//4D//////////////////AAAAAAH/A//4D//////////////////AAAAAAH4D//wH//////////////////AAAAAAPwH//wH//////////////////AAAAAAPAA7/gH//////////////////AAAAAAAAAH/Qf//////////////////AAAAAAAAB//x///////////////////AAAAAAAAB//H///////////////////AAAAAAAADwAP///////////////////AAAAAAAAGgAP///////////////////AAAAAAAAIAAP///////////////////AAAAAAAAAAA////////////////////AAAAAAAAABh////////////////////AAAAAAAAAAz////////////////////AAAAAAAAAA/////////////////////AAAAAAAAAA/////////////////////AAAAAAAAB7/////////////////////AAAAAAAAD//////////////////////AAAAAAAAB//////////////////////AAAAAAAAB//////////////////////AAAAAAAAAP/////////////////////AAAAAAAAAD/////////////////////AAAAAAAAAB/////////////////n///AAAAAAAAAB////////////////8P///AAAAAAAAAB//////////////n/B////AAAAAAAAAAf////////////+H+A////AAAAAAAAAAf////////////+E+Af///AAAAAAAAAAf////////////8AcA////AAAAAAAAAAP/////z//////wB+B////AAAAAAAAAAf/////H//////4D+x////AAAAAAAAAAX/////G//////4A/3////AAAAAAAAAAf/////Af/////wAeD///+AAAAAAAAAAf/////Bf/////AA4A///8AAAAAAAAAAf///z/AP/////AAAAf//+AAAAAAAAAAf///x/Af/////AAAAP///AAAAAAAAAAf///gfwP/////AAAAH///AAAAAAAcAAf//+Af4H/////AAAAD///AAAAAAAf/9//x8Af8A////+AAAAB///AAAAAAB/////gQAf8Bf///8AAAAA///AAAAAAB/////AACP8Af///8AAAAAP//AAAAAAA/////AAGH8AH///4AAAAAH//AAAAAAA/////gAOD+AD///4AAAAAH//AAAAAAB/////AAOB/AB///8AAAAAH//AAAAAAA/////AAEB/8A///8AB+AAD//AAAAAAA////+AAEA/4Af//+AH/gAH//AAAAAAA////8AAAAf8A////Af/gAH//AAAAAAA////wAACAD/A////+f/8Af//AAAAAAA////AAAeAD/g//j4f///////AAAAAAA////AAAeAA/4/+Rwf///////AAAAAAA///+AAAOAAfY/3A7////////AAAAAAA///+AAAOAAeMfzC/////////AAAAAAB///8AAAOAAGEP4E/////////AAAAAAB///8DgAOAAGAP4B/////////AAAAAAD///4AgAOAAHAP8Af////////AAAAAAD///8AAAMAADAH4A/////////AAAAAAD///8AAAAAACAD2AP////////AAAAAAB///8AAAAAAGAH9Af////////AAAAAAA///4AAAAAAGAFeh/////////AAAAAAA///4AAAAAI0AFzAf////////AAAAAAA///wAAAAA/wAF9AP////////AAAAAAA///wAAAAAfgAB8BP////////AAAAAAA///AAAAAAHgAA4AP////////AAAAAAB5//AAAAHADgAA4AP////////AAAAAAAA/+AAOPf4BgAA8AL////////AAAAAAAA+AAB///wAAAAEAAeP5f////AAAAAAAA4AA////wAAAAAACeHw/////AAAAAAAAQAB////gAAAAAAAADAf////AAAAAAAAIAD////wBAAAAAAAAA/////AAAAAAAAYAP////4AAAAAAAAAA/////AAAAAAAAYAf////4AAAAB4AAAQf////AAAAAAAA/4/////4AAAAAfAADwf////AAAAAAAA///////wAAAAAAAAHgf////AAAAAAAA///////wAAAAAAAAAAf////AAAAAAAB///////gAAAAAAAAAA/////AAAAAAAB///////AAAAAAAAAAA/////' };
  const COS = Math.cos((50 * Math.PI) / 180);
  const project = (v, lon, lat) => [(lon - v.lon) * v.k * COS + v.w / 2, (v.lat - lat) * v.k + v.h / 2];

  const dotCache = new Map();
  function landPath(v) {
    const key = [v.w, v.h, v.lon, v.lat, v.k].join();
    if (dotCache.has(key)) return dotCache.get(key);
    const bytes = atob(LAND.bits);
    const dlon = LAND.dlat / COS;
    let d = '';
    for (let r = 0; r < LAND.rows; r++) {
      for (let c = 0; c < LAND.cols; c++) {
        const i = r * LAND.cols + c;
        if (!((bytes.charCodeAt(i >> 3) >> (7 - (i & 7))) & 1)) continue;
        const [x, y] = project(v, LAND.lon0 + (c + 0.5) * dlon, LAND.lat0 - (r + 0.5) * LAND.dlat);
        if (x > -4 && y > -4 && x < v.w + 4 && y < v.h + 4) d += `M${x.toFixed(1)} ${y.toFixed(1)}h0`;
      }
    }
    dotCache.set(key, d);
    return d;
  }

  // A great-circle path between two places, lifted into an arc like a flight
  // track. lift > 0 bows north, lift < 0 bows south.
  function arcPoints(v, a, b, lift) {
    const rad = Math.PI / 180;
    const vec = ([lon, lat]) => [Math.cos(lat * rad) * Math.cos(lon * rad), Math.cos(lat * rad) * Math.sin(lon * rad), Math.sin(lat * rad)];
    const A = vec(a), B = vec(b);
    const ang = Math.acos(Math.min(1, A[0] * B[0] + A[1] * B[1] + A[2] * B[2]));
    const raw = [];
    for (let i = 0, n = 40; i <= n; i++) {
      const t = i / n;
      const s1 = Math.sin((1 - t) * ang) / Math.sin(ang), s2 = Math.sin(t * ang) / Math.sin(ang);
      const x = s1 * A[0] + s2 * B[0], y = s1 * A[1] + s2 * B[1], z = s1 * A[2] + s2 * B[2];
      raw.push([...project(v, Math.atan2(y, x) / rad, Math.atan2(z, Math.hypot(x, y)) / rad), t]);
    }
    const [x0, y0] = raw[0], [x1, y1] = raw[raw.length - 1];
    const len = Math.hypot(x1 - x0, y1 - y0);
    let nx = -(y1 - y0) / len, ny = (x1 - x0) / len;
    if (ny > 0) { nx = -nx; ny = -ny; }
    return raw.map(([x, y, t]) => { const o = Math.sin(Math.PI * t) * lift * len; return [x + nx * o, y + ny * o]; });
  }
  const pathD = (pts) => 'M' + pts.map(([x, y]) => `${x.toFixed(1)} ${y.toFixed(1)}`).join('L');

  // ---------- The session in the README images --------------------------
  // A player in Kraków on 5 GHz Wi-Fi, VALORANT on a server in Paris.
  // Relay names are made up and city-coded. par-1 sits next to the server.
  const PLACE = {
    you: [19.94, 50.06], server: [2.35, 48.86],
    'fra-2': [8.68, 50.11], 'my-relay': [8.6, 49.32], 'vie-1': [16.37, 48.21], 'ams-1': [4.9, 52.37],
  };
  const ROUTES = [
    { id: 'direct', who: 'Your ISP', ping: 74, jitter: 18, loss: 2.1, lift: 0.34 },
    { id: 'fra-2', who: 'community', ping: 46, jitter: 3, loss: 0.0, lift: 0.16, rounds: [52, 47, 46] },
    { id: 'my-relay', who: 'yours', ping: 49, jitter: 4, loss: 0.2, lift: 0.05, rounds: [55, 50, 49] },
    { id: 'vie-1', who: 'community', ping: 57, jitter: 6, loss: 0.4, lift: -0.2, rounds: [63, 58, 57] },
    { id: 'ams-1', who: 'community', ping: 61, jitter: 5, loss: 0.3, lift: 0.1, rounds: [60, 62, 61] },
  ];

  // Deterministic noise, so every capture draws the same lines.
  function rng(seed) {
    return () => {
      seed = (seed + 0x6d2b79f5) | 0;
      let t = Math.imul(seed ^ (seed >>> 15), 1 | seed);
      t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
      return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
    };
  }
  // Ping history for each path: one reading a second for a minute.
  const SPIKES = [4, 9, 13, 19, 24, 28, 33, 39, 44, 50, 55];
  const HIST = ROUTES.map((r, k) => {
    const rand = rng(17 + k * 31);
    const out = Array.from({ length: 60 }, () => r.ping + (rand() - 0.5) * r.jitter * 1.4);
    if (r.id === 'direct') SPIKES.forEach((i, j) => { out[i] = 128 + ((j * 37) % 70); });
    return out;
  });

  // ---------- Window --------------------------------------------------------
  function shell(body) {
    return `
    <div class="window">
      <header class="titlebar">
        <div class="brand">${mark}<span class="wordmark">Shortcut</span></div>
        <button class="tb-btn" id="paths-btn">${icon.gear}Paths</button>
        <div class="winctl"><span>${icon.min}</span><span>${icon.max}</span><span>${icon.close}</span></div>
      </header>
      <main class="view" id="view">${body}</main>
      <footer class="status" id="status"></footer>
    </div>`;
  }
  const status = (text, on = false) => { $('#status').innerHTML = `<span class="dot ${on ? 'live' : ''}"></span><span>${text}</span>`; };

  // ---------- Main screen -----------------------------------------------------
  const MAPV = { w: 1120, h: 632, lon: -4.05, lat: 51, k: 29.2 };
  let V = MAPV;

  function mapSvg(v, { fade = true } = {}) {
    return `
    <svg viewBox="0 0 ${v.w} ${v.h}" class="mapsvg">
      <defs>
        <filter id="glow" x="-20%" y="-60%" width="140%" height="220%"><feGaussianBlur stdDeviation="5"/></filter>
        <linearGradient id="fadeL" x1="0" x2="1"><stop offset="0.3" stop-color="#fff" stop-opacity="0.12"/><stop offset="0.56" stop-color="#fff"/></linearGradient>
        <mask id="landmask"><rect width="${v.w}" height="${v.h}" fill="${fade ? 'url(#fadeL)' : '#fff'}"/></mask>
      </defs>
      <path class="land" d="${landPath(v)}" mask="url(#landmask)"/>
      <g class="arcs"></g>
      <g class="places"></g>
      <g class="pkts"></g>
    </svg>`;
  }

  function place(id, kind, text, [dx, dy, anchor]) {
    const [x, y] = project(V, ...PLACE[id]);
    const shape = kind === 'server'
      ? `<rect class="pt" x="${x - 5}" y="${y - 5}" width="10" height="10" rx="1.5" transform="rotate(45 ${x} ${y})"/>`
      : kind === 'you'
        ? `<circle class="ping-ring" cx="${x}" cy="${y}" r="7"/><circle class="pt" cx="${x}" cy="${y}" r="6"/>`
        : `<circle class="pt" cx="${x}" cy="${y}" r="4"/>`;
    const g = document.createElementNS(NS, 'g');
    g.setAttribute('class', `place ${kind}`);
    g.dataset.place = id;
    g.innerHTML = `${shape}${text ? `<text x="${x + dx}" y="${y + dy}" text-anchor="${anchor}">${text}</text>` : ''}`;
    $('.places').appendChild(g);
    return g;
  }

  // Arcs for a path: you → relay → server, or you → server.
  function drawRoute(r, cls) {
    const segs = r.id === 'direct' ? [['you', 'server']] : [['you', r.id], [r.id, 'server']];
    const g = $('.arcs');
    return segs.map(([a, b], j) => {
      const pts = arcPoints(V, PLACE[a], PLACE[b], r.lift);
      const out = [];
      if (cls === 'win') out.push(addPath(g, pathD(pts), 'arc glow', r.id, j));
      out.push(addPath(g, pathD(pts), `arc ${cls}`, r.id, j));
      out[out.length - 1].apex = pts[20];
      return out;
    }).flat();
  }
  function addPath(g, d, cls, id, j) {
    const p = document.createElementNS(NS, 'path');
    p.setAttribute('d', d);
    p.setAttribute('class', cls);
    p.dataset.route = id;
    p.dataset.seg = j;
    if (/glow/.test(cls)) p.setAttribute('filter', 'url(#glow)');
    g.appendChild(p);
    return p;
  }
  // Draw a set of paths segment after segment, like a track being drawn.
  async function reveal(paths, ms) {
    const segs = [...new Set(paths.map((p) => p.dataset.seg))];
    paths.forEach((p) => { const l = p.getTotalLength(); p.style.strokeDasharray = `${l} ${l}`; p.style.strokeDashoffset = l; });
    for (const s of segs) {
      const group = paths.filter((p) => p.dataset.seg === s);
      await tween(ms / segs.length, (e) => group.forEach((p) => { p.style.strokeDashoffset = p.getTotalLength() * (1 - e); }));
    }
    paths.forEach((p) => { p.style.strokeDasharray = ''; p.style.strokeDashoffset = ''; });
  }
  function lineTag(x, y, text, cls) {
    const t = document.createElementNS(NS, 'text');
    t.setAttribute('x', x.toFixed(1));
    t.setAttribute('y', y.toFixed(1));
    t.setAttribute('class', `tag ${cls}`);
    t.setAttribute('text-anchor', 'middle');
    t.textContent = text;
    $('.places').appendChild(t);
    return t;
  }
  function tagLine(r, text, cls) {
    const [x, y] = $(`.arc[data-route="${r.id}"]:not(.glow)`).apex;
    return lineTag(x, y - 12, text, cls);
  }

  // Packets in flight along the shortcut.
  let flying = 0;
  function fly(id) {
    const g = $('.pkts');
    const segs = $$(`.arc.win[data-route="${id}"]`);
    const lens = segs.map((s) => s.getTotalLength());
    const total = lens.reduce((a, b) => a + b, 0);
    const dots = [0, 1, 2, 3].map(() => {
      const c = document.createElementNS(NS, 'circle');
      c.setAttribute('r', 2.6);
      c.setAttribute('class', 'pkt');
      g.appendChild(c);
      return c;
    });
    const t0 = performance.now();
    cancelAnimationFrame(flying);
    (function frame(now) {
      if (!document.body.contains(g)) return;
      dots.forEach((c, i) => {
        let at = (((now - t0) / 1800 + i / 4) % 1) * total;
        let s = 0;
        while (s < segs.length - 1 && at > lens[s]) { at -= lens[s]; s++; }
        const p = segs[s].getPointAtLength(Math.min(at, lens[s]));
        c.setAttribute('cx', p.x.toFixed(1));
        c.setAttribute('cy', p.y.toFixed(1));
      });
      flying = requestAnimationFrame(frame);
    })(t0);
  }

  // The left-hand block: one result, one sentence, one button.
  const HERO = {
    idle: () => `<h1 class="say">Start a game.</h1><p class="lede">We’ll pick it up.</p>`,
    found: () => `
      <p class="kick">${tile('VA')}VALORANT is running</p>
      <div class="giant"><b id="num">74</b><span>ms</span></div>
      <p class="lede">Your ping to the server in Paris.</p>
      <div class="row"><button class="primary" id="go">Take the shortcut ${icon.go}</button></div>`,
    racing: () => `
      <p class="kick">${tile('VA')}VALORANT is running</p>
      <div class="giant"><b id="num">74</b><span>ms</span></div>
      <p class="lede">Wi‑Fi is fine. Finding a faster path…</p>
      <div class="row"><button class="primary busy" id="go" disabled>Racing 5 paths</button></div>`,
    on: () => `
      <p class="kick">${tile('VA')}VALORANT is running</p>
      <div class="giant result"><b class="was">74</b>${arrow}<b class="now" id="num">46</b><span>ms</span></div>
      <p class="lede">Faster path found through Frankfurt.</p>
      <p class="small">Lag spikes: 11 a minute ${to} 0</p>
      <div class="row"><button class="toggle on" id="go"><i></i>Shortcut on</button><button class="link" id="details-btn">Show details</button></div>`,
  };
  function hero(state) {
    const h = $('#hero');
    h.className = `hero ${state}`;
    h.innerHTML = HERO[state]();
  }

  function render() {
    V = MAPV;
    document.getElementById('app').innerHTML = shell(`<div class="main">${mapSvg(MAPV)}<section class="hero" id="hero"></section><div class="scrim" id="scrim"></div><div class="sheet" id="sheet"></div></div>`);
  }

  // ---------- Show details: the race and the lag spikes -------------------------
  function sparkD(k) {
    const h = HIST[k];
    const y = (ms) => (25 - ((Math.min(ms, 140) - 30) / 110) * 22).toFixed(1);
    const x = (i) => ((i / 59) * 398 + 1).toFixed(1);
    return {
      line: 'M' + h.map((v, i) => `${x(i)} ${y(v)}`).join('L'),
      spikes: ROUTES[k].id === 'direct' ? SPIKES.map((i) => `M${x(i)} 25V${y(h[i])}`).join('') : '',
    };
  }
  const CHIP = { direct: ['Normal path', 'line'], 'fra-2': ['In use', 'hot'], 'my-relay': ['Second copy', 'line hot'] };
  function raceDetails() {
    return `
      <p class="d-title">Direct 74 ms <em>·</em> via Frankfurt 46 ms <b>(−28 ms)</b></p>
      <div class="lanes">
        <div class="lane head"><span>Path</span><span>Last 60 s</span><span>Ping</span><span>Jitter</span><span>Loss</span><span></span></div>
        ${ROUTES.map((r, k) => {
          const s = sparkD(k);
          const [chip, cls] = CHIP[r.id] || ['', ''];
          return `<div class="lane ${r.id === 'direct' ? 'base' : ''} ${r.id === 'fra-2' ? 'lead' : ''}">
            <span class="who"><b>${r.id === 'direct' ? 'Direct' : `${r.id}${to}par-1`}</b><small>${r.who}</small></span>
            <svg class="spark" viewBox="0 0 400 26" preserveAspectRatio="none"><path class="sl" d="${s.line}"/><path class="sp" d="${s.spikes}"/></svg>
            <span class="num ping"><b>${r.ping}</b>ms</span><span class="num">±${r.jitter}</span><span class="num">${r.loss.toFixed(1)}%</span>
            <span class="chip-slot">${chip ? `<span class="chip ${cls}">${chip}</span>` : ''}</span>
          </div>`;
        }).join('')}
      </div>
      <p class="d-note">Every 2 seconds Shortcut races your direct route against each relay path. It switches only when a path leads by 8 ms for 3 rounds in a row, so it never flaps.</p>`;
  }

  function mpSeries(base, spread, seed, n = 240) {
    const rand = rng(seed);
    return Array.from({ length: n }, () => base + (rand() + rand() - 1) * spread);
  }
  const MP_SPIKES = [14, 37, 58, 81, 99, 123, 141, 166, 187, 206, 228];
  const MP_LOSS = [9, 22, 31, 47, 63, 70, 88, 104, 112, 131, 150, 158, 172, 181, 195, 213, 221, 236];
  function chart(series, { spikes = [], losses = [], tone }) {
    const W = 620, H = 136, top = 6, bottom = 114;
    const y = (ms) => (bottom - (Math.min(ms, 220) / 220) * (bottom - top)).toFixed(1);
    const x = (i) => (34 + (i / (series.length - 1)) * (W - 40)).toFixed(1);
    const grid = [0, 100, 200].map((ms) => `<path class="gl" d="M34 ${y(ms)}H${W - 6}"/><text class="gt" x="26" y="${+y(ms) + 3.5}">${ms}</text>`).join('');
    const line = 'M' + series.map((v, i) => `${x(i)} ${y(v)}`).join('L');
    const sp = spikes.map((i) => `M${x(i - 1)} ${y(series[i - 1])}L${x(i)} ${y(series[i])}L${x(i + 1)} ${y(series[i + 1])}`).join('');
    const ls = losses.map((i) => `M${x(i)} ${bottom + 8}v8`).join('');
    return `<svg class="chart ${tone}" viewBox="0 0 ${W} ${H}">${grid}<path class="cl" d="${line}"/><path class="cs" d="${sp}"/><path class="lt" d="${ls}"/>${losses.length ? `<text class="ltl" x="26" y="${bottom + 16}">lost</text>` : ''}</svg>`;
  }
  function spikeDetails() {
    const direct = mpSeries(74, 13, 5);
    MP_SPIKES.forEach((i, j) => { direct[i] = 130 + ((j * 29) % 80); });
    const shortcut = mpSeries(46, 2.2, 9);
    return `
      <p class="d-title">Lag spikes: 11 a minute ${to} <b>0</b></p>
      <div class="mp">
        <div class="charts">
          <div class="ch-head"><b>Before: one copy, direct</b><span class="t-amber">11 spikes a minute <em>·</em> 2.1% lost</span></div>
          ${chart(direct, { spikes: MP_SPIKES, losses: MP_LOSS, tone: 'base' })}
          <div class="ch-head"><b>Now: two copies, two paths</b><span class="t-hot">0 spikes <em>·</em> 0.0% lost</span></div>
          ${chart(shortcut, { tone: 'hot' })}
        </div>
        <div class="how">
          <svg class="pathsvg" viewBox="0 0 300 120">
            <path class="pa" d="M20 60C64 60 84 20 150 20S236 60 280 60"/>
            <path class="pb" d="M20 60C64 60 84 100 150 100S236 60 280 60"/>
            <circle class="n you" cx="20" cy="60" r="6"/><circle class="n" cx="150" cy="20" r="4"/><circle class="n" cx="150" cy="100" r="4"/><circle class="n exit" cx="280" cy="60" r="6"/>
            <text x="20" y="84" text-anchor="middle">you</text><text x="150" y="10" text-anchor="middle">fra-2</text><text x="150" y="118" text-anchor="middle">my-relay</text><text x="280" y="84" text-anchor="middle">par-1</text>
          </svg>
          <p class="how-t"><b>Multipath.</b> Every packet goes both ways at once. par-1 keeps the first copy to land and drops the other, so one lost packet is never a spike.</p>
          <div class="share"><span style="width:71%"></span><span style="width:29%"></span></div>
          <p class="share-k"><span>fra-2 first 71%</span><span>my-relay 29%</span></p>
          <p class="how-c">Costs 94 kbit/s up and 151 kbit/s down.</p>
        </div>
      </div>
      <p class="d-note">A spike is a packet 50 ms later than usual, or lost. Last 60 seconds of this match.</p>`;
  }

  function details(seg = 'race') {
    openSheet(`
      <div class="sheet-head">
        <div class="seg"><span class="${seg === 'race' ? 'on' : ''}">Race</span><span class="${seg === 'spikes' ? 'on' : ''}">Lag spikes</span></div>
        <button class="ghost sm">Done</button>
      </div>
      <div class="sheet-body">${seg === 'race' ? raceDetails() : spikeDetails()}</div>`, 'details');
  }

  // ---------- Paths: community or your own free server -------------------------------
  function paths() {
    const steps = [
      ['done', 'Make a free Oracle Cloud account', 'Done'],
      ['done', 'Paste your key', 'Saved on this PC only'],
      ['now', 'Deploy', 'Building your server in Frankfurt…'],
    ];
    openSheet(`
      <div class="sheet-head"><p class="sheet-t">Paths</p><button class="ghost sm">Done</button></div>
      <div class="sheet-body paths">
        <div class="choices">
          <p class="lede-s">Where your game’s packets can go. Both are free.</p>
          <div class="choice"><i></i><span><b>Free community paths</b><small>The default. 16 volunteer servers in 14 cities, from a signed list.</small></span></div>
          <div class="choice on"><i></i><span><b>My own free server</b><small>A private path on Oracle Cloud’s free tier. Only you use it.</small></span></div>
          <p class="safe"><b>Why volunteers can run one safely.</b> A Shortcut server only passes one game’s packets to one game server, at game speed. It can’t be used to browse or to attack anyone.</p>
        </div>
        <div class="wizard">
          <p class="wz-k">Your own free server <em>·</em> about 3 minutes</p>
          <ol class="steps">${steps.map(([st, a, b], i) => `<li class="${st}"><i>${st === 'done' ? icon.check : i + 1}</i><span><b>${a}</b><small>${b}</small></span></li>`).join('')}</ol>
          <div class="deploy">
            <div class="clock"><b>2:07</b><span>of about 3 min</span></div>
            <div class="bar"><span style="width:70%"></span></div>
          </div>
          <p class="note">${icon.warn}<span>Free servers sometimes run out in busy regions. If that happens, Shortcut tries again or lets you pick another region.</span></p>
        </div>
      </div>`, 'paths');
    $('#paths-btn').classList.add('on');
  }

  function openSheet(html, kind) {
    const s = $('#sheet');
    s.className = `sheet ${kind} show`;
    s.innerHTML = html;
    $('#scrim').classList.add('show');
  }

  // ---------- Verdicts (README image) -----------------------------------------------
  const VERDICTS = [
    { mono: 'VA', game: 'VALORANT', big: '−28', unit: 'ms', tone: 'hot', line: 'Your route is the problem. Shortcut fixed it.', tip: 'Faster path found through Frankfurt.' },
    { mono: 'FN', game: 'Fortnite', big: 'Wi‑Fi', tone: 'amber', line: 'Your Wi‑Fi is the problem. A new path won’t help.', tip: 'Use 5 GHz or a cable.' },
    { mono: 'CS', game: 'Counter-Strike 2', big: 'Direct', tone: 'plain', line: 'Valve already routes this game. Direct is best.', tip: 'Don’t pay anyone for a booster.' },
  ];
  function verdicts(root) {
    root.innerHTML = `<div class="verdicts">${VERDICTS.map((v) => `
      <div class="vcard ${v.tone}">
        <p class="kick">${tile(v.mono)}${v.game} is running</p>
        <div class="vrow">
          <div class="giant"><b>${v.big}</b>${v.unit ? `<span>${v.unit}</span>` : ''}</div>
          <div><p class="vline">${v.line}</p><p class="vtip">${v.tip}</p></div>
        </div>
      </div>`).join('')}</div>`;
  }

  // ---------- Driving it -------------------------------------------------------------
  function idle() {
    render();
    hero('idle');
    place('you', 'you', '', [0, 0, 'start']);
    status('Shortcut is off. Nothing is rerouted.');
  }

  function found() {
    hero('found');
    $$('.places > *').forEach((n) => n.remove());
    const direct = drawRoute(ROUTES[0], 'direct');
    place('you', 'you', 'You · Kraków', [0, 28, 'middle']);
    place('server', 'server', 'Paris · game server', [0, 28, 'middle']);
    status('Only VALORANT will use the shortcut. Everything else stays as it is.');
    return direct;
  }

  async function launch() {
    const direct = found();
    $('#hero').classList.add('enter');
    await reveal(direct, 700);
    tagLine(ROUTES[0], 'Normal path', 'grey');
  }

  async function race({ round = 700 } = {}) {
    hero('racing');
    $('.tag')?.remove();
    const cands = ROUTES.slice(1).map((r) => drawRoute(r, 'cand'));
    const stops = ROUTES.slice(1).map((r) => place(r.id, 'relay', '', [0, 0, 'start']));
    const tags = [];
    reveal(cands.flat(), 900);
    await sleep(500);
    for (let n = 0; n < 3; n++) {
      tags.splice(0).forEach((t) => t.remove());
      ROUTES.slice(1).forEach((r, j) => {
        const seg = cands[j].reduce((a, p) => (p.getTotalLength() > a.getTotalLength() ? p : a));
        const [x, y] = seg.apex;
        tags.push(lineTag(x, r.lift < 0 ? y + 20 : y - 9, `${r.rounds[n]} ms`, j === 0 ? 'hot' : 'dim'));
      });
      const from = n === 0 ? 74 : ROUTES[1].rounds[n - 1];
      await tween(round, (e) => { $('#num').textContent = Math.round(from + (ROUTES[1].rounds[n] - from) * e); });
    }
    await sleep(150);
    tags.forEach((t) => t.remove());
    cands.flat().forEach((p) => p.classList.add('gone'));
    stops.forEach((s) => s.remove());
    await won(true);
    cands.flat().forEach((p) => p.remove());
  }

  async function won(animate) {
    const win = drawRoute(ROUTES[1], 'win');
    place('fra-2', 'relay hot', 'Frankfurt', [0, 26, 'middle']);
    status('Only VALORANT uses the shortcut. Discord, browser and downloads stay as they are.', true);
    if (animate) {
      await reveal(win, 900);
      hero('on');
      $('#hero').classList.add('enter');
    } else {
      hero('on');
    }
    tagLine(ROUTES[0], 'Normal path', 'grey');
    tagLine(ROUTES[1], 'Shortcut', 'hot');
    fly('fra-2');
  }

  // Final state, no animation (screenshots).
  function done() {
    render();
    found();
    won(false);
  }

  async function press(sel) {
    const b = $(sel);
    b.classList.add('pressed');
    await sleep(160);
    b.classList.remove('pressed');
  }

  // The map on its own, any size (the banner).
  function scene(el, view, { fade = false } = {}) {
    V = { ...MAPV, ...view };
    el.innerHTML = mapSvg(V, { fade });
    drawRoute(ROUTES[0], 'direct');
    drawRoute(ROUTES[1], 'win');
    place('you', 'you', 'You · Kraków', [0, 28, 'middle']);
    place('server', 'server', 'Paris · game server', [0, 28, 'middle']);
    place('fra-2', 'relay hot', 'Frankfurt', [0, 26, 'middle']);
    tagLine(ROUTES[0], 'Normal path · 74 ms', 'grey');
    tagLine(ROUTES[1], 'Shortcut · 46 ms', 'hot');
  }

  window.shortcut = { idle, launch, race, done, details, paths, press, verdicts, scene };
  document.addEventListener('DOMContentLoaded', () => { const a = document.getElementById('app'); if (a && !a.dataset.manual) done(); });
})();
