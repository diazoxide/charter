# Interface-agnostic cores: where other projects drew the seam

**Date:** 2026-09-05
**Question:** charter is CLI-first, serverless, and coordinates its panels through files plus a
nanosecond version stamp. It is already harness-agnostic. What would it take to be
*interface*-agnostic — a TUI, a web UI, an editor plugin and an API client all just consumers?
**Method:** primary sources only — specifications, official documentation, design documents in
source trees, and source code, read at pinned commits. Where a claim rests on something weaker,
or where no primary source was found, it says so in place.

Every claim carries a URL or a `file:line`. Quotes are verbatim.

---

## 1. The comparison

| Project | Where the boundary is | What forced it (stated) | What it cost | Who owns truth / how a second client learns | Notifier with no listener |
|---|---|---|---|---|---|
| **LSP** | Socket protocol: JSON-RPC, separate process | Language independence + CPU/memory isolation — *not* crash isolation | Deliberately shallow model; **one server serves one tool**; no flow control at all | Client owns document truth after `didOpen`; server pushes on the one connection | Receiver may discard: `$/` notifications "it is free to ignore" |
| **ACP** | Socket protocol: JSON-RPC over stdio; editor spawns agent as a subprocess | Editors × coding agents — the same M×N one layer up | No protocol object above the session; session welded to one `cwd` and one foreground state | Editor (client) owns filesystem, terminal and permissions | **Nothing. Spec and schema are silent; SDKs grow unboundedly — ~3 GB in 5 s, measured** |
| **MCP** | Socket protocol: JSON-RPC over stdio or Streamable HTTP | Tools and context *for a model*; "Host applications handle complex orchestration responsibilities" | Now explicitly stateless — sessions, `initialize` and server-initiated requests all removed in 2026-07-28 | Client is structurally always the initiator; state must be explicit handles | Close stdin → `SIGTERM` → `SIGKILL`, spelled out. **And the spec tells clients to poll** |
| **tmux** | Daemon + **private** socket protocol; control mode is the public text protocol | One server so sessions outlive the terminal | Protocol version 8 with hard reject; socket = full trust; server death loses everything | Server owns all state, pushes to each client | **Three layered answers: discard+repaint, queue, then pause-or-kill** |
| **git** | **Stated:** the plumbing *command* surface, plus a negotiate-or-abort version number. **In practice:** the `.git/` file format | Plumbing "sufficient to support development of alternative porcelains"; stability promised for *command interfaces*, never for files | "it is inherently impossible to lock the 'files' backend for writes"; the index lock never retries; formats specified only in C | The files are the truth; a second client `stat`s or re-reads | No notifier at all. `fsmonitor--daemon` added later "instead of the slower githooks interface"; **its clients spin, the daemon never blocks** |
| **jj** | **Library API** (`jj-lib`) over a lock-free file format | "It should be easy to create new UIs (CLIs, GUIs, TUIs, servers) without having to duplicate logic" | Rust-only, API explicitly unstable, blind to custom backends — **and they are now building an RPC API anyway** | Content-addressed op log; divergent heads **merged**, not rejected | No notifier; outsources watching to Watchman |
| **Docker** | Daemon + REST API; CLI is one client | Architecture described, not justified | Root daemon; compatibility is "best effort" across an ~8-year window | Daemon owns truth | Not established from a primary source |
| **Podman** | **Deliberately no daemon**, plus a socket-activated REST service | "No manager daemon, for improved security and lower resource utilization at idle" | Fixed SHM lock pool that can exhaust; per-user invisibility; library version skew on shared storage | Files + SQLite under POSIX semaphores | Socket activation — the listener starts the notifier |
| **Kubernetes** | API server + HTTP watch | Only the API server may touch etcd | Watch cache can go permanently stale and has downed control planes; restarts mass-invalidate watches | API server owns truth; list-then-watch from a `resourceVersion` | **Terminates the slow watcher, and counts it as a metric** |

The last column is where the research actually landed, and it splits cleanly. **Every mature system
that solved this refuses to let a slow or absent listener slow the writer** — they differ only in
what they do instead: drop the message (LSP), pause the stream and say so (tmux), evict the
subscriber and force a re-list (Kubernetes), or have no channel at all (git, jj, charter).

**But the two youngest protocols have not solved it.** ACP's specification says nothing about a
dead or stalled peer and its SDKs turn one into unbounded memory growth; MCP's Python server writer
uses zero-size buffers and **blocks indefinitely** on a client that has stopped reading — the exact
hazard charter refused, displaced one hop. Building the channel is the easy part. Building the
flow control is not, and §3 shows how much machinery the systems that got it right actually needed.

charter's version stamp sits in the "no channel at all" group, and it is the only member that needs
no recovery path, because it never had anything to lose.

---

## 2. The two direct answers

### 2.1 Is "the files are the API" respectable, or a stage projects grow out of?

**Respectable — but it is a position on where *truth* lives, not a claim that files are a
sufficient *interface*. Those get conflated, and the conflation is what projects grow out of.**

