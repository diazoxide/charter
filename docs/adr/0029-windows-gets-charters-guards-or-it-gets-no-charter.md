# Windows gets charter's guards, or it gets no charter

**DRAFT — this needs the operator's sign-off before anything is built on it.** Two things
about it are not settled and should be settled by whoever accepts it. The number: charter's
ADRs live in the charter repo, `docs/adr/0001`–`0026`, and this is the first one written in
charter-app; `0027` and `0028` are referred to by name throughout `crates/charter-core` but
have no file there yet, so `0029` is the first number that certainly collides with nothing.
And the place: if ADRs stay in the charter repo, this file moves beside 0025 and 0026 and
loses nothing but its path.

ADR 0025 rebuilt charter as a desktop app on a Rust core and named three platforms: macOS,
Linux and Windows. macOS and Linux build, test and package. Nothing has ever been compiled on
Windows — charter-app#93 is the first time any of this code has run there — and the estimate
for the whole rebuild has been resting on that gap.

**The decision: Windows does not ship until charter's containment guards have a Windows
expression, and until then every guard that cannot be expressed there REFUSES rather than
degrades.** A platform charter declines to run on is a known quantity. A platform where
charter runs with its gate open is not, and the difference is not visible from inside the
app — which is exactly why it has to be decided here rather than discovered in a review.

This is not a decision to drop Windows. It is a decision about the order: the guards first,
then the port, and the guards are the part that needs a design rather than a translation.

## What is already in the tree, and why it is the problem

The core was written with Windows in mind. Before charter-app#93, eleven places carried an
explicit `#[cfg(not(unix))]` arm, and several more simply omit a `#[cfg(unix)]` block that
sets a mode. (That PR adds five more, in `hookwire`, and they are all refusals — which is what
this ADR is asking for everywhere else.)
**None of them has ever been compiled.** They were written to be right rather than left
broken, which was the correct instinct, and several of them are right. But a handful are not a
translation of the guard — they are the guard removed, with a comment where the guard used to
be:

- `contain::nofollow` is `options` unchanged. `open_no_link` and `create_no_link` exist to
  move the last component's link question from charter to the kernel, at the instant of the
  open; without the flag they are `no_link_on_the_way` alone, which is the version the module
  measured at 1881 escapes per 20,000 reads and 7600 per 20,000 writes. The three tests that
  prove the flag bites are `cfg(unix)`, so nothing on Windows goes red when it is gone.
- `plane::private_dir` and `plane::write_private` drop `0700` and `0600` — and `.charter/` is
  the directory the vault registry lives in.
- `profiletrust::write_private` drops `0600` from the record of the operator's consent to run
  a command.
- `usage::write_row` and `memstore::write_private` drop the same.

