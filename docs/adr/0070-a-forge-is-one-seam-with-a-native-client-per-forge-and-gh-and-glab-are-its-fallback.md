# A forge is one seam with a native client per forge, and `gh` and `glab` are its fallback

**Accepted 2026-09-30** by the operator (ruling V22c), drafted for program-map ticket FG-1 (#710).
It follows these of the operator's rulings:

- **The forge amendment:** GitHub and GitLab are both first-class now. Every feature that
  touches a forge ships for both, behind one forge seam.
- **X10:** forge parity comes from one shared seam, with contract tests for both forges in CI.
- **FI1 to FI14** (2026-09-30), and FI1 above all: native API clients **in the Rust core** (REST
  and GraphQL for GitHub and GitLab, Forgejo REST later). They sit behind the forge seam, and the
  CLIs stay as a fallback and for tests. FI1 re-cuts this ticket from "wrap `gh` and `glab`" to
  "a native client per forge". FI2 (sign-in), FI3 (the human's token never reaches an agent),
  FI4 (the neutral work model), FI7 (a derived cache) and FI14 (budgets, rights and audit) set
  what the seam must carry.
- **V13:** webhooks come later, through the relay.
- **V16c:** per-agent GitHub tokens come from a GitHub App that each organisation registers
  itself. purlis's shared GitHub App is for sign-in only.

**FI1 overrides part of the forge amendment, until PE-30a/b.** The amendment puts every forge
feature "behind the forge extension seam", and E1 says GitHub and GitLab ship as first-party
extensions. FI1 is later and more specific: the clients are "in the Rust core". This ADR
follows FI1. The seam is a Rust trait in `charter-core`, not the public extension API. PE-30a/b
move the two clients behind that API later, and the trait is shaped so that move changes where
the code runs, not what it answers.

It builds on [ADR 0055](0055-a-workspaces-repos-are-picked-from-what-your-own-forge-login-reaches.md)
(the repo picker asks as the operator's own login) and
[ADR 0047](0047-a-vault-lives-in-the-system-keyring-by-default.md) (the keyring). It keeps to
[ADR 0067](0067-a-chat-runs-in-a-sandbox-charter-compiles-for-its-harness.md) (the chat
sandbox) and to [ADR 0068](0068-a-chat-lives-in-charterd-and-the-app-is-its-client.md) (`purlisd`,
the session host, and its client scopes).
The tickets that build on it are the native clients (FW-2a and FW-2b), sign-in (FW-1, FW-3a and
FW-3b), the parity audit (FG-3), the test harness (FG-4 and FW-15), capability detection and its
card (FG-2 and FG-16), the budget (FW-4), the cache (FW-7), audited writes (FW-14), per-agent
tokens (SD-7a and SD-7b), and every `a`/`b` twin after them.

## Where purlis is today

**Every forge call runs the forge's own CLI.** `crates/purlis-core/src/forge.rs` finds `gh` or
`glab` (the operator's `PATH` first, then pinned by absolute path), starts it with an emptied
environment that keeps only the CLI's credential variables, and parses what it prints. Its
module docs state the rule this came from: *"The token lives in the forge's own CLI … charter
only ever runs that CLI. It is never read here."* Three modules hold the calls:

- **`forge.rs`:** the auth check, listing an owner's repos (`discover`), listing what the
  operator reaches (the picker, ADR 0055), a repo's top-level tree (stack detection), and a
  branch's open request and CI word (`gl-refresh`).
- **`forge/pr.rs`:** open or update a PR or MR, read its state, find it by head, and request
  auto-merge (ADR 0051; `planegit/prsave.rs` and `reposave.rs` call it).
- **`forge/checks.rs`:** the checks at one exact commit, as five closed values (ADR 0060;
  `change/view.rs` calls it).

`report.rs` files on purlis's own tracker through `gh_as_the_operator`, which withholds every
`GH_TOKEN`-style variable so that the issue is filed under the operator's own login.

**There is no seam.** Each function matches on `Kind` and builds `gh api` or `glab api`
arguments inline. Parity is a matter of discipline, and so is the choice between a strict read
and a permissive one (`crates/purlis-core/docs/forges.md`). The tests pin argv:
`a_forge_cli_is_asked_exactly_what_python_asked.rs` and
`a_pr_is_opened_or_updated_and_set_to_auto_merge.rs` run a stand-in `gh` and `glab`
(`tests/support/forge_cli.rs`) that answers only the exact questions a test wrote down.
`charter-core` has no HTTP client. `reqwest` is in the lockfile only through the app.

**Git's own credential is separate.** A clone's credential helper is `gh auth git-credential`
or `glab`'s own (`Forge::credential_helper`, `helper_for`). An agent in a chat pushes through the
same clone, so today a chat reaches the operator's CLI login. SD-7a and SD-7b close that gap, and
#752 decides purlis's own git credential path after FD-27. This ADR does not widen the gap.

**What FI1 to FI14 need, and running a CLI cannot give:** a sign-in purlis owns (FI2), with no
CLI installed; conditional requests and a per-account budget (FW-4); a write audit that knows
which chat and which human it was for (FW-14); a network log entry per call (OB-15); GraphQL for
sub-issues, Projects v2, epics and iterations (FI4); and a test that runs with no CLI on `PATH`
(FW-2a and FW-2b's acceptance).

## The decision

**purlis talks to a forge through one seam: a small set of traits, one per area, that each
forge implements. The implementations are native code in purlis's Rust core, speaking REST and
GraphQL. Every call carries a `Caller`, reads included. Each implementation sends its requests
through a `Transport`. The native transport is HTTPS with a token purlis holds. The fallback
transport is the forge's own CLI (`gh api`, `glab api`) with that CLI's own login. Which
transport, and which credential, a call gets is decided once, from the account and the `Caller`,
and never switched after a request has failed. What a forge can do is a set of capability flags,
one set per forge, host and tier. One contract suite, written once, runs against both forges.**

### 1. One trait per area, neutral types, no default methods

The seam lives in `crates/purlis-core/src/forge/backend.rs`. It speaks only neutral types:
FI4's work model (FW-5 defines the types), plus the types the core already has (a repo record, a
request, `Checks`, `AutoMerge`). A forge's own identifiers (a GitHub node id, a GitLab global id
or `iid`) travel inside a neutral type as an opaque `ForgeRef`, so that a round trip never
re-derives them.

It is split by area, so that no one trait grows with every ticket. An area trait is added in the
same PR as the first ticket that needs it, with both forges' implementations:

```rust
pub trait Repos {        // now (FG-3): list an owner's, list what an account reaches, a repo's tree
    fn reachable(&self, caller: &Caller, owner: &Owner) -> Result<Vec<RepoRecord>, ForgeError>;
    /* … */
}
pub trait Requests {     // now (FG-3): open or update, state, by head, auto-merge, checks at a sha
    /* … */
}
pub trait Capabilities { // now (FG-2): section 2
    fn support(&self, caller: &Caller, at: &Reach, what: Capability) -> Support;
}
pub trait WorkItems { /* FW-6a/b */ }
pub trait Reviews { /* FG-5a/b, FW-16a/b */ }
pub trait Notifications { /* FW-13 */ }

pub trait ForgeBackend: Repos + Requests + Capabilities {}   // grows as each area lands
```

The method names are FG-3's and FW-2a/b's to fix. This ADR fixes the split, the neutral types,
and the rules below.

- **No area trait has default method bodies.** A method added to one area does not compile until
  every backend has a body for it, even if the body only returns `Err(ForgeError::Unavailable(…))`.
  That stops a missing method, and it does not stop a GitLab body that is a stub. **Parity is
  enforced by the contract suite's parity test (section 7)**, which fails when a method has a
  contract test for one forge and not the other. W7's rule, that a GitLab twin may ship one
  release after its GitHub twin, is kept visible: until then the twin's body returns
  `Unavailable` with the reason `NotYetBuilt`, its contract test asserts exactly that, and the
  capability card (FG-16) says so in the product.
- **The trait has no `kind()`.** A caller that needs a forge-specific word (a change's `!` or
  `#` sigil, "group" or "owner") reads it from the account's `Kind`, which the caller already
  holds because it chose the account. A method on the backend would invite callers to match on
  it, and matching on the forge outside the backends is what this ADR removes.
- **The traits are synchronous** (section 3 says why).
- **Errors are neutral and closed:** `Auth` (the token is refused), `Forbidden` (the account
  lacks the right), `NotFound`, `RateLimited { reset }`, `Conflict`, `Unavailable(Unavailable)`
  (section 2), `Transport` (could not ask), and `Unrecognised` (an answer purlis does not
  understand). Each carries the forge's own words. The **strict** and **permissive** disciplines
  in `docs/forges.md` stay with the caller: the trait always returns the error, and a permissive
  caller (`gl-refresh`) is the one that turns it into "nothing".
- **Forgejo has its slot now.** The kind word `forgejo` is reserved and means nothing else. Its
  backend, and the `Kind::Forgejo` variant with it, are FG-13's. Until FG-13 there is no Forgejo
  backend at all, rather than one that answers `Unavailable` to everything: a plane that declares
  `kind = "forgejo"` is refused, as an unknown kind is today, with a message that names FG-13's
  backend as not yet built.

### 2. Capability flags are per forge, host and tier, and silence never reads as "yes"

A **forge capability** is one thing a forge may or may not do for one repo, such as a merge
queue. It is not an extension's **capability** (CONTEXT.md), and the word is always qualified.

```rust
pub enum Capability { AutoMerge, MergeQueue, SubIssues, IssueTypes, Epics, Iterations, Boards, Dependencies, CloseReasons }
// The enum grows by ticket: each ticket that maps a field some forge lacks adds its capability
// (`CloseReasons`: FW-6a, #733). `src/forge/backend.rs` holds the current list.

pub enum Support { Available, Unavailable(Unavailable), Unknown(UnknownWhy) }

pub struct Unavailable { pub what: Capability, pub reason: Reason, pub fallback: Fallback }

pub enum Reason {
    NotOnThisForge,              // GitHub has no merge trains; GitLab Free has no epics API
    NeedsTier(Tier),             // Tier::GitLabPremium, Tier::GitHubOrganisation, …
    HostTooOld(HostVersion),     // a GHES or self-managed GitLab older than the feature
    NoRight(Right),              // present, but this account may not use it
    NotYetBuilt,                 // W7: the twin ships one release later
}

pub enum Fallback {
    AutoMergeInLandingOrder,     // no merge queue or train: auto-merge, charter orders (ADR 0060)
    LabelsAndMilestones,         // no epics or iterations (FW-6b)
    ParentLinkInBody,            // no sub-issues or dependencies: a link block in the body
    HumanClick,                  // charter cannot do it; the person does it on the forge
    Hidden,                      // the control is not shown (FI14: the UI hides what the account lacks)
}
```

- **"Unavailable" is one type, said once.** A backend returns `Err(ForgeError::Unavailable(u))`
  from an operation exactly when `Capabilities::support` answers `Support::Unavailable(u)` for
  that capability, and both come from the same function in each backend. The contract suite
  checks that the two agree.
- **The flags are the ones a ticket consumes now:** auto-merge and merge queue (ADR 0051's saves,
  ADR 0060's landing, FG-2 and FG-16), and the work-item flags FW-6a/b map. Each later flag
  (uploads for FG-10a/b, per-agent tokens for SD-7a/b, required signatures, stacked changes) is
  added by the ticket that reads it.
- **`Reach` says what a question is about:** the instance (a host and its version), an owner (a
  GitHub organisation or personal account, a GitLab namespace, and its plan or licence), or one
  repo (its settings and the account's rights there). A flag is a function of those four things:
  the kind, the host and version, the tier, and the rights.
- **Detection reads what the forge reports, and probes the feature when it does not report
  it.** For example, a self-managed GitLab does not show its licence to a non-admin, so purlis
  asks for an epic and reads the refusal. FG-2 writes the probes. Results are kept per account
  in the machine tier with an expiry, and `purlis doctor` and the capability card (FG-16) read
  them.
- **`Unknown` is never treated as `Available`.** This is `checks.rs`'s rule ("no word for
  silence reads as passing"), applied to features. A feature whose flag is unknown takes the
  fallback its capability names, and the card says why.

### 3. The native clients live in `charter-core`, over one synchronous transport

```
crates/purlis-core/src/forge/
  backend.rs     the area traits, Caller, Reach, Support, Unavailable, ForgeError, ForgeRef
  transport.rs   Transport: one request in, one response out, and its middleware chain
  http.rs        the native transport: ureq, rustls, a token from a TokenSource
  cli.rs         the fallback transport: today's `call_with`, sending `gh api` / `glab api`
  github/        the GitHub backend: REST and GraphQL (FW-2a)
  gitlab/        the GitLab backend: REST and GraphQL (FW-2b)
```

- **`charter-core`, not a new crate** (FI1: "in the Rust core"). Every caller is in the core
  already (`discover`, the picker, saves, changes, `gl-refresh`), and the core never depends on
  Tauri or the UI (CLAUDE.md), which the clients do not need either. When PE-30a/b move the
  clients behind the public extension API, the `github/` and `gitlab/` directories are what
  moves. Open question 3 asks about a separate crate.
- **The HTTP client is `ureq` 3, a synchronous client.** `charter-core` is plain blocking Rust,
  and its callers (`discover`, a save, the CLI) are blocking. `reqwest`'s blocking client starts
  its own runtime and panics when it is called from inside an async runtime, which `purlisd` and
  the app both run. So a blocking `reqwest` in the core would be a trap. `ureq` has no runtime.
  It speaks the `http` crate's request and response types, and it has its own middleware trait.
  Open question 10 asks whether to go async instead.
- **Community clients are used where they let purlis supply the HTTP.**
  - **GitLab REST uses the `gitlab` crate's endpoint builders.** Its `api::Client` trait takes an
    `http` request and returns an `http` response, and the caller supplies it. purlis implements
    that trait over its own `Transport`, so the crate's typed endpoints, pagination and error
    mapping come for free, and the budget, audit and log still see every request.
  - **GitHub has no client that fits.** `octocrab` is async-only and brings its own `hyper`
    stack. It can take a tower service, but only as an async stack, which would bring the
    runtime into the core. So GitHub REST bodies are `serde` types that purlis owns, over the
    same `Transport`.
  - **GraphQL on both forges uses `graphql_client`.** Queries are `.graphql` files checked at
    compile time against each forge's published schema, so a renamed field fails the build, not
    a user. The `gitlab` crate uses the same library for its own GraphQL.

  Open question 4 asks about this.
- **Every request goes through `Transport::send`, and each concern is a standard middleware, not
  hand-rolled plumbing.** The seam is purlis's, and the layers inside it use `ureq`'s middleware
  trait, in this order:
  1. **The credential** (section 4): the `Authorization` header, and nothing else of purlis's.
  2. **Conditional requests:** `http-cache-ureq`'s middleware, with a cache manager purlis
     supplies. This is the transport's own ETag store: per account, in the machine tier (FR-30),
     and keyed by URL. It is **separate from FW-7's item cache**, which sits above the traits
     (next bullet). A `304 Not Modified` answers from the store. If `http-cache-ureq` has not
     reached a stable release by FW-4, FW-4 uses a small middleware that sends `If-None-Match`
     and stores the ETag, and swaps in the crate when it is stable.
  3. **The budget** (FW-4, section 6).
  4. **The network log** (OB-15): the host, method, path template, status and timing, and never
     a body or a header value.
  5. **The audit** of writes (FW-14, section 6).

  The CLI transport (section 5) runs the same chain after the credential step, because
  `--include` gives it the status and headers.
- **FW-7's item cache sits above the traits.** It reads through a backend, stores neutral items
  in the machine tier, and never becomes a second source of truth (FI7). It holds no ETags and
  makes no requests of its own. Writes go straight to the forge.
- **Webhooks do not enter the traits (V13).** Push input goes to the trigger-source seam (AC-18)
  after GT-CLOUD. When FG-12a/b land, each backend gains one pure function that turns a verified
  payload into neutral events, and that function makes no requests.

### 4. Credentials reach the client, and never an agent

**A `Caller` rides on every call, reads included.** It names the account, the principal (a
human, or a chat by its id), the human on whose behalf the call is made, the surface (the
window, the CLI, MCP, a trigger), and the priority (foreground or background). Credential
resolution is a function of the account **and** the `Caller`, and nothing else decides it.

- **A forge account is one sign-in:** a forge kind, a host and a login. There can be several per
  forge (github.com, a GHES, gitlab.com, self-managed), and each repo is bound to one (FI2). An
  account's sign-in resolves to a `TokenSource`, not to a string. A `TokenSource` hands out a
  token when a request is sent, refreshes an expiring one (GitHub App user tokens and GitLab
  OAuth tokens both expire), and writes the refreshed token back to the keyring. FW-1 decides the
  flows and scopes. This ADR decides that the seam takes a source, so that no caller ever holds
  the token.
- **The sign-in token is the human's, and it never reaches a chat (FI3).** It is a keyring item,
  service `charter/forge/<host>/<account>`, stored the same way as a vault's (ADR 0047), and it
  falls under ADR 0067's first denial class: a chat's sandbox denies it, as it denies a vault's
  storage.
- **The token type is `secrecy::SecretString`,** which zeroizes its memory on drop (`zeroize`) and
  has no `Debug`, `Display` or `Serialize`. It is read when a request is sent, set as the
  `Authorization` header, and dropped. It never goes on a command line or into a child's
  environment. It is never written to a transcript, a session record, a log, the network log, the
  audit, the plane or a crash report. OB-11's crash reports are stack-only minidumps with paths
  scrubbed, and no heap. The test is
  `a_forge_token_appears_in_no_log_record_or_crash_report`. It signs in a canary token against the
  recorded forge, runs every contract operation and one forced crash, and searches every file the
  run wrote for the canary: the logs, the audit, the session records and the minidump.
- **Only a human `Caller` resolves to the sign-in token.** The window is one. After FD-2 and
  FD-27, `purlisd` resolves it for a connection on a human client scope, and refuses that scope
  to a chat's process tree (V16a). The `purlis` binary **never** reads a forge token from the
  keyring itself, because a `purlis` a chat runs is part of that chat (ADR 0067 section 2).
  Until `purlisd`'s scopes exist, every call a `purlis` command makes resolves to the CLI
  transport (section 5), which is exactly what it does today.
- **A chat `Caller` never resolves to the sign-in token.** A forge call made for a chat (a
  `purlis` command it runs, or an MCP tool it calls) gets the chat's own credential:
  - **Before SD-7a/b:** the CLI transport, with the CLI login the chat already reaches today,
    **unless** that login has been imported (next bullet).
  - **After SD-7a:** a per-agent token that `purlisd` mints for that chat from the
    organisation's own GitHub App (V16c; purlis's shared App mints nothing).
  - **After SD-7b:** a GitLab project or group access token. **On GitLab Free, where those tokens
    do not exist, a chat gets no forge token at all.** Pushes and MRs go through the human's click
    (FW-3b's token stays in purlis), and doctor says so (SD-7b).
- **An imported CLI login becomes the human's, and chats lose it** (FI2's import). Importing
  copies the CLI's stored token into the keyring as a sign-in account, and from then on it is the
  human's UI token (SD-7b's words). So from the import on, a chat `Caller` for that host no longer
  resolves to the CLI transport either. It gets the SD-7a/b credential when that exists, and
  otherwise no forge credential: SD-7b's Free mode (pushes and requests go through the human's
  click), on either forge. The chat's sandbox should also deny the CLI's own stored login for
  that host, because otherwise the chat can still run `gh` itself. That denial is SD-2's to
  compile into ADR 0067's third class ("human powers"). This rule costs a chat its forge writes
  on an imported host until SD-7a/b land. Open question 1 asks whether to take that cost or
  refuse imports until then.
- **No MCP tool is served by a backend holding the sign-in token.** Chat-facing MCP tools are
  served by `purlisd` on the chat's own connection. The `Caller` built there is always a chat
  principal, and it is never taken from the tool call's arguments. The test is
  `an_mcp_tool_call_from_a_chat_never_sends_the_sign_in_token`. With a signed-in account, a chat's
  MCP tool reads and writes against the recorded forge, and the recorded server checks every
  request it gets for the sign-in token's `Authorization` header.
- **Git's credential path is not part of the seam.** Clones keep `gh`'s or `glab`'s credential
  helper. A charter-provided helper would run inside a chat whenever the chat runs `git push` in
  that clone, so none is added now. SD-7a/b give a chat's pushes their own identity, and #752 (M31)
  decides purlis's own git credential path once FD-27's scopes can refuse it to a chat.
- **Extensions never receive the token.** When PE-29 opens the seam to extensions, an extension
  asks purlis to make a call, and purlis makes it through these traits with the extension's
  `Caller` and audits it. CONTEXT.md's "Forge extension" entry is updated by this ADR, since it
  says an extension reaches the forge "through `gh` or `glab`'s own login", which stays true only
  until PE-29.

### 5. The fallback is a transport, so each operation has one implementation per forge

`gh api` and `glab api` each send an arbitrary REST or GraphQL request (`--method`, `--header`,
`--input -`, `--include`, `--hostname`) and authenticate with the CLI's own login. So the
fallback is a second `Transport`, and not a second backend. The GitHub backend builds one
request, and either transport sends it. `cli.rs` is today's runner, kept as it is: the pinned
path, the emptied environment, and the withheld `TOKEN_ENV`. The one change is that the body goes
on stdin instead of in `-f` fields, which also removes #323's `@` hazard by construction.

- **When a call uses the CLI transport:** the account has no purlis sign-in but the CLI is
  logged in on that host; or the `Caller` is a `purlis` command before `purlisd`'s scopes exist;
  or the `Caller` is a chat before SD-7a/b and the login was not imported (section 4); or the
  operator chose it for the account in settings. Doctor shows each account's transport.
- **It is chosen at resolution, never on failure.** A `401` or `403` from the native transport
  is reported as `Auth` or `Forbidden` for that account, and purlis does not retry the request
  through the CLI. Retrying would repeat the request under a different identity, possibly a
  different human's, and a write would then be audited under the wrong login.
- **Importing a CLI login** (FI2) reads `gh auth token` or `glab`'s stored token once, when the
  operator asks, and stores it as a keyring account, with section 4's consequences.

### 6. Rights, budgets and audit are properties of the seam (FI14)

- **The budget is per account, shared, and persisted.** The budget middleware reads the forge's
  rate-limit headers into one per-account record in the machine tier. So the window, a `purlis`
  command and `purlis doctor` all see and spend one count, and both transports feed it. A `304`
  is counted as unchanged. Each call's `Caller` priority is what lets the middleware refuse
  **background** requests below FW-4's floor while foreground ones go through. The polling tiers
  and the floor are FW-4's to set.
- **Every write is audited from its `Caller`:** the account, the human, the chat (if any) and the
  surface. The backend marks each operation as a read or a write in its own code. The transport
  never guesses from the HTTP method, because a GraphQL mutation is a `POST` and so is a GraphQL
  query. Until the audit chain exists (AU-1 to AU-3), write entries go to the machine-state log,
  as ADR 0067's audit events do. AU-1 takes them over without dropping any.
- **No automatic comment on a public repo** unless the plane turns it on (X39). This rule sits
  **above the transport**, in the one policy wrapper FW-14 puts around the backends
  (`Guarded<B>`), where the repo's visibility is known from its record. The transport does not
  know what a repo is. The wrapper refuses a comment-shaped write whose `Caller` surface is
  automatic on a public repo, so a new feature cannot forget the rule. The rights check (FI14:
  the UI hides what the account lacks) reads `Reason::NoRight` there too.

### 7. One contract suite, run three ways

The contract tests are written **once**, against the area traits, and instantiated per forge
(`contract!(github)`, `contract!(gitlab)`). Each test is named for the behaviour it checks, for
example `open_or_update_never_opens_a_second_request_for_the_same_pair`. A test that one forge
cannot pass asserts that forge's `Unavailable` and the matching `Support` answer. A test is
never skipped silently.

| Run | Transport | Where | Gates |
|---|---|---|---|
| **Recorded** | native HTTP against a `wiremock` server loaded from recorded exchanges; an unrecorded request fails the test | every PR, no network, no `gh` or `glab` on `PATH`; recordings from github.com, gitlab.com **and a self-managed GitLab** | yes |
| **Live** | native HTTP, then the CLI transport, with the neutral results compared (the CLI is the oracle, FI1) | nightly: a real GitHub repo, a gitlab.com project and a self-managed GitLab (FW-15, FG-4, FG-14) | no; a red nightly keeps an issue open, as the mutation nightly does |
| **CLI argv** | the CLI transport against today's stand-in `gh` and `glab` | every PR | yes |

- **Self-managed GitLab gates every PR** through its own recordings, because FW-2b's acceptance
  names it. The live nightly is on top of that, and is not the only check.
- **Recorded exchanges change only on purpose.** The live run can re-record with
  `CHARTER_FORGE_BLESS=1`, and a token is scrubbed from a recording before it is written. A PR
  that moves a recording names in its body which contract moved and why, as ADR 0046 requires for
  `behaviour.jsonl`.
- **The parity test is what enforces parity.** It fails if a method of an area trait has a
  contract test for one forge and not the other. That is FG-3's "a test per call on both forges",
  made mechanical, and it is the check the missing default bodies only help.
- **The existing argv tests stay** as the CLI transport's own tests. FG-3 moves their callers onto
  the traits, one operation at a time. Each operation gets its recorded contract test in the same
  PR that moves it.

## What changes where

Nothing changes in code with this ADR. Its tickets make these changes:

- **`crates/purlis-core/src/forge/`** gains `backend.rs`, `transport.rs`, `http.rs`, `cli.rs`,
  `github/` and `gitlab/` (FW-2a/b, FG-3). `forge.rs`, `forge/pr.rs` and `forge/checks.rs` shrink
  to the plane-config half (`Kind`, hosts, `[[forge]]` blocks, credential helpers), and their
  forge calls move behind the traits.
- **`crates/purlis-core/docs/forges.md` must change**, because its first paragraph says every
  operation "goes through that forge's own official CLI". FG-3 rewrites it to describe the traits,
  the two transports, the capability flags and the parity table.
- **`crates/purlis-core/Cargo.toml`** gains `ureq`, `gitlab`, `graphql_client`, `secrecy` and the
  conditional-request middleware.
- **`crates/purlis-core/tests/`** gains the contract suite and its recordings. The stand-in CLI
  tests stay.
- **`CONTEXT.md`** gains "Forge account" and "Forge capability", and its "Forge extension" entry
  gains the PE-29 caveat (this PR).
- **Out of scope:** moving `report.rs` off `gh_as_the_operator` is FG-3's, like every other caller.
  Its rule, to file under the reporter's own login and never a token from the environment, becomes
  the CLI transport's default when FG-3 moves it.

## What was rejected

- **A CLI backend beside each native one.** It doubles every operation and gives the nightly no
  single request to compare. The CLI is a transport instead (section 5).
- **Falling back to the CLI when a request fails**, because the retry would run under another
  identity.
- **`reqwest`'s blocking client in the core.** It panics inside an async runtime, which is where
  `purlisd` and the app call from.
- **`octocrab`**, because it is async-only with its own stack. The `gitlab` crate stays, because
  it lets purlis supply the HTTP.
- **One `ForgeBackend` trait holding every operation**, which would grow with every ticket.
- **Hand-rolled caching, budget and logging plumbing** where a standard middleware exists.
- **A default method body on any area trait**, and a GitHub-only method with no GitLab body.
- **Treating an unknown capability as available.**
- **A forge token anywhere but the transport:** an environment, a command line, a log, a record,
  a crash report, an extension request, or any process a chat started. That includes the `purlis`
  binary reading one from the keyring.
- **Forge data in the plane.** The ETag store and the item cache are both in the machine tier
  (FI7).
- **Webhook handling in the request path.**

## What this costs

- **Two HTTP clients' worth of code that `gh` and `glab` used to own:** pagination (the `gitlab`
  crate carries GitLab's), rate limits, GraphQL schemas, GHES and self-managed version
  differences, and token refresh. That code is the price of FI1. The recorded suite and the live
  nightly are what keep it honest.
- **Schemas drift.** Checking against the published schemas at compile time turns a forge's
  rename into a red build on the next schema update. It does not catch an older GHES or
  self-managed GitLab that lacks a field. That is `Reason::HostTooOld` and the self-managed
  recordings.
- **Until `purlisd`'s scopes exist, the CLI keeps the CLI path.** A `purlis` command typed in a
  terminal still needs `gh` or `glab` logged in for forge work, even after the operator signs in
  in the window.
- **An import costs chats their forge writes on that host** until SD-7a/b, if question 1 is ruled
  as recommended.
- **The window alone runs the native client until FD-2 lands.** That puts a network client in the
  app process. ADR 0068 moves it to `purlisd`.

## Ruled (V22, 2026-09-30)

1. **An imported CLI login becomes the human's, and chats lose it.**
2. **The CLI fallback is a transport**, not a backend.
3. **The native clients live in `charter-core`.**
4. **`ureq`, the `gitlab` crate's endpoints, `graphql_client`, and purlis's own serde types for
   GitHub.**
5. **The `purlis` binary uses the CLI transport until the host's human scope exists.**
6. **No purlis git credential helper yet** ([#752](https://github.com/diazoxide/charter/issues/752)).
7. **The transport is chosen at account resolution**, and never switched after a failure.
8. **An `Unknown` capability takes its fallback.**
9. **Forgejo is refused as a kind until FG-13.**
10. **The traits are synchronous.**
11. **The transport's ETag store is separate from FW-7's item cache.**

## Amended by FG-3 (#711, PR #792), 2026-10-01

FG-3 built the seam's first areas, `Repos` and `Requests`, and moved every caller onto them,
with the CLI transport as the only transport until FW-2a/b. Five things departed from the text
above. Each holds until the ticket named, and the text above is left as accepted.

1. **Errors stay as they were, until FW-2a/b's closed error set.** §1 names a closed `ForgeError`
   enum (`Auth`, `Forbidden`, `NotFound`, `RateLimited`, …). The CLI transport cannot tell those
   apart except by parsing a CLI's words, which purlis does not do to decide anything. So the
   strict methods of both traits fail with today's `ForgeError(String)`, unified across `Repos`
   and `Requests`. The permissive pair keeps `Raised`, and `checks_at` keeps `UNKNOWN` with its
   reason. The enum arrives with the native transport, which has HTTP statuses.
2. **`report.rs` stays on `gh` until FW-6a (#733).** *Out of scope* above gives that move to FG-3.
   `purlis report` files on purlis's own tracker, which is on GitHub whatever forge a project
   uses, so it has no GitLab twin to hold parity with, and its natural home is the work-item area.
   It keeps `forge::gh_as_the_operator` until #806 moves it there.
3. **The body goes as `-f` and `-F` fields until the native transport.** §5 sends the CLI
   transport's body on stdin. FG-3's CLI transport sends exactly the argv purlis sent before,
   because the recorded Python behaviour (ADR 0046) and the argv tests pin it. A `Call`'s fields
   are literal (`-f`) or typed (`-F`), and only purlis's own values are ever typed, which keeps
   #323 closed. The move to stdin comes with FW-2a/b, which moves those recordings on purpose.
4. **The recorded run uses a `Recorded` transport until FW-2a's `wiremock`.** §7's recorded run
   serves native HTTP from `wiremock`. With no native transport yet, FG-3's contract suite runs
   every method through `forge::recorded::Recorded`, which answers a `Call` from a JSON recording
   and fails one nobody recorded. The recordings are taken from each forge's API documentation
   (GitHub REST `2022-11-28` and GraphQL; GitLab 19.4), and each names its source. A
   self-managed GitLab recording is still missing; FW-15 (#742) records it.
5. **`Repos` speaks `serde_json::Value` until a `RepoRecord` type exists.** §1's sketch has
   `reachable(&self, caller, owner: &Owner) -> Vec<RepoRecord>`. The neutral record the inventory
   already writes is a JSON object with forge-independent keys, so `owned` and `reachable` return
   `Vec<Value>` in that shape and take the owner as `&str`. FW-5 defines the typed record.
   *Since ADR 0088 (V40): [#857](https://github.com/diazoxide/charter/issues/857), split out of
   FW-5, defines it.* *#857 defined it: `owned` and `reachable` take an `Owner` and return
   `Vec<RepoRecord>`, and `top_level` takes a `RepoRecord`. The inventory's keys are unchanged.*
   *#911: `about` takes a `RepoRecord`; `inventory::read` reads a row as one.*

`Caller` carries the surface and the priority only. The account, the principal and the human join
it with FW-1 and FD-27, and until then every `Caller` resolves to the CLI transport, as §4 says.

## Amended by FW-2a (#727, PR #800), 2026-10-01

FW-2a built the native GitHub transport behind FG-3's seam. Four things depart from the text
above, or from FG-3's amendment. Each holds until the ticket named, and the text above is left as
accepted.

1. **The CLI transport gets no conditional requests; FW-4 (#731) decides.** §3 says the CLI
   transport runs the same middleware chain as the native one, because `--include` gives it the
   status and headers. That needs `gh api --include`, which changes every argv the recorded
   Python behaviour pins (ADR 0046). So only the native transport makes a `GET` conditional, and
   its ETag store is the native transport's alone, keyed by the account it sends as. FW-4 is the
   first ticket that needs the CLI's headers, for the budget, and it decides whether to make that
   ADR 0046 move.
2. **The body stays as `-f` and `-F` fields.** FG-3's amendment 3 left the move to stdin to
   FW-2a/b. It is not made: the CLI transport is FG-3's, unchanged, and the native transport reads
   a `Call`'s fields as `gh api` reads them, so one request means the same on both. #323 stays
   closed because only purlis's own values are ever typed.
3. **The closed error set is a kind beside the words.** FG-3's amendment 1 kept
   `ForgeError(String)` until the native transport. `ForgeError` now carries a `Failure` from §1's
   set beside its words, and `Display` is the words alone, so every pinned message stays. The
   native transport reads the kind from the HTTP status. A CLI refusal is `Unrecognised`,
   because purlis does not parse a CLI's sentence to decide anything.
4. **Zeroizing the token is best effort: two copies live for one request.** §4 says the token is
   a `SecretString`, zeroized on drop, and that holds. The `Bearer …` string built from it is a
   `Zeroizing` string, wiped on drop too. Two copies are beyond purlis's reach: the
   `http::HeaderValue`, whose bytes `http` keeps in a buffer it frees without wiping, and
   `ureq`'s write buffer. Both live for the one request they carry.

A chat's sandbox denies the ETag store to read and to write (ADR 0067's human-powers class),
because its answers were fetched with the human's token.

## Amended by FW-2b (#728), 2026-10-03

FW-2b built the native GitLab client on FW-2a's transport. Three things depart from the text
above. Each holds until the ticket named, and the text above is left as accepted.

1. **GitLab REST does not use the `gitlab` crate (ruling 4, amended by the operator's V72).**
   The crate's endpoint builders are behind its `client_api` feature, which brings in
   `reqwest`, the client "Rejected" names for the core. And FG-3's GitLab bodies are already
   `Call`s, built once, whose recordings and `glab api` argv are pinned (ADR 0046). So GitLab's
   REST bodies are purlis's own, as GitHub's are, sent over the same `Transport`, and
   pagination is the short-page loop both backends use. If the crate ever offers its endpoints
   without `reqwest`, the move is a rewrite of how a `Call` is built, and the recordings say
   whether it changed what is asked.
2. **GitLab's GraphQL documents are not checked at compile time yet** ([#1031](https://github.com/diazoxide/charter/issues/1031)).
   GitLab publishes no schema file to vendor, as GitHub does; the schema is read by
   introspection, and part of it describes GitLab's Enterprise Edition, whose licence is not
   GitLab's MIT one. Until #1031 settles that, the three GitLab documents (a work item's
   children, setting its parent, setting an issue's iteration) are constants that the recorded
   tests pin, and FW-15's live nightly is what catches a renamed field.

   **Settled by the operator's V80 (2026-10-03), recorded by #1031 on 2026-10-10.** The schema
   to vendor is GitLab Community Edition's (MIT), and a document that names an Enterprise Edition
   field stays a hand-checked constant, pinned word for word by a recording. Those documents are:

   - the work item read, `read::WORK_ITEM` and `read::WORK_ITEM_WITHOUT_STATUS`: their iteration
     and status fragments name EE widget types. Recorded in
     `tests/forge_contract/gitlab/read.json` and `read.self_managed.json`;
   - setting an issue's iteration, `work::SET_ITERATION` (`issueSetIteration`). Recorded in
     `forge/gitlab/work_tests.rs` (`RECORDED_SET_ITERATION`).

   The CE documents, a work item's children (`work::CHILDREN`) and setting its parent
   (`work::SET_PARENT`), are the ones to check against the vendored schema. GitLab stopped
   publishing a schema file in the FOSS repository after 13.9
   (`doc/api/graphql/reference/gitlab_schema.graphql`), before work items came in 15.1, so the
   CE schema has to be introspected from a CE release at a named tag. Until it is, those two are
   constants too, recorded word for word in `forge/gitlab/work_tests.rs`.
3. **Epics use v4's REST epics endpoints.** GitLab deprecated them in 17.0 in favour of work
   items and still serves them in v4. FW-6b, which maps epics onto the neutral model, moves them
   to work items ([#1032](https://github.com/diazoxide/charter/issues/1032)).

## Amended by FW-4 (#731), 2026-10-04

FW-4 built the request budget per account (`forge::budget`) and the polling policy
(`forge::poll`). Six things depart from the text above, or settle what it left open. The text
above is left as accepted.

1. **The budget is a `Transport` decorator, not a `ureq` middleware (D-FW4a).** §3 lists the
   budget third in the native transport's middleware chain. A middleware inside `Http` sees only
   the native route, and §6 says both transports feed one count. So `Resolver::for_caller` wraps
   whichever transport it resolves, native or CLI, in the account's `Metered` transport, which
   admits and counts every `send` and `check_auth`. Only the resolver builds the native transport
   outside tests, and a guard test holds that
   (`tests/only_the_resolver_builds_the_native_transport.rs`).
2. **FW-2a's amendment 1 is settled: the move is not made (D-FW4c).** The CLI transport keeps its
   argv without `--include` (ADR 0046), so it makes no conditional request, and a CLI call made as
   an account is counted but tells the budget nothing of the forge's limit. Polling is the native
   route's.
3. **The floor is two thresholds (D-FW4d).** §6 says the budget refuses background requests below
   FW-4's floor. Below 20% of the forge's own limit remaining, polling **backs off**: every
   interval is four times longer. A background request is **held back** only once purlis's own
   allowance, 1,000 counted requests an hour per account, is spent; it fails as `RateLimited`
   with the hour's end as its reset. Holding back at 20% would stop polling, not back it off.
4. **What a `304` costs depends on the forge.** §6 counts a `304` as unchanged. GitHub does not
   count a `304` to an authorized conditional request against its primary rate limit. GitLab's
   rate-limit documentation throttles API requests and makes no exception for a `304`, so it
   counts as one request there. The budget counts each as its forge does; polling paces itself on
   every request sent, a `304` included, because GitLab counts one and GitHub's secondary limits
   count requests whatever their answer.
5. **A call that names no account has no budget (D-FW4b).** It goes over the CLI with the CLI's
   own login, not a purlis account. A chat's or a `purlis` command's call that names an account
   takes the CLI route (§4), so it spends the CLI's login, and is counted against the account it
   named.
6. **Only a person is foreground (D-FW4j).** For admission, a call whose principal is a chat, or
   that came over MCP, is background whatever priority it carries. A looping agent is held back
   once the hour is spent, and cannot starve the person's own refreshes while never being held
   itself.

Nothing in the app drives the poller or a resolver yet: FW-7 (#735) wires the window's state
into `forge::poll` and keeps the budgets in the machine tier, where `purlis doctor` and the
app's doctor show them.
