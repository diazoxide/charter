# Embedding a terminal in a cross-platform GUI: what clears ADR 0018's bar

**Date:** 2026-09-17
**Question:** the operator wants a desktop GUI (macOS, Linux, Windows) that runs harness TUIs
(Claude Code, Codex, opencode) directly, with no background daemon, and orchestrates charter's
workspaces, personas, todos and repos. ADR 0018 rejected owning the emulator after a Textual +
`pyte` spike measured **1.85 MB/s** against **tmux at 25.2 MB/s end to end**. Which way of
embedding an emulator can host these harnesses and clear that bar?
**Follow-up question (same day):** the operator is also weighing a from-scratch rewrite. Four
more questions are covered in Part II (§5–§8):
* how each option holds up with 30–100 live terminals;
* Rust + Tauri 2, Rust + GPUI, Electron + TypeScript and Go + Wails as whole-app stacks;
* whether one core can also drive a terminal-mode frontend;
* whether Rust's mutation-testing story is actually faster.
**Method:** primary sources only: READMEs, source files, official docs, maintainers' own pull
requests, issues and benchmarks, and registry metadata (crates.io, npm, GitHub API). They were
read on 2026-09-17 at the commits pinned in §10. Every claim carries a URL or a path. Where no
primary source was found, the text says so in place. **This note runs no benchmarks of its
own.** Every number below was measured by someone else, on their machine and their corpus.

---

# Part I: embedding a terminal

## 0. What the bar actually is, and why most published numbers do not compare to it

ADR 0018 (`docs/adr/0018-charter-may-run-the-harness-but-never-draws-it.md:27-35`) and the spec
behind it (`docs/superpowers/specs/2026-08-21-harness-wrapper-design.md:17-31`) measured on
darwin, Python 3.14.4, tmux 3.7c, a **150×42 frame**, with **one corpus shaped like an agent
streaming** ("coloured text, full repaint every 40 lines"). Four results matter:

| | Textual + pyte | tmux composes |
| --- | --- | --- |
| end-to-end burst | 1.85 MB/s | **25.2 MB/s** |
| 2 MB log | 1.08 s frozen | 0.08 s |
| 13 MB build log | ~7 s frozen | 0.51 s |
| VT parse alone | 2.4 MB/s (0.9 with scrollback) | ~37 MB/s |

Three things follow for anyone reading the numbers below:

1. **The bar is end to end.** It covers PTY read, parse, grid update and pixels at 150×42. Most
   published figures are **parser-stage** numbers, with no PTY, no IPC hop and no renderer. Those
   are upper bounds and are marked as such in the table.
2. **The bar includes an outer terminal.** tmux re-emits the screen to whatever terminal the
   benchmark ran in. The ADR does not name that terminal. A GUI that embeds an emulator replaces
   tmux *and* the outer terminal.
3. **The corpus is not in the repository.** A search of `docs/`, `charter/` and `tests/` for the
   corpus or the spike found only the prose above. A prototype must re-record a corpus and
   **re-measure tmux on the same machine**. Otherwise the comparison is between two computers.

The spec also records *why* the numbers mattered: a frozen UI (the 1.08 s and 7 s rows). One
candidate below (xterm.js) decouples throughput from freezing by design (§3.1). So the bar
has two parts, and the prototype benchmark in §9.3 measures both.

---

## 1. Comparison

"Parser" means parser or terminal-stream stage only. "E2E" means app end to end. **No number**
means none was found in a primary source.