One is a guard that answers `true` to everything: `doctor::profiles::on_path` asks whether a
program is one this process could run, and off unix the answer is "it is a file". Every file
on `PATH` is then runnable, `PATHEXT` is not consulted, and the `program.contains('/')` that
decides whether a name is a path does not know about `\`.

Two more are not a removed guard but a coin toss that nobody has called:

- `glstate::alive` answers `false` off unix — every refresh lock reads as abandoned, so two
  refreshers can run at once.
- `news::alive` answers `true` off unix — every marker reads as live, so a stale one stops the
  check for ever.

Two modules for the same question with opposite defaults is not a platform decision; it is
two people guessing on different days.

## The guards, one at a time, and what Windows has instead

**Reparse points are not symlinks, and `is_symlink` does not say so.**
`contain::no_link_on_the_way` walks every component and refuses a `symlink_metadata` whose
file type is a symlink. On Windows the standard library answers that question for exactly two
reparse tags — `IO_REPARSE_TAG_SYMLINK` and `IO_REPARSE_TAG_MOUNT_POINT`. A path redirected
through any other tag reads as an ordinary file or directory: a Store app execution alias
(`IO_REPARSE_TAG_APPEXECLINK`), a OneDrive placeholder (`IO_REPARSE_TAG_CLOUD*`), a WSL
symlink (`IO_REPARSE_TAG_LX_SYMLINK`), a container's projected file. A plane inside a synced
folder is not an exotic case; it is where a lot of people keep their repositories. The walk
would pass, and `contain::resolved` — which asks `fs::read_link` the same question — would
resolve the path to itself and call it contained.

The Windows expression is a question about the reparse **tag**, not about `is_symlink`: refuse
any component carrying `FILE_ATTRIBUTE_REPARSE_POINT` at all. That is stricter than unix and
it is the right way round: charter's own paths are made by charter, so a reparse point
anywhere on the way has no honest use.

**`O_NOFOLLOW` has a near-equivalent that answers a different question.**
`FILE_FLAG_OPEN_REPARSE_POINT` does not refuse a reparse point; it opens the point itself
instead of following it. So the atomic refusal `open_no_link` gives on unix becomes "open,
then ask the handle whether it is a reparse point, then decide" — still atomic with respect to
a swap, because the second question is asked of the handle and not of the path, but a
different shape of code and a different error. The `O_NONBLOCK` half has no counterpart and
needs none: a Windows named pipe lives in `\\.\pipe\`, so the FIFO-at-an-arbitrary-path hazard
that flag exists for does not arise. `contain.rs` already says this arm "needs its own decision
at M4, not a guess now". This is that decision, and the answer is that it is a rewrite of the
pair rather than a flag swap.

**A mode bit is not an ACL.** `0600` and `0700` are how charter keeps the vault registry, the
trust record, the memory store and the usage trend off every other account on the machine.
Windows has no mode; it has a DACL, and the equivalent is creating the file or directory with
a security descriptor that grants the owner's SID alone. That is `CreateFileW` with
`SECURITY_ATTRIBUTES`, which is `unsafe` — and the workspace is `unsafe_code = "forbid"`. So it
is a crate (`windows-acl`, or `windows-sys` behind a small wrapper), a licence review, and a
test that proves the ACL bites, which means a second account on the runner. None of that is
hard. All of it is work that has not been costed, and until it is done the honest arm is a
refusal: charter cannot write private state on this platform.

**Windows resolves names charter believes are ordinary.** `contain::segment_ok` refuses a
separator, a NUL, `.`, `..`, an absolute path and a drive-qualified name — and it is used
*alone*, without the `^[A-Za-z0-9][A-Za-z0-9._-]*$` alphabet, on session ids, memory
identifiers, repo names on clone, worktree names and inventory entries. It does not refuse:

- a reserved device name — `con`, `nul`, `prn`, `aux`, `com1`–`com9`, `lpt1`–`lpt9`, with or
  without an extension. The alphabet does not catch these either: `persona_name_ok("nul")` is
  `true` today, and a session id of `nul` makes `.charter/sessions/nul.usage` the null device;
- a trailing dot — `alpha.` resolves to `alpha`, and the alphabet admits `.` deliberately, for
  names like `my-repo.v2`. Two workspaces the plane believes are distinct are one directory;
- an alternate data stream — `drive_qualified` catches `C:x` by looking at the *second*
  character, so `alpha:evil` passes `segment_ok` and names a stream of `alpha`. The alphabet
  does catch this one, so today it is reachable through the `segment_ok`-only callers and not
  through a workspace name;
- an 8.3 short name — `PROGRA~1` names a directory whose long name it does not begin with, so
  `contained`'s `starts_with` refuses a path that is in fact inside. Fail-closed, so a
  usability bug rather than a hole, but it should be known rather than discovered.

And the case rule runs the other way from the one `persona_name_ok` already documents:
personas are lowercase-only *because* a case-insensitive filesystem would let `DevOps` reach
`devops`'s files, but `workspace_name_ok` admits mixed case, so a committed plane holding both
`workspaces/Alpha` and `workspaces/alpha` cannot be checked out on Windows at all.

Every one of these is a string rule, which makes them the cheapest guards on this list and the
ones to write first — and each needs a test, because `segment_ok`'s existing tests run on a
platform where none of these strings mean anything.

**`ETXTBSY` does not exist, and its absence is not good news.** `crates/stand-in` is charter's
one answer to writing a program a test is about to run, and it is `#[cfg(unix)]` from top to
bottom: `/bin/sh` writes the bytes, `/bin/cp` copies the binary, `chmod 0755`, then a rename.
On Windows the crate compiles to nothing and every test that writes a stand-in fails to
compile. The failure it defends against is different too — a sharing violation, not `ETXTBSY`
— and the shape of the answer is different: there is no executable bit to set, the extension
decides, and a running image cannot be deleted though it can be renamed. This is a rewrite of
`stand-in`, not a `cfg` arm, and it gates most of the test suite.

