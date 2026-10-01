# A forge sign-in is the human's, through one registration per forge host, and it mints nothing for an agent

**Accepted 2026-10-01** by the operator (ruling V30a), with dispatcher decisions D-0077a/b/c,
drafted for program-map ticket FW-1 (#726). It follows these of the
operator's rulings:

- **FI2:** *"Charter's own forge sign-in: GitHub OAuth device flow (no client secret), GitLab
  OAuth with PKCE, a PAT for self-managed; an existing `gh`/`glab` login can be imported on first
  run. Tokens in the vault/keyring; minimum scopes, each explained on the settings page; several
  accounts per forge (github.com, GHES, gitlab.com, self-managed), one chosen per repo."*
- **FI3:** *"The human's UI token never reaches an agent; agents keep their own narrow identity
  (SD-7a/b, gap G3), and gain no new power from this feature before those land."*
- **V13**, in part: *"The FI2 GitHub registration is a GitHub App with device flow, so one
  registration later serves webhooks and SD-7a."*
- **V16b:** *"Vault enforcement moves into `charterd`: it resolves secrets and holds the V15
  approval gate, and the chat sandbox denies direct access to the vaults, so approval is
  enforced, not advisory."*
- **V16c**, in part: *"Per-agent GitHub tokens (SD-7a) come from a GitHub App the organisation
  registers itself, with charter guiding the setup; […] charter's shared GitHub App (FW-1) is for
  sign-in only and its private key never ships."*
- **V22c**, in part: *"an imported CLI login becomes the human's and chats lose it; the CLI
  fallback is a transport; […] the `charter` binary uses the CLI transport until the host's human
  scope exists; no charter git credential helper yet (#752)"*.
- **V9**, in part: *"PRs are opened from the laptop, so the runner needs no forge credential (FI3
  by construction)"*.
- **V14**, in part: the community launch requires *"FW's core (sign-in, Work view, issue → chat →
  PR)"*.
- **GH2**, in part: the repo move brings *"org-installed GitHub Apps (SD-7a)"*.

**V16c narrows V13.** V13 chose a GitHub App so that one registration would later serve both
webhooks and SD-7a. V16c is later and moves SD-7a to an App each organisation registers itself.
This record follows V16c: charter's App serves sign-in now, and webhooks when FG-12a decides them.
It serves no agent, ever.

It builds on [ADR 0070](0070-a-forge-is-one-seam-with-a-native-client-per-forge-and-gh-and-glab-are-its-fallback.md)
(the forge seam, the `Caller`, the `TokenSource` and the keyring item), [ADR 0047](0047-a-vault-lives-in-the-system-keyring-by-default.md)
(the keyring), [ADR 0067](0067-a-chat-runs-in-a-sandbox-charter-compiles-for-its-harness.md)
(the chat sandbox and its denial classes) and [ADR 0072](0072-charter-has-five-concepts-and-every-other-word-belongs-to-one-of-them.md).
It amends [ADR 0055](0055-a-workspaces-repos-are-picked-from-what-your-own-forge-login-reaches.md),
[ADR 0066](0066-a-chat-is-a-ulid-a-run-is-a-stretch-of-its-conversation-and-a-device-is-random.md),
[ADR 0068](0068-a-chat-lives-in-charterd-and-the-app-is-its-client.md),
[ADR 0069](0069-every-store-charter-writes-is-in-one-of-four-tiers.md), ADR 0070 and
[ADR 0075](0075-an-audit-entry-is-metadata-in-a-store-of-its-own-and-telemetry-never-reads-it.md),
each in a section of its own below. FW-3a (GitHub sign-in) and FW-3b (GitLab sign-in) build it,
and FW-14, SD-7a and SD-7b build on it.

Its concept is **Project**. A forge account and a forge registration hold for the whole machine,
across every workspace and every project on it, and ADR 0072 §2 puts machine-wide things under
Project (*"Machine-wide things belong to Project. Settled by X22 and ST6."*). The rest of the forge work stays under Workspace (FI13). The settings page shows
the accounts.

## Where charter is today

**charter has no sign-in of its own.** Every forge call runs `gh` or `glab` with that CLI's own
login (ADR 0070, "Where charter is today"). A person with neither CLI logged in gets no forge
features, and the repo picker says `gh auth login` (ADR 0055).

