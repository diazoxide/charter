# A chat's model is one choice from four sources, and purlis never carries a harness login

**Accepted 2026-10-01** by the operator (ruling V36), with dispatcher decisions D-0087a/b,
drafted for program-map ticket MS-1 (#700). It follows these of the operator's rulings:

- **M1:** *"One model choice with three sources: (1) the harness's own login, (2) a BYO API key
  from the vault, desktop → provider directly, (3) Charter credits through the cloud gateway.
  Chosen per chat, persona or workspace, and always shown."*
- **M2**, in short: the first two sources and the model picker work offline and need no account.
  Only the third needs the cloud.
- **M3:** *"The E2EE promise covers live sessions, not model traffic. The credits path processes
  prompts like any provider: zero content retention, only token count, cost and model are logged,
  and ZDR upstreams are used where available. The picker shows a lock (direct) or a gateway icon
  (via Charter)."*
- **M5**, in part: *"Org model governance through the C9 policy layer: model and provider
  allowlists, defaults per harness and persona, "must" or "must not" route via the gateway, and
  org-owned provider keys (Bedrock/Vertex/Azure) plugged into the gateway."*
- **M6:** *"A persona declares its model preference and fallbacks as plane data, and charter
  compiles that into each harness's config at spawn."*
- **M7**, in part: *"BYO is the product; credits are the convenience layer."*
- **M9**, in short: the gateway's credits are offered only on harness and model pairs that are
  cleared for them, never on Claude Code until that is cleared, and *"The picker hides credits
  where forbidden, says why, and offers BYO."*
- **M10:** *"A harness×model matrix as plane data, with the states native / supported / degraded
  (behind an experimental switch, breakage listed) / blocked (never offered)."*
- **N15**, in short: local models (Ollama, LM Studio, vLLM) are a fourth model source, and work
  offline with no account.
- **V9**, in part: *"Harness logins happen only through the harness's own flow in a runner shell
  tab; charter never reads, copies, stores or relays a harness credential"*.
- **X23** and **X40**, accepted with the consistency review as recommended. X23: *"Is N52 limited
  to choosing among (harness, source, model) tuples marked native or supported in M10, applied
  only when a chat is spawned (never by re-routing a running chat), and never picking a gateway
  route for a harness on a subscription login?"* X40: *"Does setting "must route via gateway"
  show admins "this disables Claude/ChatGPT subscription logins for members", and does the picker
  hide source 1 for affected harnesses with that reason?"*
- **X12**, through SD-28's row: *"unattended runs (triggers, gardeners, workflows, off-peak,
  headless, race) prefer API-key or local-model sources, with a one-time acknowledgement before a
  consumer subscription runs unattended"*.
- **C3**, its wording requirement, in short: the encryption protects content from purlis's
  servers, and never from the harness's model provider.
- **V15**, in part: a vault or secret sets its approval, *"off · every use · once per chat · for
  N minutes in this chat · require OS authentication"*. *"The command needing the secret
  **waits** (it does not fail)"*, and an unanswered request is denied after a timeout (default 5
  minutes). *"Creating a scheduled or triggered agent warns when it uses an approval-required
  secret."*
- **V36**, the ruling on this record (*Ruled*, below): a key reaches the harness through a proxy
  in `purlisd` wherever the harness accepts a base URL.
- **V34c**, through [ADR 0083](0083-telemetry-is-one-otel-pipeline-on-the-device-and-only-the-signed-updater-calls-charter.md):
  *"A spend budget may pause a chat on harness-reported spend."*