**`env_clear()` means something else on Windows.** `worktree::git` clears the environment and
puts back only `HOME`, `PATH`, `GIT_TERMINAL_PROMPT` and `LC_ALL`. On Windows a process
started without `SystemRoot` cannot initialise winsock, so every git call that crosses a
network fails in a way that has nothing to do with git; `HOME` is not where git looks for a
global config (`USERPROFILE`, or `HOMEDRIVE`+`HOMEPATH`); the `PATH` it builds is joined with
`:` and needs `;`; and the four fixed directories it searches — `/usr/bin`, `/usr/local/bin`,
`/opt/homebrew/bin`, `/bin` — hold no `git.exe`, while the search looks for `git` and not
`git.exe` in any case. The guard's *reasoning* survives: an attacker-settable `PATH` must not
choose the binary. Its implementation does not.

**Before any of that: the repository had no `.gitattributes`.** Git for Windows turns
`core.autocrlf` on by default, and almost every test here is a byte comparison —
`tests/fixtures/planes/**` is regenerated and diffed byte for byte, the differential compares
every file under two plane copies, `fixtures/corpora/*.raw` are raw terminal recordings full of
escape sequences. A checkout that rewrote a line ending would make all of them measure the
checkout instead of the code, silently and on one platform only. charter-app#93 adds
`* -text`; it marks nothing in the tree as changed, because everything here is already LF, and
it is the precondition for trusting any Windows measurement at all.

**And when a chat does start, it never says anything.** `harness::hook_command` builds each
armed state hook as `shell_quoted(binary) + " hook <word>"`, POSIX single-quoting, and on
Windows a hook command runs through `cmd.exe`, where `'` quotes nothing: the program is
literally named `'C:\…\charter.exe'` and there is none. A session's state comes from hooks
only (ADR 0018), so a Windows charter's board would never move and would have no way to say
why. #103.

**And a chat has no program to run.** `app/src-tauri/src/sessions.rs:296` is
`std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_owned())`. On Windows `SHELL` is
unset and `/bin/sh` is not there, so every chat fails to start before any of the above is
reached. The Windows shape is `%ComSpec%`, or PowerShell — which is a product decision as much
as a technical one, and it changes what a profile's command line means.

**A unix socket with `0600` on it is the hook channel.** There is no expression at all: Rust's
standard library does not surface `AF_UNIX` on Windows, and a named pipe's access is an ACL
again. charter-app#93 changes the `compile_error!` that used to stand here into a refusal, so
the crate can be built and the rest of the platform measured; that refusal is the shipped
behaviour until a named pipe with a security descriptor exists.

## What was measured

charter-app#93 adds a non-gating `windows-latest` job to CI — deliberately not one of the nine
required checks, because a required check on a platform with no port blocks every merge in the
repo including the ports that would make it green. It reports the whole `cargo check` error
list rather than its first line, and it runs `tools/windows-probe`, which asks a real ConPTY
the three things `session.rs` takes from a unix pty.

> The numbers from the first run belong here. Until they are filled in, this section is the
> method and not the evidence.

## The work, as issues

Each of these is filed on charter-app, so this ADR decides the order rather than holding the
detail:

| issue | what it is | shape |
| --- | --- | --- |
| #95 | the hook channel: a named pipe, and an ACL where the `0600` was | design |
| #96 | `segment_ok` accepts four kinds of name Windows resolves elsewhere | string rules, cheap |
| #97 | every gate asks "is this a symlink", which misses most reparse tags | design |
| #98 | the `0600`/`0700` on charter's own state silently vanishes | design |
| #99 | ConPTY breaks the session lifecycle in three places | fix, sized |
| #100 | charter cannot find or run git, and two more lookups share the bugs | fix, sized |
| #101 | `crates/stand-in` is `cfg(unix)` end to end, so the tests cannot compile | rewrite |
| #102 | `glstate::alive` and `news::alive` disagree off unix | decide once |
| #103 | the armed state hooks are POSIX shell commands, so no chat ever reports | design |

## The options, and the one this takes