**ADR 0070 fixed the seam a sign-in plugs into, and left the sign-in to this record.** It decided:

- a **forge account** is one sign-in: a kind, a host and a login; several per forge; each repo
  bound to one;
- an account resolves to a `TokenSource`, which refreshes an expiring token and writes it back to
  the keyring, so no caller holds a token;
- the token is a keyring item, service `charter/forge/<host>/<account>` (renamed in "ADR 0070,
  amended" below), in ADR 0067's first
  denial class;
- only a human `Caller` resolves to it; the `charter` binary never reads it; a chat never
  resolves to it;
- an imported CLI login becomes the human's, and from then on a chat gets no forge credential on
  that host until SD-7a/b (ruled, V22c);
- *"FW-1 decides the flows and scopes."*

**Nothing is registered anywhere.** charter owns no GitHub App and no GitLab OAuth application.
Neither `docs/plane-format.md` nor ADR 0069's table has a row for a forge account or its token.

**Four facts about the forges shape the decision:**

- **A GitHub App's user token reaches only what both the person and the App can reach.** GitHub's
  documentation: *"The app can only access resources in an account where it is installed."* An
  App used for sign-in must be installed on each organisation whose private repos the person
  works on. An OAuth App, which `gh` uses, has no such step.
- **A GitHub App's device flow needs no client secret, and neither does refreshing its token.**
  The refresh endpoint's `client_secret` is *"Required unless the user access token was generated
  using the device flow."* The user token expires after 8 hours, and its refresh token after six
  months.
- **GitLab's authorization code flow with PKCE needs no client secret** for an application that is
  not marked confidential. Its access token expires after two hours and refreshes without one. The
  redirect URI must match one the application registered.
- **A registration belongs to one host.** charter's GitHub App exists on github.com only, and an
  application registered on gitlab.com exists there only. A GHES or a self-managed GitLab needs a
  registration made on that host.

## The decision

**A forge account is signed in by the human, in the window, through one registration per forge
host: charter's own on github.com and gitlab.com, and one made on the host for a GHES or a
self-managed GitLab, with a PAT or an imported CLI login where there is none. The flows are
public-client flows, so charter holds no client secret for sign-in anywhere. The token is a
keyring item that only the process holding the human scope reads and refreshes. charter's
registrations mint nothing for an agent: an agent's forge identity comes only from SD-7a/b.**

### 1. One registration per forge host, and charter's own hold no secret

**Settled by FI2** (*"GitHub OAuth device flow (no client secret), GitLab OAuth with PKCE, a PAT
for self-managed"*), **V13** (*"The FI2 GitHub registration is a GitHub App with device flow"*)
and **V16c** (*"charter's shared GitHub App (FW-1) is for sign-in only and its private key never
ships"*). This section fixes where each registration lives and how it is set up.

A **forge registration** is what a host knows charter by when a person signs in: a GitHub App on
GitHub, an OAuth application on GitLab. Each is identified by its **client id**, which is public.

| Host | Registration | Registered by | Flow |
|---|---|---|---|
| github.com | charter's GitHub App | charter's maintainers, once (GH2) | OAuth device flow |
| gitlab.com | charter's OAuth application, not confidential | charter's maintainers, once | authorization code with PKCE |
| a GHES | a GitHub App registered on that host from charter's settings (the same permissions, device flow on) | the host's organisation owner | OAuth device flow |
| a self-managed GitLab | an instance-wide application its admin registers, or a user-owned application the person registers in their own preferences, from charter's settings (not confidential, the same scope and redirect URIs) | the admin, or the person | authorization code with PKCE |
| any host with no registration | none | — | a PAT (0077 §4) or an imported CLI login (0077 §5) |

- **Beyond the ticket's text: a user-owned GitLab application.** FW-1 names one application per
  instance. A person on a self-managed GitLab whose admin never registers one can still use PKCE,
  instead of a long-lived PAT, because GitLab lets any user register an application of their own.
- **charter's own two registrations are compiled into the binary** as a table of host, kind,
  client id and flow, in `crates/charter-core/src/forge/registration.rs`. A changed client id is a
  release. A registration made on another host is part of the account record (0077 §6), keyed
  by host.
- **charter's GitHub App is registered with device flow on, user-token expiry on, webhooks off, no
  client secret and no private key.** Sign-in needs neither. V16c's *"its private key never
  ships"* is kept by there being none: webhooks through the relay (FG-12a, after GT-CLOUD) decide
  whether the App ever gets one, and where it is kept, in their own record. The same holds for
  charter's gitlab.com application, which is not confidential.
- **The redirect for PKCE is a loopback address** (RFC 8252 §7.3): `http://127.0.0.1:<port>/charter/signin`.
  The application registers three fixed ports, and the listener binds the first unused one, so
  the flow works whether or not a host ignores the port when it matches. **Before FD-2 the
  window's process listens; after FD-2 `charterd` does**, the same process that runs the flow
  (0077 §3). The listener takes one request whose `state` matches, and closes. The page opens in
  the system browser, never in a webview charter controls (RFC 8252 §8.12).
- **Sign-in never happens on a runner.** A runner holds no forge credential (V9). Signing in is a
  `local-ui` capability (ADR 0068, amended below), a runner's host is reached from the desktop only
  over `remote-link` ([ADR 0078](0078-a-runner-is-charterd-behind-a-connector-and-charters-own-keys-say-who-is-on-the-link.md)
  §4), and `remote-link` is refused the forge token and every sign-in act, as it is vault values.

### 2. The minimum permissions, each with the feature that needs it

**Settled by FI2:** *"minimum scopes, each explained on the settings page"*.

**GitHub has no scopes for an App.** The App declares permissions, and a user token has the
intersection of those and the person's own rights. The App declares only what a ticket in the
forge work reads, and the settings page shows each with that feature:

| Permission | Access | Why, on the settings page |
|---|---|---|
| Metadata (repository) | read | required by GitHub for every App |
| Contents (repository) | read and write | a repo's files and branches (stack detection), and merging a pull request (ADR 0051's saves, ADR 0060's landing) |
| Pull requests (repository) | read and write | opening, updating and reading pull requests, and auto-merge |
| Issues (repository) | read and write | work items: issues, sub-issues and their links (FI4) |
| Checks (repository) | read | CI state at a commit (ADR 0060) |
| Commit statuses (repository) | read | CI state reported as statuses (ADR 0060) |

- **A permission is added by the ticket that first reads it**, as ADR 0070 §2 adds a capability
  flag. The table holds what the forge work reads now. Projects (organisation), for boards and
  project fields, is added by FW-6a when it first maps them (FI4, FI8), and not before. Changing
  an App's permissions asks every installation's owner to approve the change, so the list grows
  rarely and in batches, and the settings page says which installations have not approved yet.
- **Git pushes are not the App's.** Clones keep the CLI's credential helper until #752
  (ADR 0070 §4), so the App needs no permission for workflow files or for pushing.

**GitLab's scope is `api`, and only `api`, asked for at sign-in.** GitLab has no narrower scope
that can open a merge request or write an issue. `read_api` cannot, and `read_repository` and
`write_repository` are git's, which the CLI's helper still serves. The settings page says exactly
that: `api` is the only scope that lets charter write, and charter uses it only through the seam,
as the person.

**Decided (dispatcher, D-0077c):** `api` is asked for at sign-in, not `read_api` first and
`api` at the first write. The token is the human's only and never reaches an agent (FI3), so its
breadth is the person's own, and V14's launch needs writes (*"issue → chat → PR"*). The rejected
read-first option is under "What was rejected".

**A PAT or an imported login shows the scopes it actually has.** charter reads them from the forge
once, when the account is added (GitHub's `X-OAuth-Scopes` header, GitLab's token self-lookup),
lists them, and marks each one wider than 0077 §2's table as wider than charter needs. It never asks
for less than the person gave, and never narrows a token it did not mint.

### 3. The flows run where the human scope is, and the token is read only there

**Settled by V22c:** *"the `charter` binary uses the CLI transport until the host's human scope
exists"*. This section applies it to signing in.

**Sign-in, refresh and sign-out are human acts, and only the process that holds the human scope
performs them.** Before FD-2 that is the app's window process. After FD-2 and FD-27 it is
`charterd`, asked on the `local-ui` scope (ADR 0068, amended below). The `charter` binary performs
none of them (V22c), because a `charter` a chat runs is part of that chat.

- **Device flow (GitHub):** charter asks the forge for a device code with charter's client id,
  shows the user code and the verification address in the settings page, opens the address in the
  system browser, and polls at the interval the host gives. The code is shown nowhere else: not in
  a terminal, a notification or a log.
- **PKCE (GitLab):** charter builds a random verifier and `state`, opens the authorization page
  with the S256 challenge, takes the one loopback callback, and exchanges the code.
- **Nothing in a flow is stored.** A device code, a verifier and a `state` live in memory for the
  flow's length. A flow the person abandons leaves nothing behind.
- **One writer refreshes.** A refresh rotates the refresh token, so two processes refreshing the
  same account would log it out. Only the human-scope process refreshes, just before a request
  when the token is within five minutes of expiry, and writes the new pair to the keyring in one
  write. A refresh the forge refuses marks the account **signed out** (0077 §6), and it is never
  retried under another account or the CLI (ADR 0070 §5).
- **Sign-out** deletes the keyring item and the account's bindings. On GitLab it also revokes the
  token, which a public client can do. GitHub's revocation endpoint needs the App's client secret,
  which does not exist, so on GitHub the settings page links to the person's authorised Apps page
  and says the token lapses within eight hours.

### 4. A PAT is the fallback where no registration exists

A PAT is pasted into the settings page, never passed on a command line or read from an
environment variable. It is checked with one request (who it belongs to, and its scopes), and
stored as the account's keyring item. A PAT does not refresh. When the forge refuses it the account
is signed out and the page says the token expired or was revoked. **Beyond the ticket's text: a PAT on
github.com.** FW-1 names a PAT for self-managed hosts. On github.com a fine-grained PAT is
accepted as well, because it is the one way a person whose organisation will not install charter's
App reaches its repos without handing over a `gh` login (0077 §1, V13).

### 5. An imported CLI login is offered once, and never taken silently

**Settled by FI2** (*"an existing `gh`/`glab` login can be imported on first run"*) and **V22c**
(*"an imported CLI login becomes the human's and chats lose it"*).

**The first run offers the import; it never imports by itself.** When `gh auth status` or `glab
auth status` reports a login on a host that has no charter account, the first run (FR-4) and the
settings page offer to import it. The offer says what V22c's rule costs: after the import, chats
on that host lose the forge access the CLI gave them, until SD-7a/b. The person can decline and
sign in through the registration instead. That leaves the CLI's login to chats as it is today
(ADR 0070 §4 takes it from them only on import), and it is the offer's default.

- **Import copies the token once** (`gh auth token`, or `glab`'s stored token for that host) into
  the account's keyring item, and records the account's method as `import`. The CLI keeps its own
  copy, which SD-2 compiles into ADR 0067's third denial class for chats (ADR 0070 §4).
- **charter never refreshes an imported token.** If the CLI's token expires (a `glab` OAuth
  login does), refreshing it would mean using the CLI's registration as if it were charter's.
  When the forge refuses it, the account is signed out and offers sign-in or a fresh import.

### 6. The account record, and one account per repo

**Settled by FI2:** *"several accounts per forge (github.com, GHES, gitlab.com, self-managed), one
chosen per repo"*.

The accounts live in `<config>/forge-accounts.json` (0077 §8). Each account has:

- an **id**, a ULID minted when it is added, which names its keyring item
  (`charter/@forge/<host>/<id>`) and is what the audit and the bindings refer to;
- its **kind**, **host** and **login**;
- its **method**: `device`, `pkce`, `pat` or `import`;
- for a host charter has no compiled registration for, that host's **client id**, and nothing
  secret;
- its **state**: `signed-in` or `signed-out`, and the scopes or permissions last read.

**A repo's account is chosen once and bound.** With one account on a host there is nothing to
choose. With several, the first request for a repo on that host asks the person which account,
in the window, and records the choice as a binding on the repo's owner or on the repo itself. A
chat never makes the choice, and a binding is never inferred from a CLI's current login. The
bindings are in the same file, because they name this machine's accounts, and each engineer
binds their own.

### 7. Nothing here reaches an agent

**Settled by FI3** (*"The human's UI token never reaches an agent; agents keep their own narrow
identity (SD-7a/b, gap G3), and gain no new power from this feature before those land."*) and
**V16c** (*"Per-agent GitHub tokens (SD-7a) come from a GitHub App the organisation registers
itself"*).

This is ADR 0070 §4, applied to what ADR 0077 adds:

- **The token is a keyring item charter owns, and never a vault entry.** FI2's "vault/keyring" is
  read as the keyring. **Decided (dispatcher, D-0077b):** this is the stricter reading of FI2,
  supported by FI3 and V16b, and FW-3a's and FW-3b's "the keyring or a vault" is corrected to "the
  keyring" (comments on #729 and #730). A vault entry can be asked for through V15's gate. V16b enforces that gate,
  but a gate still opens: an approved request resolves a secret into a chat's command. A forge
  token must not be reachable by any approval, so it is outside every vault, and a PAT the person
  keeps in a vault is copied into the account's item when it is added.
- **charter's registrations mint nothing for an agent,** on either forge. SD-7a's token comes from
  the organisation's own App, and SD-7b's from GitLab's project or group access tokens. Where
  SD-7a keeps that App's private key is SD-7a's to decide, with its tier; it is never charter's
  sign-in item.
- **FW-3a's and FW-3b's tests are FI3's:** a signed-in account, a chat that runs `charter`
  commands, reads its environment, and calls every MCP tool, and a recorded forge that fails the
  test if any request from the chat carries the sign-in token (ADR 0070's
  `an_mcp_tool_call_from_a_chat_never_sends_the_sign_in_token`), plus a search of the chat's
  environment and every file the run wrote for a canary token.

### 8. Every store and its tier

**Settled by V22b:** *"the tier test checks the document, and "names its tier" is a review
question on every PR that adds a store"* (ADR 0069).

| Store | Tier | What it holds |
|---|---|---|
| `<config>/forge-accounts.json` | Machine, device-bound | the accounts of 0077 §6 and their repo bindings. Device-bound because each entry points into this machine's keyring (ADR 0069 §2). Backed up by FR-10, and restored by ADR 0069 §5's rule: only when the new machine replaces the old one |
| keyring `charter/@forge/<host>/<id>` | Keyring | an account's token: the access token, and the refresh token and expiry where the flow gives them. Never backed up |

**A restore follows ADR 0069 §5 unchanged, because that is safest for tokens.** On a machine that
replaces the old one, `forge-accounts.json` comes back and every account in it is **signed out**:
the Keyring is never copied, so there is no token to come back, and the person signs in again. On
any other machine it does not come back at all. No token travels in a backup either way, and a
second machine never inherits a binding to accounts it has no token for.
charter's own client ids are in the binary and are not a store. SD-7a names its own store for an
organisation App's key. A flow's device code, verifier
and `state` are in memory only.

## ADR 0055, amended

The picker lists what the operator's own login reaches. Through a GitHub account signed in with
charter's App, that is **what the person reaches in the organisations and accounts where the App
is installed**. The picker therefore shows, under each declared owner the App is not installed
on, one line saying so, with GitHub's page to install it or to ask an owner to. An owner whose
repos are reached through another account on the host (a PAT or an imported login) lists through
that account instead. A forge the person has not signed in to still says why, now in charter's
own words ("Sign in to github.com in Settings") rather than the CLI's, once FW-3a has landed.

## ADR 0066, amended

A **forge account is not an account** in ADR 0066's sense. Signing in to a forge never writes
`account.link`, and the forge login is never a principal: the local principal stays the actor for
everything the operator does, and the forge login appears only in the account record and in what
the forge itself shows. A sign-in sends the forge only what its flow needs (the client id, and the
code or verifier), and never the device id or the local principal.

## ADR 0068, amended

- **The forge token is reachable from `local-ui` only**, as vault values are (ADR 0068 §5). Every other
  scope is refused it, `terminal`, `fleet-mcp`, `approval`, `remote-link` and `chat` included. A
  forge call made for a chat is made by the host with the chat's own credential (ADR 0070 §4),
  never with the sign-in token.
- **Signing in, signing out, importing and refreshing are `local-ui` capabilities.** Before FD-2
  the window's process runs the flow and the loopback listener of 0077 §1. After FD-2 the host
  runs both, and the window only shows the code and opens the browser.
- **`charter` on the `terminal` scope cannot sign in.** A person working only in a shell signs in
  from the window, or keeps using the CLI transport (V22c).

## ADR 0069, amended

The table of stores decided and not yet written gains the two rows of 0077 §8, rows 78 and 79 of
ADR 0069's inventory (row numbers renumbered 2026-10-01; see ADR 0069):
`<config>/forge-accounts.json` (Machine, device-bound, backed up) and the account token
`charter/@forge/<host>/<id>` (Keyring). ADR 0070 named the account token's item and gave it no
row; this is that row. `docs/plane-format.md` records both, each **decided, not yet written**.
ADR 0069 §5 is not amended: a restore brings the accounts back only onto a machine that replaces the old
one, and signed out (0077 §8).

## ADR 0070, amended

- **ADR 0070 §4, "FW-1 decides the flows and scopes":** decided above. A `TokenSource` refreshes
  only in the human-scope process (0077 §3), and a refused refresh signs the account out.
- **ADR 0070 §4, "Only a human `Caller` resolves to the sign-in token":** narrowed to **only a
  `Caller` on the `local-ui` scope**: the window before FD-2, and `charterd` serving a `local-ui`
  connection after it. A human on `terminal`, `approval`, `fleet-mcp` or `remote-link` does not
  resolve to it. A `charter` command on `terminal` uses the CLI transport, as V22c rules.
- **ADR 0070 §4, the keyring item:** `charter/forge/<host>/<account>` becomes
  **`charter/@forge/<host>/<id>`**. The `@` marks it as charter's own item, as
  `charter/@identity/…` is, so a vault named `forge` (whose items are `charter/<vault>/…`) can
  never collide with it. `<id>` is the account's ULID (0077 §6), not its login: a login can be
  renamed on the forge, and the item must not follow it.
- **ADR 0070 §4, "several per forge … each repo is bound to one":** the binding is chosen by the
  person when a host has more than one account, and kept in `forge-accounts.json` (0077 §6).
- **ADR 0070 §5, importing a CLI login:** it is offered, never done by itself, and the offer
  states V22c's cost (0077 §5). An imported token is never refreshed by charter.

## ADR 0075, amended

- **`forge.write`'s `meta`:** *"forge, account name, surface, operation, item"* now reads
  **forge, account id, surface, operation, item**. The account id is the ULID of 0077 §6. An
  account's name would be the forge login, which is a person's name.
- **The action registry gains three actions.** Until the audit exists (ADR 0075 §10) they are
  events in the host's event log. Every `meta` field is a type in ADR 0075 §4's closed set.

| Action | From | `meta` |
|---|---|---|
| `forge.account.added` | ADR 0077 | forge (enum), host (name), account id (id), method (enum: `device`, `pkce`, `pat`, `import`), scope fit (enum: `exact`, `wider`, `narrower`, against 0077 §2's table), extra scopes (count) |
| `forge.account.removed` | ADR 0077 | forge (enum), host (name), account id (id), revoked at the forge (boolean) |
| `forge.account.refused` | ADR 0077 | forge (enum), host (name), account id (id), refusal (enum: `expired`, `revoked`, `refresh-refused`) |

Each is a `human` entry for `added` and `removed`, and a `host` entry for `refused`. The target
is the account by its id (type `forge-account`). The login is never in an entry: it is a person's
name, and the account id already says which.

## What changes where

The code does not change with ADR 0077.

| Where | Change |
|---|---|
| `CONTEXT.md` | Gains **Forge registration**; **Forge account** says how one is signed in; both are settings of the Project (in this PR) |
| `docs/plane-format.md` | The two stores of 0077 §8 in the table of what charter keeps outside every project, each **decided, not yet written** (in this PR) |
| ADR 0055, ADR 0066, ADR 0068, ADR 0069, ADR 0070, ADR 0075 | Amended above. Their texts are left as accepted, and ADR 0077 is the amendment |
| charter's maintainers | Register charter's GitHub App and gitlab.com application with 0077 §1's settings and 0077 §2's permissions (FW-1's acceptance; question 1) |
| FW-3a | `forge/registration.rs`, the device flow, GHES registrations, `gh` import, the settings page's GitHub half, and the FI3 test |
| FW-3b | PKCE with the loopback listener, self-managed registrations, PATs, `glab` import, the GitLab half, and the FI3 test |
| FR-4 | The first run's import offer (0077 §5) |
| FW-6a | Adds the Projects (organisation) permission when it first maps boards |
| FW-14 | The three audit actions, and `forge.write` with the account id |
| SD-7a, SD-7b | Agent tokens, and SD-7a's own store for its App key |

## What this costs

- **An organisation must install charter's App before its private repos appear.** Many
  organisations restrict third-party Apps, and a member can only ask. This is the price of a
  GitHub App (V13), and the reason PATs and imports stay (0077 §4, §5).

  **Decided (dispatcher, D-0077a):** this reach is accepted, and settled by V13 (*"The FI2
  GitHub registration is a GitHub App with device flow"*). An OAuth App would reach every
  organisation without a step, but only with `repo`, every private repo the person can see. The
  picker names each owner the App is missing from (ADR 0055, amended), so the cost is visible.
- **GitHub's notifications API does not accept an App's user token.** GitHub documents it for
  classic PATs only. FI11's GitHub half (FW-13) needs another source, such as an imported login,
  and FW-13 decides it.
- **Sign-out cannot revoke a GitHub token.** Revocation needs a client secret charter does not
  have. The token lapses within eight hours, and the refresh token is gone with the keyring item.
- **An import costs chats their forge writes** on that host until SD-7a/b (V22c). The first-run
  offer says so, and defaults to signing in through the registration, which does not.
- **Every GHES and self-managed GitLab needs its own registration**, made by someone with the right
  on that host. Where nobody makes one, the account is a PAT or an import.
- **A device-flow client id is public**, so anyone can start a flow that shows charter's name. That
  is true of every public client, including `gh`. The settings page shows the code only for a
  flow the person started, and GitHub shows the App's name and permissions before approval.
- **`api` is a broad GitLab scope.** GitLab offers nothing narrower that can write.

## What was rejected

- **A GitHub OAuth App instead of a GitHub App.** It needs no installation, but its scopes are
  coarse (`repo` is every private repo), and V13 chose a GitHub App.
- **A client secret or private key in the binary.** Anything shipped is public, and neither flow
  needs one.
- **One registration everywhere**, charter's github.com App used against a GHES. A registration
  exists on one host only.
- **GitLab's device authorization grant.** It is generally available only from GitLab 17.9, so a
  self-managed instance older than that would need PKCE anyway, and FI2 names PKCE. It can be
  added as a second flow for a host that supports it, without changing the account record.
- **The `charter` binary signing in from a terminal.** A `charter` run inside a chat would then
  hold a human power (V22c, V16a).
- **Refreshing an imported CLI token** with the CLI's registration.
- **Choosing a repo's account automatically**, from the CLI's active login or the first account
  that answers. A request made under the wrong identity is audited under the wrong person.
- **Keeping the forge token in a vault.** An approval could then hand it to a chat (0077 §7).
- **`read_api` at sign-in and `api` at the first write, on GitLab.** It makes a person consent
  twice, adds a signed-in-but-read-only state to every GitLab screen, and buys nothing an agent
  could exploit, since the token is the human's only (FI3). For most people the first write, a
  save's merge request, comes minutes after sign-in.
- **A registration secret in charter's sign-in store.** An organisation App's private key is
  SD-7a's, in a store SD-7a names.
- **Storing a device code, verifier or `state`.** A flow is short, and a stored one is a replay
  risk with nothing gained.

## Ruled (V30a, 2026-10-01)

The operator accepted the one open question as recommended:

1. **V30a: charter's GitHub App and its gitlab.com application are registered now, under the
   repo's current owner, and transferred to charter's GitHub organisation in GH2's step 4.**
   FW-3a checks that the client id survives the transfer. The operator does the registration,
   which is FW-1's remaining acceptance line on #726.

## Decided (dispatcher, 2026-10-01)

1. **D-0077a: a GitHub App reaches only the organisations that installed it**, and PATs and
   imports cover the rest. Settled by V13 ("What this costs").
2. **D-0077b: the sign-in token is a Keyring item, never a vault entry.** The stricter reading of
   FI2, supported by FI3 and V16b (0077 §7). #729 and #730 carry the correction.
3. **D-0077c: GitLab asks for `api` at sign-in.** The token is the human's only (FI3), and the
   launch needs writes (V14) (0077 §2).