| Candidate | Best throughput evidence vs 25.2 MB/s E2E | Windows PTY | Maturity / licence / API stability | No-daemon, child on quit | Natural Python integration | Modern-TUI correctness (documented) |
|---|---|---|---|---|---|---|
| **xterm.js (WebGL) + node-pty in Electron** | Maintainer's guide: **5–35 MB/s** (written 2019). Maintainer's in-repo headless bench 2026-04: **28–33 MB/s** parse+buffer, no render. Parser-only micro-bench: ~2.5 GB/s. **Straddles the bar. No E2E number.** | node-pty: ConPTY only, Win10 1809+. Optional bundled `conpty.dll`. **Several open ConPTY kill/leak bugs** (Aug–Sep 2026). | xterm.js MIT, 6.0.0 (2025-12-22), 6.1 betas weekly. Used by VS Code. 6.0 had breaking changes. node-pty MIT, 1.1.0 latest, 1.2.0 betas. | Yes. PTY children get SIGHUP (POSIX) or CTRL_CLOSE_EVENT (ConPTY) when the app goes. Nothing survives quit. | Sidecar: spawn `charter` from Node main process, read plane files | 2026 ✓ (6.0; **open ~1 fps bug under continuous sync frames**). Kitty kbd ✓ only in 6.1 betas, opt-in. OSC 8 ✓, OSC 52 ✓ (6.0). Bracketed paste ✓, SGR mouse ✓, truecolor ✓ |
| **xterm.js + portable-pty in Tauri** (`tauri-plugin-pty`) | Same xterm.js parse numbers, plus Tauri IPC. The plugin pulls **one 4 KiB read per `invoke`**. **No number.** | portable-pty: ConPTY. Prefers a side-loaded `conpty.dll`. Passes `WIN32_INPUT_MODE`. | tauri-plugin-pty: MIT, 22 stars, README "Developing!". portable-pty 0.9.0 (2025-02). Linux webview is WebKitGTK. | Yes (same OS semantics) | Tauri "sidecar" is documented for bundled Python CLIs | Same as xterm.js. **Linux WebKitGTK WebGL2 availability risk** (maintainer, 2022; see §3.2) |
| **ghostty-web** (libghostty-vt → wasm, xterm.js-compatible API) | libghostty wasm `vt_write` in V8: **88–1103 MB/s** by workload (merged 2026-08-14). That is **after** npm `ghostty-web@0.4.0` (2025-12). Canvas-2D renderer. **No E2E number.** | Uses whatever PTY the host uses (node-pty in Coder's app) | MIT, 0.4.0, one company (Coder). Builds Ghostty with a patch. xterm.js maintainer calls libghostty adoption an open exploration. | Same as host | Same as host | Inherits Ghostty's VT core (2026 ✓, kitty ✓). Renderer/link layer is ghostty-web's own. **Maturity of that layer unmeasured** |
| **libghostty-vt native** (C/Zig; Rust via libghostty-rs) | Parser stage, M4 Max: **242–1186 MB/s**, and **342 MB/s on a 2.6 GB real-session recording**. Ghostty *app* E2E (`cat > tty`): **114–183 MB/s**. **Clears the bar, on the maintainer's machine** | libghostty-vt has **no PTY**. Ghostty's own `src/pty.zig` uses ConPTY. Ghostty GUI ships only Mac and Linux apps. | MIT. **"API is not yet stable and is definitely going to change."** No tagged version. | You write it | Third-party Python bindings exist (2 stars). Realistically a Rust/Zig host with a sidecar | Modes table: 2004, 2026, 2027, 1006/1016 mouse, kitty kbd encoder, kitty graphics. OSC clipboard "not properly exposed yet" (Ghostling) |
| **wezterm-term** (+ portable-pty) | **No number found** | portable-pty as above | MIT. **Not published on crates.io.** Maintainer promises **no API stability**. Last tagged wezterm release 2024-02-03, commits ongoing. | You write it | Rust host with a sidecar | OSC 8, sixel, iTerm2 and kitty images, bracketed paste. **2026 is "handled in wezterm's mux"**, not in the crate. Kitty keyboard via keyboard stack |
| **alacritty_terminal** in egui / iced / gpui (Zed) | vtebench exists, but the project publishes no MB/s. Ghostty's README says Ghostty and Alacritty are "within a few percentage points". **No first-party number** | Own ConPTY. Prefers a side-loaded `conpty.dll` + OpenConsole | Apache-2.0, 0.26.0 (2026-04-06), changelog marks breaking changes in bold. iced_term 0.8 ("Unstable widget API"), egui_term 0.1.0 (2025-04). gpui Apache-2.0, pre-1.0. **Zed's `terminal` crate is GPL-3.0** and uses a Zed fork of alacritty. | You write it | Rust host with a sidecar, or PyO3 | 2026 buffered in `vte::ansi` (150 ms timeout, 2 MiB). Kitty kbd, bracketed paste, SGR mouse, OSC 52 (configurable), hyperlinks |

---

## 2. The cross-cutting answers first

### 2.1 No daemon means sessions die with the app, on every stack

This is not a property of any candidate. It is how PTYs work.

* **POSIX:** "If fildes refers to the master side of a pseudo-terminal, and this is the last
  close, a SIGHUP signal shall be sent to the controlling process…"
  (https://pubs.opengroup.org/onlinepubs/9699919799/functions/close.html). node-pty's `kill()`
  also defaults to SIGHUP (`typings/node-pty.d.ts:182-188`).
* **Windows:** "Closing a pseudoconsole will send CTRL_CLOSE_EVENT to each client application that
  is still connected." Before Windows 11 24H2, `ClosePseudoConsole` "will wait indefinitely" if
  the output pipe is not drained
  (https://learn.microsoft.com/en-us/windows/console/closepseudoconsole).

**What charter loses by leaving tmux is detach and reattach.** Today's frame runs on a tmux
*server* (`charter/frame/tmuxctl.py`), which already outlives its client. Without a daemon, a
harness session survives a GUI quit only through the harness's own resume. Charter already
records that: `charter/frame/reopen.py:22` carries `--resume`. It also knows the cost:
`charter/frame/leave.py:306` notes "`claude --resume` re-renders the conversation and restores no
scrollback". VS Code, the largest xterm.js host, solves persistence with a separate pty-host
process and `terminal.integrated.enablePersistentSessions`
(`src/vs/platform/terminal/common/terminal.ts:104-105` in microsoft/vscode). That is a
helper process, which is the thing the operator ruled out.

### 2.2 Python integration: every GUI stack ends in a sidecar

* Charter ships `dependencies = []` and exposes almost no machine-readable CLI output. The one
  `--json` flag found in `charter/cli.py` is on doctor (`charter/cli.py:124`). The plane's
  existing answer is that **the files are the API**
  (`docs/research/2026-09-05-interface-agnostic-cores.md` §2.1). A GUI in any language can read
  `workspace.json`, todos and memory files directly, and shell out to `charter` for anything
  that writes.
* **Tauri** documents that pattern by name: a "sidecar", with "Python CLI applications or API
  servers bundled using `pyinstaller`" as the stated use case (tauri-docs
  `src/content/docs/develop/sidecar.mdx:6-8`).
* **Electron/Node:** spawn `charter` as a child process from the main process. Electron's
  `utilityProcess` is "the equivalent of child_process.fork"
  (https://www.electronjs.org/docs/latest/api/utility-process).
* **Rust-native (alacritty_terminal, wezterm-term, libghostty-rs):** sidecar, or embed CPython
  through PyO3 (https://pyo3.rs/main/python-from-rust.html). Embedding breaks charter's
  zero-dependency packaging story for nothing, because the CLI already exists.
* **Keeping the GUI in Python** is possible only with a native parser. `ghosttpy-vt` offers
  Python bindings for libghostty-vt (https://github.com/luckydonald/ghosttpy-vt, 2 stars, last
  push 2026-04-05). ADR 0018 found the parse was the cost, not the paint (7.2 ms/frame), so
  "Python GUI + native parser" is not ruled out by that ADR's own numbers. No primary source
  measures it, and the bindings are too young to lean on.

**Blocker for the Windows target, independent of the GUI:** charter declares itself POSIX-only.
`pyproject.toml:21-27` says "the harness builds and drives a tmux session and writes its vaults
at 0o600. Neither exists on Windows, there is no Windows CI… (#476)". `charter/frame/events.py:203`
and `charter/frame/palette.py:67` import `termios`. A Windows GUI that shells out to `charter`
needs charter's non-frame commands to run on Windows first. That is its own piece of work.

### 2.3 Windows: ConPTY is the only door, and the inbox version lags

* Every candidate that owns a PTY on Windows uses ConPTY: node-pty
  (README: "Windows 10 version 1809 (build 18309) or later is now required", winpty removed),
  portable-pty (`pty/src/win/pseudocon.rs:43-55`), alacritty_terminal
  (`alacritty_terminal/src/tty/windows/conpty.rs:31-38`) and Ghostty (`src/pty.zig:325-338`).
* **Newer ConPTY ships out of band.** portable-pty and Alacritty both side-load `conpty.dll` +
  OpenConsole from the Windows Terminal project when present. Alacritty's comment says it "offers
  many improvements and bugfixes compared to the standard conpty that ships with Windows".
  node-pty has an EXPERIMENTAL `useConptyDll` (`typings/node-pty.d.ts:99-105`). Windows Terminal
  release notes refer to "The ConPTY package"
  (https://github.com/microsoft/terminal/releases/tag/v1.25.923.0).
* **The 2024 rewrite matters for harnesses.** microsoft/terminal#17510 (merged 2024-08-01)
  removed ConPTY's re-rendering VtEngine: "any VT output that an application generates will now
  be given to the terminal unmodified". It reports ~20x throughput through ConPTY, and it opened
  the path to "synchronized updates". WT 1.22 calls it "the future we're staking ConPTY on"
  (https://github.com/microsoft/terminal/releases/tag/v1.22.10352.0). **Not found:** which inbox
  Windows build ships the rewritten ConPTY. Until that is known, a GUI should plan to bundle the
  ConPTY package.
* **Kitty keyboard through ConPTY:** Windows Terminal implemented the protocol
  (microsoft/terminal#19817, merged 2026-02-17). Translating ConPTY's own win32-input-mode to
  kitty is still an open question (#19847).
* **node-pty's open ConPTY bugs (all opened Aug–Sep 2026):** kill races teardown (#952), a
  handle leak per pty (#947), an unhandled socket error "kills the embedding host" (#960), a
  leaked `conhost.exe` per pty on natural exit (#965), and `kill()` able to "hard-kill an
  unrelated live process" (#967). Also #894: ~3.5 s output delay with `useConptyDll`.

---

## 3. Per-candidate detail

### 3.1 xterm.js (+ node-pty in Electron)

**Throughput.**
* The xterm.js flow-control guide, written by maintainer jerch in 2019, says the write path has
  "a rather low throughput (5 - 35 MB/s)". Data beyond a hardcoded write-buffer limit ("50MB at
  time of writing") **"gets discarded"**. `write` is time-sliced "to take less time than a single
  frame (16ms)" (xtermjs.org `_docs/guides/flowcontrol.md:20-22`). That last point means xterm.js
  does not freeze the way the Textual spike did. It falls behind and buffers instead. The embedder
  must implement watermark flow control, or output is lost.
* jerch's in-repo benchmark, 2026-04-24 (xtermjs/xterm.js#5838), for "Terminal: ls -lR /usr/lib"
  headless: write/string **27.98 MB/s**, write/utf8 **33.17 MB/s**. No renderer, no PTY, no IPC.
* Parser-only micro-benchmark in merged #5825 (contributor, merged by jerch, 2026-04-23):
  weighted **1,949 → 2,487 MB/s**. That is the parser state machine alone, so the gap to
  28–33 MB/s is buffer and handler work.
* Maintainer Tyriar (xterm.js#5686, 2026-02-09): "xterm.js is pretty competitive with performance
  of native terminals… we're definitely 'fast enough'". He adds that VS Code "prioritizes certain
  features over raw throughput". jerch estimates a wasm parser would be "roughly… a factor of 4"
  faster (microsoft/vscode#236991 comment, 2025-12-14).
* **Verdict against 25.2 MB/s:** the only first-party numbers put parse+buffer *at* the bar before
  rendering and before the node-pty→renderer IPC hop. **No primary source shows xterm.js clearing
  25.2 MB/s end to end.**

**Correctness.** Synchronized output (DEC 2026) added in 6.0.0 (#5453). OSC 52 added in 6.0.0
(#4220). OSC 8 fixes in 6.0.0 (release notes, https://github.com/xtermjs/xterm.js/releases/tag/6.0.0).
The docs list OSC 8 ✓, bracketed paste 2004 ✓, SGR 1006 and SGR-pixels 1016 mouse ✓, and RGB
SGR ✓. 1049 is "Partial: does not clear the alternate buffer" (xtermjs.org
`_docs/api/vtfeatures.md:343-354,481,628`). The kitty keyboard protocol landed 2026-01-10 (#5600),
**after** 6.0.0, behind `vtExtensions.kittyKeyboard` (`src/common/InputHandler.ts:259-262,2022`),
with open bugs (#5819, #5823, #6112). **Open:** "DEC 2026 synchronized output renders at ~1 fps
under continuous full-screen animation" (#6071), with fix PR #6073 unmerged. Claude Code was
measured emitting `?2026` (`docs/superpowers/specs/2026-08-21-harness-wrapper-design.md:38`).
**This is the one to test.** The WebGL addon needs a "webgl2 context" (README), and the canvas
renderer was removed in 6.0.0 (#5105).

**Maturity.** MIT. 21k stars. 6.0.0 on 2025-12-22, and `6.1.0-beta.304` on npm 2026-08-30. The
README says VS Code "typically uses the latest or near the latest beta build… can potentially
contain bugs or breaking changes". node-pty: MIT (README licence section), `latest` 1.1.0, beta
1.2.0-beta.15 (2026-08-03). Its README pins supported Node "to whatever version Visual Studio
Code is using".

**Production proof for this exact use.** Coder's agentic desktop app (repo `coder/xum`, described
as "A desktop app for isolated, parallel agentic development") ships Electron 40 +
`node-pty` + `@xterm/headless` + `ghostty-web` (`package.json:68,94-95,111,242-243`).

### 3.2 xterm.js in Tauri (portable-pty / tauri-plugin-pty)

* **Data path.** `tauri-plugin-pty` exposes a `read` command that allocates `vec![0u8; 4096]`,
  does one blocking read and returns it as `tauri::ipc::Response` (`src/lib.rs:115-138`). The JS
  side loops on `invoke("plugin:pty|read")` (`api/index.ts:337-341`). At 25.2 MB/s that is
  ~6,000 IPC round-trips a second. **No throughput number exists.** Tauri's docs say events are
  "not designed for low latency or high throughput… event payloads are always JSON strings".
  Channels "are designed to be fast and deliver ordered data" (tauri-docs
  `develop/calling-frontend.mdx:18-26,149-154`), and `ipc::Response` avoids JSON for array buffers
  (`develop/calling-rust.mdx:196-198`). A serious Tauri build would write its own PTY bridge on a
  Channel rather than use this plugin.
* **Maturity.** tauri-plugin-pty: MIT (Cargo.toml), 22 GitHub stars, 0.3.1 (2026-07-08), README
  "Developing! Wellcome to contribute!". portable-pty: MIT, 0.9.0 (2025-02-11), lives in the
  wezterm repo.
* **Webview risk on Linux.** Tauri uses WKWebView on macOS and `webkit2gtk` on Linux (tauri-docs
  `reference/webview-versions.md`). In tauri-apps/tauri#5761 (2022-12) a Tauri member wrote that
  "webkit2gtk published on almost all distros don't enable WebGL 2.0 and it fallsback to WebGL
  1.0", and another said WebGL2 might arrive in webkitgtk 2.40. xterm.js's WebGL addon requires
  WebGL2. **Not found:** a current primary statement on WebGL2 in distro WebKitGTK. Treat it as a
  risk to measure, not a fact.
* **Windows ConPTY** via portable-pty: prefers side-loaded `conpty.dll`. Defines
  `PSEUDOCONSOLE_PASSTHROUGH_MODE` but creates consoles with `INHERIT_CURSOR | RESIZE_QUIRK |
  WIN32_INPUT_MODE` (`pty/src/win/pseudocon.rs:25-29,85-87`).

### 3.3 ghostty-web (libghostty-vt compiled to wasm)

* "Ghostty for the web with xterm.js API compatibility… Migrate from xterm by changing your import"
  and "~400KB WASM bundle". It "builds from Ghostty's source with a patch"
  (coder/ghostty-web README). MIT. npm `latest` 0.4.0 (2025-12-09), `next` 0.4.0-next.20
  (2026-06-28). Renderer is Canvas 2D: `canvas.getContext('2d')` (`lib/renderer.ts:143`). A
  WebGL renderer is an open request (#155).
* **Throughput.** Ghostty merged ghostty-org/ghostty#13821 on 2026-08-14. It makes wasm
  `ghostty_terminal_vt_write` "1.4x to 13x faster", measured in V8 and JSC: ascii 1070 MB/s,
  scroll 304, cursor 255, utf8 169, sgr16 133, sgr-truecolor **88 MB/s**. The PR calls this
  "roughly 50-85% of the native ReleaseFast+SIMD build". These are terminal-write numbers with no
  canvas paint, and they **postdate every published ghostty-web build**. ghostty-web ships
  `bench/versus.ts` against xterm.js but publishes no results. Tyriar's prototype of libghostty
  inside xterm.js (#5686, February, before #13821) was "similar to the current parser".
* **Correctness.** Inherits Ghostty's VT state (§3.4). Its README claims better complex-script
  handling and XTPUSHSGR/XTPOPSGR than xterm.js. Links: `lib/providers/osc8-link-provider.ts`.
* **Governance risk, stated by the people involved.** On xterm.js#5686, jerch "downvote[s]"
  adopting libghostty because "we outsource the whole VT part to foreign maintainers", the parser
  is not hookable the way xterm.js's is, and wasm is hard to audit. Mitchell Hashimoto offered to
  extend the C API and noted "we haven't benchmarked [the C API or Wasm builds] yet". #13821 has
  since done the wasm half.

### 3.4 libghostty-vt (native)

* **What it is.** "A zero-dependency library… for parsing terminal sequences and maintaining
  terminal state". It excludes PTY management and GPU rendering
  (https://mitchellh.com/writing/libghostty-is-coming, 2025-09-22). The README (lines 157-160)
  says it is "usable today for Zig and C and is compatible for macOS, Linux, Windows, and
  WebAssembly… the API signatures are still in flux". `include/ghostty/vt.h:9-11`: "WARNING: This
  is an incomplete, work-in-progress API. It is not yet stable and is definitely going to
  change." README: "We haven't tagged libghostty with a version yet". MIT.
* **Throughput, all from the maintainer on an M4 Max, macOS 26, ReleaseFast:**
  * #13226 (merged 2026-07-06), `ghostty-bench +terminal-stream`, 120×80: **real 2.6 GB session
    recording 276 → 342 MB/s**, TUI redraw 240 → 342, ascii 838 → 1186. The PR says: "These are
    parser-stage numbers, not end-to-end app numbers."
  * #13220 (merged 2026-07-06): csi mix 154 → 242 MB/s, TUI redraw 156 → 344, CJK 236 → 794.
  * #13209 (merged 2026-07-06), **end to end** by `cat file > /dev/ttysN` into the Ghostty app:
    ascii 6.5 MB **114–123 MB/s**, unicode 8 MB **180–183 MB/s**. IO "is now within noise (1 to
    3%) of our VT parsing and processing throughput".
  * README: Ghostty and Alacritty are "usually within a few percentage points of each other on
    various benchmarks, but are both something like 100x faster than Terminal.app and iTerm".
* **Verdict against 25.2 MB/s:** the only candidate with a first-party end-to-end number. It is
  4.5–7x the bar, on different hardware and a different corpus. An embedder that writes its own
  renderer inherits the parser speed, not the app's.
* **Correctness.** `src/terminal/modes.zig:297-344` includes 1000/1002/1003/1006/1016 mouse,
  `bracketed_paste` 2004, `synchronized_output` 2026, `grapheme_cluster` 2027,
  `report_color_scheme` 2031 and `in_band_size_reports` 2048. `vt.h` exposes key encoding with the
  Kitty keyboard protocol, mouse encoding, paste (including "Kitty clipboard protocol paste
  events"), kitty graphics, scrollback, reflow, search and snapshot. Ghostling, the official
  minimal embedder, lists "Kitty keyboard protocol" and "Full 24-bit color". It also lists "OSC
  clipboard support" as not "properly exposed by libghostty-vt yet", and Windows as "could work
  but haven't been tested" (ghostty-org/ghostling README).
* **Windows.** libghostty-vt has no PTY. Ghostty's own `src/pty.zig` has a `WindowsPty` over
  `HPCON`, but the README describes shipped apps only for macOS (SwiftUI/Metal) and Linux (GTK).
  Community Windows front-ends exist (`mightty`, `phantty`, listed in Uzaaft/awesome-libghostty).
* **Bindings.** `libghostty-vt` on crates.io is Uzaaft/libghostty-rs, MIT OR Apache-2.0, 0.2.1
  (2026-07-18). It is not published by ghostty-org.

### 3.5 wezterm-term (+ portable-pty)

* **What it is.** "The core of the virtual terminal emulator implementation used by wezterm…
  terminal escape sequence parsing, keyboard and mouse input encoding, a model for the screen
  cells including scrollback, sixel and iTerm2 image support, OSC 8 Hyperlinks… This crate does
  not provide any kind of gui, nor does it directly manage a PTY" (`term/README.md`). MIT
  (`term/Cargo.toml`).
* **Not published.** crates.io answers "crate `wezterm-term` does not exist". The request to
  publish is still open (wezterm/wezterm#6663). There wez agreed in principle "with the proviso
  that I will make no particular effort for API stability across any semver change… the primary
  purpose of that crate will always be to serve the wezterm project" (2025-02-12).
* **Maintenance.** Last tagged release `20240203-110809-5046fc22` (2024-02-03). Commits continue
  (latest 2026-09-17), and `pty/` moved to `windows-sys` in 2026-08 (#8073).
* **Throughput.** **No primary-source number found.** A search of wezterm issues for vtebench
  found only #4788 (a lag bug).
* **Correctness caveat.** Synchronized output is not in the crate. `term/src/terminalstate/mod.rs:1693-1710`
  says "This is handled in wezterm's mux", and a DECRQM query "always report[s] false". An
  embedder must re-implement 2026. The keyboard encoder uses a per-screen `keyboard_stack`
  (`term/src/terminalstate/keyboard.rs:7-18`) for CSI-u/kitty-style encodings. Bracketed paste
  is handled (`mod.rs:880-959`). Kitty image placement is in `terminalstate/kitty.rs`.

### 3.6 alacritty_terminal in egui, iced or gpui (Zed)

* **Crate.** Apache-2.0, 0.26.0 (2026-04-06), 1.56 M downloads (crates.io). The changelog marks
  breaking changes in bold, e.g. "**`ChildEvent::Exited`… now contain `ExitStatus` instead of
  `i32`**" (`alacritty_terminal/CHANGELOG.md`). It includes its own PTY layer, with ConPTY on
  Windows.
* **Throughput.** The Alacritty README says it "uses vtebench to quantify terminal emulator
  throughput and manages to consistently score better than the competition". vtebench itself
  warns: "The only factor this benchmark stresses is the speed at which a terminal reads from the
  PTY… please do not jump to any conclusions" (alacritty/vtebench README). **Neither repository
  publishes MB/s.** The only comparative claim is Ghostty's (§3.4). Embedded in a widget toolkit,
  the renderer is the toolkit's, not Alacritty's OpenGL one, so even that claim does not transfer.
* **Correctness (source).** Synchronized output is buffered in `vte::ansi::Processor`:
  `SYNC_UPDATE_TIMEOUT` 150 ms, `SYNC_BUFFER_SIZE` 2 MiB, BSU/ESU `\x1b[?2026h/l`
  (alacritty/vte `src/ansi.rs:36-48`). The embedder must drive `sync_timeout()`
  (`ansi.rs:292`). Kitty keyboard protocol mode stack, gated by `config.kitty_keyboard`
  (`alacritty_terminal/src/term/mod.rs:81,349-350,1276-1289`). `BRACKETED_PASTE`, `SGR_MOUSE`,
  `osc52: Osc52` config (`mod.rs:60-62,352-372`). `Hyperlink` type in vte
  (`ansi.rs:51`). `scrolling_history` default 10000 (`mod.rs:336,359`).
* **Widgets.**
  * `iced_term`: MIT, 0.8.0 (2026-03-27), 178 stars. README: "Unstable widget API… I can not
    promise the stable API". "does not provide full terminal features". "tested on MacOS, Linux
    and Windows".
  * `egui_term`: MIT, a single release 0.1.0 (2025-04-24), 72 stars. Same disclaimer.
  * **Zed / gpui:** Zed's `crates/terminal` is `license = "GPL-3.0-or-later"` and depends on
    `alacritty_terminal = { git = "https://github.com/zed-industries/alacritty", rev = … }`
    (`crates/terminal/Cargo.toml`; root `Cargo.toml:523`). **It cannot be lifted into an
    MIT-licensed charter.** `gpui` itself is Apache-2.0 (`crates/gpui/Cargo.toml:9`) but "still
    pre-1.0. There will often be breaking changes between versions" (`crates/gpui/README.md`).
    crates.io has `gpui` 0.2.2 (2025-10-22), far behind Zed `main`.

### 3.7 Other contenders

* **restty** (wiedymi/restty): MIT, 408 stars. "Powered by libghostty-vt, WebGPU, and
  text-shaper" with xterm.js API compatibility. Same family as ghostty-web with a GPU renderer.
  **No numbers found.** Not evaluated further.
* **rio-backend** (raphamorim/rio): MIT, 0.5.26 (2026-08-23). Rio targets "desktops and browsers".
  **Not evaluated beyond registry metadata.**
* **Qt (QTermWidget / Konsole KPart), VTE (GTK), Windows Terminal's control:** not researched.
  Each looked single-platform or single-toolkit from the brief, and this note makes no claim
  about them.

---

## 4. Where the evidence points

1. **Only libghostty-vt has a first-party end-to-end number that clears 25.2 MB/s**, and it clears
   it several times over (114–183 MB/s). That is Ghostty's own app on an M4 Max, not an embedder.
2. **xterm.js's own maintainers put it at the bar, not above it** (5–35 MB/s documented, 28–33
   MB/s headless in 2026). It avoids the Textual spike's *freeze* by time-slicing, at the cost of
   buffering and, past 50 MB, discarding. Whether it clears the bar end to end on the harness
   corpus is unmeasured.
3. **alacritty_terminal and wezterm-term are credible engines with no published MB/s.**
   wezterm-term is unpublished and explicitly unstable. Both lock the GUI into a Rust UI toolkit
   that is itself pre-1.0 (iced, gpui) or thin (egui_term).
4. **ghostty-web joins the two halves.** It is xterm.js's API, which the whole Electron/Tauri
   ecosystem speaks, over Ghostty's parser, whose wasm build now measures 88–1103 MB/s. It is
   young (0.4.0, one vendor, a patched Ghostty build) and already in production in an agentic
   desktop app.

---

# Part II: if charter is rewritten

## 5. Thirty to a hundred live terminals in one app

### 5.1 Web renderers: a GPU context per visible terminal, and a hard cap of 16

* **Chromium caps active WebGL contexts at 16 per renderer process by default** (8 on Android).
  `prefs.max_active_webgl_contexts = 16u;` is overridable by the `max-active-webgl-contexts`
  switch (chromium `content/renderer/webgraphicscontext3d_provider_impl.cc:121-139`). The switch
  is described as the "maximum number of active WebGL contexts per renderer process"
  (`content/public/common/content_switches.cc:509-511`). Past the cap, Blink logs "WARNING: Too
  many active WebGL contexts. Oldest context will be lost."
  (`third_party/blink/renderer/modules/webgl/webgl_rendering_context_base.cc:505`).
* **WebKit's cap is a compile-time constant with no switch:** `static constexpr size_t
  maxActiveContexts = 16;` (WebKit `Source/WebCore/html/canvas/WebGLRenderingContextBase.cpp:192`,
  commit `a9821969a0`). WebKit backs Tauri and Wails on macOS (WKWebView) and Linux (WebKitGTK)
  (tauri-docs `reference/webview-versions.md`; Wails README lines 110-114). **So on those two
  platforms a web UI cannot hold more than 16 live WebGL terminals per page, whatever the app
  does.** Electron and WebView2 on Windows are Chromium, so the switch applies there. Whether
  Tauri or Wails pass that switch to WebView2 was not checked.
* **VS Code hit the cap and raised it.** microsoft/vscode#285575 (Tyriar, merged 2026-01-05)
  adds `app.commandLine.appendSwitch('max-active-webgl-contexts', '32')` in `src/main.ts`, with
  the comment "each terminal may use up to 2", and the PR says it "Defaults to 16".
  vscode#285579 (merged 2026-01-02): "The webgl renderer has a limited number of contexts,
  because there are potentially many chat terminals… we don't want to consume one of the
  contexts". Chat terminals attach with `{ enableGpu: false }`. When a context is lost, VS Code
  disposes the WebGL renderer and falls back to DOM. If WebGL fails to load, **every later
  terminal** is set to DOM (`XtermTerminal._suggestedRendererType = 'dom'`,
  `src/vs/workbench/contrib/terminal/browser/xterm/xtermTerminal.ts:941-958`). VS Code's
  `setVisible` does not release the renderer when a terminal is hidden
  (`terminalInstance.ts:1442-1458`).
* **Disposing doesn't give the context back today.** xterm.js#6068 (open, 2026-07-24):
  "`WebglAddon.dispose()` does not release the underlying WebGL2 context". Past ~16 create and
  dispose cycles, "a **still-live** terminal loses its renderer". xterm.js#4379 ("Support dozens
  of terminals on a single page", open since 2023) proposes one shared context. Tyriar replied
  that the hard part is "a nice method of creating and sharing the context… not all xterm.js
  instances are necessarily visible at once".
* **ghostty-web uses Canvas 2D** (`lib/renderer.ts:143`), so the WebGL cap does not apply. Canvas
  2D on WebKitGTK has its own Linux performance complaint in tauri-apps/tauri#5761.

**What this means for design:** in a web UI, only *visible* terminals can own a GPU renderer.
Hidden sessions have to live somewhere headless, and a renderer is attached when they are shown.
xterm.js ships exactly that piece: "`@xterm/headless`… keep track of a terminal's state where the
process is running and using the serialize addon so it can get all state restored upon
reconnection" (xterm.js README line 118). Coder's agentic app depends on `@xterm/headless` for
this (`coder/xum` `package.json:95`). libghostty's equivalent is its snapshot format: "libghostty
is mainly used on web as a terminal _viewer_ and snapshots are the best, most efficient way to
ship down full terminal state". Wasm decode runs at 758–1100 MB/s (ghostty-org/ghostty#13848,
merged 2026-08-15; `include/ghostty/vt/snapshot.h`).

**A TypeScript core caps how many sessions it can parse at once.** Headless xterm.js parses on
the Node event loop at the 28–33 MB/s measured in §3.1, shared by every session in that process.
node-pty "is not thread safe so running it across multiple worker threads in node.js could cause
issues" (node-pty README, Thread Safety). **Not found:** any measurement of N concurrent headless
xterm.js instances.

### 5.2 Memory per terminal (derived from source; nobody has measured this)

Each figure is the source's cell size multiplied out for a 150-column terminal with its
scrollback full. JS object overhead, allocator slack and renderer memory are not counted.

| Engine | Per-cell / per-surface figure (source) | 150 cols, full scrollback | ×100 sessions |
|---|---|---|---|
| xterm.js | 3 `Uint32Array` slots per cell = 12 bytes (`src/common/buffer/BufferLine.ts:33-37,87`). VS Code default scrollback 1000 lines (`terminalConfiguration.ts:317-320`) | 1,042 lines → ~1.9 MB. 10,042 lines → ~18 MB | ~190 MB / ~1.8 GB |
| alacritty_terminal | `EXPECTED_CELL_SIZE: usize = 24`, a test asserts `size_of::<Cell>() <= 24` (`alacritty_terminal/src/term/cell.rs:310-313`). Zed defaults 10,000 lines, max 100,000 (`crates/terminal/src/terminal.rs:890-891`) | 10,042 lines → ~36 MB | ~3.6 GB |
| libghostty-vt / Ghostty | `scrollback-limit-bytes` default **50 MB per surface**, "allocated lazily", with idle compression of historical pages (`src/config/Config.zig:1373-1394`) | cap 50 MB | cap 5 GB |

A coding-agent session is long-lived and chatty, so **the scrollback limit, not the engine, sets
the memory bill at 100 sessions**. A rewrite should cap it per session and treat the transcript
as the harness's own record. That is ADR 0018's 2026-09-01 amendment again: the transcript is
kept as a file.

### 5.3 Native renderers

No per-process context cap like WebKit's or Chromium's was found for GPUI, iced or egui. Zed
draws its terminals with GPUI in one window. **No primary source measures a native toolkit with
dozens of live terminal views.** Zed's code sets a scrollback cap (above) but documents no view
cap.

---

## 6. Whole-app stacks for a rewrite

| | Rust core + Tauri 2 (TS UI) | Rust core + GPUI | Electron + TypeScript | Go + Wails |
|---|---|---|---|---|
| **Binary size** | Tauri benchmark, hello world, 2026-09-17: **2.95 MB** Linux, **2.79 MB** macOS (`tauri-apps/benchmark_results` gh-pages `tauri-recent-*.json`). Docs: "a minimal Tauri app can be less than 600KB" (tauri-docs `start/index.mdx:41`) | Only data point is Zed itself, a full editor: 73–86 MB `.exe`, 122–135 MB `.dmg`, 121–129 MB Linux tarball (release v1.20.2 assets). **No minimal GPUI app measured** | Electron v44.4.1 runtime zips: **130 MB** darwin-arm64, **123 MB** linux-x64, **158 MB** win32-x64 (release assets). Tauri's benchmark: Electron hello world **166 MB** (data last updated 2023-09-24) | **No primary number found** |
| **Idle / peak memory** | Tauri benchmark `max_memory` on Linux CI, via `mprof run -C` (children included, `tauri/bench/src/run_benchmark.rs:95-115`): hello world **421 MB** (2026-09-17) | **No primary number found** | Same benchmark: hello world **476 MB**, **stale since 2023-09-24** | **No primary number found** |
| **Windows / Linux maturity** | Tauri 2.0.0 released 2024-10-02, 2.11.5 on 2026-07-01. Windows = WebView2, Linux = WebKitGTK, with maintainers' own Linux perf caveats (§3.2) | Zed: "Windows is now a fully supported platform" (zed.dev/blog/zed-for-windows-is-here, 2025-10-15). gpui "still pre-1.0. There will often be breaking changes" (`crates/gpui/README.md:8`). crates.io `gpui` 0.2.2 (2025-10-22) lags Zed `main` | VS Code on all three. Chromium bundled, so one engine everywhere | Wails v2.14.0 stable (2026-08-10). v3 is `beta.23` (2026-09-16) and defaults to GTK4 + WebKitGTK 6.0 on Linux (README 110-114). WebView2 on Windows (`v2/internal/webview2runtime/`) |
| **End-to-end UI tests** | WebdriverIO `@wdio/tauri-service` "works on **Windows, Linux, and macOS**". Plain `tauri-driver` is "only Windows and Linux… macOS has no WKWebView driver tool" (tauri-docs `develop/Tests/WebDriver/index.mdx:13-21,82-83`). Also a mock runtime for unit/integration tests (`develop/Tests/index.mdx:10-11`) | In-process only: `#[gpui::test]`, `TestAppContext`, "test implementation of the `ForegroundExecutor` and `BackgroundExecutor` which ensure that your tests run deterministically" (`crates/gpui/src/test.rs:1-9`). `simulate_keystrokes`, `simulate_window_resize`, `run_until_parked` (`app/test_context.rs:456,560,582`). **No black-box driver documented** | Playwright `_electron`: "Playwright has **experimental** support for Electron automation" (microsoft/playwright `docs/src/electron-api/class-electron.md`) | The guide drives Playwright against the **dev server in a browser** (`baseURL: 'http://localhost:9245'`), not the packaged webview (wails `docs/mpress/content/guides/e2e-testing.mpd`) |
| **Terminal path** | Rust owns PTYs and VT state. The webview renders visible panes via IPC Channels + wasm/JS viewer, under WebKit's 16-context cap on macOS/Linux | Rust owns PTYs, VT state and GPU paint in one process. Zed's shape, but Zed's `terminal` crate is GPL-3.0 | node-pty + xterm.js (§3.1), headless for hidden panes | **Not evaluated**: no Go-native VT engine was researched, so there is no evidence either way |

## 7. One core, a GUI and a terminal-mode frontend

**It is being done in production.** In OpenAI's Codex, `codex-rs/tui` is a ratatui app
(`tui/Cargo.toml:93`) that depends on `codex-app-server-client` and
`codex-app-server-protocol` (`tui/Cargo.toml:32-34`). The app-server README describes
"bundled, in-process TUI sessions (`codex-tui`) and local stdio desktop sessions (`Codex
Desktop`)" served by the same app-server (`codex-rs/app-server/README.md:72-75`, commit
`e269f2164c`). So the shape is one core crate, one protocol, run in-process for the TUI and over
stdio for the desktop app. The same repository also carries an `app-server-daemon` crate. A
charter rewrite would have to decide not to grow one.

**What it costs is the harness pane, and it is ADR 0018's question again.** A GUI paints the VT
engine's grid with the GPU. A TUI has to paint that grid *into another terminal*. Two known
answers:

* **Nest an emulator in the TUI.** `tui-term` does this for ratatui with the `vt100` crate
  ("Reading the output from the process and using the vt100 crate to parse the output bytes",
  a-kenji/tui-term `docs/ARCHITECTURE.md`, commit `d2d73a1c2a`). Its README says: "currently in
  active development and should be considered a work in progress". Zellij, the mature Rust
  example, owns its own grid and VT interpretation in a server process (zellij `docs/ARCHITECTURE.md`:
  `zellij-server/src/panes/grid.rs`, `PtyBus`). In a Rust core the inner parse is native, so the
  Python parse that sank the ADR 0018 spike is gone. But every byte is still emulated twice (core,
  then the outer terminal), and whatever does not survive a cell grid has to be re-emitted by the
  TUI: synchronized output, OSC 8, kitty keyboard and graphics. **No primary source measures
  tui-term or Zellij throughput.**
* **Delegate the pane to tmux**, which is today's charter. The TUI frontend then stays a thin
  client of the core, and the core's own VT engine goes unused in terminal mode.

**Cheap part:** everything that is not a harness pane (workspaces, personas, todos, repos) is
a list or a form over the core's protocol, and ratatui draws that directly. **Expensive part:**
a TUI that hosts harness panes itself is a terminal multiplexer, the thing ADR 0018 decided not
to build. Keeping tmux for terminal mode avoids that.

## 8. Mutation testing in Rust: `cargo-mutants`

From the cargo-mutants book (sourcefrog/cargo-mutants `book/src/`, commit `fe82f18327`; MIT;
crate 27.1.0, 2026-06-02):

* **Every mutant costs an incremental build plus a test run.** "Most of the runtime for
  cargo-mutants is spent in running the program test suite and in running incremental builds:
  both are done once per viable mutant" (`performance.md:4-5`). Per mutant it runs `cargo test
  --no-run`, then `cargo test` (`how-it-works.md`).
* **It does not pick tests per mutant.** The tracking issue "Use coverage data to decide which
  functions to mutate and which tests to run" has been open since 2022-02-13 (#24). Python's
  mutmut, by contrast, advertises "Knows which tests to execute, speeding up mutation testing",
  but it runs pytest and "must be run on a system with `fork` support" (boxed/mutmut README
  lines 21, 35-36, 50). Charter's suite is stdlib `unittest` (`CONTRIBUTING.md:13`).
* **Link time dominates fast suites.** "Because cargo-mutants does many incremental builds, link
  time is important". Mold gives "a 20% performance improvement", and "On one tree, using Wild cut
  the time to run cargo-mutants by more than half" (`performance.md:67-73`). The book also
  suggests a `mutants` profile with `debug = "none"` (`performance.md:28-42`).
* **Parallelism is bounded by disk and RAM, not cores.** "start at `-j2` or `-j3`… Higher
  settings are only likely to be helpful on very large machines, perhaps with >100 cores and
  >256GB RAM". "Rust `target` directories can commonly be 2GB or more, and there will be one per
  parallel job" (`parallelism.md:101-113`). Sharding across CI machines follows
  `N_SHARDS * (SHARD_STARTUP + CLEAN_BUILD + TEST) + N_MUTANTS * (INCREMENTAL_BUILD + TEST)`
  (`shards.md`).
* **Narrowing tools exist.** `--in-diff` "test[s] only mutants that overlap with regions changed
  in the diff" and composes with `--package` (`in-diff.md`). nextest fails fast but "allows
  straggling tests to run to completion" (`nextest.md`).
* **A cross-platform core hits a known gap.** "cargo-mutants does not yet understand conditional
  compilation, such as `#[cfg(target_os = "linux")]`. It will report functions for other platforms
  as missed" (`limitations.md:38-40`). The PTY layer is exactly that kind of code (ConPTY vs
  `openpty`).

**What that adds up to.** No primary source compares mutation-run time between a Python and a
Rust codebase. Rust does not make mutation testing fast by default. It swaps the cost of Python's
test runs for an incremental compile and link per mutant, with no per-mutant test selection.
What makes it fast is structure you choose: a small domain-core crate with no GUI or PTY
dependencies, mutated alone (`--package`), with a fast linker, `--in-diff` in CI and shards for
full runs. If the GUI toolkit or the terminal engine sits in the same crate as the logic under
test, every mutant pays its compile.

---

## 9. Recommendation

### 9.1 If charter stays Python and only gains a GUI

**Build the prototype in Electron with node-pty, behind the xterm.js API, and benchmark two
engines in the same shell: `@xterm/xterm` + WebGL, and `ghostty-web` rebuilt from a Ghostty
commit at or after #13821.** Ship whichever clears the bar.

* **Not native Rust.** A Rust GUI over a Python core is two codebases plus a sidecar, for a
  throughput win the web engines may not need.
* **Not Tauri.** Its xterm.js path gets the same engine but a weaker PTY bridge (a 22-star plugin
  doing one 4 KiB `invoke` per read). It also runs on WebKit, whose 16-context cap cannot be
  raised on macOS/Linux (§5.1).
* **Electron + node-pty + xterm.js is the stack VS Code runs on Windows**, and VS Code has
  already solved the many-terminal WebGL budget with a documented switch (§5.1). node-pty is
  maintained by the same organisation as ConPTY. Coder's agentic desktop app ships Electron +
  node-pty + `@xterm/headless` + ghostty-web, so the engine swap is an import change.
* **Charter stays Python and `dependencies = []`.** The GUI reads the plane's files and shells out
  to `charter`. Owed first: a Windows-capable `charter` (POSIX-only today, `pyproject.toml:21-27`)
  and a bundled ConPTY package.
* **"No daemon" costs reattach.** Quitting ends every harness session (SIGHUP /
  CTRL_CLOSE_EVENT). Continuity comes from harness resume, which charter already records.

### 9.2 If charter is rewritten

**A Rust workspace, with the GUI in GPUI and terminal mode kept on tmux:**

* **`core`**: the domain (workspaces, personas, todos, repos, files-as-API). No PTY, UI or
  engine dependency, so `cargo mutants --package core` pays only its own compile (§8).
* **`sessions`**: `portable-pty` plus one VT engine per session, all of them headless and owned
  here, for 30–100 sessions (§5). The engine sits behind a trait. **Default to
  `alacritty_terminal`**: Apache-2.0, released on crates.io, and the engine Zed draws with GPUI on
  all three OSes. Benchmark `libghostty-vt` against it (below) and switch only if the numbers
  say so, because libghostty's C API "is definitely going to change" (§3.4).
* **`protocol`**: the Codex app-server shape (§7), run in-process for both frontends. No
  daemon crate.
* **GUI in GPUI.** Terminal paint reads the engine's grid in the same process, with no IPC, no
  wasm and no WebGL context budget. Its test harness is deterministic and in-process (§6), which
  is the property a mutation-heavy workflow needs. The costs, stated: pre-1.0 with "often…
  breaking changes"; pinned to Zed's git rather than crates.io; no black-box E2E driver; Zed's
  own terminal crate is GPL-3.0 and cannot be copied; slower UI iteration than web.
* **TUI in ratatui for everything except harness panes, which stay in tmux.** A TUI that hosts
  panes itself is a multiplexer (§7), and ADR 0018 already decided that tmux is.

**Why not the others, for a rewrite.** Tauri puts every visible terminal behind IPC into a
WebKit page capped at 16 WebGL contexts on macOS/Linux, and runs on WebKitGTK on Linux. That is
the most exposed spot for "many live terminals". Electron + TypeScript parses every hidden
session on one Node event loop at the ~30 MB/s xterm.js measures. Go + Wails has the same webview
limits as Tauri, no Go VT engine was evaluated, and its E2E guide tests a browser, not the app.

**What would change this call:** GPUI churn making a pinned revision unmaintainable; the
operator weighting UI iteration speed over terminal density (then Tauri, with ghostty-web as the
viewer); or the benchmark below showing `alacritty_terminal` and `libghostty-vt` both below the
bar once paint is included.

### 9.3 The one benchmark to run

**ADR 0018's end-to-end burst, run in the prototype, with tmux re-measured beside it on the same
machine.** For the rewrite, add a many-session load.

1. **Re-record the corpus.** It is not in the repo. Replay an ordinary Claude Code session's own
   output, "coloured text, full repaint every 40 lines", keeping its `?2026` blocks. Make 2 MB
   and 13 MB versions too.
2. **Frame:** 150×42. The child is a real PTY process (`cat corpus`), so PTY read and any IPC
   are inside the measurement.
3. **Throughput:** time from the child's first byte until a sentinel line appended to the
   corpus has been **painted** (seen in the grid, then one more frame presented). Report MB/s.
4. **Freeze half, in the same run:** longest main-thread stall and keystroke-echo latency during
   the 2 MB and 13 MB replays. These are the spec's "1.08 s frozen" and "~7 s frozen" rows. Add
   xterm.js's `yes` then Ctrl-C test (`flowcontrol.md:75`).
5. **Arms.** GUI-only (§9.1): `@xterm/xterm` WebGL vs post-#13821 `ghostty-web` vs tmux. Rewrite
   (§9.2): GPUI + `alacritty_terminal` vs GPUI + `libghostty-vt` vs tmux. Run on macOS, and on
   Windows 11 with the bundled ConPTY package.
6. **Many-session load (rewrite arm):** repeat step 3 on one visible pane while 29, then 99,
   other headless sessions replay the corpus in a loop. Record process RSS with every session's
   scrollback full at the cap you intend to ship.
7. **Pass:** visible-pane throughput at or above the tmux arm on that machine, no multi-second
   stall, RSS at 100 sessions within whatever budget the operator names, and `claude` / `opencode`
   still rendering a continuous `?2026` animation at more than 1 fps (xterm.js#6071 is why).

---

## 10. Sources (pinned where a repository was read)

Repositories read at these commits on 2026-09-17:
xtermjs/xterm.js `c58ea3637f` · xtermjs/xtermjs.org `9935301eff` · microsoft/node-pty `8209066178`
· ghostty-org/ghostty `f9a3f24a56` · ghostty-org/ghostling `63842bf8e5` · coder/ghostty-web
`1858a59477` · wezterm/wezterm `b09b56c29c` · alacritty/alacritty `d692748d3f` · alacritty/vte
`abeae765dd` · zed-industries/zed `b6171bcc98` · Tnze/tauri-plugin-pty `dfbc2d1824` ·
tauri-apps/tauri-docs `e708480b83` · tauri-apps/benchmark_results (gh-pages) `b6967f0494` ·
kemokempo/iced_term `fafdae9d08` · kemokempo/egui_term `31bbc7ab85` · microsoft/vscode
`d9e7941114` · coder/xum `af4e317c9f` · WebKit/WebKit `a9821969a0` · chromium/src `main` via
chromium.googlesource.com (2026-09-17) · microsoft/playwright `f1ea64bd00` · wailsapp/wails
`6d919787ca` · openai/codex `e269f2164c` · a-kenji/tui-term `d2d73a1c2a` · zellij-org/zellij
`474ea0cef6` · sourcefrog/cargo-mutants `fe82f18327` · boxed/mutmut `14a7230049`.

Issues and pull requests (github.com/<repo>/issues/<n> or /pull/<n>):
* xtermjs/xterm.js: #4379, #4220, #5003, #5105, #5453, #5600, #5686 (with comments by Tyriar,
  mitchellh and jerch), #5819, #5823, #5825, #5838, #6068, #6071, #6073, #6112, #6137
* microsoft/vscode: #236991 (comment 3650962189), #285575, #285579
* microsoft/node-pty: #894, #947, #952, #960, #965, #967
* microsoft/terminal: #17510, #19817, #19847. Releases v1.22.10352.0 and v1.25.923.0
* ghostty-org/ghostty: #13209, #13220, #13226, #13821, #13848
* wezterm/wezterm: #6663, #4788, #8073
* tauri-apps/tauri: #3988, #5761. Release tauri-v2.0.0 (2024-10-02), tauri-v2.11.5
* electron/electron: release v44.4.1 assets. zed-industries/zed: release v1.20.2 assets.
  wailsapp/wails: v2.14.0, v3.0.0-beta.23
* coder/ghostty-web: #155. sourcefrog/cargo-mutants: #24

Documents:
* https://mitchellh.com/writing/libghostty-is-coming
* https://zed.dev/blog/zed-for-windows-is-here
* https://pubs.opengroup.org/onlinepubs/9699919799/functions/close.html
* https://learn.microsoft.com/en-us/windows/console/closepseudoconsole
* https://www.electronjs.org/docs/latest/api/utility-process
* https://pyo3.rs/main/python-from-rust.html

Registries: crates.io API (`alacritty_terminal`, `portable-pty`, `wezterm-term` (absent),
`termwiz`, `tauri-plugin-pty`, `libghostty-vt`, `iced_term`, `egui_term`, `gpui`, `rio-backend`,
`tui-term`, `cargo-mutants`). npm registry (`@xterm/xterm`, `node-pty`, `ghostty-web`).

In this repo: `docs/adr/0018-charter-may-run-the-harness-but-never-draws-it.md`,
`docs/superpowers/specs/2026-08-21-harness-wrapper-design.md`, `charter/frame/tmuxctl.py`,
`charter/frame/reopen.py`, `charter/frame/leave.py`, `charter/cli.py`, `pyproject.toml`,
`CONTRIBUTING.md`, `docs/research/2026-09-05-interface-agnostic-cores.md`.

## 11. Where the evidence is thin

* **No candidate has a published end-to-end number in an embedding context.** Ghostty's 114–183
  MB/s is Ghostty's own app. Everything else is a parser-stage or headless number.
* **No MB/s exists in any primary source for alacritty_terminal, wezterm-term, tui-term or
  Zellij.** The §9.2 default of `alacritty_terminal` rests on licence, release discipline and
  Zed's use, not on a throughput figure.
* **xterm.js's "5–35 MB/s" dates from 2019.** The 28–33 MB/s figure is one headless benchmark on
  an unstated machine.
* **ghostty-web publishes no benchmark results.** The wasm figures are libghostty's and postdate
  ghostty-web's releases.
* **Memory at 100 sessions (§5.2) is arithmetic from cell sizes, not a measurement.** Whether
  alacritty_terminal allocates scrollback lazily was not verified.
* **No idle-memory figure exists for GPUI or Wails.** Electron's memory figure in Tauri's
  benchmark is three years old. Tauri's own figure comes from a benchmark Tauri runs.
* **No primary source compares mutation-testing time between Python and Rust**, and cargo-mutants
  gives no general build-vs-test numbers.
* **Not checked:** whether Tauri or Wails expose Chromium switches on WebView2; a Go VT engine;
  Qt/VTE-based options.
* **Inbox ConPTY version by Windows build:** not found. **WebGL2 in current distro WebKitGTK:**
  only a 2022 maintainer statement.
* **The ADR 0018 corpus, and the outer terminal the tmux arm ran in,** are not recorded in the
  repository.