It builds on [ADR 0066](0066-a-chat-is-a-ulid-a-run-is-a-stretch-of-its-conversation-and-a-device-is-random.md)
(the run and its `model_source`), [ADR 0067](0067-a-chat-runs-in-a-sandbox-charter-compiles-for-its-harness.md)
(the sandbox and its egress presets), [ADR 0068](0068-a-chat-lives-in-charterd-and-the-app-is-its-client.md)
(one path to a secret's value), [ADR 0072](0072-charter-has-five-concepts-and-every-other-word-belongs-to-one-of-them.md)
(the five concepts), [ADR 0073](0073-a-harness-is-declared-as-data-and-a-chat-runs-it-at-one-of-three-levels.md)
(harness declarations and capabilities), [ADR 0078](0078-a-runner-is-charterd-behind-a-connector-and-charters-own-keys-say-who-is-on-the-link.md)
(runners) and ADR 0083 (telemetry and spend). It amends ADR 0066, ADR 0073 (including its §4), ADR 0078 and ADR 0083,
each in a section of its own below. MS-2 (the matrix), MS-3 (the picker), MS-4 (a key from a
vault), MS-5 (the harness's login), MS-6 (persona preferences), MS-14, MS-17 (local models), MS-18
and SD-28 build on it. The gateway's own tickets (MS-7 to MS-16) are cloud work and wait for
GT-CLOUD. This record fixes only how the gateway appears as a source.

Its concept is **Chat**: the model is one of a chat's settings, beside its harness, profile and
persona (ADR 0072 §2). A persona and a workspace give a chat its default.

## Where purlis is today

- **purlis chooses no model.** A chat runs its harness with the harness's own default model and
  the harness's own login. The only "model" in purlis's code is the word in a refusal.
- **purlis refuses to hold a credential where the model can read it.** A profile may not set an
  environment variable named like a credential (`profiles::named_like_a_credential`), because
  *"anything set on the harness process reaches the shell the model runs"*, which was measured on
  Claude Code and Codex. `chatenv` starts every chat from an empty environment and does not pass
  `ANTHROPIC_*`, `OPENAI_*`, `GEMINI_*` or the other provider prefixes unless the operator lists
  them in `[chat_env] pass`.
- **ADR 0066 already names the attribute.** A run's `model_source` is *"where the model is
  reached: the profile's login, an API key, a gateway, a local model. It is read from the
  profile's declaration, never from output"*, and a model switch starts a new run. Nothing sets it
  yet.
- **Each harness is configured differently.** Research 09 §1.1 measured it. Claude Code takes
  `ANTHROPIC_*` variables, `--model` and `--settings`. Codex takes `-c` overrides, `--profile` and
  `--model`. opencode takes `OPENCODE_CONFIG_CONTENT` and `--model provider/model`. Some
  harnesses, such as Cursor's, take nothing from outside. Each of the three built-ins also reads
  configuration committed in the repo it runs in, such as a `.claude/settings.json` `env` block
  or a `.codex/config.toml`.
- **Vendor terms draw hard lines** (research 09 §3, §6.3). A subscription login's credential must
  not be collected, stored or relayed. A subscription session must not be pointed at a gateway.
  Claude Code is not to be routed to a non-Claude model.

## The decision

**A chat runs on one model choice: a model, its provider, and the source it is reached through.
There are four sources: the harness's own login, a key from a vault, the gateway, and a local
model. The choice is made per chat, persona, workspace or project, applied by the harness's
adapter when a run starts, and always shown with the account it is billed to. purlis never reads,
stores or relays a harness login, and never lets one be pointed anywhere but its vendor. The
end-to-end encryption promise covers live sessions, never model traffic.**

### 1. One model choice

```toml
[model]
source = "key"                         # login | key | local; gateway only where §3 allows it
provider = "anthropic"                 # who serves the model; implied by the harness for login
model = "claude-sonnet-4-5"            # as the provider names it; omitted = the harness's default
key = "vault:providers/anthropic"      # source "key" only: a vault reference, never a value
endpoint = "ollama"                    # source "local" only: a detected local endpoint (MS-17)

[model.codex]                          # optional: applies only when the chat's harness is codex
model = "gpt-5-codex"
```

- **`source` is one of four words, and only four.** Settled by M1 and N15, which give three
  sources and a fourth. A new source is a change to this record. An extension may add an
  *endpoint* to `local` or a *provider* to `key`, but not a fifth source (E1's model-source seam is
  the provider and endpoint lists).
- **A choice may differ per harness.** A `[model.<harness>]` table overrides the plain `[model]`
  table when the chat's harness has that declared name. This is M5's *"defaults per harness"* at
  every level where a choice is set. Org defaults per harness and per persona come through the
  C9 policy layer (MS-7).
- **`model` may be left out.** Then the harness uses its own default for that source, which is
  today's behaviour, and the chat shows *"the harness's default"* until the harness reports the
  model it used (§6).
- **`key` and `endpoint` are references, not values.** A `key` is a vault reference
  (`vault:<vault>/<name>`), the form `secretshape` already exempts. A committed reference names
  where each member keeps their own key. It never holds anyone's key. It may name only an entry
  marked as a model key (§7).
- **`gateway` is never committed.** **Decided by D-0087b:** *"Committed files may name `login`,
  `key` or `local`, never `gateway`. `gateway` comes only from the chat's picker,
  `charter.local.toml` or org policy, and waits for GT-CLOUD."* A committed `source = "gateway"`
  is refused when it is read, with a line saying why, and the next level's choice applies. No
  merged pull request makes other people spend.
- **The same shape everywhere it is set**, so one reader parses it: `charter.toml`,
  `charter.local.toml`, `workspace.json`, a persona's `persona.md` front matter, and the chat
  itself (§3).

### 2. Four sources

| `source` | What it is | Billed to | Route | Shown as |
|---|---|---|---|---|
| `login` | **The harness's own login**: whatever `claude /login`, `codex login` or the harness's own flow set up. The default | the harness's account: the user's plan, or a key the harness itself stores | harness → its vendor. purlis is not on the path | lock · *"your Claude plan"* (the harness and its plan, as the harness reports it) |
| `key` | **A key from a vault**: the user's own provider key (M1's second source) | the key's owner, at the provider | harness → provider, from this device | lock · *"your Anthropic key"* |
| `gateway` | **Through purlis's gateway**: an org's own provider keys (M5, M8 step 2), or credits where they are cleared (M9) | the org's key, or credits | harness → purlis's gateway → provider | gateway icon. Its wording is GT-CLOUD's copy to decide |
| `local` | **A local model** on this device or this runner: Ollama, LM Studio, vLLM (N15, MS-17) | nobody | harness → a loopback or LAN endpoint | lock · *"on this machine"* |

- **Lock for direct, gateway icon for via purlis.** Settled by M3. `login`, `key` and `local`
  never pass through purlis's servers, so all three show the lock. Only `gateway` shows the
  gateway icon.
- **`login`, `key` and `local` need no account and no connection to purlis's servers.** Settled
  by M2. `local` works with no network at all.
- **`gateway` waits for GT-CLOUD.** Until then the picker does not offer it. Its tickets
  (MS-7 to MS-16) build it, and this record changes nothing in them. What the picker calls it,
  such as the word for credits, is GT-CLOUD's copy to decide.
- **`login` covers a harness's own key too.** Some harnesses log in with a key the harness itself
  stores, such as an API key typed into the harness's own login prompt. That is still the
  harness's login: purlis does not read it, and it is not source `key`.

### 3. Five places a choice is set, and which one wins

A chat's choice is the first of these five that sets one:

1. **The chat**: the picker in the new-chat flow or the chat header (MS-3). Changing it starts a
   new run (ADR 0066's `switch`).
2. **The persona the chat runs as**: its preference and fallbacks, from `persona.md` (M6, MS-6).
3. **The workspace**, from `workspace.json`.
4. **This machine's override for the project**, from `charter.local.toml`.
5. **The project**, from `charter.toml`.

With none of them set, the chat uses `login` with the harness's default model, as it does today.

- **Each field falls back separately.** A persona that names only a model keeps the workspace's
  source. A choice that becomes invalid at a lower level is skipped with a line saying why: a
  model the matrix blocks for this harness, a committed `gateway` (D-0087b), or a key reference to
  an entry not marked as a model key (§7).
- **The persona wins over the workspace.** A persona is a role, and its model is part of the role,
  as a reviewer persona that names a stronger model shows. A workspace's choice is a default for
  work that has no role. **`charter.local.toml` sits below both** because it is one machine's
  override of the project's default, not of a role's or a task's.
- **Org policy clamps all of them** (M5, C9; MS-7, after GT-CLOUD). Strictest wins. An allowlist
  can refuse a choice, and "must route via the gateway" removes `login` for the harnesses it
  covers, with X40's sentence in the picker and in the policy view.
- **Persona fallbacks stay within their source** (M6). A persona's fallback list is a list of
  models. If the first is blocked or refused, the next model on the *same* source is tried.
- **An unattended run prefers `key` or `local`.** Settled by X12 through SD-28. A chat started by
  a trigger, a schedule, a workflow, a race, or headless resolves its choice the same way. If that
  choice is `login`, purlis asks once, per project, harness and login on this machine, for the
  operator's acknowledgement before a subscription runs unattended. Until the acknowledgement is
  given, the run waits as a needs-you item. The acknowledgement is a new store, decided here and
  written by SD-28 (§11).

### 4. Applied at spawn by the harness's adapter

**Settled by M6:** compiled into each harness's config at spawn.

- **The choice is resolved once, when a run starts,** and passed to the harness's adapter
  (FD-13). The adapter turns it into that harness's flags, environment or config, the
  per-harness mapping of research 09 §6.1. A running chat is never moved to another choice by
  purlis (X23). A new choice is a new run.
- **The matrix decides what may be offered** (M10, MS-2). A (harness, source, model) tuple that is
  `blocked` is never offered or applied. A `degraded` one is offered only behind its switch, with
  its breakage listed. Credits are hidden where they are not cleared, with the reason and BYO
  offered (M9, MS-14). Claude Code is never offered a non-Claude model.
- **A harness that cannot take a source does not get it.** A harness whose configuration cannot
  be injected per session, such as Cursor's CLI, runs on `login` only, and the picker says why.
- **A key never goes into a file.** A harness that accepts a base URL gets the key through the
  proxy of §7, and sees only a loopback URL and a per-chat token. A harness that does not gets the
  key in its process environment. A harness that reads its configuration from a file is given a
  generated file that names a variable (`${ANTHROPIC_API_KEY}`, `{env:…}`), never a value.
  purlis never writes a key to disk.

### 5. What purlis never does with a login

**Settled by V9:** purlis never reads, copies, stores or relays a harness credential.

- **A `login` chat starts with no routing variables.** The adapter removes `ANTHROPIC_BASE_URL`,
  `ANTHROPIC_AUTH_TOKEN`, `OPENAI_BASE_URL` and their like for that harness, whatever the
  operator's `[chat_env] pass` lists.
- **Committed harness config cannot reroute a login either.** Before a `login` chat starts, the
  adapter reads the configuration the harness would load from the chat's directory:
  - **Where the harness gives purlis a flag that takes precedence, purlis uses it.** For Codex
    that is `-c model_provider=…` naming the vendor's own provider, which overrides a committed
    `.codex/config.toml`'s `model_provider` and `model_providers` entries.
  - **Where it does not, purlis refuses to start the chat.** A committed `.claude/settings.json`
    or `.claude/settings.local.json` whose `env` block sets a base URL, an auth token or a
    provider switch for Claude Code is one such case. The refusal names the file and the key, and
    offers the chat on a `key` source instead.
  - **The user's own global harness config** (`~/.claude/settings.json`, `~/.codex/config.toml`)
    is the user's choice, not a project's. purlis does not refuse it. It shows the chat as
    *"routed by your own settings"*, with a warning, and records that the route was not
    purlis's.

  The adapter of each harness lists the keys it checks, and a harness without an adapter cannot
  take `login` with a chosen model at level 2 or above. MS-5's test covers the committed cases:
  *"no subscription token crosses charter"*, and none is sent anywhere but its vendor.
- **purlis never captures a login.** Logging in is the harness's own flow, in a shell tab (V9),
  with the harness's own files. purlis only says which login is in use, from what the harness
  reports in a hook.
- **Smart model choice never moves a `login` chat** (X23, MS-18). It picks among tuples marked
  native or supported, only at spawn, and never a gateway route for a harness on a login.

### 6. Always shown

**Settled by M1:** always shown.

- **The chat header and the picker show the model, the source's icon, and the account it is
  billed to** (§2's "Billed to" and "Shown as" columns), from the run's resolved choice. The header
  also says where the choice came from (*"from the persona"*). While the model is the harness's
  default and the harness has not reported it, the header says *"the harness's default"*. A
  reported model then replaces it. If the harness reports a model other than the one chosen, the
  header shows the reported one with a warning. That is a report, so per ADR 0066 it starts a new
  run and is never acted on beyond that.
- **The first-hour words** (ADR 0072 §3). The picker says *"your key"*, never *"vault"*, and names
  the harness by its product name. "Source" itself is not shown in the first hour. The icon and
  the "billed to" line carry it.
- **Each run records its choice.** A run's `model_source` is the source word, and the run also
  records the provider and the model, from the choice and then from the harness's report (ADR 0066,
  amended below). Session records and the audit's `run.started` carry the same values (ADR 0075).

### 7. A key's path, and what it costs to keep it there

- **Only an entry marked as a model key is ever injected.** A vault secret becomes usable as a
  `key` source only when the operator marks it as a model key for one provider. The mark is set
  from a human scope and kept in the vault's metadata, and MS-4 records it in `docs/plane-format.md`
  with its tier. A committed `key = "vault:…"` reference to an entry without the mark is refused
  when it is read, so a merged pull request cannot make `purlisd` put an arbitrary secret, such
  as a database password, into a chat's environment. A marked key is injected only into the
  variables of the provider its mark names, or used by the proxy only toward that provider.
- **One path to the value.** `purlisd` reads the key at spawn, as it reads every vault value
  (ADR 0068 §1), and keeps it in the proxy or hands it to the harness's process (below). **The key's approval setting applies**
  (V15). A key set to *every use* or *OS authentication* makes the chat's start wait for the
  operator, and the ask shows the chat, the harness, the key's name and that it is the chat's model
  key. **An ask unanswered within V15's timeout (default 5 minutes) is denied.** The chat does not
  start, and the needs-you item says why. The read is recorded like any other.
- **Creating a scheduled or triggered agent warns** when its resolved choice uses a model key whose
  approval would make it wait (V15). An unattended start that hits the timeout is denied like any
  other, and nothing falls back (below).
- **The proxy is the default for every harness that accepts a base URL.** **Settled by V36:**
  *"A key reaches the harness through a proxy in `charterd`. The harness gets a loopback base URL
  and a per-chat token, never the key; the host swaps in the credential header and passes bytes
  through. Every harness that accepts a base URL uses it, which includes Claude Code, Codex and
  opencode."*
  - **What the harness gets:** a loopback base URL (Claude Code's `ANTHROPIC_BASE_URL`, Codex's
    `model_providers.<id>.base_url`, opencode's `baseURL`) and a token minted for that chat's run,
    in the variable the harness reads its key from. The token is worth nothing outside this host.
  - **What the proxy does:** it checks the token, swaps it for the key in the credential header,
    sends the request to the provider the key's mark names, and passes the request and its stream
    through byte for byte. It changes nothing else, and it shares a crate with M8's passthrough
    (V36).
  - **What it gives:** the agent never holds the key, so it cannot print it, send it through an
    open preset or use it outside the harness. Every model call crosses the host, so the host meters
    it, which closes the spend gap below for these harnesses. Ending a run drops its token.
  - **What it never does:** carry a `login` chat (§5, V36). A `login` chat never has a proxy URL
    set, and the proxy refuses a request that does not carry a `key` run's token.
  - **What it costs:** a chat's model calls fail while its host is down, and a chat already lives
    in its host (ADR 0068). A harness that treats a custom base URL as a gateway, for example in
    model discovery, may behave differently. The matrix (MS-2) records that per harness and model.
- **For a harness that takes no base URL, the key is in its environment, and the agent can read
  it.** That is the measurement the profile refusal was built on. **Settled by V36:** *"Environment
  injection is only for the rest, with the residual stated."* What follows is that residual, and it
  applies only to these harnesses. The picker states it beside a key chosen for one of them:
  - **The sandbox's egress narrows where the key can go, and does not close it.** A new project's
    presets are `model-providers`, `forge` and `toolchains` (ADR 0067 §3). The last two open the
    forge hosts and the package registries, and any of those can accept data an agent sends.
  - **The key can land in what purlis keeps.** An agent that prints it puts it in the
    conversation, and from there in the transcript archive, a session record or a memory. LW-8a's
    redaction masks vault values in the archive. The save scanner's shapes catch many provider key
    formats before a memory or record is committed. Neither is a guarantee.
  - **The key can be used outside the meter.** An agent that calls the provider itself with the key
    spends without the harness reporting it, so V34c's spend budget does not see that spend. The
    provider's own limit on the key is the only hard cap.
  - **The key is per user, billed to its owner and revocable at the provider.** `chatenv`'s refusal
    stands for every other route: only the adapter, from a source of `key` on a harness with no base URL, puts a provider key
    into a chat.
- **No fallback across sources.** **Decided by D-0087a:** *"charter never switches a chat's model
  source on its own after a failure. The start stops with a needs-you item that offers the other
  sources."* That covers a missing vault entry, a refused or timed-out approval, a provider that
  rejects the key, and an unreachable local endpoint.

### 8. Local models

**Settled by N15:** local models are a fourth source, and work offline.

- **An endpoint is detected, never assumed** (MS-17): Ollama, LM Studio or vLLM on loopback, or
  one the operator names on the LAN. The adapter points a harness at it where the harness takes an
  OpenAI-compatible or Anthropic-compatible base URL. The matrix states which harness and model
  pairs work (M10).
- **The sandbox lets a `local` chat reach its endpoint** through the `localhost` lane preset
  (ADR 0067 §3), and nothing more. A LAN endpoint is an audited exception, as any unlisted host is.
- **On a runner, `local` means the runner's own endpoint** (ADR 0078). A runner with a GPU serves
  its own chats. A desktop's local endpoint is never tunnelled to a runner.

### 9. A runner's chats

- **`login` on a runner is the runner's own login**, made in a shell tab on the runner (V9). The
  desktop's login never travels.
- **A `key` for a runner chat is forwarded once, when the run starts**, over the link, through V15's
  gate on the desktop and V9's allowlist and step-up (ADR 0078 §7). The runner keeps it in memory
  for that run only: in the runner host's own proxy (§7), or, for a harness with no base URL, in
  the run's process environment. It is never written there, and it is dropped when the run ends. **This is a new exception to ADR 0078 §7's "for that one command", and ADR 0078 is amended
  below.** A harness calls its provider throughout a run, with no command purlis sees, so a key
  forwarded per command would not be there when the harness needed it. An unattended runner uses
  its resident store (V9), whose approvals still happen on the desktop.

### 10. What the encryption promise covers

**Settled by M3 and C3's wording requirement.** purlis's end-to-end encryption, where it exists
(shared live sessions, sync, the relay), protects content from purlis's servers. It does not cover
model traffic, and it is never described as if it did:

- **`login`, `key` and `local`:** the prompt goes from the harness to the provider or the local
  endpoint, and never through purlis's servers. The provider sees it, under the user's own terms
  with that provider.
- **`gateway`:** purlis's gateway processes the prompt in memory, like any provider, with zero
  content retention. Only the token count, cost and model are logged, and ZDR upstreams are used
  where they exist (M3, MS-11, MS-15). The picker and the chat header show the gateway icon, and
  the gateway's own page says this.

The sentence purlis uses wherever encryption is described, from FR-15's site to the shared-session
view: **"End-to-end encryption protects your live sessions from Charter. Your model provider still
sees what you send it."**

### 11. The stores

No file is added for the choice itself. The `[model]` table joins files that already exist, and
it takes their tiers: `charter.toml` and a persona's `persona.md` (Plane), `workspace.json` (Plane
when LIVE, Clone state when LOCAL) and `charter.local.toml` (Clone state). A chat's own choice is
part of what its run records (ADR 0066), wherever that is kept.

One store is new. `docs/plane-format.md` records it now, marked **decided, not yet written**:

| Store | Tier | Backed up (FR-10) |
|---|---|---|
| `.charter/unattended-logins.json`: the operator's acknowledgement that a harness's subscription login may run unattended in this project, keyed by harness and login (SD-28, §3) | Clone state | yes |

The model-key mark of §7 lives in a vault's metadata, whose file already has a tier. MS-4 names
where it is kept when it writes it.

## ADR 0066, amended

- **`model_source` is read from the run's resolved model choice (§3), not from the profile's
  declaration.** ADR 0066 said *"It is read from the profile's declaration, never from output"*.
  The second half stands: a source is never read from output. The first half changes because a
  profile never holds a model choice. A profile still says which program runs on this machine,
  and the choice comes from the chat, the persona, the workspace or the project (see *What was
  rejected*).
- **`model_source` takes one of four values:** `login`, `key`, `gateway` and `local`. ADR 0066's
  four descriptions (*"the profile's login, an API key, a gateway, a local model"*) become those
  words, and "the profile's login" is read as the harness's own login.
- **A run also records `provider` and `model`**: the choice resolved at its start, then the model
  the harness reports. A report that differs from the choice is ADR 0066's `switch` and starts a
  new run, as a reported model switch already does.

## ADR 0073, amended

- **A declaration gains a `[model]` table** with the one template a level-1 harness needs:
  `flag`, an argv fragment that takes `{model}` only (for example `["--model", "{model}"]`).
  With it, a declared harness can run `login` with a chosen model, subject to §5's check where the
  harness has an adapter.
- **The other sources need an adapter.** `key`, `gateway` and `local` put a credential or a base
  URL into the harness's environment or config. A declaration may not name a variable that reads
  as a credential (ADR 0073 §5, ADR 0022), so these are adapter code, like level 2. So is §5's
  check of committed harness config.
- **§4 is narrowed for a credential-header swap.** **Settled by V36:** *"ADR 0073 §4's "proxy
  that edits its traffic" is narrowed to exclude a credential-header swap."* §4 counts a proxy that
  edits a harness's traffic as standing in. It now reads: **a proxy that edits what the harness or
  its model sends or receives stands in. The key proxy of ADR 0087 §7 does not.** That proxy
  changes only the credential header on a request the harness sends to the provider the user
  chose, and passes the rest, the stream included, byte for byte. The model never sees that header.
  Any other change to a request or a response is still standing in, and purlis never does it.
- **A capability, `model_sources`**, lists the sources a harness can take (ADR 0073 §6: yes, no
  with a reason, or unknown). The card and the picker read it, so a *no* is shown in words.
- **The harness × model matrix stays its own project data** (M10, MS-2), as ADR 0073 §3 says.

## ADR 0078, amended

- **§7's "Secrets" row gains one exception.** A model key for a runner chat (§9 above) is pushed at
  the run's start, through the same path and the same V15 gate. It is held in memory on the runner
  for that run only, and dropped when the run ends. Every other secret is still pushed for one
  command, and nothing else is kept on the runner. RR-18's allowlist covers model keys like any
  other secret.
- **MS-4's acceptance line is read with this exception.** *"Key never leaves the machine except to
  the provider"* becomes: **a key never leaves the desktop except to its provider, or to a runner
  the operator approved, for one run, over the link.**

## ADR 0083, amended

- **A spend meter names its source.** A `login` chat's spend is what the harness reports for a
  plan, which may not be money the user is billed per token. The meter and the budget view say
  *"plan usage, as the harness reports it"* for `login` and *"billed to your key"* for `key`. For
  `gateway` they use GT-CLOUD's wording, and for `local` they show nothing.
- **Behind the proxy, the host meters `key` spend itself.** Every model call of a proxied chat
  crosses `purlisd`, which counts it from the provider's own usage fields. Harness-reported spend
  stays V34c's source for every other chat.
- **Without the proxy, the meter can miss `key` spend.** On a harness with no base URL, an agent
  that uses the key directly is not reported (§7). The budget view says so where a key source is
  chosen for one, beside V34c's stated residual.

## What changes where

The code does not change with this record.

| Where | Change |
|---|---|
| `CONTEXT.md` | Gains **Model choice** and **Model source** (in this PR) |
| `docs/plane-format.md` | An entry for `.charter/unattended-logins.json`, **decided, not yet written** (in this PR). The `[model]` table is added to the entries of the files that hold it when MS-3 and MS-6 write it |
| ADR 0066, ADR 0073, ADR 0078, ADR 0083 | Amended above. Their texts are left as they are, and this record is the amendment |
| FD-13 | `ModelChoice` joins the neutral model, and the adapter applies it |
| MS-2 | The matrix, keyed by (harness, source, model) |
| MS-3, MS-14 | The picker, the icons, the "billed to" line, hidden and degraded tuples, per-harness tables |
| MS-4 | The model-key mark, and a key read by `purlisd` through V15 and never written to a file. Its environment path is only for harnesses that take no base URL, and waits on the proxy ticket for every other |
| The proxy ticket (new, V36) | The model-key proxy in `purlisd`: a loopback listener, per-run tokens, the header swap over M8's shared passthrough crate, the host's own metering, the refusal of `login` |
| MS-5 | Routing variables removed for `login`, committed harness config overridden or refused, and the test |
| MS-6 | Persona preferences and fallbacks, within one source |
| MS-17 | Local endpoints, the `localhost` preset, a runner's own endpoint |
| SD-28 | The unattended preference and its acknowledgement store |
| RR-18 | A model key forwarded once per run start (ADR 0078, amended) |

## What this costs

- **A proxy in the host's path.** Every proxied model call crosses `purlisd`, so a host that is down
  takes its chats' model calls with it, and the passthrough must keep up with streaming.
- **A key in the environment is still readable by the agent** on a harness with no base URL (§7).
  For those, the sandbox's egress only narrows where it can go, and the meter can miss spend made
  with it.
- **No automatic fallback.** A key that hits its quota stops the next chat's start until the
  operator picks another source, even when a working login is right there.
- **A committed harness config can block a `login` chat.** A repo whose `.claude/settings.json`
  points Claude Code at a gateway cannot run that harness on a login in purlis. It runs on a key
  instead.
- **A harness's default model is shown as unknown** until the harness reports it, and some
  harnesses report it late or not at all.
- **Five places a default can come from.** The header says where a chat's choice came from, so the
  precedence of §3 can be read off the screen.

## What was rejected

- **A model choice per profile.** A profile says which program runs on this machine (ADR 0073 §5).
  The model is a property of the work and the role, which travel with the project. A profile
  already may not hold a credential. This is why ADR 0066's "read from the profile's declaration"
  is amended.
- **Writing a key into the harness's config file.** It would put a long-lived credential on disk
  where every backup and every process of the user's can read it.
- **Gateway tokens for the direct path.** Research 09 §6.1's short-lived tokens are the gateway's.
  A direct key is the provider's, and purlis cannot mint a shorter one.
- **Falling back to `login` when a key fails.** It moves the bill silently (D-0087a).
- **Treating a harness-stored key as source `key`.** purlis would have to read the harness's
  credential store to know it, which V9 forbids.
- **Forwarding a runner's model key per command**, as ADR 0078 §7 does for every other secret. A
  harness calls its provider with no command purlis sees, so the key would never be there in time.
- **Any vault entry as a key source.** A committed reference could then make the host inject any
  secret into a chat (§7).

## Decided (dispatcher)

Two questions of this record's first draft were not major, and the dispatcher decided them,
recorded in DECISIONS.md as D-0087:

- **D-0087a:** *"charter never switches a chat's model source on its own after a failure. The
  start stops with a needs-you item that offers the other sources (M1, who-pays always shown)."*
  Applied in §7.
- **D-0087b:** *"Committed files may name `login`, `key` or `local`, never `gateway`. `gateway`
  comes only from the chat's picker, `charter.local.toml` or org policy, and waits for GT-CLOUD."*
  Applied in §1 and §3.

## Ruled (V36, 2026-10-01)

The operator chose option (a), as recommended:

1. **V36: a key reaches the harness through a proxy in `purlisd`.** The harness gets a loopback base
   URL and a per-chat token, never the key, and the host swaps in the credential header and passes
   bytes through. Every harness that accepts a base URL uses it, which includes Claude Code, Codex
   and opencode. Environment injection is only for the rest, with the residual stated. The proxy
   shares a crate with M8's passthrough. ADR 0073 §4's "proxy that edits its traffic" is narrowed to
   exclude a credential-header swap. The proxy is never used for `login`. Rejected: environment
   injection everywhere with a hardening ticket, which would have given up the claim that the model
   never sees the key.