**A. Port mechanically and accept the degraded guards on Windows.** Rejected. The attacker
`contain.rs` exists for holds a *commit*, not a process — a committed
`workspaces/evil -> ../../elsewhere` travels to every machine that clones the plane. That
commit reaches a Windows clone too, and on that clone the gate is open. A guard that is a
comment on one of three platforms is a guard that is documented rather than enforced.

**B. Refuse on Windows until each guard has an expression.** Taken. The string rules are
cheap and can land immediately; the ACL, the reparse-tag walk and `stand-in` are each a small
piece of design with a test that has to be seen to fail. Until they land, a Windows charter
refuses to start rather than starting without them.

**C. Do Windows together with the descriptor-based containment rewrite.** This is the
recommendation for the *order*, not an alternative to B. ADR 0028 already puts an
`openat`-beneath-a-descriptor rewrite at M3 — "what is beneath this descriptor" rather than
"where does this name land" — and that is the same question Windows is asking. Handle-based
containment on Windows (`NtCreateFile` with `OBJ_DONT_REPARSE`, or `CreateFileW` with
`FILE_FLAG_OPEN_REPARSE_POINT` and a tag check) is the same shape as `openat` with
`O_NOFOLLOW`. Building a path-based Windows model now would mean building the model twice and
throwing the first away.

So: the string rules and the two coin-toss `alive` defaults are worth fixing now, because they
are cheap and they are wrong on every platform's terms. Everything else waits for M3 and lands
with the rewrite, and charter refuses on Windows in the meantime.

## What is left, in weeks rather than in adjectives

The estimate ADR 0025 rested on had no Windows number in it at all. This is one, with its
basis written next to it so it can be argued with. It is the work to reach a Windows charter
as trustworthy as the macOS and Linux ones — not a Windows charter that starts.

| | work | basis | estimate |
| --- | --- | --- | --- |
| **A** | the string rules (#96), one `alive` answer (#102), and the mechanical half of the program lookups (#100: `git.exe`, `join_paths`, `USERPROFILE`, the `SystemRoot` allowlist, `PATHEXT`) | each is a named function with a test that can go red on macOS today | **3–5 days** |
| **B** | the guards that need a design: ACLs for private state (#98), the reparse-tag walk (#97), the named-pipe channel (#95) | each is a crate choice, a `deny.toml` licence review, and a test that needs a second account on the runner | **3–5 weeks** |
| **C** | the session lifecycle (#99): end from a wait on the handle, a job object for the kill, `259` | needs either an upstream `portable-pty` change or charter assigning the job after `spawn_command` | **1 week**, and it cannot be trusted until the scenario tests run on Windows |
| **D** | test infrastructure (#101): `stand-in` rewritten, and the 44 files that reach for `std::os::unix` | 1177 tests; 21 `#!/bin/sh` stand-ins across 12 files; symlink tests need Developer Mode or admin on the runner; `mkfifo` has no counterpart | **2–3 weeks** |
| **E** | not attempted and not costed here: the Tauri bundle, WebView2 bootstrapping, signing, the tray, single-instance, and the scenario suite on Windows | nothing has been built, so any number would be invented | **unknown** |

**6–10 weeks** for A–D, of which roughly half is test infrastructure that buys no shipped
behaviour — plus E.

**Taking option C above moves the number.** The reparse-tag work in B is the same work as the
`openat`-beneath-a-descriptor rewrite ADR 0028 already puts at M3, so doing Windows after that
rewrite rather than before it takes B down to the ACLs and the pipe. A–D then lands nearer
**4–6 weeks**. That is the strongest argument for the order this ADR recommends, and it is an
argument about cost rather than about safety, which is why it comes second.

## What this costs, stated so it is not a surprise

Windows is not a differential platform and will not become one. `tests/differential/run.py`
plants symlinks in almost every scenario, writes `#!/bin/sh` stand-ins and `chmod 0755`s them,
and compares each file's mode; the oracle it compares against is the Python charter, which
imports `fcntl` and `termios` and drives tmux. Windows can be a build-and-unit-test platform
and nothing more, which means the guarantee spec decision 15 gives on macOS and Linux — that
the two implementations leave the same plane — has no Windows counterpart, and the Windows
port is covered by unit tests alone. That is a real reduction in what is known about the
platform and it belongs in the estimate.