**For.** Git's on-disk repository is read and written directly by multiple independent
reimplementations and by dozens of UIs, none of which link git's code — though note that this is
*revealed* rather than *offered* stability, and point 3 below is the correction. Maildir is the
purest case, and there the claim is made outright: Bernstein's spec opens by answering "Why should
I use maildir?" with **"Two words: no locks."** — the format is designed so that "each message is
stored in a separate file with a
unique name, so it isn't affected by operations on other messages", a half-written message is
never visible because "each message is safely written to disk in the tmp subdirectory before it
is moved to new", and the result "is reliable even over NFS"
*(<https://cr.yp.to/proto/maildir.html>)*. Maildir has not grown out of anything in thirty years.

The strongest modern datapoint is **Jujutsu**, a greenfield VCS that could have chosen anything
and chose a file format *specifically to avoid* the alternative every other DVCS took:

> "most DVCSs treat local concurrency quite differently, typically by using lock files to prevent
> concurrent edits. Unlike those DVCSs, Jujutsu treats concurrent edits the same whether they're
> made locally or remotely."
> "To avoid depending on lock files, Jujutsu takes a different approach by accepting that
> concurrent changes can always happen. It instead exposes any conflicting changes to the user"
> — *(<https://docs.jj-vcs.dev/latest/technical/concurrency/>)*

**Against.** Three real limits, each with a source:

1. **A file format does not carry liveness.** git has no answer to "tell me when something
   changed"; every git UI polls. When repositories got large, git added `fsmonitor` — a daemon —
   purely to avoid `stat`ing every file. jj sidesteps this by *outsourcing* the daemon: its
   `fsmonitor.backend` option shells out to Watchman rather than becoming one
   *(<https://docs.jj-vcs.dev/latest/config/>)*.
2. **A file format does not carry authority.** The moment a browser or an untrusted caller is
   involved, "read the files" becomes "have filesystem access", with nowhere to put a permission
   check. Every project here that needed authorisation grew a socket — including the ones that
   refused a daemon (§2.2).
3. **The layout is usually less of a contract than it looks — and git, the flagship example, does
   not actually offer it as one.** `gitrepository-layout(5)` hedges: "These things **may** exist in
   a Git repository" *(git 2.50.1)* — an inventory, not a guarantee. And when git addresses this
   document's exact audience, it points them somewhere else entirely:

   > "**Although Git includes its own porcelain layer, its low-level commands are sufficient to
   > support development of alternative porcelains.** … The interface (input, output, set of options
   > and the semantics) to these low-level commands are **meant to be a lot more stable** than
   > Porcelain level commands, because these commands are primarily for scripted use."
   > — *(`man git`, LOW-LEVEL COMMANDS (PLUMBING), git 2.50.1)*

   **git's stated seam for third-party front ends is a command surface, not a file format.** The
   only explicit forward-compatibility promise it makes is likewise about output: `--porcelain`
   "will remain stable across Git versions and regardless of user configuration"
   *(`man git-status`)*. The one thing git *does* say to programs reading its files is a rule for
   safely refusing: an implementation that does not understand the repository's
   `core.repositoryformatversion` "**MUST NOT** operate on that repository", and format 0 is defined
   as everything git did originally, with "**Specifying the complete behavior of git is beyond the
   scope of this document**" *(<https://git-scm.com/docs/repository-version>)*.

   This is the single biggest correction the research produced: **the canonical "the files are the
   API" project does not make that claim about itself.** What is true is subtler and more
   interesting — the format became a de-facto contract by consequence rather than by promise, to the
   point that git can no longer freely change it. `BreakingChanges.adoc`, on the Git 3.0 default ref
   backend: "A prerequisite for this change is that the ecosystem is ready… **Most importantly,
   alternative implementations of Git like JGit, libgit2 and Gitoxide need to support it.**"

**The synthesis.** Files answer "what is true". They never answer "what changed, when, and may
you see it". The projects that aged best kept the file format as truth and added a *derived,
disposable* channel beside it — git kept `.git` and added `fsmonitor`; jj keeps its op log and
delegates watching to Watchman; systemd keeps unit files and adds a bus. The projects that
regretted things are the ones that made the daemon the truth (§2.2).

**And the derived channel does not retire the polling.** The single most deflating datapoint for
"polling is a phase" is that MCP — designed in 2025, revised in 2026, holding a live bidirectional
connection, having specified a notification mechanism — instructs its clients to poll anyway:
"**Best Effort**: There are no guarantees that every notification will be sent or received,
particularly across transport reconnects. **Clients should also rely on polling to preserve
freshness of results.**"
*(<https://modelcontextprotocol.io/docs/2026-07-28/learn/architecture.md>)*. A push channel is an
optimisation over a poll. It is not a replacement for one, and the newest protocol in this study
says so in its own architecture documentation.

**One correction to the framing of the question.** LSP is usually cited as the model for "one
backend, many frontends". On the specific point of *simultaneous* frontends the spec says the
opposite:

> "The protocol currently assumes that one server serves one tool. There is currently no support
> in the protocol to share one server between different tools. Such sharing would require
> additional protocol e.g. to lock a document to support concurrent editing."
> — *(<https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#languageServerProtocol>)*

The issue tracking multi-client support (#1160) has been open since 2020. In December 2020
dbaeumer wrote "There are currently no plans to actively work on this"; asked again in **May
2026** — explicitly in the context of AI harnesses each spawning their own server — the answer
was still "we discussed this internally and have some ideas however there is a lot more to it
then just multiple clients. But no concrete plans yet."
*(<https://github.com/microsoft/language-server-protocol/issues/1160#issuecomment-4438486651>)*.

LSP solved M×N in **implementations**, not in **concurrent clients**. If the goal is a TUI and a
web UI on the same charter plane at once, LSP is not the precedent. tmux and Kubernetes are.

### 2.2 What does a project lose by adding a daemon?

Six costs, each sourced, and charter currently pays none of them:

**1. Privilege becomes long-lived — the stated reason projects walk daemons back.** bpfman
shipped a daemon and removed it, in its own words:

> "The rationale behind running as a daemon was because something needs to be listening on the
> unix socket for API requests, and that we also maintain some state in-memory about the programs
> that have been loaded. However, since this daemon requires root privileges to load and unload
> eBPF programs it is a security risk for this to be a long-running - even with the mitigations we
> have in place to drop privileges and run as a non-root user. **This risk is equivalent to that of
> something like Docker.**"
> — *(<https://github.com/bpfman/bpfman/blob/main/docs/design/daemonless.md>)*

Docker's own security page agrees with the characterisation: "This daemon requires `root`
privileges unless you opt-in to Rootless mode… First of all, only trusted users should be
allowed to control your Docker daemon" *(<https://docs.docker.com/engine/security/>)*. Podman's
README states the counter-position as a design bullet: "No manager daemon, for improved security
and lower resource utilization at idle"
*(<https://github.com/containers/podman/blob/main/README.md>)*.

**2. The socket becomes a full-authority surface, and it is not a security boundary.** tmux is
unusually direct about this in its FAQ:

> "tmux relies on file system permissions to restrict access to its socket. By default these are
> intentionally restrictive and must be manually changed. **It does not provide any security
> boundary between users who can access the same socket.**
> **Any user with access to a tmux socket should be considered *fully trusted*** and it should be
> assumed that they can fully control the tmux server, even if they are restricted with the
> `server-access` command. This command and the read-only flag are **convenience features to
> prevent accidental changes by trusted users, not security mechanisms.**"
> — *(<https://github.com/tmux/tmux/wiki/FAQ>)*

Podman says the same about its optional service: "the API grants full access to all Podman
functionality, and thus allows arbitrary code execution as the user running the API, with no
ability to limit or audit this access"
*(<https://docs.podman.io/en/latest/markdown/podman-system-service.1.html>)*.

**3. Version skew becomes a runtime failure mode, and the recovery destroys state.** tmux carries
`PROTOCOL_VERSION 8` in the message header; a mismatch is a hard reject, not a negotiation
(`proc.c:132-147` `peer_check_version()`, `client.c:663-665` printing "protocol version
mismatch"). Its release note for 1.9 states the operational consequence:

> "**NOTE: This release has bumped the tmux protocol version. It is therefore advised that the
> prior tmux server is restarted when this version of tmux is installed, to avoid protocol
> mismatch errors for newer clients trying to talk to an older running tmux server.**"
> — *(tmux `CHANGES`, 1.8 → 1.9)*

"Restart the prior server" means destroy every running session. Bazel takes the same approach
automatically: "the client first checks that the server is the appropriate version; if not, the
server is stopped and a new one started" *(<https://bazel.build/run/client-server>)*. Docker
instead pays for negotiation, maintaining API compatibility back to 1.40 (and 1.24 until
recently) — roughly an eight-year window — and still says "compatibility is 'best effort'"
*(<https://docs.docker.com/reference/api/engine/>)*.

**charter has already been bitten by this exact shape** through Playwright's daemon: "Two
commands that resolve different versions talk to different daemons, and the second reports `The
browser 'owner' is not open, please run open first` while the first browser is alive and still
logged in" *(`docs/browser.md:82-85`)*.

**4. Concurrency gets serialised at the daemon whether or not the work conflicts.** Bazel: "**Each
server can handle at most one invocation at a time; further concurrent invocations will either
block or fail-fast**" *(<https://bazel.build/run/client-server>)*. This is the same complaint jj
makes about coarse locks — "operations that wouldn't actually conflict would still have to wait
for each other" *(<https://docs.jj-vcs.dev/latest/technical/concurrency/>)* — reached from the
opposite direction.

**5. You acquire a cache-coherence problem you did not have.** Once a daemon holds state derived
from files, they can disagree, and something must reconcile them. systemd made that reconciliation
a user-visible command: `daemon-reload` "will rerun all generators, reload all unit files, and
recreate the entire dependency tree", and `systemctl` warns when a unit was "updated on disk and
the daemon-reload command was not issued since"
*(<https://www.freedesktop.org/software/systemd/man/latest/systemctl.html>)*. Mercurial's command
server carries the same tax as a documented defect: "Configuration changes don't reload; aliases
become permanent" *(<https://wiki.mercurial-scm.org/CommandServer>)*.

**6. The cache can become the outage.** Kubernetes' watch cache — added because reading through to
etcd did not scale — is now itself a failure mode. KEP-4568:

> "there are several issues when watchcache is either not yet initialized on kube-apiserver startup
> or requires re-initialization due to not keeping up with load later on. **Both of these cases may
> lead to significant overload of the whole control plane (including etcd), in worst case even
> bringing it down.**"
> — *(<https://github.com/kubernetes/enhancements/blob/master/keps/sig-api-machinery/4568-resilient-watchcache-initialization/README.md>)*

The mitigation is now documented user-visible behaviour: watch and unpaginated list requests
during cache initialisation "are rejected immediately with an HTTP `429 Too Many Requests` status
code" *(<https://kubernetes.io/docs/reference/using-api/api-concepts/#watch-cache-initialization>)*.
And KEP-2340 notes the cache can go "permanently stale" if the watch stream clogs.

**What is genuinely bought.** Gradle: "The Daemon can reduce build times by 15-75% when you build
the same project repeatedly" *(<https://docs.gradle.org/current/userguide/gradle_daemon.html>)*.
Bazel's server exists to allow "caching of BUILD files, dependency graphs, and other metadata from
one build to the next". Mercurial's command server eliminates "significant performance overhead for
launching Mercurial repeatedly". These are real numbers for real problems — all of them
*startup-cost* problems, which is not charter's problem.

**The compromise three projects reached independently: a daemon that exists only while someone is
talking to it.** bpfman's replacement "will shutdown after a timeout of inactivity - provided by a
`--timeout` flag defaulting to 5 seconds" and "will support being run as a systemd service, via
socket activation, which will allow it to be started on demand when a request is made to the unix
socket". Podman's `system service` does exactly the same, with the same 5-second default:

> "If the systemd service is not already running, it will be activated as soon as a client connects
> to the listening socket… After some time of inactivity, as defined by the `--time` option, the
> command terminates… **No unnecessary compute resources are wasted.**"
> — *(<https://docs.podman.io/en/latest/markdown/podman-system-service.1.html>)*

Note what Podman's README does *not* do: it does not treat "no daemon" and "has an API" as
opposites. The two are adjacent bullets in the same list. **Refusing a daemon is not refusing a
protocol.**

Socket activation is also the direct structural answer to charter's FIFO objection: it inverts
the dependency so the *listener* brings the notifier into existence, which is why the writer can
never block on a missing reader.

---

## 3. The cross-cutting finding: how everyone handles a notifier with no listener

charter rejected a FIFO for a stated reason, and the reason is correct at the specification level.
POSIX:

> "If O_NONBLOCK is clear, an `open()` for reading-only shall block the calling thread until a
> thread opens the file for writing. An `open()` for writing-only shall block the calling thread
> until a thread opens the file for reading."
> — *(<https://pubs.opengroup.org/onlinepubs/9699919799/functions/open.html>)*

The non-blocking escape hatch does not rescue the case, it renames it: with `O_NONBLOCK`, an open
for writing "shall return an error if no process currently has the file open for reading" —
`ENXIO`. So a FIFO offers a writer *hang* or *fail*, and charter's hook path can afford neither
*(`charter/frame/notify.py:59-62`)*.

The interesting result is what everyone else does, because **none of them eliminated the hazard;
the mature ones made it survivable and the young ones have not yet.** Ten systems, and the first
eight converge on one shape:

| System | What happens when the listener can't keep up | Source |
|---|---|---|
| **Kubernetes** | Terminates the watcher, and counts it | `// This means that we couldn't send event to that watcher. Since we don't want to block on it infinitely, we simply terminate it.` — plus `metrics.TerminatedWatchersCounter` and the log line "Forcing %v watcher close due to unresponsiveness". *(`staging/src/k8s.io/apiserver/pkg/storage/cacher/cache_watcher.go:158-230`)* |
| **tmux (control)** | Pauses the pane and says so — or kills the client after 300s | `pause-after` → `%pause pane-id`; otherwise `c->exit_message = xstrdup("too far behind")`. *(`control.c:532-565` `control_check_age()`)* |
| **tmux (tty)** | Discards output and repaints instead | `log_debug("%s: can't keep up, %zu discarded", ...); evbuffer_drain(tty->out, size);` then a 100ms timer forces a full redraw. *(`tty.c:218-245`)* |
| **Linux inotify** | Drops events, delivers `IN_Q_OVERFLOW` | "Events in excess of this limit are dropped, but an `IN_Q_OVERFLOW` event is always generated." … "events are lost. Robust applications should handle the possibility of lost events gracefully. For example, it may be necessary to rebuild part or all of the application cache." *(<https://man7.org/linux/man-pages/man7/inotify.7.html>)* |
| **macOS FSEvents** | Coalesces or drops, tells you to rescan | "you must do a full scan of any directories that you are monitoring **because there is no way to determine what may have changed**." *(<https://developer.apple.com/library/archive/documentation/Darwin/Conceptual/FSEvents_ProgGuide/UsingtheFSEventsFramework/UsingtheFSEventsFramework.html>)* |
| **Watchman** | Declares the stream invalid, returns everything as new | `is_fresh_instance` "is true if the particular clock value indicates that it was returned by a different instance of watchman…only files that currently exist will be returned, and all files will have `new` set to `true`." *(<https://facebook.github.io/watchman/docs/cmd/query.html>)* |
| **git (simple IPC)** | Makes "nobody is listening" a normal enumerated state, and puts the waiting on the *client* | `IPC_STATE__NOT_LISTENING` — "Perhaps it is very busy. Perhaps the daemon died without deleting the path… Perhaps it is dead, but other clients are lingering". Clients opt into `wait_if_busy` / `wait_if_not_found`, both timeout-bounded. *(`simple-ipc.h:9-60`)* |
| **LSP** | Permits the receiver to discard | "If a server or client receives notifications starting with '$/' it is free to ignore the notification." Also: before `initialize`, "Notifications should be dropped, except for the exit notification." **The spec specifies no flow control at all** — zero occurrences of backpressure, flow control, or throttling in the 3.17/3.18 trees. |
| **MCP** | Declares delivery best-effort and tells clients to poll | "**Best Effort**: There are no guarantees that every notification will be sent or received, particularly across transport reconnects. **Clients should also rely on polling to preserve freshness of results.**" *(<https://modelcontextprotocol.io/docs/2026-07-28/learn/architecture.md>)* |
| **ACP** | **Nothing — and the SDKs make it worse than nothing** | Spec, schema and error-code enum are silent on a dead or stalled peer. See below. |

Six things follow.

**First, every system that handles this at all degrades to "resynchronise from the truth".** inotify
says rebuild your cache. FSEvents says rescan. Watchman says treat every file as new. Kubernetes
says re-list. MCP says poll. tmux's own wiki says the same of a resumed pane: "**It is up to the
client to update the content of the pane if necessary, for example using `capture-pane`**"
*(<https://github.com/tmux/tmux/wiki/Control-Mode>)*. This is not coincidence: a bounded
notification channel cannot be the source of truth, so every design with one keeps a way to
reconstruct state without it. **The odd one out is ACP, which has no recovery path because it has
not acknowledged the failure** — which is a statement about ACP's age, not a counterexample.

**Second, Kubernetes shows how much machinery a well-behaved push channel actually needs.** Its
dispatcher makes a non-blocking attempt to every watcher first, "which make faster watchers not be
blocked by slower ones", then spends a **shared 100ms budget refilled at 50ms/s**
(`cacher/time_budget.go:26-29`) on the stragglers, with per-watcher buffers sized to hold "roughly
one second of history" (10–1000 events, `watch_cache_history.go:177-211`). Even with all of that,
the terminal case is still eviction. That is the true cost of push, and it is not a small
component.

**Third, tmux is the sharpest single answer to charter's literal question — "what if nobody is
listening?"** — because it addresses it directly and answers the opposite of what you might expect
(`server-client.c:2090-2101`):

```c
/*
 * If there is data remaining, and there are no clients able to consume
 * it, do not read any more. This is true when there are attached
 * clients, all of which are control clients which are not able to
 * accept any more data.
 */
```

with, twenty lines above, `if (attached_clients == 0) off = 0;`. **Zero listeners means keep
reading.** tmux applies backpressure only when there are listeners and *all* of them are behind.
No listeners is never a reason to stop the writer — which is precisely charter's rule, stated in
C.

**Fourth, tmux also demonstrates that a real push channel still coalesces onto a clock.** Its
format subscriptions report "changes to the format… with the `%subscription-changed` notification,
**at most once a second**" *(`man tmux`, `refresh-client -B`)*, with the *server* sampling on the
client's behalf. A push system with a live connection and full backpressure machinery still
concluded that state-change notification should be a fixed-rate sampled diff. That is functionally
charter's version stamp, arrived at from the other direction.

**Fifth, the newest protocol in this study — the one closest to charter's own domain — has no answer
at all, and its SDKs turn the hazard into an OOM.** ACP is JSON-RPC over stdio with an agent
subprocess streaming `session/update` notifications back to an editor. Its specification, its 246 KB
JSON schema, and its complete `ErrorCode` enum contain **no transport or peer-gone error code**, and
the single occurrence of "backpressure" anywhere in its docs is about a draft HTTP transport, not
stdio. In the SDKs this is not a gap but an inversion:

- **Rust:** `send_notification` is not async and its `Result` cannot report a write failure — three
  unbounded `mpsc` channels in series feed one awaiting sink, so `Ok(())` means "enqueued in memory",
  nothing more. `grep -riE 'sigpipe|epipe|broken.?pipe'` over the protocol write path returns zero
  hits.
- **TypeScript:** the natural fire-and-forget `sessionUpdate()` idiom produces an unhandled rejection
  on `EPIPE`, which under Node's default since v15 **kills the process**. Measured against the
  published `@agentclientprotocol/sdk@1.4.0`.
- **Alive-but-not-reading is handled by neither.** No `desiredSize` check, no drain wait, no bounded
  queue in either stdio write path. Measured against a real pipe with a parent that never reads:
  `sent=4730000 rssMB=2992.3` — **roughly 3 GB of buffered notifications in five seconds**, with
  nothing blocked, rejected or reported.

*(Measurements performed during this research against the published SDKs, not quoted from a
document — see §7.)* The point is not that ACP is badly built. It is that the naive version of the
channel charter declined to build has, in the most actively developed agent protocol of 2026, the
exact failure mode charter predicted — displaced from "the writer hangs" to "the writer's memory
grows without bound", which is the same bug wearing different clothes.

**Sixth — and this is the strongest retrospective argument for charter's design — the most
sophisticated critique of LSP names charter's mechanism as the thing LSP should have been.**
matklad, rust-analyzer's original author:

> "And this touches what I think is the biggest architectural issue with LSP. LSP is an RPC
> protocol — it is formed by 'edge triggered' requests that make something happen on the other
> side. But this is not how most of IDE features work. **What actually is needed is 'level
> triggered' *state synchronization*.** The client and the server need to agree what something
> *is*, deciding the course of action is secondary."
> — *(<https://matklad.github.io/2023/10/12/lsp-could-have-been-better.html>)*

*(Flag: this is an implementer's critique, not a Microsoft position.)* A version stamp polled at a
fixed rate is a level-triggered state synchroniser. charter has the thing matklad says LSP lacks,
and it has it because it refused the edge-triggered channel.

**The honest cost of charter's position.** Polling has a latency floor and a per-client wakeup cost
that none of these sources excuses. At `TICK = 0.2` *(`charter/frame/panel.py:147`)* charter's
worst-case notification latency is 200ms. The measured idle cost is small — "about 5µs per tick
added to the ~26µs a panel already spends checking the version file, five times a second, or
roughly 0.003% of one core while idle" *(`docs/frame.md:412`)* — and the design keeps a 424µs
gather "behind a 4.6µs `stat`" *(`charter/frame/panel.py:648`)*. But poll cost multiplies by
**client count**, while push cost multiplies by **event count**. That asymmetry is the one honest
argument for adding a channel later, and it only bites at a client count charter does not have.

**One thing charter already has that most of these lack: cache coherence by construction.** The
ordering rule in `charter/frame/notify.py:18-22` — refresh the cache *before* bumping the version,
so a reader that sees the new version can never read the old cache — is exactly the invariant
systemd pushes onto the user as `daemon-reload`. It is also what makes a second interface cheap:
any consumer that polls `version` and reads `gather.json` gets a coherent snapshot with no
protocol at all.

---

## 4. Per-project detail

### 4.1 Language Server Protocol

**Where the M×N framing actually lives.** Not on `microsoft.github.io` and not in the 2016
announcement — a grep of the whole LSP site repo for `M*N`, `MxN`, `quadratic` returns nothing.
Microsoft states it in the VS Code extension docs:

> "Finally, integrating multiple language toolings with multiple code editors could involve
> significant effort. From language toolings' perspective, they need to adapt to code editors with
> different APIs. From code editors' perspective, they cannot expect any uniform API from language
> toolings. This makes implementing language support for `M` languages in `N` code editors the work
> of `M * N`."
> — *(<https://code.visualstudio.com/api/language-extensions/language-server-extension-guide>)*

**Why a separate process — the two stated reasons, and the one that is not there.** Same page:

> "As briefly stated above there are two benefits of running the Language Server in a separate
> process:
> - The analysis tool can be implemented in any languages, as long as it can communicate with the
>   Language Client following the Language Server Protocol.
> - As language analysis tools are often heavy on CPU and Memory usage, running them in separate
>   process avoids performance cost."

So: **language independence and resource isolation**. Crash isolation is *not* given as a rationale
anywhere found; the spec treats server crashes as an operational condition to be handled ("if a
client notices that a server exits unexpectedly, it should try to restart the server. However
clients should be careful not to restart a crashing server endlessly").

**Why not a shared semantic model or file format — the key paragraph, on Microsoft's own overview:**

> "These data types are programming language neutral and apply to all programming languages. The
> data types are not at the level of a programming language domain model which would usually provide
> abstract syntax trees and compiler symbols (for example, resolved types, namespaces, ...). The
> fact that the data types are simple and programming language neutral simplifies the protocol
> significantly. **It is much simpler to standardize a text document URI or a cursor position
> compared with standardizing an abstract syntax tree and compiler symbols across different
> programming languages.**"
> — *(<https://microsoft.github.io/language-server-protocol/overviews/lsp/overview/>)*

**Why JSON-RPC — the spec asserts it, the reasoning is in issue comments.** dbaeumer on rejecting a
binary format: "Agree but this adds additional bars to the implementor of a language server.
Currently a server can be implemented in any language and most of them do have libraries for JSON.
If we go with binary encoding we need to ensure that libs are available for the major programming
languages." *(<https://github.com/microsoft/language-server-protocol/issues/211#issuecomment-293274190>)*.
And stdio is a *recommendation*, not a requirement — the spec lists `stdio`, `pipe`, `socket` and
`node-ipc` as suggested command-line channels.

**The clearest non-goal statement, from the maintainer:**

> "The design goal of the LSP was to provide language 'smartness' that can be shared between tools
> **not to implement UI that can be shared between tools**. I think there is some space for shared
> descriptive UI as well but I would argue that this should be a different protocol than LSP."
> — dbaeumer, closing a request for a tree-view API
> *(<https://github.com/microsoft/language-server-protocol/issues/253#issuecomment-308675336>)*

He has refused features on scope grounds repeatedly: client-side language configuration (#554,
"I will close this at out of scope"), enumerating other machines' servers (#1359, "This is nothing
that should come through LSP… I disagree since it is not in scope of LSP"), test execution (#313).

**Who owns the truth — explicitly the client, and the server is forbidden from reading disk:**

> "The document open notification is sent from the client to the server to signal newly opened text
> documents. **The document's content is now managed by the client and the server must not try to
> read the document's content using the document's Uri.**"
> — *(spec, `#textDocument_didOpen`)*

The spec calls open/close "ownership transfer notifications" in several places. The overview page
puts it plainly: "From now on, the truth about the contents of the document is no longer on the
file system but kept by the tool in memory."

**And the client, not the server, is told to do the file watching** — with four stated reasons that
read like a design memo charter could have written:

> "Servers are allowed to run their own file system watching mechanism and not rely on clients to
> provide file system events. However this is not recommended due to the following reasons:
> - to our experience getting file system watching on disk right is challenging, especially if it
>   needs to be supported across multiple OSes.
> - file system watching is not for free especially if the implementation uses some sort of polling
>   and keeps a file system tree in memory to compare time stamps…
> - **a client usually starts more than one server. If every server runs its own file system watching
>   it can become a CPU or memory problem.**
> - in general there are more server than client implementations. So this problem is better solved on
>   the client side."
> — *(spec, `#workspace_didChangeWatchedFiles`)*

**Versioning: capabilities, chosen deliberately over a version number.** dbaeumer closing "Declare
protocol version": "I choose capability flags instead of protocol version since it makes things
more explicit from a client side."
*(<https://github.com/microsoft/language-server-protocol/issues/116#issuecomment-271297232>)*. The
forward-compatibility rule is "Servers receiving a `ClientCapabilities` object literal with unknown
properties should ignore these properties", and symmetrically "Clients should ignore server
capabilities they don't understand".

**Costs, from the spec's own mouth.** LSP documents one of its own design mistakes: push
diagnostics could not be prioritised for the file the user was actually looking at, because
"Inferring the client's UI state from the `textDocument/didOpen` and `textDocument/didChange`
notifications might lead to false positives since these notifications are ownership transfer
notifications." The fix — pull diagnostics — shipped in 3.17, six years in. UTF-16 position
encoding took **4.5 years** from report (#376, 2017-11) to a negotiable fix (3.17, 2022-05), during
which servers shipped a de-facto extension (clangd's `offsetEncoding`) to route around the spec.

**The implementer position worth copying.** rust-analyzer treats LSP as an adapter, not as its
architecture:

> "**Architecture Invariant:** `ide` crate strives to provide a _perfect_ API. Although at the moment
> it has only one consumer, the LSP server, **LSP *does not* influence its API design.** Instead, we
> keep in mind a hypothetical _ideal_ client…"

> "**Architecture Invariant:** `rust-analyzer` is the only crate that knows about LSP and JSON
> serialization. If you want to expose a data structure `X` from ide to LSP, don't make it
> serializable. Instead, create a serializable counterpart in `rust-analyzer` crate and manually
> convert between the two."

> "If you want to use IDE parts of rust-analyzer via LSP, custom flatbuffers-based protocol or just
> as a library in your text editor, this is the right API."

> "Shout outs to LSP developers for popularizing the idea that '**UI**' is a good place to draw a
> boundary at."
> — *(<https://github.com/rust-lang/rust-analyzer/blob/master/docs/book/src/contributing/architecture.md>)*

That last line is the most transferable sentence in this entire document. The thing LSP got right
was not the protocol; it was choosing **presentation** as the boundary. matklad expands on it:
"it just doesn't provide a semantic model of the code base. Instead, it is focused squarely on the
presentation. No matter how different each programming language is, they all, in the end, use the
same completion widget."

### 4.2 Agent Client Protocol (ACP)

**What it is:** "The Agent Client Protocol (ACP) standardizes communication between code editors/IDEs
and coding agents and is suitable for both local and remote scenarios."
*(<https://agentclientprotocol.com/get-started/introduction.md>)*. The comparison it draws for itself
is the obvious one: "ACP solves this by providing a standardized protocol for agent-editor
communication, **similar to how the Language Server Protocol (LSP) standardized language server
integration**."

**Governance — not Zed alone.** The repo has moved from `zed-industries` to a dedicated
`agentclientprotocol` org (the old URL 301-redirects), and:

> "ACP is jointly governed by Zed and JetBrains, who collaborate to ensure the protocol serves the
> broader ecosystem. We aim to operate transparently and make decisions in the best interests of ACP
> and its community, **while working toward transitioning to an independent foundation.**"
> "ACP has two lead maintainers: Ben Brandt (Zed Industries) and Sergey Ignatov (JetBrains). Lead
> Maintainers can veto any decision by core maintainers or maintainers."
> — *(<https://agentclientprotocol.com/community/governance.md>)*

Apache-2.0, no CLA. Stable wire version `1`, with a **v2 draft** published 2026-07-20.

**Role inversion, confirmed.** The *editor* is the Client and it spawns the coding agent as a child:
"Agents are programs that use generative AI to autonomously modify code. **They typically run as
subprocesses of the Client.**" … "Clients provide the interface between users and agents. They are
typically code editors… Clients manage the environment, handle user interactions, and control access
to resources." *(<https://agentclientprotocol.com/protocol/v1/overview.md>)*. In v1 this means
`fs/read_text_file` is a call the *agent* makes *to the editor*.

**The actual method set, from the schema** (`schema/v1/meta.json`), not from prose — 13 agent
methods, 11 client methods:

| Agent methods (client→agent) | Client methods (agent→client) |
|---|---|
| `initialize`, `authenticate`, `session/new`, `session/load`, `session/set_mode`, `session/set_config_option`, `session/prompt`, `session/cancel`, `session/list`, `session/delete`, `session/resume`, `session/close`, `logout` | `session/request_permission`, `session/update`, `fs/write_text_file`, `fs/read_text_file`, `terminal/create`, `terminal/output`, `terminal/release`, `terminal/wait_for_exit`, `terminal/kill`, `elicitation/create`, `elicitation/complete` |

**The v2 draft removes the entire client-side execution surface** — all of `fs/*` and `terminal/*` —
for a reason worth quoting, because it is a seam decision in miniature:

> "In practice this surface was inconsistently implemented outside of a few IDEs, and agents already
> needed their own file and execution handling for clients that didn't offer it. Clients that want to
> expose file access, unsaved editor state, or command execution to agents should do so by providing
> an **MCP server** to the session… which puts those tools on the same footing as every other tool
> the Agent uses."
> — *(<https://agentclientprotocol.com/protocol/v2/migration.md>)*

**Transport:** JSON-RPC, newline-delimited, over stdio. "Agents and clients **SHOULD** support stdio
whenever possible" and "The agent **MUST NOT** write anything to its `stdout` that is not a valid ACP
message." Streamable HTTP is still a draft proposal.

**Non-goals: NO PRIMARY SOURCE FOUND.** There is no non-goals or out-of-scope section anywhere in
ACP's docs, README or contributing guide. The nearest thing is a design-philosophy bullet that is a
genuine scope boundary:

> "**Trusted**: ACP works when you're using a code editor to talk to a model you trust. You still have
> controls over the agent's tool calls, but the code editor gives the agent access to local files and
> MCP servers."
> — *(<https://agentclientprotocol.com/get-started/architecture.md>)*

**ACP is not a containment protocol.** It assumes a trusted agent.

**Adoption is real and broad.** `opencode acp` is confirmed in source
(`packages/opencode/src/cli/cmd/acp.ts`, importing `@agentclientprotocol/sdk`), added October 2025.
Gemini CLI has native `--acp` (`--experimental-acp` now deprecated). But two corrections to
assumptions worth stating plainly:

- **Claude Code does not speak ACP natively.** No `acp` command, no `--acp` flag, zero matches for
  "acp"/"zed" in its documentation index, zero code search hits in `anthropics/claude-code`. The
  adapter is **Zed-authored** (`agentclientprotocol/claude-agent-acp`, npm author "Zed Industries")
  and wraps the Claude *Agent SDK*, not the CLI.
- **Codex does not speak ACP natively either.** It ships its own: `codex app-server`, described in
  its README as "**Similar to** MCP, `codex app-server` supports bidirectional communication using
  JSON-RPC 2.0 messages". The ACP bridge is external.

**Is ACP a plausible seam for charter?** The evidence turned out sharper than expected, in both
directions.

*More of charter's vocabulary maps than I assumed.* Todos are first-class — `PlanEntry` with
`content`, `priority` and `status`, and a replace-whole-list contract: "The Agent **MUST** send a
complete list of all plan entries in each update and their current status. The Client **MUST**
replace the current plan completely" *(<https://agentclientprotocol.com/protocol/v1/agent-plan.md>)*.
Personas map onto session modes (`ask`, `architect`, `code`). Multiple repos map onto
`additionalDirectories`. Workspaces map, loosely, onto `session/list` filtered by `cwd`.

*And people are already shipping charter-shaped products on it.* The official clients page lists,
verbatim: "**Braide** — Parallel sessions, worktrees, personas and interactive agent responses";
"**Jockey** — open-source multi-agent orchestrator … that coordinates Claude Code, Gemini CLI, and
Codex CLI via ACP"; "**CompozyOS** — agent OS: runs ACP agents as a team on loops and schedules, with
shared memory and approvals"; plus Rust/ratatui TUI clients
*(<https://agentclientprotocol.com/get-started/clients.md>)*.

*But the ceiling is structural.* The unit of everything is `(one session, one cwd, one conversation,
one foreground state)`. "The `cwd`… **MUST** remain the base for relative-path resolution", and v2's
state machine reports exactly one foreground stream per session (`running`/`idle`/`requires_action`).
**There is no protocol object above the session** — no workspace, no project, no fleet. The two RFDs
that would add one — proxy chains with a central "conductor", and `session/fork` — have sat in
**Draft since December 2025 and November 2025** respectively.

*The direction of travel is toward charter, though.* The v2 announcement:

> "When we launched ACP v1, most agents happily emitted events only after a user-initiated message and
> usually stopped once they finished generating. But these days, **agents are able to work longer and
> even orchestrate more and more work in the background.** … If we want to allow for queueing,
> steering, or receiving updates from work that isn't necessarily initiated by the user, we need to
> make it clearer that the prompt request/response doesn't own the entire lifecycle of work being
> done." … "which also makes it easier for both replay and **the potential for multiple clients
> observing the same session**."
> — *(<https://agentclientprotocol.com/announcements/acp-v2-draft.md>)*

**Verdict:** ACP is a proven seam for *driving individual agents* — 100+ shipped clients say so — and
it is the natural adapter if charter wants editor integration. It is **not** a seam for the
orchestration layer itself, and the parts that would make it one are unmoved after eight months. The
productive framing is the inverse of the obvious one: **charter is closer to being an ACP client than
an ACP agent** — the thing that owns the workspace and hands work to agents that speak it — which is
exactly the relationship `charter/harness/registry.py` already abstracts. The orchestrators on ACP's
own clients list are doing precisely that.

### 4.3 Model Context Protocol (MCP)

**What it standardises,** and the scope note that answers the operator's question directly:

> "MCP focuses solely on the protocol for context exchange—**it does not dictate how AI applications
> use LLMs or manage the provided context.**"
> — *(<https://modelcontextprotocol.io/docs/2026-07-28/learn/architecture.md>)*

Server primitives: **Tools**, **Resources**, **Prompts**. Client primitive: **Elicitation**. Sampling,
Roots and Logging are all **deprecated** as of protocol version `2026-07-28`.

**The design principles are the real boundary statement.** Verbatim from the spec's architecture page
*(<https://modelcontextprotocol.io/specification/2026-07-28/architecture/index.md>, identical in
2025-06-18)*:

> "1. **Servers should be extremely easy to build**
>    * **Host applications handle complex orchestration responsibilities**
>    * Servers focus on specific, well-defined capabilities
> …
> 3. **Servers should not be able to read the whole conversation, nor "see into" other servers**
>    * Servers receive only necessary contextual information
>    * **Full conversation history stays with the host**
>    * Each server maintains isolation
>    * Cross-server interactions are controlled by the host
>    * Host process enforces security boundaries"

Principle 1 is decisive: **orchestration is explicitly the host's job, not the server's.**

**But there is no "what MCP is not" section in the spec, and no maintainer has ever written "MCP is
not an agent protocol."** The only written out-of-scope statement in the whole project is in the
Agents Working Group charter:

> "**Building or standardizing general-purpose agent frameworks and runtimes, including internal
> choices about planning, memory, model selection, and orchestration.** The group standardizes
> behavior at MCP interoperability boundaries rather than host or server implementation internals."
> — *(<https://modelcontextprotocol.io/community/working-groups/agents>)*

**Is the boundary real, or are people using MCP as a control plane anyway? Both — and the split is
clean along a first-party/third-party line.**

*Held, in the official surface.* `modelcontextprotocol/servers` `src/` contains exactly seven
directories today — `everything`, `fetch`, `filesystem`, `git`, `memory`, `sequentialthinking`,
`time`. **None controls an agent.** Thirteen further servers were pruned to an archive repo; all were
data/tool servers.

*Broken, in the registry.* Querying the public registry
(`https://registry.modelcontextprotocol.io/v0/servers?search=…`) returns **177 unique servers**
matching orchestration terms — `agent` 100+, `orchestrat` 38, `swarm` 24, `delegate` 6, `spawn` 5.
Verbatim descriptions include "Let Codex orchestrate external coding agents through their native
harnesses", "Multi-tool AI orchestration: Claude Code, Codex, Gemini on shared repos", and
"Decentralized task broker and message relayer for multi-agent swarms". The registry has no curation
gate; it is an app store, not a scope statement.

*And the first parties do not do it.* When OpenAI and Google needed real control planes they wrote
`codex app-server` and adopted ACP respectively — neither reached for MCP. Claude Code's `claude mcp
serve` is the nearest case, and it is explicitly a toolbox, not a loop: "This MCP server only exposes
Claude Code's tools to your MCP client, so **your own client is responsible for implementing user
confirmation for individual tool calls**" *(<https://docs.claude.com/en/docs/claude-code/mcp>)*.

**Crucially, MCP is actively withdrawing from the control-plane direction, not drifting into it.**
Four changes in `2026-07-28`, all pointing the same way:

- **Sampling deprecated** — the one primitive that let a server drive. Its own page still says
  "Sampling in MCP allows servers to implement **agentic behaviors**". The stated reason (SEP-2577):
  "These features were identified during a core contributor meeting as having **low adoption relative
  to their implementation complexity** … Sampling: Complex to implement (human-in-the-loop, model
  selection, security), low client adoption". The recommended migration is blunt: "New implementations
  **SHOULD NOT** adopt it; existing implementations **SHOULD** migrate to integrating directly with
  LLM provider APIs."
- **Server-initiated requests removed as a breaking change.** "The previous pattern of
  server-initiated requests is no longer supported. **This is a breaking change.**" The client is now
  structurally always the initiator.
- **Sessions removed.** `Mcp-Session-Id` is gone; servers "**MUST NOT** rely on prior requests over the
  same connection to establish context", and "**an open connection, such as a STDIO process, is not a
  conversation or session**". State spanning requests "**MUST** be referenced by an explicit
  identifier the client passes on each request".
- **A proposal to merely *label* agent-backed tools (`agencyHint`, SEP-1938) was rejected by Core
  Maintainer vote, 0 in favour and 5 against.**

**So the boundary is real, and it is being actively defended by deprecation rather than by a scope
statement.** For charter specifically: MCP is not a candidate seam for controlling the plane, and the
protocol is moving further from that role, not closer.

**The no-listener hazard: MCP is the most explicit of any protocol studied.** The stdio shutdown
sequence is spelled out —

> "1. First, closing the input stream to the child process (the server)
> 2. Waiting for the server to exit, or sending `SIGTERM` if the server does not exit within a
>    reasonable time
> 3. Sending `SIGKILL` if the server does not exit within a reasonable time after `SIGTERM`"
> — *(<https://modelcontextprotocol.io/specification/2025-06-18/basic/lifecycle#shutdown>)*

— and the current version adds a normative obligation on the other side: "**Servers SHOULD exit
promptly when their standard input is closed or reads return end-of-file. This is the primary
graceful-shutdown signal and the only portable one.**"

**And then the sentence that matters most to charter, from the spec's own architecture page:**

> "**Best Effort**: There are no guarantees that every notification will be sent or received,
> particularly across transport reconnects. **Clients should also rely on polling to preserve
> freshness of results.**"

A 2026 protocol with a live bidirectional connection, having built a notification mechanism, tells
its clients to poll anyway — for exactly the reason §3 gives: a bounded channel cannot be the source
of truth.

**The SDKs show the hazard is not theoretical.** MCP's Python server writer uses zero-size buffers, so
backpressure propagates all the way to the caller and **a server writing to a client that is not
reading blocks indefinitely** (`src/mcp/server/stdio.py`). The client SDK carries scar tissue from
exactly that deadlock, in comments:

```python
# Unblock the reader into its drain: a server stuck writing stdout cannot
# read its stdin, so draining is what lets the flush below complete.
```

```python
async def _drain_stdout(process):
    """Consumes and discards the server's remaining stdout.
    Keeps a server flushing buffered output from blocking on a full pipe and
    missing its chance to exit..."""
```

That is the FIFO hazard, in production, in 2026, in a protocol designed by people who knew about it —
displaced one hop, from "opening the pipe blocks" to "writing to the pipe blocks, which stops you
reading the pipe that would unblock you".

### 4.4 tmux

**The split.** "In tmux, a session is displayed on screen by a client and all sessions are managed
by a single server. The server and each client are separate processes which communicate through a
socket in /tmp." *(`man tmux`, DESCRIPTION, tmux 3.7c)*. The socket lives in a per-UID directory
that "is created by tmux and must not be world readable, writable or executable"; `tmux.c:229-246`
enforces that with `mkdir(base, S_IRWXU)` (0700), an `lstat` to reject symlinks, a `st_uid` check,
and a mode check against `TMUX_SOCK_PERM` (o+rwx) — re-run by **every client on every invocation**,
not only at server start.

**The wire protocol between tmux and tmux is private and explicitly fragile.** It is OpenBSD
`imsg(3)` framing carrying fixed C structs, with the version in the header's `peerid`. The header
says so itself (`tmux-protocol.h:74-78`):

```c
/*
 * Message data.
 *
 * Don't forget to bump PROTOCOL_VERSION if any of these change!
 */
```

The struct layout *is* the wire format, no header is installed for third parties, and a mismatch is
rejected outright (`peer_check_version()` uses `!=`, not `<`). The whole rendered man page mentions
"protocol" exactly once, and that once is about *control* mode.

**Control mode is the public seam, and its design thesis is the most directly relevant sentence in
this document for charter:**

> "Control mode clients accept standard tmux commands and return their output, and additionally
> sends control mode only information (mostly asynchronous notifications) prefixed by `%`. **The
> idea is that users of control mode use tmux commands (`new-window`, `list-sessions`,
> `show-options`, and so on) to control tmux rather than duplicating a separate command set just
> for control mode.**"
> — *(<https://github.com/tmux/tmux/wiki/Control-Mode>)*

**tmux did not design a second API. It framed the CLI it already had, and added notifications.**
That is a third option beside "file format", "library" and "protocol", and it is the cheapest one
for a project that already has a good command surface.

The framing is minimal: `%begin`/`%end`/`%error` guards with a timestamp, command number and flags;
"A notification will never occur inside an output block" *(`man tmux`)* — enforced by queuing
notifications that arrive mid-block (`control.c:500-530`). `-C` leaves the terminal canonical for
testing; "Given twice (`-CC`) disables echo" and emits a `\033P1000p` DSC so a listening terminal
can detect control mode.

**Stability of the control-mode format: NO EXPLICIT STATEMENT FOUND** in either the wiki or the man
page. What exists instead is forward-compatibility built into the format — "Any subsequent arguments
up until a single `:` are for future use and should be ignored" — and advice to pin your own format
(`-F`) rather than parsing defaults. The notification set has demonstrably grown across releases.

**Backpressure: three layers, with different loss semantics per client class.** This is the most
thoroughly engineered answer to charter's question found anywhere.

*Layer 0 — never block, never die.* `SIGPIPE` is `SIG_IGN` unconditionally in both processes
(`proc.c:254`); every descriptor is non-blocking; a failed write becomes an ordinary libevent error
routed to `server_client_lost()`.

*Layer 1 — tty clients: discard and repaint.* Above `width * height * 8` bytes buffered, tmux drops
everything and repaints on a 100ms timer (`tty.c:218-245`, `log_debug("%s: can't keep up, %zu
discarded")`). The author's rationale, commit `fa6deb58`: "**This helps to prevent tmux sitting on a
huge buffer of data when there are processes with fast output running inside tmux but the outside
terminal is slow.**" This works because a terminal is a *projection*: dropping intermediate frames
is lossless if you repaint the final state.

*Layer 2 — control clients: bounded, fair-share queues.* `%output` is a stream, not a projection, so
it queues instead — per client and per pane, with watermarks (`CONTROL_BUFFER_LOW 512`,
`CONTROL_BUFFER_HIGH 8192`) and a fair-share writer that divides remaining space among pending panes
(`control.c:876-909`).

*Layer 3 — the age check.* Backpressure is measured in **time**, not bytes (`control.c:532-565`):

```c
if (c->flags & CLIENT_CONTROL_PAUSEAFTER) {
        if (age < c->pause_age)
                return (0);
        cp->flags |= CONTROL_PANE_PAUSED;
        control_discard_pane(c, cp);
        control_notify_write(c, "%%pause %%%u", wp->id);
} else {
        if (age < CONTROL_MAXIMUM_AGE)
                return (0);
        c->exit_message = xstrdup("too far behind");
        c->flags |= CLIENT_EXIT;
        control_discard(c);
}
```

Opt in to `pause-after` and you get a recoverable pause plus a `%pause` notification; don't, and
after 300 seconds you are killed with "too far behind". Resuming does **not** replay the gap —
`control_continue_pane` resets the offset to the pane's current position, which is why the wiki tells
clients to `capture-pane`.

*Layer 4 — real backpressure to the producer, but only under a specific condition.* Discussed in §3:
tmux stops reading the pty only when there are attached clients and every one of them is a control
client that is behind. Zero clients means keep reading.

*Layer 5 — a stuck client cannot hold up shutdown.* A 10-second exit timer force-discards
(`server-client.c:2333-2358`), added because "Do not let a stuck client prevent the server from
exiting - give up after 10 seconds."

**What tmux refuses.** A client cannot reach a pty directly: the pane pty is created inside the
server (`spawn.c:478`) and **the server never sends a file descriptor to a client** — every
`proc_send` in the tree passes `-1` except two calls in `client.c` where the *client* donates its own
stdin/stdout. The inversion is striking: the server then reads keystrokes off the client's tty
itself, so a client is essentially an fd donor and signal relay. OpenBSD `pledge` mirrors this
exactly — the client drops `sendfd` after identify; the server holds `recvfd` and not `sendfd`.
Read-only clients are enforced server-side, with command lists rejected unless every command carries
`CMD_READONLY`.

**What tmux gave up.** Sessions outlive the terminal — but if the server dies, everything is gone
("If not, then the server probably crashed or was killed and the sessions are gone", FAQ), there is
no persistence or journal, upgrading forces the choice between a client that cannot connect and
losing all state (§2.2 cost 3), the socket confers full trust, and the fan-out problem became the
server's permanently — three separate flow-control implementations with different loss semantics,
which a per-terminal design would have left to the kernel.

### 4.5 git

git is cited as the strongest case for "the file format IS the API". The primary sources
**partially refute that**, and the refutation is the most useful finding in this section.

**git's own framing of the split**, from `git(1)`:

> "We divide Git into high level ("porcelain") commands and low level ("plumbing") commands."
> — *(`man git`, GIT COMMANDS, git 2.50.1)*

**And git's own stated seam for third-party UIs is the plumbing *commands*, not the `.git`
directory:**

> "**Although Git includes its own porcelain layer, its low-level commands are sufficient to support
> development of alternative porcelains.** Developers of such porcelains might start by reading about
> git-update-index(1) and git-read-tree(1).
>
> The interface (input, output, set of options and the semantics) to these low-level commands are
> **meant to be a lot more stable** than Porcelain level commands, because these commands are
> primarily for scripted use. The interface to Porcelain commands on the other hand are subject to
> change in order to improve the end user experience."
> — *(`man git`, LOW-LEVEL COMMANDS (PLUMBING), git 2.50.1)*

Read that carefully. git addresses exactly the audience this document is about — people building
alternative front ends — and points them at a **command surface with a stability promise**, not at
the on-disk format. The stability language is about "input, output, set of options and the
semantics" of commands.

**The same pattern shows up in the one place git makes an explicit forward-compatibility promise**,
and it is again about output:

> "`--porcelain[=<version>]` Give the output in an easy-to-parse format for scripts. This is similar
> to the short output, but **will remain stable across Git versions and regardless of user
> configuration**."
> — *(`man git-status`, git 2.50.1)*

Note the terminology trap for anyone reading this quickly: the flag named `--porcelain` produces the
*machine-stable* output. "Porcelain" in the command taxonomy means "for humans, may change";
`--porcelain` as a flag means "for machines, will not change". They are opposite senses of one word.

**Meanwhile the file layout is documented descriptively, not contractually.**
`gitrepository-layout(5)` introduces its entire contents with "These things **may** exist in a Git
repository" *(git 2.50.1)*. That is an inventory, not a guarantee.

**So why does everyone read `.git` directly anyway?** Because the format is stable *in practice* —
several independent implementations (libgit2, JGit, go-git, gitoxide) demonstrate it, *though that
list is asserted here from general knowledge rather than verified against each project's own docs;
see §7* — and because
the plumbing surface is a process-spawn per query, which is too slow for an interactive UI. The
evidence supports "the file format is a durable de-facto contract" and does **not** support "git
offers the file format as its API". Those are different claims, and only the first survives contact
with the sources.

**The one genuine on-disk contract is a negotiate-or-abort protocol, not a stable format.** This is
the most precise thing git says about third parties reading its files, and it says the opposite of
what the thesis needs:

> "Every git repository is marked with a numeric version in the `core.repositoryformatversion` key of
> its `config` file. This version specifies the rules for operating on the on-disk repository data.
> **An implementation of git which does not understand a particular version advertised by an on-disk
> repository MUST NOT operate on that repository**; doing so risks not only producing wrong results,
> but actually losing data."
> — *(<https://git-scm.com/docs/repository-version>, `Documentation/technical/repository-version.adoc:3-9`)*

And in the same document, describing what format `0` — the format nearly every repository on earth
uses — actually is:

> "This is the format defined by the initial version of git, including but not limited to the format
> of the repository directory, the repository configuration file, and the object and ref storage.
> **Specifying the complete behavior of git is beyond the scope of this document.**"

So git's on-disk API is: *a version number, and a rule for safely refusing to read anything you do
not recognise.* Jeff King's commit introducing the extensions mechanism states the intent plainly —
older git will "not do something dangerous when confronted with these new formats… **This is
annoying, of course, but much better than the alternative of claiming that there are no refs in the
repository, or writing to a location that other implementations will not read.**"
*(`00a09d57eb8a`, 2015-06-23)*.

**The formats themselves carry no stability language.** `gitformat-index(5)`, `gitformat-pack(5)`
and `gitformat-loose(5)` contain no "stable", "guarantee", "backward" or "compat" statements. The
index specifies forward-compatibility by convention instead — an extension whose 4-byte signature
begins `A`–`Z` "is optional and can be ignored", lowercase means required — and two shipped
extensions (`link` for split index, `sdir` for sparse directories) are lowercase, so a reader that
ignores them is simply wrong. `sparse-index.adoc` is blunt about the consequence: the required
extension exists to prevent "Git versions that do not understand the sparse-index from operating on
one, while allowing tools that do not understand the sparse-index to operate on repositories **as
long as they do not interact with the index**."

**And two of git's most fundamental on-disk formats are specified only in C.** There is no
`gitformat-packed-refs`; the `# pack-refs with:` trait header is defined by a comment in
`refs/packed-backend.c:697-723` that hedges even there ("**Probably** no references are peeled"). The
loose-ref file format — the single most-read file in `.git/refs` — is defined by
`refs/files-backend.c:2063-2068` writing a hex OID followed by `'\n'`, while the read path
(`files-backend.c:631`) calls `strbuf_rtrim()` first, so a writer that omits the newline is silently
tolerated. That is not hypothetical: gitoxide shipped exactly that bug for years and only found it by
contribution — "Did you know that the long-stable and mature `gix-ref` crate wrote slightly
incompatible loose references? I didn't either, until a contribution finally added the missing
newline character at the end of the hash" *(gitoxide, `etc/reports/25-08.md`)*.

**Where the file-format seam breaks down: concurrency, and git says so repeatedly.**

The lockfile design itself is sound, and it is worth quoting because **it is charter's `os.replace`
model exactly** (`lockfile.h:4-33`):

> "When we want to change a file, we create a lockfile `<filename>.lock`, write the new file contents
> into it, and then rename the lockfile to its final destination… **Please note that lockfiles only
> block other writers. Readers do not block, but they are guaranteed to see either the old contents
> of the file or the new contents of the file** (assuming that the filesystem implements `rename(2)`
> atomically)."

But four things break around it:

1. **The index does not retry — uniquely.** `repo_hold_lock_file_for_update()` passes `timeout_ms = 0`,
   and `lockfile.c:202-216` documents that "If `timeout_ms` is 0, try locking the file exactly once."
   There is no configuration knob for it. Meanwhile `core.filesRefLockTimeout` (100ms),
   `core.packedRefsTimeout` (1000ms), `core.configLockTimeout` (1000ms) and `reftable.lockTimeout`
   (100ms) all exist. Refs, packed-refs, config and reftable retry; **the index alone fails
   immediately.**
2. **The lock has no ownership check, and git's own test proves it.** `t/t0031-lockfile-pid.sh:23-34`
   does `touch .git/index.lock` and then asserts `test_must_fail git add .`. An empty file created by
   anyone is enough to break every writer. Jeff King's stated reason for failing rather than waiting:
   "**Historically Git does not wait for locks because whoever is holding the lock is likely to
   invalidate the changes we're proposing to make by taking the lock in the first place**"
   *(<https://lore.kernel.org/git/20190501183638.GF4109@sigill.intra.peff.net/>)*.
3. **A "read-only" command takes the write lock.** `git status` refreshes and writes the index by
   default; `git(1)` therefore ships `--no-optional-locks`, and `git-status(1)` warns that "When
   `status` is run in the background, the lock held during the write may conflict with other
   simultaneous processes, **causing them to fail**." King's commit message for that flag is directly
   about the tools this document is about: "Some tools like IDEs or fancy editors may periodically run
   commands like `git status` in the background… But taking the index lock may conflict with other
   operations in the repository." He also rejected the negotiated alternative for a reason charter
   will recognise: "This is likely to be complicated and error-prone to implement (**and maybe even
   impossible with just dotlocks to work from, as it requires some inter-process communication**)."
4. **The files ref backend cannot be locked at all — a maintainer's own words.** Patrick Steinhardt,
   introducing ref-storage migrations: "It is not safe with concurrent writers. This is the limitation
   that is most critical in my eyes. The root cause here is that **it is inherently impossible to lock
   the "files" backend for writes. I have been thinking about this issue a lot and have not found any
   solution that works.**"
   *(<https://lore.kernel.org/git/cover.1716451672.git.ps@pks.im/>)*

**Git makes no general concurrency guarantee, and states specific unsafety in at least five places.**
`git-gc(1)`: "users who run commands concurrently have to live with some risk of corruption (which
seems to be low in practice)." `git-maintenance(1)`: `git gc` "modifies the object database but does
not take the lock in the same way as `git maintenance run`" — two of git's own commands lock
different files. `git-refs(1)` on migration: "There is no way to block concurrent writes to the
repository during an ongoing migration… **Users are expected to block writes on a higher level.**"
`reftable.adoc:947`: "Because a single `tables.list.lock` file is used to manage locking, **the
repository is single-threaded for writers.**" And `gitfaq(7)` warns that file-syncing services "can
cause corruption… These services tend to sync file by file on a continuous basis and **don't
understand the structure of a Git repository**", requiring the repository to be "in a quiescent state
for the duration of the transfer".

That last one is the thesis's epitaph in miniature: git's own FAQ says that a process which sees the
file layout but not the protocol will corrupt the repository.

**And the `.lock` convention is a "should", not a contract.** Steinhardt again, 2025-12-18: "We know
that all alternative implementations *should* ignore it due to the '.lock' suffix… I'd consider any
implementation that doesn't honor these 'shoulds' to be buggy, but that doesn't mean that there are
no buggy implementations."

**What the reimplementations actually did is the best available evidence — and they split.** Three of
four arrived at git's `.lock` convention by reading the C source, not a specification:

- **gitoxide** states the position most clearly, as a *non-goal*: "**be incompatible to git** — the
  on-disk format must remain compatible, and we will never contend with it", alongside goals to
  "assure reads never interfere with concurrent writes" and "assure multiple concurrent writes don't
  cause trouble". `gix-lock` is its only Stability Tier 1 crate, and its own module doc concedes
  "**locking is merely a convention rather than being enforced**". On the difficulty of matching:
  "Whenever I took creative liberties they soon proved to cause test failures. **For the most part, it
  really needed to be exactly what was done in C.**"
- **JGit** implements the same protocol and has an interop test that takes `index.lock` and asserts
  the git CLI then fails *by matching git's exact error string*
  (`CGitLockFileTest.java`, regex `"fatal: Unable to create .*/\\.git/index\\.lock': File exists\\."`).
  It also documents where the primitive breaks: on NFS, `createNewFile()` is not guaranteed atomic.
- **go-git does not implement it.** `grep -rn "O_EXCL" --include="*.go"` over the whole codebase
  returns zero results; the index is truncated in place via `fs.Create`. It uses advisory `flock(2)`,
  which git does not, so it does not interoperate — and skips even that on Windows. Its maintainers
  report the predictable outcome: "In some cases, concurrent reads may lead to repositories becoming
  corrupted."
- **libgit2** implements `.lock` with `O_EXCL` and maps `EEXIST` to `GIT_ELOCKED`, while declining to
  promise stability of its own: "we cannot promise a completely stable API", with deliberate
  divergences tracked in `docs/differences-from-git.md`.

**The strongest single argument *for* the thesis is that the ecosystem now gates git's own roadmap.**
`BreakingChanges.adoc`, on switching the default ref backend in Git 3.0: "A prerequisite for this
change is that the ecosystem is ready to support the 'reftable' format. **Most importantly,
alternative implementations of Git like JGit, libgit2 and Gitoxide need to support it.**" git cannot
freely change its on-disk format any more. That is the format being an API *de facto* — an unwritten
one, enforced by consequences rather than by promise. The cost of that model is quantified in git's
own man page: index format v4 shipped in 2012, "support for it was added to libgit2 in 2016 and to
JGit in 2020" *(`git-update-index(1)`)*. Four years and eight years, for an optional, purely local
format bump.

**Meanwhile git's maintainers are actively steering documentation away from the `.git` directory.**
The new `gitdatamodel(7)` was written under the explicit rule "Don't mention the `.git` directory, to
avoid getting too much into implementation details", and review comments on it say why — Steinhardt
on a draft sentence about `.git/refs`: "This isn't true anymore with the introduction of the reftable
backend… **this is another implementation detail that the user shouldn't have to worry about**"; Junio
Hamano: "Especially when the reftable backend is in use, **you cannot even read the raw representation
like you can do with files backend**." git also refactored its own test suite off direct on-disk
access when the format changed ("t5551: stop writing packed-refs directly").

**Hooks: the extension seam that runs inside the writer, synchronously, holding the locks, with a
veto.** `githooks(5)` is unambiguous that a hook is not an observer but a participant in the
transaction. The pattern recurs about thirty times: "Exiting with a non-zero status from this script
causes the `git commit` command to **abort** before creating a commit"; "If the hook exits with
non-zero status, **none of the refs will be updated**."

Worse for anyone reasoning about latency: the reference-transaction hook's `"prepared"` state is
documented as "All reference updates have been queued to the transaction and **references were
locked on disk**", and in that state "a non-zero exit status will cause the transaction to be
aborted". The source confirms the ordering — `refs.c:2713-2727` runs the hook *after*
`transaction_prepare()` has taken every ref lock and *before* commit, dispatched through
`run_processes_parallel`, a blocking call. **An arbitrary user program runs to completion inside the
writer's process while the repository's ref locks are held.**

**This is precisely the posture charter's hook path already takes seriously, approached from the
opposite side.** git's hooks are *designed* to be able to veto and to block; every charter hook-path
module is written on the rule that they must not. `charter/frame/notify.py`'s contract is "never
raise, and never cost them anything worth measuring", and its FIFO rejection refuses to put a
*blocking* dependency where git deliberately puts a *failing* one. Same insight — whatever runs
inside the writer's transaction owns the writer's reliability — opposite answers, because the stakes
differ: git wants a hook to be able to stop a bad commit; charter must never let a panel cost a
session its turn.

**And git did eventually add a daemon — with its own reason for abandoning the hook seam stated in
the man page.** `git fsmonitor--daemon`:

> "This daemon communicates directly with commands like `git status` using the simple IPC interface
> **instead of the slower `githooks(5)` interface**. This daemon is built into Git so that no
> third-party tools are required."
> — *(`Documentation/git-fsmonitor--daemon.adoc:22-27`)*

That is git's own admission that the hook seam did not scale, in a shipped man page. The daemon
exists because `stat`ing every file is too slow — a *liveness* problem the file format could not
solve — and it leaves the format untouched, which is the §2.1 pattern: truth stays in files, the
derived channel is separate and disposable.

**Git's IPC layer answers the no-listener question the same way everyone else does: the client
spins, the daemon never blocks.** `simple-ipc.h:9-34` enumerates the failure a client must expect,
and the list is unusually honest:

```c
/*
 * The pipe/socket exists, but the daemon is not listening.
 * Perhaps it is very busy.
 * Perhaps the daemon died without deleting the path.
 * Perhaps it is shutting down and draining existing clients.
 * Perhaps it is dead, but other clients are lingering and
 * still holding a reference to the pathname.
 */
IPC_STATE__NOT_LISTENING,
```

with the client-side options being `wait_if_busy` ("Spin under timeout if the server is running but
can't accept our connection yet") and `wait_if_not_found` ("Spin under timeout if the pipe/socket is
not yet present on the file system"). **The waiting is on the reader's side, bounded by a timeout,
and the absence of a listener is a normal enumerated state rather than an error.** That is the
inverse of a FIFO, and it is why charter's objection does not apply to this shape.

**One more caveat worth carrying into any charter design discussion, because it is the daemon
version-skew problem inside a *file*:** `core.fsmonitor`'s own documentation warns that different
git versions read the same config value incompatibly —

> "Note that if you concurrently use multiple versions of Git, such as one version on the command
> line and another version in an IDE tool… **Git versions prior to 2.26 default to hook protocol V1
> and will silently assume there were no changes to report (no scan), so status commands may report
> incomplete results.** For this reason, it is best to upgrade all of your Git versions before using
> the built-in file system monitor."
> — *(`Documentation/config/core.adoc:87-99`)*

Note the failure mode: **silently incomplete results**, not an error. A version-skewed reader of a
shared file can be worse than a version-skewed client of a socket, because a socket at least has
somewhere to put a handshake.

### 4.6 Jujutsu (jj)

The most useful comparison in the set: it made the library choice explicitly, wrote down why, wrote
down what it cost, and is now adding a protocol seam on top.

**The stated goal, in the project's core tenets:**

> "Separation of logic and UI: It should be easy to create new UIs (CLIs, GUIs, TUIs, servers)
> without having to duplicate logic."
> — *(<https://github.com/jj-vcs/jj/blob/main/docs/core_tenets.md>)*

**And the two design rules that fall out of it, which are worth stealing regardless of seam:**

> "The `jj` binary consists of two Rust crates: the library crate (`jj-lib`) and the CLI crate
> (`jj-cli`). The library crate is currently only used by the CLI crate, but it is meant to also be
> usable from a GUI or TUI, or in a server serving requests from multiple users. As a result, **the
> library should avoid interacting directly with the user via the terminal or by other means**; all
> input/output is handled by the CLI crate. Since the library crate is meant to usable in a server,
> **it also cannot read configuration from the user's home directory, or from user-specific
> environment variables.**"
> — *(<https://docs.jj-vcs.dev/latest/technical/architecture/>)*

**No I/O in the core; no ambient environment in the core.** A core that writes to a terminal or reads
`$HOME` cannot be reused by a second interface, and jj enforces both at the crate boundary.

**The cost, stated by the project in its own FAQ** — and note it concedes *both* seams are unstable:

> "* Using `jj-lib` avoids parsing command output and makes error handling easier.
> * `jj-lib` is not a stable API, so you may have to make changes to your tool when the API changes.
> * The CLI is not stable either, so you may need to make your tool detect the different versions and
>   call the right command.
> * Using the CLI means that your tool will work with custom-built `jj` binaries, like the one at
>   Google (if you're using the library, you will not be able to detect custom backends and more)."
> — *(<https://docs.jj-vcs.dev/latest/FAQ/>)*

Plus the architecture doc's candour: "A lot of thought has gone into making the library crate's API
easy to use, but not much has gone into 'details' such as which collection types are used, or which
symbols are exposed in the API."

**Correcting a likely assumption: jj did not refuse a protocol. It is building one.** There is no
statement anywhere in the repo rejecting a daemon or an RPC seam; the roadmap plans both, and the
reason is precisely that the library seam leaked:

> "One problem with writing tools using the Rust API is that they will only work with the backends
> they were compiled with… We want to provide an RPC API for tools that want to work with an unknown
> build of `jj` by having the tool run something like `jj api` to give it an address to talk to."
> "In addition to helping with the problem of unknown backends, having an RPC API should make it
> easier for tools like VS Code that are not written in Rust. **The RPC API will probably be at a
> higher abstraction level than the Rust API.**"
> — *(<https://docs.jj-vcs.dev/latest/roadmap/>)*

And on the seam having already leaked in the other direction: "UIs like `gg` currently have to
duplicate quite a bit of logic from `jj-cli`. We need to make this code not specific to the CLI (e.g.
**return status objects instead of printing messages**) and move it into `jj-lib`."

**The concurrency design — the direct answer to "what about two writers", rejecting both locks and a
daemon.** From `concurrency.md`, the reader/writer contract:

> "When a command starts, it loads the repo at the latest operation. Because the associated view
> object completely defines the repo state, the running command will not see any changes made by
> other processes thereafter. When the operation completes, it is written with the start operation as
> parent. **The operation cannot fail to commit** (except for disk failures and such). It is left for
> the next command to notice if there were divergent operations."

The mechanism is content-addressed storage plus a filename-as-pointer:

> "The operation objects and view objects are stored in content-addressed storage just like Git
> commits are. That makes them safe to write without locking."
> "We do that by keeping the ID of the current head(s) as a file in a directory. The ID is the name of
> the file; it has no contents. When an operation completes, we add a file pointing to the new
> operation and then remove the file pointing to the old operation. Writing the new file is what makes
> the operation visible… This scheme ensures that transactions are atomic."

Divergence is resolved by 3-way merging the view objects; unmergeable cases become *visible
conflicts* rather than errors — "if bookmark `main` was moved from commit A to commit B in one
operation and moved to commit C in a concurrent operation, then `main` will be recorded as 'moved
from A to B or C'."

The source confirms the design intent: the lock that does exist is an optimisation only
(`lib/src/op_heads_store.rs:63-71`) —

```rust
/// is to prevent concurrent processes from resolving the same divergent
/// operations. It is not needed for correctness; implementations are free
/// to return a type that doesn't hold a lock.
```

— and the code explicitly tolerates the lock silently failing: "It's fine if the old head was not
found. It probably means that we're on a distributed file system where the locking doesn't work.
We'll probably end up with two current heads. We'll detect that next time we load the view."
(`lib/src/simple_op_heads_store.rs:87-92`).

**Two honest caveats jj states or its source shows:**
- The *working copy* does take a real file lock (`lib/src/local_working_copy.rs:2662-2678`,
  `working_copy.lock`, with an explicit re-read after acquiring). Repo state is lock-free; the
  working copy is lock-and-re-read. jj is not fully lock-free.
- "there are known bugs in this area. Most notably, with the Git backend, repository corruption is
  possible because the backend is not entirely lock-free."

**Why this matters to charter.** charter already writes state the way jj writes op-log heads —
`config.replace_for(d / "version", f"{time.time_ns()}\n")` *(`charter/frame/state.py:346`)*, an
atomic `os.replace` of a whole file whose docstring notes that a failed write "leaves the previous
version exactly as a reader last saw it rather than corrupting it silently". jj is the proof that
this pattern scales from a version stamp to an entire mutable repository state without a lock or a
server — and also the proof that the library seam alone will not carry every consumer.

### 4.7 Podman, Docker and bpfman — the daemonless argument

Mostly covered in §2.2. Two additions.

**Podman publishes the seam trade-off explicitly**, in a form directly usable as a template:

> "libpod today is a Golang library and a CLI. The choice of interface you make has advantages and
> disadvantages."
> **Using the REST API** — "Advantages: Stable, versioned API; Language-agnostic; Well-documented API.
> Disadvantages: Error handling is less verbose than Golang API; May be slower"
> **Running as a subprocess** — "Advantages: Many commands output JSON; Works with languages other
> than Golang; Easy to get started. Disadvantages: Error handling is harder; May be slower; Can't hook
> into or control low-level things"
> **Vendoring into a Go project** — "Advantages: Significant power and control. Disadvantages: You are
> now on the hook for container runtime security updates; Binary size; **Potential skew between
> multiple libpod versions operating on the same storage can cause problems**"
> "**Making the choice** — A good question to ask first is: Do you want users to be able to use
> `podman` to manipulate the containers created by your project? If so, that makes it more likely that
> you want to run `podman` as a subprocess or using the HTTP API."
> — *(<https://github.com/containers/podman/blob/main/docs/tutorials/podman-derivative-api.md>)*

That bolded disadvantage is the specific failure mode of a daemonless library seam: **two library
versions writing the same on-disk state**. A daemon makes that impossible by construction. This is
the strongest argument in the whole document *for* a daemon, and it comes from the flagship
daemonless project.

**What Podman actually pays for refusing a daemon.** It still needs cross-process mutual exclusion,
so it allocates one POSIX semaphore per container in a shared-memory segment
(`libpod/lock/shm/shm_lock.go`), handles `EOWNERDEAD` with `pthread_mutex_consistent` for processes
that died holding a lock, and re-reads state after acquiring ("this function should suffice to
ensure a container's state is accurate", `libpod/state.go:14-19`). The pool is **fixed size**:

> "**When all available locks are exhausted, no further containers and pods can be created until some
> existing containers and pods are removed.** This can be avoided by increasing the number of locks
> available via modifying **containers.conf** and subsequently running **podman system renumber**…
> **If possible, avoid calling `podman system renumber` while there are other Podman processes
> running.**"
> — *(<https://docs.podman.io/en/latest/markdown/podman-system-renumber.1.html>)*

And the visibility cost is stated outright: "Containers created by a non-root user are not visible to
other users and are not seen or managed by Podman running as root."

### 4.8 Kubernetes

**The seam.** The API server is "the front end for the Kubernetes control plane"; etcd is "Consistent
and highly-available key value store for **all API server data**"; and access is exclusive:
"**Access to etcd is equivalent to root permission in the cluster so ideally only the API server
should have access to it**"
*(<https://kubernetes.io/docs/tasks/administer-cluster/configure-upgrade-etcd/#securing-etcd-clusters>)*.
kubectl is stated to be a client like any other: most operations go through kubectl or kubeadm,
"**which in turn use the API**".

**How a second client learns something changed — the best-documented answer in this set.** It is
list-then-watch over a chunked HTTP response:

> "The Kubernetes API allows clients to make an initial request for an object or a collection, and
> then to track changes since that initial request: a **watch**… every Kubernetes object has a
> `resourceVersion` field… **The overall watch mechanism allows a client to fetch the current state and
> then subscribe to subsequent changes, without missing any events.**"
> — *(<https://kubernetes.io/docs/reference/using-api/api-concepts/>)*

**And the history window is bounded, which is where the design shows its hand:**

> "**A given Kubernetes server will only preserve a historical record of changes for a limited time.
> Clusters using etcd 3 preserve changes in the last 5 minutes by default.** When the requested
> **watch** operations fail because the historical version of that resource is not available, **clients
> must handle the case by recognizing the status code `410 Gone`, clearing their local cache, performing
> a new get or list operation, and starting the watch from the `resourceVersion` that was returned.**"

`resourceVersion` is deliberately opaque: "**This value MUST be treated as opaque by clients and
passed unmodified back to the server**… the application should *not* rely on the implementation
details of the versioning system"
*(<https://github.com/kubernetes/community/blob/master/contributors/devel/sig-architecture/api-conventions.md>)*.
That is precisely charter's version stamp contract — a token you compare, never interpret.

**Bookmarks, and the deliberate refusal to promise delivery.** `BOOKMARK` events were added
(KEP-956) because reconnecting watchers were re-processing history and sometimes falling out of the
window entirely. The design note is instructive:

> "**We consciously make it just a boolean flag - this gives kube-apiserver an ability to choose when
> they should be send without setting any expectations on user side how frequently and when it would be
> happening. In particular client isn't guaranteed to get any bookmarks.**"

And the rejected alternative is a direct warning against putting per-client state in a notifier:

> "Instead of introducing an API for bookmarks, we can try memorizing in watchcache what we already
> processed for a watcher and when it is restarted use that information. However, **that would require
> being able to identify and match a watcher across restarts which is non-trivial. Moreover, it doesn't
> work in HA setups with multiple kube-apiserver.**"
> — *(<https://github.com/kubernetes/enhancements/blob/master/keps/sig-api-machinery/956-watch-bookmark/README.md>)*

**The slow-watcher answer** is quoted in §3. Note that it is not an error path: there is a metric,
`TerminatedWatchersCounter`, so dropping listeners is a normal operating condition.

**What it cost.** KEP-2340 documents that the watch cache exists because reading through to etcd did
not scale — "when kubelets requests pods schedule against it in a 5k node cluster with 30pods/node,
the kube-apiserver must list the 150k pods from etcd and then filter that list down to the list of 30
pods that the kubelet actually need. This must occur for each list request from each of the 5k
kubelets." And it acknowledges a known correctness compromise left standing for performance
reasons: "**We have held off on switching reflectors to using consistent read for the initial list,
even though we know it is more correct, due to concerns with the impact on large scale use cases.**"
Meanwhile the cache "may be permanently stale" if the watch stream clogs, can down the control plane
on initialisation (§2.2 cost 6), and an API-server restart forces every watcher to relist:

> "In case of rolling upgrade of kube-apiserver, when no object of a given resource type is changing
> (but objects of other types do), **all watchers will eventually be forced to relist, causing
> significant performance and scalability issues for larger clusters.**"
> — *(<https://github.com/kubernetes/enhancements/blob/master/keps/sig-api-machinery/1904-efficient-watch-resumption/README.md>)*

### 4.9 opencode — the live example, verified first-hand

Worth its own section because it is a harness charter already supports, and it has already built the
thing this document is about. From `opencode --help` (v1.18.23, installed locally):

```
opencode acp        start ACP (Agent Client Protocol) server
opencode serve      starts a headless opencode server
opencode attach     attach to a running opencode server
opencode web        start opencode server and open web interface
```

And from its documentation *(<https://opencode.ai/docs/server/>)*:

> "The `opencode serve` command runs a headless HTTP server that exposes an OpenAPI endpoint that an
> opencode client can use."
> "…lets opencode support multiple clients and allows you to interact with opencode programmatically."
> "**When you run `opencode` it starts a TUI and a server. Where the TUI is the client that talks to
> the server.**"
> "GET `/event` — Server-sent events stream. First event is `server.connected`, then bus events"
> "The `/tui` endpoint can be used to drive the TUI through the server."

Four things to take from it:

1. **The TUI-as-client pattern works and is shipping.** opencode's terminal UI has no privileged
   access to its own core; it is one HTTP client among several.
2. **The server is session-scoped, not a system daemon.** It starts with the TUI, binds `127.0.0.1`
   on an ephemeral port by default, and dies with the session — the compromise shape from §2.2,
   avoiding long-lived privilege, cross-session version skew, and orphan processes.
3. **They chose SSE, and the direction matters.** The *client* opens the channel; the server writes
   into a response the client already established. That is the socket-activation insight in HTTP
   form, and it is the one push mechanism that does not reintroduce charter's FIFO hazard — the
   writer never opens anything.
4. **They needed two seams, not one.** `serve` (HTTP + OpenAPI + SSE) for UIs, `acp` (JSON-RPC/stdio)
   for editors. Neither subsumes the other.

---

## 5. What this suggests for charter

Not a recommendation — this is input to a design discussion, so here is what the evidence supports
and what it does not.

**Supported:**

1. **Do the rust-analyzer / jj move first; it is the cheap, seam-agnostic part.** Both projects treat
   a **library API** as the real boundary and every protocol as a projection of it. charter's
   obstacle to a second interface is not the absence of a protocol; it is that the only
   machine-readable surface today is the frame's own cache, and `--json` appears exactly once in the
   entire CLI, on `doc_check` *(`charter/cli.py:123`)*. jj's two rules are the concrete, testable
   form: no terminal I/O in the core, no `$HOME`/user-env reads in the core. jj's roadmap note is the
   warning shot — "return status objects instead of printing messages" is a refactor they are doing
   *after* a second UI already had to duplicate CLI logic.
2. **Consider framing the CLI rather than designing a second API — two of the most durable projects
   here did exactly that.** tmux's control mode is the cheapest seam in this study and the most
   explicit about why: "**users of control mode use tmux commands… rather than duplicating a separate
   command set just for control mode**". And git — despite being everyone's example of a file-format
   seam — points third-party front ends at its *command* surface: plumbing is "sufficient to support
   development of alternative porcelains" and its interfaces are "meant to be a lot more stable"
   precisely because they are "primarily for scripted use". charter has a large, well-factored
   command surface already. Structured output plus a notification frame is a much smaller step than
   a protocol, and it keeps one command vocabulary rather than two.
3. **The version-stamp design should survive contact with a second interface, and §3 is why.** Every
   push system studied degrades to "resync from truth" under load; charter starts there. Two
   properties it already has make a second consumer nearly free: the refresh-before-bump ordering
   *(`charter/frame/notify.py:18-22`)* guarantees a coherent snapshot, and the stamp is an opaque
   comparand — Kubernetes' `resourceVersion` contract, arrived at independently.
4. **If a channel is ever needed, take opencode's shape, not a daemon's.** Client-initiated,
   session-scoped, ephemeral port, dies with the session. That eliminates the FIFO hazard
   structurally and avoids five of §2.2's six costs.
5. **Expect two seams eventually.** opencode ships both `serve` and `acp` because they answer
   different questions. ACP is the right adapter for editor integration and for handing work to
   agents; it is not the right shape for the plane itself.
6. **If a channel is built, do not hand-roll the flow control.** The ACP measurements in §3 are the
   warning: two mature SDKs, written by people who know this problem, both turn an unreading peer
   into unbounded memory growth. The mechanisms that work — Kubernetes' non-blocking-then-budget
   dispatch, tmux's three layers — are substantial pieces of engineering, not incidental detail.

**Not supported:**

- That "files are the API" is something to grow out of (§2.1) — though "files are the whole
  *interface*" is.
- That LSP is the precedent for simultaneous frontends. It explicitly is not, and after six years
  the maintainers still have "no concrete plans".
- That ACP is a plausible primary seam. There is no protocol object above the session, and the RFDs
  that would add one have been in Draft for eight months. charter is closer to an ACP *client*.
- That MCP could serve as a control plane "since everyone's doing it anyway". The registry says
  people are; the spec says orchestration is the host's job, and the 2026-07-28 revision removed
  sessions, server-initiated requests and sampling. It is moving away from the role, not toward it.
- That polling is a compromise. It is the state seven mature systems fall back to under stress — and
  MCP, with a live connection in hand, tells its clients to do it anyway.

**The two real arguments for change, stated fairly.**

First, poll cost scales with *client count* while push cost scales with *event count*; charter has
~4 panels, and a web UI with many browsers inverts that.

Second — and this is the one that does not depend on client count — **a file seam has no answer to
two writers except convention, and the research is unanimous that convention is not enough.** Podman
names it as the specific disadvantage of vendoring its library ("potential skew between multiple
libpod versions operating on the same storage"). git's own maintainer says of its oldest format that
"it is inherently impossible to lock the 'files' backend for writes… I have not found any solution
that works". Three of four git reimplementations reconstructed the lock protocol from C source
because no document specifies it, the fourth did not and reports corruption, and git's FAQ warns that
any process seeing the layout but not the protocol will corrupt the repository.

There are exactly two escapes in the evidence, and neither is a daemon. **jj's**: make divergence
*mergeable* rather than preventable — content-addressed writes, filename-as-pointer, 3-way merge of
concurrent heads, conflicts surfaced rather than errored. **Maildir's**: make writers structurally
non-interfering — unique names, write-then-rename, "two words: no locks". charter's version stamp is
already Maildir-shaped (`os.replace` of a whole file, unique temp name per writer — a lesson
`charter/frame/state.py`'s docstring records paying for in #893). Whether the rest of charter's
state has that property, and whether a second *writing* interface is even in scope, are the questions
this research cannot answer from outside — but they are the right questions, and they matter more
than the daemon question.

---

## 6. Sources

**Specifications and standards**
- POSIX `open()` — <https://pubs.opengroup.org/onlinepubs/9699919799/functions/open.html>
- LSP 3.17 specification — <https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/>
- LSP overview — <https://microsoft.github.io/language-server-protocol/overviews/lsp/overview/>
- VS Code Language Server Extension Guide (the M*N and separate-process rationale) — <https://code.visualstudio.com/api/language-extensions/language-server-extension-guide>
- Maildir — <https://cr.yp.to/proto/maildir.html>
- `inotify(7)` — <https://man7.org/linux/man-pages/man7/inotify.7.html>

**Design documents and source, at pinned commits**
- jj architecture / concurrency / core tenets / FAQ / roadmap — <https://docs.jj-vcs.dev/latest/technical/architecture/>, `.../concurrency/`, <https://github.com/jj-vcs/jj/blob/main/docs/core_tenets.md>, <https://docs.jj-vcs.dev/latest/FAQ/>, <https://docs.jj-vcs.dev/latest/roadmap/> (jj `1d41436`)
- rust-analyzer architecture — <https://github.com/rust-lang/rust-analyzer/blob/master/docs/book/src/contributing/architecture.md>
- bpfman daemonless design — <https://github.com/bpfman/bpfman/blob/main/docs/design/daemonless.md>
- ACP: introduction, architecture, governance, v1 overview / transports / session-setup / agent-plan, v2 migration and prompt-lifecycle, v2 announcement, proxy-chains and MCP-over-ACP RFDs — <https://agentclientprotocol.com/>; schema at `schema/v1/meta.json` and `schema/v2/meta.json` in <https://github.com/agentclientprotocol/agent-client-protocol>
- MCP spec 2026-07-28 (architecture, basic, transports/stdio, deprecated) and 2025-06-18 (lifecycle, transports) — <https://modelcontextprotocol.io/>; SEP-2577, SEP-2567, SEP-1938 and PR #206 in <https://github.com/modelcontextprotocol/modelcontextprotocol>
- MCP reference servers — <https://github.com/modelcontextprotocol/servers>; registry at <https://registry.modelcontextprotocol.io/v0/servers>
- Zed, "Bring Your Own Agent to Zed" — <https://zed.dev/blog/bring-your-own-agent-to-zed>
- tmux source: `tmux.c`, `client.c`, `server.c`, `server-client.c`, `proc.c`, `control.c`, `tty.c`, `tmux-protocol.h` — <https://github.com/tmux/tmux> (HEAD `578e07fc`)
- Kubernetes source: `staging/src/k8s.io/apiserver/pkg/storage/cacher/` — <https://github.com/kubernetes/kubernetes> (`b2ec8b6`)
- Podman source and docs: `libpod/lock/`, `docs/tutorials/podman-derivative-api.md` — <https://github.com/containers/podman> (`89425c3`)
- Kubernetes KEPs 956 (bookmarks), 1904 (watch resumption), 2340 (consistent reads), 4568 (resilient watchcache init) — <https://github.com/kubernetes/enhancements>

**Official documentation**
- tmux man page and wiki, tmux 3.7c — `man tmux`; <https://github.com/tmux/tmux/wiki/Control-Mode>; <https://github.com/tmux/tmux/wiki/FAQ>
- git man pages, git 2.50.1 (read locally) — `man git` (GIT COMMANDS; LOW-LEVEL COMMANDS (PLUMBING)), `man git-status` (`--porcelain`), `man gitrepository-layout`, `man githooks`, `man gitfaq`
- git source and docs at `3cb9185f65` (v2.55.0-787, 2026-09-02) — `Documentation/git.adoc`, `technical/repository-version.adoc`, `gitformat-index.adoc`, `technical/sparse-index.adoc`, `technical/reftable.adoc`, `technical/api-simple-ipc.adoc`, `BreakingChanges.adoc`, `git-fsmonitor--daemon.adoc`, `config/core.adoc`, `git-gc.adoc`, `git-maintenance.adoc`, `git-refs.adoc`, `gitfaq.adoc`, `gitdatamodel.adoc`; and `lockfile.h`, `lockfile.c`, `simple-ipc.h`, `refs.c`, `refs/packed-backend.c`, `refs/files-backend.c`, `t/t0031-lockfile-pid.sh`
- git mailing list (lore.kernel.org/git) — Jeff King on lock waiting (`20190501183638.GF4109@sigill.intra.peff.net`) and `--no-optional-locks` (`20170921043214.pyhdsrpy4omy54rm@sigill.intra.peff.net`); Patrick Steinhardt on the files ref backend (`cover.1716451672.git.ps@pks.im`); Michael Haggerty on ref lock retries (`4ff0f01cb7dd`)
- Reimplementations, own docs and source — libgit2 (`README.md`, `docs/differences-from-git.md`), JGit (`LockFile.java`, `CGitLockFileTest.java`, `FS.java`), go-git (`storage/filesystem/dotgit/`, `COMPATIBILITY.md`), gitoxide (`README.md` Non-Goals, `gix-lock/src/lib.rs`, `etc/reports/`)
- Docker security / Engine API — <https://docs.docker.com/engine/security/>, <https://docs.docker.com/reference/api/engine/>
- Podman — <https://docs.podman.io/en/latest/markdown/podman-system-service.1.html>, `.../podman-system-renumber.1.html`
- Kubernetes API concepts — <https://kubernetes.io/docs/reference/using-api/api-concepts/>
- Bazel client/server — <https://bazel.build/run/client-server>
- Gradle daemon — <https://docs.gradle.org/current/userguide/gradle_daemon.html>
- Mercurial command server — <https://wiki.mercurial-scm.org/CommandServer>
- systemctl — <https://www.freedesktop.org/software/systemd/man/latest/systemctl.html>
- Apple FSEvents guide — <https://developer.apple.com/library/archive/documentation/Darwin/Conceptual/FSEvents_ProgGuide/UsingtheFSEventsFramework/UsingtheFSEventsFramework.html>
- Watchman query — <https://facebook.github.io/watchman/docs/cmd/query.html>
- opencode server / ACP — <https://opencode.ai/docs/server/>, <https://opencode.ai/docs/acp/>

**Semi-primary — implementer critique, flagged as such**
- matklad, "LSP could have been better" — <https://matklad.github.io/2023/10/12/lsp-could-have-been-better.html>
- matklad, "Why LSP?" — <https://matklad.github.io/2022/04/25/why-lsp.html>

**First-hand observation**
- `opencode --help` / `serve --help` / `acp --help`, opencode v1.18.23, 2026-09-05
- `tmux -V` → 3.7c; `git --version` → 2.50.1

**charter itself (read-only)**
- `charter/frame/notify.py:18-22`, `:59-62`, `:84`
- `charter/frame/panel.py:9-13`, `:147`, `:646-648`
- `charter/frame/state.py:346`; `charter/frame/gather.py:67`
- `charter/harness/registry.py`; `charter/cli.py:123`
- `docs/frame.md:412`; `docs/browser.md:80-86`
- `docs/adr/0018-charter-may-run-the-harness-but-never-draws-it.md`

---

## 7. Where the evidence is thin

1. **Microsoft never states why LSP is a protocol rather than a library in the spec itself.** The
   reasoning exists, but at `code.visualstudio.com` (language independence + resource isolation) and
   in maintainer issue comments (JSON over binary), not in the specification. The commonly repeated
   "crash isolation" rationale is **not stated anywhere found** — treat it as inference.
2. **There is no LSP FAQ page.** `microsoft.github.io/language-server-protocol/faq/` 404s and no such
   page exists in the site's source tree. Anything cited as "the LSP FAQ" is not a Microsoft page.
3. **The M×N framing is not on microsoft.github.io** and not in the 2016 announcement — a full grep
   of the site repo returns nothing. Cite the VS Code extension guide for it.
4. **LSP specifies no flow control whatsoever.** Zero occurrences of backpressure, flow control,
   throttling or congestion in the 3.17/3.18 trees, and no on-point issues. This is an absence, and
   it is a finding.
5. **jj's oft-quoted "the CLI is a thin wrapper around the library" README line does not exist.** All
   172 historical revisions of `README.md` were searched for it; zero hits. The substance is real but
   lives in `architecture.md` and `core_tenets.md`.
6. **jj has not refused a daemon.** No such statement exists in the repo; the roadmap plans both an
   RPC API (`jj api`) and a caching daemon. Do not cite jj as a daemonless-by-principle project.
7. **tmux makes no stability statement about the control-mode output format** — neither the wiki nor
   the man page uses "stable", "compatible" or "guaranteed". What exists is forward-compatibility
   convention ("should be ignored") rather than a promise.
8. **tmux's upgrade problem is documented in exactly one place** — a 2014 release note in `CHANGES`,
   not the man page or FAQ. There is no version-negotiation mechanism; `peer_check_version` uses
   `!=`.
9. **Docker's client/daemon split is described, not justified.** No document found in which Docker
   states *why* a daemon was the right seam. The §1 row reflects that.
10. **No Kubernetes document names "kubectl needs the cluster reachable" as an acknowledged cost.**
    It follows from the architecture but is not stated as a trade-off.
11. **D-Bus overflow behaviour is unverified.** `max_outgoing_bytes` bounds the per-connection queue
    (`dbus-daemon(1)`), but whether the bus drops or disconnects on overflow was not confirmed. It is
    omitted from §3's table for that reason.
12. **Mercurial's `chg` is still not the default entry point after more than a decade**, which is
    suggestive about appetite for optional daemons — but **no stated reason was found**. Observation,
    not finding.
13. **Scalar is not a "regretted daemon" story.** It is often cited as one. The stated cause was
    platform deprecation — "Apple deprecated the kernel features that provided the filesystem
    virtualization that was required for that flow" — plus simplification, not architectural regret
    *(<https://github.blog/open-source/git/the-story-of-scalar/>)*. Included so it is not mis-cited.
14. **ACP has no non-goals section at all** — grepping the docs, README and contributing guide for
    `non-goal|not a goal|out of scope|does not standardize|deliberately` returns zero matches. The
    "Trusted" design-philosophy bullet is the only real scope boundary, and it is a philosophy note,
    not a non-goal.
15. **MCP's spec has no "what MCP is not" section either**, and no maintainer has ever written "MCP
    is not an agent protocol." The boundary is enforced by deprecations and maintainer votes rather
    than by a scope statement. The Agents WG charter is the only written out-of-scope text.
16. **The ACP SDK memory figures in §3 are measurements taken during this research, not quotes.**
    They were produced against the published `@agentclientprotocol/sdk@1.4.0` and the Rust SDK at
    `726c503`, by writing notifications into a pipe with no reader. They are reproducible but they
    are not a document ACP published, and should be re-run before being relied on.
17. **Several widely repeated claims about ACP adoption are wrong.** Claude Code does not speak ACP
    natively (the adapter is Zed's, over the Claude Agent SDK); Codex does not either (it ships
    `codex app-server`, its own JSON-RPC protocol). Gemini CLI's flag is now `--acp`;
    `--experimental-acp` is deprecated, and Gemini's own CLI reference is stale about it.
18. **git publishes no general statement either way about concurrent processes on one repository.**
    A search of `Documentation/`, `CodingGuidelines`, `SubmittingPatches` and the mailing list found
    neither a "do not read `.git` directly" instruction nor an endorsement of doing so. What exists
    is five specific admissions of unsafety (§4.5) and one informal contrary voice from a contributor
    rather than the maintainer. **The absence is the finding**, but treat it as an argument from
    absence.
19. **gitoxide does not say git's format is "under-specified".** A full-tree grep for
    `under-specified|underspecified|not documented|undocumented` found nothing on point. Their actual
    position is the stronger and different one quoted in §4.5: the format is authoritative and
    non-negotiable, and matching it meant transcribing the C.
20. **git's documentation filenames changed.** All `Documentation/*.txt` became `*.adoc` in
    `1f010d6bdf75` (2025-01), and `technical/index-format.txt` became the man page
    `gitformat-index.adoc`. Older citations to the `.txt` paths will 404.
21. **git file/line references in §4.5 are against `3cb9185f65` (v2.55.0-787, 2026-09-02)**, while the
    man-page quotes were read locally at git 2.50.1. The two agree on every quoted passage, but line
    numbers drift — cite by function or heading rather than by line where it matters.
