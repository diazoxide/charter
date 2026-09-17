# The app's stack is locked by what M0 measured

ADR 0025 chose Tauri 2, React and TypeScript for the UI, with a Rust core holding every
terminal headless on `alacritty_terminal`, and it named the condition for changing its mind:
GPUI "stays the fallback only if the M0 skeleton misses a limit a person would notice". The
skeleton exists. This is what it measures against the spec's limits, and the decision that
follows.

**The stack is locked as ADR 0025 chose it, with one addition: xterm.js draws with its own DOM
renderer, not WebGL.** Two limits are not met, neither of them the drawing layer's fault, and
both are recorded below rather than designed around.

## How it was measured

`node tools/bench.mjs` in charter-app, on the operator's machine — Apple M4 Pro, 14 cores,
48 GB, macOS 26.2, tmux 3.7c, Node 24.12.0, cargo 1.98.1 — on 2026-09-17. The numbers below
are from the run at 22:27–22:47 local, of the tree this PR's charter-app commit records; the
chunking numbers are from 23:05, of the same tree plus the spec that takes them.

The method is research §9.3, adapted: the limits are the perceptible ones the spec lists, and
tmux is re-measured beside the app as a reference, not as a gate.

- **Release builds only.** A debug build's numbers say nothing about what a person feels. The
  app is built twice: as it ships, for cold start, and again with the `e2e` feature, which is
  what WebDriver can drive.
- **The frame is 150×42**, the size the recorded corpus was taken at, held there against the
  pane's own fitting.
- **The load is `fake-harness`**, replaying `fixtures/corpora/claude-code-session.raw` — 132 KB
  of real Claude Code output — ×16 for 2 MB and ×99 for 13 MB, and its synthetic
  harness-shaped output where a `?2026` animation is needed, because that recording has none.
- **A burst starts once the pane is watching** (`--wait-for-input`). Output written before the
  view opens reaches the pane as a snapshot of the screen, and timing that would measure the
  wrong thing.
- **"Painted" means the row is in the grid and one more frame has been presented**, measured
  inside the window through a seam the `e2e` build alone has. Measured from the test process
  instead, every sample carries the driver's own round trip, which is the size of the budget.
- **Percentiles are over the samples named in each row.** With 20–30 samples, p99 is the worst
  one; both are given where they differ.

Two conditions the benchmark needs, learnt by getting them wrong: **the window must be in
front, and the screen must be unlocked.** A covered window, a sleeping display and a locked
session are each not drawn at all — no animation frames, no terminal renders — so every
measurement that waits for a paint waits forever and a frame count reads zero. The benchmark
brings each window forward, holds the display awake, and refuses to start the window phase on a
locked screen.

## What it measured

| Limit | Measured | |
| --- | --- | --- |
| **Live sessions: 50** | 50 running, 49 of them streaming; the app at 139–148 MB | met |
| **2 MB burst: no freeze, input and other panes responsive** | painted 77 ms after the first byte (27.5 MB/s); longest frame 33 ms | met |
| **13 MB burst: the same** | 350 ms (37.4 MB/s); longest frame 62 ms; the whole burst arrived while the pane beside it was typed into, keystroke p50 24 ms, worst 80 ms | met, with the worst keystroke above the 50 ms row |
| **Keystroke to screen ≤ 50 ms while 49 others stream** | worst 33 ms with 49 streaming at ~1 MB/s each, worst 25 ms with 49 flat out (harnesses using 620% CPU between them) | met |
| **Tab or pane switch ≤ 100 ms** | back to a light tab worst 37 ms; to a tab whose session has all 5000 lines of history worst 53 ms; back to the typed tab under the 49-session load worst 40 ms | met |
| **Hook call ≤ 50 ms** | `charter hook pretooluse` through Python charter: **p50 101.5 ms**, worst 108 ms | **missed** |
| **Cold start ≤ 2 s** | p50 341 ms to the first frame on screen; worst 1035 ms, which was the first launch after the build and the only one cold on disk | met |
| **Idle hidden session ≤ 50 MB at the shipped scrollback cap** | **20.7 MB** each: 50 sessions with 5000 lines of history at 150 columns took the app from 115 MB to 1149 MB | met |
| **`?2026` animation ≥ 30 fps** | **51.8 draws/s** where the harness writes each repaint promptly; **1.0 draws/s** where it pauses inside an open update | met, with a hazard recorded below |
| tmux, as a reference | 25.8 MB/s at 2 MB, 27.0 MB/s at 13 MB, in a 150×42 window with a client attached through a pseudo-terminal | the app is above it |

Two more numbers that no limit asks for, because they are what a person does:

- **Opening a pane by splitting**, until the new pane has painted: worst 68 ms with a handful of
  panes; p50 105 ms and worst 530 ms when the twentieth pane is added, since every split
  re-lays out every pane.
- **Twenty panes on screen at once** all drew, each with its own live session.

## The renderer is xterm.js's own, not WebGL

The research expected WebGL and warned that WebKit caps a page at 16 WebGL contexts (§5.1), so
both were measured. **The DOM renderer is locked**, because WebGL won nothing and cost
something:

| | DOM | WebGL |
| --- | --- | --- |
| 132 KB corpus, first byte to painted | 40 ms | 84 ms |
| 13 MB burst | 37.4 MB/s | 37.2 MB/s |
| Keystroke beside a 13 MB burst, worst | 80 ms | 105 ms |
| Switch to a tab with 5000 lines of history, p50 | 37 ms | 48 ms |
| Keystroke with 49 streaming, worst | 33 ms | 25 ms |
| App memory with 50 sessions | 139 MB | 159 MB |
| `?2026`, repaints written promptly | 51.8 draws/s | 51.8 draws/s |

The context cap did not bite: **twenty panes each kept a live WebGL context, none fell back.**
That is worth knowing and it changes nothing, since the arm it would have justified is not
faster. `@xterm/addon-webgl` stays in the tree behind `app/src/renderer.ts`, loaded only if the
renderer is switched, so the arm can be re-measured rather than re-argued.

## The hook call is missed, and not by this stack

`charter hook pretooluse` costs 101.5 ms at the median. That is the Python start ADR 0025
already counted as a reason for the rewrite, measured again here: the hook path today is Python
charter's, and no drawing layer changes it. The Rust `charter` binary answers `charter root` in
**1.8 ms** on the same machine, which is the floor the limit will be held to once M2 and M3 move
hooks off Python. **The limit stays missed until then, and M3 is where it is met** — it is not a
reason to reopen the stack.

## The `?2026` hazard, and the decision it needs

xterm.js skips any render that falls while a synchronized update is open (`RenderService`), and
forces one only on a one-second safety timeout. Measured:

- Repaints written promptly: **51.8 draws/s**, well past the 30 fps the limit asks.
- Repaints written 4 KB at a time with 16 ms between writes, so an update stays open across
  frames: **1.0 draws/s** — the pane updates once a second. This is xterm.js#6071, which is why
  the spec lists the limit at all.

What decides it is not that a repaint arrives in pieces: **every repaint does.** A repaint
written in one write reaches the pane as roughly one message per kilobyte — a 3 KB repaint in
about four, a 10 KB one in about ten — because that is what the pseudo-terminal hands over.
Those arrive back to back and are parsed before the next frame, which is why the prompt case
draws at 52/s. A harness that opens an update and then pauses is the case that stalls.

No harness is known to do it: the recorded Claude Code session uses no `?2026` at all. It is
still a hazard the app can close for good, in the core rather than the UI: **the core already
parses every session's output and knows when an update is open, so it need never hand a pane a
chunk that ends inside one.** That is the recommendation; it is a change to the core's streaming,
not to the stack, and it belongs to M1. GPUI would also make the hazard impossible, by not
having xterm.js — which is the trade ADR 0025 weighed and priced, and one stalled animation
shape does not change it.

## The scrollback cap

The spec left open "the scrollback cap that the idle-session limit is measured at". It is
**5000 lines**, `SCROLLBACK` in the app, and it is measured: a hidden session holding 5000 lines
at 150 columns costs **20.7 MB**, against a 50 MB limit. Fifty of them cost 1.15 GB of the app's
memory, which is the number to weigh before raising the cap — the limit is per session, and the
product's scale is fifty.

## What would reopen this

- A limit a person notices that the drawing layer causes, and that the core cannot close. The
  `?2026` stall is the only candidate so far and the core can close it.
- Tauri's WebKit refusing contexts or frames at a pane count the app needs. Twenty panes, each
  with its own session, draw today.
- A harness whose output the DOM renderer cannot keep up with. It is at 37 MB/s on a 13 MB
  burst, above tmux on the same machine.

Until one of those happens, GPUI is not the fallback it was: it is an option that costs a
rewrite of the UI and buys nothing the limits ask for.

## The benchmark

`node tools/bench.mjs` in charter-app, with a line in its README. It builds what it needs,
measures cold start, the hook call, tmux and the window — the window once per renderer arm, one
WebdriverIO run per spec file so no spec measures what the one before it left running — and
writes `target/bench/<time>/results.json`. Every number here can be taken again with one
command, which is the point of it.
