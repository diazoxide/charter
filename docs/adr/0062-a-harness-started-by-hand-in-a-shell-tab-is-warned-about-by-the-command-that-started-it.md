# A harness started by hand in a shell tab is warned about by the command that started it

**Accepted 2026-09-26**, for SI-5 (plain shell tabs).

A **shell tab** is the operator's own `$SHELL` in a tab of the app: no profile, no persona, no
harness. It is `open_session` with no program, which until now only the Saving tab's blocked
save offered ("Open terminal here"). It is now a catalogue row, `New shell`, beside `New tab`: in
the palette, on the panes' menu, and on each workspace's menu as `New shell in <workspace>`,
with the key ⌘⇧T on a Mac and Ctrl+Shift+T elsewhere (`app/src/shellKey.ts` says why that key
and what it takes from a chat). It starts where a new chat would, is filed on that workspace's
strip, is recorded and put back like any chat, and its tab wears a terminal's mark.

A shell tab has one hazard the operator asked charter to catch. Typing `claude` there starts a
harness charter did not start: no profile, no persona, no state hooks of its own, nothing to
resume it by. It works, and it is invisible to everything charter does for a chat.

## The decision

**The detection is the command being launched, and nothing else.** A shell tab's `PATH` begins
with a directory of charter's own holding one script per harness charter starts — `claude`,
`codex`, `opencode` — each of which runs `charter shell-guard <harness> -- "$@"`. That command:

1. says one line on standard error: this harness runs outside charter's session tracking, and
   a chat tab is where it should run;
2. tells the app over the hook socket, as a `StartedByHand` line keyed by `CHARTER_CHAT` — the
   third kind of line on the socket, beside a report and an ask. It moves no chat and asks for
   nothing. The window draws a banner on that tab with **Open as chat**, which opens the picker
   in the directory the shell was standing in with that harness's profile picked (ADR 0022: no
   harness starts until somebody picks a profile, so the banner starts nothing by itself), and
   **Dismiss**;
3. finds the real program on `PATH` with the shim directory left out, compared as spelled and
   as resolved, and replaces itself with it (`exec`), under the same argv.

Nothing reads the harness's output: which command the operator started is a fact of the same
kind as a hook's event word, because charter wrote the thing that fires it. Spec decision 3 and
ADR 0018 are untouched.

**It never stands in the way.** Steps 1 and 2 drop every failure — no app, a socket that has
gone, an app that is not reading (the write has a 250 ms deadline) — and step 3 runs anyway. A
shim whose `charter` has gone (the app moved since it wrote the shim) runs the real program
itself. A harness found nowhere but the shims exits 127, which is what a shell answers for a
command it cannot find.

**It cannot recurse.** The real program is started with the shim directory taken off its
`PATH`, so the harness, and every shell its model's tools start, finds the real `claude`. The
shim's own fallback runs at most once per process tree (`CHARTER_SHIM_PASSED`), so a `PATH` that
names the directory some third way cannot loop.

**The shims are written at every launch**, under the app's data directory (`<app data>/shims/`,
`0700`), each file written beside itself and renamed into place, so each runs the `charter` of
the app that is running. They go on a shell tab's `PATH` and on nothing else's: a chat on a
profile, or running a harness, never has them. They are never recorded — a relaunch works them
out again.

## Why the shell's own start files are involved

A `PATH` handed to a shell is only where it starts. An interactive shell then reads the
operator's `~/.zshrc` or `~/.bashrc`, and the usual line there is
`export PATH="$HOME/.local/bin:$PATH"` — which is where Claude Code and Codex install
themselves. **Measured on the operator's own machine, 2026-09-26:** `~/.zshrc` puts
`~/.local/bin` and `~/.opencode/bin` in front, so a shim directory that was only first in the
environment would have stood behind all three harnesses it exists to stand in front of, and the
feature would have done nothing for the one person who asked for it.

So the shims are put first again after the operator's own files have run, the way VS Code's
terminal shell integration does it:

- **zsh** is started with `ZDOTDIR` pointing at charter's own directory. Its `.zshenv`,
  `.zprofile` and `.zshrc` each source the operator's own from the `ZDOTDIR` they had (or
  `$HOME`), following a `.zshenv` that moves it; `.zshrc` then hands `ZDOTDIR` back — unset
  again where the operator had none — and puts the shims first.
- **bash** is started with `--rcfile` naming charter's file, which sources `~/.bashrc` and then
  puts the shims first.
- **Any other shell** gets the `PATH` alone, and a start file there that puts a harness's
  directory first wins. That is a known gap, said here rather than discovered.

Tests start a real interactive zsh and bash with a `HOME` whose start file prepends
`~/.local/bin`, and check that the shims are first and the operator's own directory is still
there (`charter_core::shellguard`).

## What was not done

- **No output is read, and no prompt hook is installed.** A `preexec` hook would see every
  command, which is more than the question needs and is shell-specific in a way `PATH` is not.
- **The harness is not stopped.** The operator typed it; charter says what that costs and
  offers the better place, and runs it.
- **Windows gets no shims.** The hook channel is a unix socket that does not exist there yet
  (`hookwire`'s `off_unix`), and the shims are POSIX scripts; `Shims::write` refuses with
  `Unsupported` and a shell tab there is a plain shell. `charter shell-guard` itself compiles
  and runs its program as a child there, so a `.cmd` shim is all a later port needs.

## Consequences

- `charter shell-guard` is a hidden command and one of `CORE_WORDS`, so no extension can take
  the word.
- `hookwire::Line` has a third, last variant. A report or an ask never reads as one (it requires
  `started_by_hand`, which neither carries), and a `charter` older than this never writes one.
- The Saving tab's "Open terminal here" is now a shell tab like any other, opened by the same
  function, named `shell <N>`.
- A new CONTEXT.md term: **Shell tab**.
