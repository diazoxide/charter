# A forge is one seam with a native client per forge, and `gh` and `glab` are its fallback

**Proposed 2026-09-30.** An agent drafted it for program-map ticket FG-1 (#710). The operator
rules it (W7: an ADR merges only after the operator's ruling). It follows these of the
operator's rulings:

- **The forge amendment:** GitHub and GitLab are both first-class now. Every feature that
  touches a forge ships for both, behind one forge seam.
- **X10:** forge parity comes from one shared seam, with contract tests for both forges in CI.
- **FI1 to FI14** (2026-09-30), and FI1 above all: native API clients in the Rust core (REST and
  GraphQL for GitHub and GitLab, Forgejo REST later). They sit behind the forge seam, and the
  CLIs stay as a fallback and for tests. FI1 re-cuts this ticket from "wrap `gh` and `glab`" to
  "a native client per forge". FI2 (sign-in), FI3 (the human's token never reaches an agent),
  FI4 (the neutral work model), FI7 (a derived cache) and FI14 (budgets, rights and audit) set
  what the seam must carry.
- **V13:** webhooks come later, through the relay.
- **V16c:** per-agent GitHub tokens come from a GitHub App that each organisation registers
  itself. charter's shared GitHub App is for sign-in only.

It builds on [ADR 0055](0055-a-workspaces-repos-are-picked-from-what-your-own-forge-login-reaches.md)
(the repo picker asks as the operator's own login) and
[ADR 0047](0047-a-vault-lives-in-the-system-keyring-by-default.md) (the keyring). It keeps to
[ADR 0067](0067-a-chat-runs-in-a-sandbox-charter-compiles-for-its-harness.md) (the chat
sandbox) and to the proposed ADR 0068 (`charterd`, the session host, and its client scopes).
The tickets that build on it are the native clients (FW-2a and FW-2b), sign-in (FW-1, FW-3a and
FW-3b), the parity audit (FG-3), the test harness (FG-4 and FW-15), capability detection and its
card (FG-2 and FG-16), the budget (FW-4), the cache (FW-7), audited writes (FW-14), per-agent
tokens (SD-7a and SD-7b), and every `a`/`b` twin after them.

## Where charter is today

**Every forge call runs the forge's own CLI.** `crates/charter-core/src/forge.rs` finds `gh` or
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

`report.rs` files on charter's own tracker through `gh_as_the_operator`, which withholds every
`GH_TOKEN`-style variable so that the issue is filed under the operator's own login.

**There is no seam.** Each function matches on `Kind` and builds `gh api` or `glab api`
arguments inline. Parity is a matter of discipline, and so is the choice between a strict read
and a permissive one (`crates/charter-core/docs/forges.md`). The tests pin argv:
`a_forge_cli_is_asked_exactly_what_python_asked.rs` and
`a_pr_is_opened_or_updated_and_set_to_auto_merge.rs` run a stand-in `gh` and `glab`
(`tests/support/forge_cli.rs`) that answers only the exact questions a test wrote down.

**Git's own credential is separate.** A clone's credential helper is `gh auth git-credential`
or `glab`'s own (`Forge::credential_helper`, `helper_for`). An agent in a chat pushes through the
same clone, so today a chat reaches the operator's CLI login. That is the gap SD-7a and SD-7b
close. This ADR does not widen it.

**What FI1 to FI14 need, and running a CLI cannot give:** a sign-in charter owns (FI2), with no
CLI installed; conditional requests and a per-account budget (FW-4); a write audit that knows
which chat and which human it was for (FW-14); a network log entry per call (OB-15); GraphQL for
sub-issues, Projects v2, epics and iterations (FI4); and a test that runs with no CLI on `PATH`
(FW-2a and FW-2b's acceptance).

## The decision

**charter talks to a forge through one trait, `ForgeBackend`. There is one implementation per
forge kind, and it is native code in charter's Rust core, speaking REST and GraphQL. Each
implementation sends its requests through a `Transport`. The native transport is HTTPS with a
token charter holds. The fallback transport is the forge's own CLI (`gh api`, `glab api`)
with that CLI's own login. Which transport an account uses is chosen once, when the account is
resolved, and never switched after a request has failed. What a forge can do is a set of
capability flags, one set per forge, host and tier. One contract suite, written once, runs
against both forges.**

### 1. One trait, neutral types, no default methods

`ForgeBackend` lives in `crates/charter-core/src/forge/backend.rs`. It speaks only neutral types:
FI4's work model (FW-5 defines the types), plus the types the core already has (a repo record, a
request, `Checks`, `AutoMerge`). A forge's own identifiers (a GitHub node id, a GitLab global id
or `iid`) travel inside a neutral type as an opaque `ForgeRef`, so that a round trip never
re-derives them.

The shape (the method names are FG-3's and FW-2a/b's to fix, and the groups are this ADR's):

```rust
pub trait ForgeBackend {
    fn kind(&self) -> Kind;
    fn capabilities(&self, at: &Scope) -> Result<Capabilities, ForgeError>;   // section 2

    // identity: whoami, the auth check, the budget left (FW-4)
    // repos: list an owner's, list what this account reaches (ADR 0055), a repo's tree
    // requests: open or update, state, by head, request auto-merge, checks at a sha
    // work items (FW-6a/b): items, milestones, iterations, boards, labels, links
    // later, each added with both forges in one milestone: reviews and threads (FG-5a/b,
    //   FW-16a/b), uploads (FG-10b), notifications (FW-13)
}
```

- **The trait has no default method bodies.** A method added for a GitHub ticket does not
  compile until the GitLab backend answers it too, either with an implementation or with
  `Err(ForgeError::Unsupported(capability))` naming the capability it lacks. That is how X10's
  "one seam" is kept by the compiler rather than by review. W7's rule, that a GitLab twin may
  ship one release after its GitHub twin, is kept the same way: the twin's first answer is an
  explicit `Unsupported`, and the capability card (FG-16) says so in the product.
- **The trait is synchronous.** `charter-core` is plain blocking Rust today, and each call is a
  handful of requests. A host that wants concurrency runs calls on a blocking pool. An async
  trait is a separate decision, made if `charterd` (ADR 0068) needs one.
- **Errors are neutral and closed:** `Auth` (the token is refused), `Forbidden` (the account
  lacks the right), `NotFound`, `RateLimited { reset }`, `Conflict`, `Unsupported(capability)`,
  `Transport` (could not ask), and `Unrecognised` (an answer charter does not understand). Each
  carries the forge's own words. The **strict** and **permissive** disciplines in
  `docs/forges.md` stay with the caller: the trait always returns the error, and a permissive
  caller (`gl-refresh`) is the one that turns it into "nothing".
- **Forgejo has its slot now.** The kind word `forgejo` is reserved and means nothing else. Its
  backend, and the `Kind::Forgejo` variant with it, are FG-13's. Until FG-13 there is no Forgejo
  backend at all, rather than one that answers `Unsupported` to everything: a plane that declares
  `kind = "forgejo"` is refused, as an unknown kind is today, with a message that names FG-13's
  backend as not yet built.

### 2. Capability flags are per forge, host and tier, and silence never reads as "yes"

A **forge capability** is one thing a forge may or may not do for one repo, such as a merge
queue. It is not an extension's **capability** (CONTEXT.md), and the word is always qualified.

```rust
pub enum Support {
    Available,
    Missing { fallback: Fallback },          // the forge or this host has no such feature
    NeedsTier { tier: String, fallback: Fallback },   // GitLab Premium, a GitHub org or plan
    NotAllowed { fallback: Fallback },       // present, but this account lacks the right
    Unknown { why: String },                 // charter could not find out
}
```

- **The set of flags, to start:** merge queue (GitHub), merge train (GitLab), auto-merge,
  native stacked changes, per-agent tokens (an organisation's own GitHub App; GitLab project or
  group access tokens), comment uploads (GitLab yes; GitHub has no API for them, FG-10a),
  required commit signatures, GraphQL, sub-issues, issue types, epics, iterations, boards
  (Projects v2 or GitLab boards), dependencies (blocked-by), and check runs as opposed to commit
  statuses or pipelines.
- **A flag is a function of four things:** the forge kind; the host and its version (github.com,
  a GitHub Enterprise Server version, gitlab.com, a self-managed GitLab version); the tier of
  the owner (a GitHub organisation or personal account and its plan, or a GitLab namespace's
  plan, or a self-managed licence); and the account's rights on the repo. `Scope` names which of
  these a question is about: instance, owner or repo.
- **Detection reads what the forge reports, and probes the feature when it does not report
  it.** For example, a self-managed GitLab does not show its licence to a non-admin, so charter
  asks for an epic and reads the refusal. FG-2 writes the probes. Results are cached in FW-7's
  cache with an expiry, and `charter doctor` and the capability card (FG-16) read that cache.
- **`Unknown` is never treated as `Available`.** This is `checks.rs`'s rule ("no word for
  silence reads as passing"), applied to features. A feature whose flag is unknown takes its
  fallback, and the card says why.
- **Every `Missing`, `NeedsTier` and `NotAllowed` names its fallback** (W10): for example, no
  merge queue falls back to auto-merge plus charter's own landing order (ADR 0060), and no epics
  on GitLab Free falls back to labels and milestones (FW-6b). The UI hides what the account
  lacks the right to do (FI14) by reading `NotAllowed`.

### 3. The native clients live in `charter-core`, over one transport

```
crates/charter-core/src/forge/
  backend.rs     the trait, Scope, Support, ForgeError, ForgeRef
  transport.rs   Transport: one request in, one response out; the budget, audit and log
  http.rs        the native transport: reqwest, rustls, a token from a TokenSource
  cli.rs         the fallback transport: today's `call_with`, sending `gh api` / `glab api`
  github/        GitHubBackend: REST and GraphQL (FW-2a)
  gitlab/        GitLabBackend: REST and GraphQL (FW-2b)
```

- **`charter-core`, not a new crate.** Every caller is in the core already (`discover`, the
  picker, saves, changes, `gl-refresh`), and the core never depends on Tauri or the UI (CLAUDE.md),
  which the clients do not need. When PE-30a/b move the clients behind the public extension API,
  the `github/` and `gitlab/` directories are what moves. Open question 2 asks about a separate
  crate.
- **Standard crates, used as they are meant to be used.** HTTP goes through `reqwest` with
  `rustls`, which is already in the lockfile through the app. GraphQL queries are written as
  `.graphql` files and checked against each forge's published schema by `graphql_client` at
  compile time, so a renamed field fails the build, not a user. REST bodies are `serde` types.
  charter does **not** adopt `octocrab` or the `gitlab` crate. Each of them owns its own HTTP
  stack, and the budget, audit, network log and recorded tests below all need one transport
  that charter owns. Open question 3 asks about this.
- **Every request goes through `Transport::send`, and that is the one place that:**
  - adds the credential (section 4) and nothing else of charter's;
  - sends `If-None-Match` from FW-7's cache and reports `304 Not Modified` as "unchanged";
  - reads the rate-limit headers into the account's budget (FW-4), and refuses background
    requests below FW-4's floor;
  - writes an entry to the network log (OB-15): the host, method, path template, status and
    timing, and never a body or a header value;
  - writes an audit entry for every request the backend marks as a write (section 6).
- **The backends hold no state of their own.** The cache (FW-7) sits above the trait. It reads
  through the backend, stores what came back in the Machine tier (FR-30), and never becomes a
  second source of truth (FI7). Writes go straight to the forge.
- **Webhooks do not enter the trait (V13).** Push input goes to the trigger-source seam (AC-18)
  after GT-CLOUD. When FG-12a/b land, each backend gains one pure function that turns a verified
  payload into neutral events, and that function makes no requests.

### 4. Credentials reach the client, and never an agent

- **A backend is built for one account.** A forge account is one sign-in: a forge kind, a host,
  and a login. There can be several per forge (github.com, a GHES, gitlab.com, self-managed),
  and each repo is bound to one account (FI2). An account resolves to a `TokenSource`, not to
  a string. A `TokenSource` hands out a token when a request is sent, refreshes an expiring one
  (GitHub App user tokens and GitLab OAuth tokens both expire), and writes the refreshed token
  back to the keyring. FW-1 decides the flows and scopes. This ADR decides that the seam takes
  a source, so that no caller ever holds the token.
- **Tokens are keyring items**, service `charter/forge/<host>/<account>`, stored the same way as
  a vault's (ADR 0047). They are covered by ADR 0067's first denial class: a chat's sandbox
  denies them, as it denies a vault's storage.
- **A token lives only inside the transport.** It is a wrapper type with no `Debug`, `Display`
  or `Serialize`. It is read when a request is sent, set as the `Authorization` header, and
  dropped. It never goes on a command line or into a child's environment, and it is never logged
  or written to the plane.
- **Only a human client runs a backend with the sign-in token.** The window runs it. After FD-2
  and FD-27, `charterd` runs it, for a connection on a human client scope. `charterd` refuses
  that scope to a chat's process tree (V16a). The `charter` binary **never** reads a forge token
  from the keyring itself, because a `charter` a chat runs is part of that chat (ADR 0067
  section 2). Until `charterd`'s scopes exist, a `charter` command resolves every account to the
  CLI transport (section 5). That is exactly what it does today.
- **A chat gains no forge power from this ADR (FI3).** A forge call made for a chat (a `charter`
  command it runs, or an MCP tool it calls) uses the chat's own identity. Until SD-7a/b, that
  identity is whatever the chat reaches today through the CLI transport. After SD-7a/b, it is a
  per-agent token that `charterd` mints for that chat: from the organisation's own GitHub App
  (V16c; charter's shared App mints nothing), or from a GitLab project or group access token.
  SD-7a/b's per-agent token is one more `TokenSource`, used through the same trait, and never the
  human's.
- **Git's credential path is not part of the seam.** Clones keep `gh`'s or `glab`'s credential
  helper. A charter-provided git credential helper would run inside a chat whenever the chat
  runs `git push` in that clone, so none is added. Open question 5 asks whether it is wanted once
  `charterd` can refuse it to a chat.
- **Extensions still never receive the token.** CONTEXT.md's "Forge extension" stays true. When
  PE-29 opens the seam to extensions, an extension asks charter to make a call, and charter makes
  it through this trait and audits it. No token is handed over.

### 5. The fallback is a transport, so each operation has one implementation per forge

`gh api` and `glab api` each send an arbitrary REST or GraphQL request (`--method`, `--header`,
`--input -`, `--include`, `--hostname`) and authenticate with the CLI's own login. So the
fallback is a second `Transport`, and not a second backend. `GitHubBackend` builds one request,
and either transport sends it. `cli.rs` is today's runner, kept as it is: the pinned path, the
emptied environment, the withheld `TOKEN_ENV`, and the body on stdin instead of `-f` fields,
which also removes #323's `@` hazard by construction.

- **When an account uses the CLI transport:** there is no charter sign-in for that host but the
  CLI is logged in there; or the caller is a `charter` command or a chat, before `charterd`'s
  scopes exist (section 4); or the operator chose it for the account in settings. Doctor shows
  each account's transport.
- **It is chosen at resolution, never on failure.** A `401` or `403` from the native transport
  is reported as `Auth` or `Forbidden` for that account, and charter does not retry the request
  through the CLI. Retrying would repeat the request under a different identity, possibly a
  different human's, and a write would then be audited under the wrong login.
- **Importing a CLI login (FI2)** reads `gh auth token` or `glab`'s stored token once, when the
  operator asks, and stores it as a keyring account. From then on that account uses the native
  transport. An imported login is the human's token, with every rule in section 4 (SD-7b).
- **`report` moves onto the trait** under the operator's account. `gh_as_the_operator`'s rule, to
  withhold `GH_TOKEN` and file under the reporter's own login, becomes the CLI transport's
  default, applied to every call and not only to this one.

### 6. Rights, budgets and audit are properties of the seam (FI14)

- **Every write carries a `Caller`:** the account, the human on whose behalf it is made, the chat
  (if any), and the surface (the window, the CLI, MCP, a trigger). The backend marks each
  operation as a read or a write in its own code. The transport never guesses from the HTTP
  method, because a GraphQL mutation is a `POST` and so is a GraphQL query. Until the audit chain
  exists (AU-1 to AU-3), write entries go to the machine-state log, as ADR 0067's audit events
  do. AU-1 takes them over without dropping any.
- **No automatic comment on a public repo** unless the plane turns it on (X39). The transport
  refuses such a write when the `Caller`'s surface is automatic, so a new feature cannot forget
  this rule.
- **The budget is per account**, kept by the transport from the forge's rate-limit headers, and
  shown in doctor (FW-4). A `304` is counted as unchanged. Polling tiers and the back-off floor
  are FW-4's to set.

### 7. One contract suite, run three ways

The contract tests are written **once**, against `&dyn ForgeBackend`, and instantiated per forge
(`contract!(github)`, `contract!(gitlab)`). Each test is named for the behaviour it checks, for
example `open_or_update_never_opens_a_second_request_for_the_same_pair`. A test that one forge
cannot pass asserts that forge's `Unsupported(capability)` and the matching `Support` flag. A test
is never skipped silently.

| Run | Transport | Where | Gates |
|---|---|---|---|
| **Recorded** | native HTTP against a `wiremock` server loaded from recorded exchanges; an unrecorded request fails the test | every PR, no network, no `gh` or `glab` on `PATH` | yes |
| **Live** | native HTTP, then the CLI transport, with the neutral results compared (the CLI is the oracle, FI1) | nightly: a real GitHub repo, a gitlab.com project and a self-managed GitLab (FW-15, FG-4, FG-14) | no; a red nightly keeps an issue open, as the mutation nightly does |
| **CLI argv** | the CLI transport against today's stand-in `gh` and `glab` | every PR | yes |

- **Recorded exchanges change only on purpose.** The live run can re-record with
  `CHARTER_FORGE_BLESS=1`, and a token is scrubbed from a recording before it is written. A PR
  that moves a recording names in its body which contract moved and why, as ADR 0046 requires for
  `behaviour.jsonl`.
- **Parity is checked, not assumed.** A test in the suite fails if a trait method has a contract
  test for one forge and not the other. That is FG-3's "a test per call on both forges", made
  mechanical.
- **The existing argv tests stay** as the CLI transport's own tests. FG-3 moves their callers onto
  the trait, one operation at a time. Each operation gets its recorded contract test in the same
  PR that moves it.

## What this rules out

- A second backend per forge that wraps the CLI. The CLI is a transport, and each operation has
  one implementation per forge.
- Falling back to the CLI after a request has failed.
- A forge token in any environment, command line, log, plane file or extension request, or in
  any process a chat started.
- The `charter` binary reading a forge token from the keyring.
- A `ForgeBackend` method with a default body, and a GitHub-only method with no GitLab answer.
- Treating an unknown capability as available.
- Forge data in the plane. The cache is in app data (FI7).
- Webhook handling inside the backend's request path.

## What this costs

- **Two HTTP clients' worth of code that `gh` and `glab` used to own:** pagination, rate limits,
  GraphQL schemas, GHES and self-managed version differences, and token refresh. That code is the
  price of FI1. The live nightly is what keeps it honest.
- **Schemas drift.** Checking against the published schemas at compile time turns a forge's
  rename into a red build on the next schema update. It does not catch an older GHES or
  self-managed GitLab that lacks a field. That is a capability flag (the host's version) and a
  nightly on a self-managed host (FG-14).
- **Until `charterd`'s scopes exist, the CLI keeps the CLI path.** A `charter` command typed in a
  terminal still needs `gh` or `glab` logged in for forge work, even after the operator signs in
  in the window. That is the price of never letting a chat's `charter` read the token.
- **The window alone runs the native client until FD-2 lands.** That puts a network client in the
  app process. ADR 0068 moves it to `charterd`.

## For the operator's ruling

These calls go beyond the words of FI1 to FI14, X10, V13 and V16c, and each is worth a yes or a
no:

1. **The CLI fallback is a transport, not a backend** (section 5): one GitHub implementation and
   one GitLab implementation, each sendable through `gh api` or `glab api`. The alternative, a
   CLI backend beside each native one, doubles the parity work FG-3 does and has no single
   oracle to compare against.
2. **The native clients live in `charter-core/src/forge/`** (section 3). The alternative is a
   `charter-forge` crate that the core depends on, which would make the later move to an
   extension (PE-30a/b) a crate move, at the cost of one more crate now.
3. **`reqwest` plus `graphql_client`, not `octocrab` and the `gitlab` crate** (section 3), so
   that one transport charter owns carries the budget, the audit, the network log and the
   recorded tests. The alternative uses the two community clients and wraps each one's HTTP
   stack, which `octocrab` allows through a tower layer and the `gitlab` crate does not in the
   same way.
4. **The `charter` binary never reads a forge token, and uses the CLI transport until
   `charterd`'s human scope exists** (section 4). The alternative lets a `charter` run outside a
   chat use the keyring token. charter cannot tell those two runs apart before FD-27, which is
   why this draft says no.
5. **No charter git credential helper now** (section 4). Clones keep `gh`'s and `glab`'s helpers,
   so pushing still needs a CLI login. A charter helper would let a signed-in operator push
   without a CLI. It is safe only once `charterd` refuses it to a chat's process tree, so it would
   be a later ticket after FD-27, if it is wanted at all.
6. **Fallback is chosen at resolution and never on failure** (section 5), even for a read.
7. **`Unknown` takes the fallback** (section 2), so a self-managed host charter cannot probe
   behaves as its lowest tier until the probe answers.
8. **Forgejo is refused as a kind until FG-13**, rather than accepted with a backend that
   answers `Unsupported` to everything (section 1).
9. **The trait is synchronous** (section 1). `charterd` runs its calls on a blocking pool.
