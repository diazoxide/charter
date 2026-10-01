# A chat's model is one choice from four sources, and charter never carries a harness login

**Proposed 2026-10-01**, drafted for program-map ticket MS-1 (#700). It follows these of the
operator's rulings:

- **M1:** *"One model choice with three sources: (1) the harness's own login, (2) a BYO API key
  from the vault, desktop → provider directly, (3) Charter credits through the cloud gateway.
  Chosen per chat, persona or workspace, and always shown."*
- **M2:** *"Sources 1 and 2, including the model picker, are offline and therefore free. Only
  credits need the cloud."*
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
- **M9**, in part: *"Charter credits never power Claude Code without an Anthropic agreement. …
  The picker hides credits where forbidden, says why, and offers BYO."*
- **M10:** *"A harness×model matrix as plane data, with the states native / supported / degraded
  (behind an experimental switch, breakage listed) / blocked (never offered)."*
- **N15:** *"local models (Ollama/LM Studio/vLLM) as a fourth model source, free and offline"*.
- **V9**, in part: *"Harness logins happen only through the harness's own flow in a runner shell
  tab; charter never reads, copies, stores or relays a harness credential"*.
- **X23** and **X40**, accepted with the consistency review as recommended. X23: *"Is N52 limited
  to choosing among (harness, source, model) tuples marked native or supported in M10, applied
  only when a chat is spawned (never by re-routing a running chat), and never picking a gateway
  route for a harness on a subscription login?"* X40: *"Does setting "must route via gateway"
  show admins "this disables Claude/ChatGPT subscription logins for members", and does the picker
  hide source 1 for affected harnesses with that reason?"*
- **C3**, its wording requirement: *"the encryption protects content from Charter Cloud, not from
  the harness's model provider."*
- **V15**, in part: a vault or secret sets its approval, *"off · every use · once per chat · for
  N minutes in this chat · require OS authentication"*, and *"The command needing the secret
  **waits** (it does not fail)"*.
- **V34c**, through [ADR 0083](0083-telemetry-is-one-otel-pipeline-on-the-device-and-only-the-signed-updater-calls-charter.md):
  *"A spend budget may pause a chat on harness-reported spend."*

It builds on [ADR 0066](0066-a-chat-is-a-ulid-a-run-is-a-stretch-of-its-conversation-and-a-device-is-random.md)
(the run and its `model_source`), [ADR 0067](0067-a-chat-runs-in-a-sandbox-charter-compiles-for-its-harness.md)
(the sandbox and its egress presets), [ADR 0068](0068-a-chat-lives-in-charterd-and-the-app-is-its-client.md)
(one path to a secret's value), [ADR 0072](0072-charter-has-five-concepts-and-every-other-word-belongs-to-one-of-them.md)
(the five concepts), [ADR 0073](0073-a-harness-is-declared-as-data-and-a-chat-runs-it-at-one-of-three-levels.md)
(harness declarations and capabilities), [ADR 0078](0078-a-runner-is-charterd-behind-a-connector-and-charters-own-keys-say-who-is-on-the-link.md)
(runners) and ADR 0083 (telemetry and spend). It amends ADR 0066, ADR 0073 and ADR 0083, each in
a section of its own below. MS-2 (the matrix), MS-3 (the picker), MS-4 (a key from a vault),
MS-5 (the harness's login), MS-6 (persona preferences), MS-14, MS-17 (local models), MS-18 and
SD-28 build on it. The gateway's own tickets (MS-7 to MS-16) are cloud work and wait for
GT-CLOUD. This record fixes only how the gateway appears as a source.

Its concept is **Chat**: the model is one of a chat's settings, beside its harness, profile and
persona (ADR 0072 §2). A persona and a workspace give a chat its default.

## Where charter is today

- **charter chooses no model.** A chat runs its harness with the harness's own default model and
  the harness's own login. The only "model" in charter's code is the word in a refusal.
- **charter refuses to hold a credential where the model can read it.** A profile may not set an
  environment variable named like a credential (`profiles::named_like_a_credential`), because
  *"anything set on the harness process reaches the shell the model runs"*, which was measured on
  Claude Code and Codex. `chatenv` starts every chat from an empty environment and does not pass
  `ANTHROPIC_*`, `OPENAI_*`, `GEMINI_*` or the other provider prefixes unless the operator lists
  them in `[chat_env] pass`.
- **ADR 0066 already names the attribute.** A run's `model_source` is *"where the model is
  reached: the profile's login, an API key, a gateway, a local model"*, and a model switch starts
  a new run. Nothing sets it yet.
- **Each harness is configured differently.** Research 09 §1.1 measured it. Claude Code takes
  `ANTHROPIC_*` variables and `--model`. Codex takes `-c` overrides, `--profile` and `--model`.
  opencode takes `OPENCODE_CONFIG_CONTENT` and `--model provider/model`. Some harnesses, such as
  Cursor's, take nothing from outside.
- **Vendor terms draw hard lines** (research 09 §3, §6.3). A subscription login's credential must
  not be collected, stored or relayed. A subscription session must not be pointed at a gateway.
  Claude Code is not to be routed to a non-Claude model.

## The decision

**A chat runs on one model choice: a model, its provider, and the source it is reached through.
There are four sources: the harness's own login, a key from a vault, the gateway, and a local
model. The choice is made per chat, persona, workspace or project, compiled into the harness by
its adapter when a run starts, and always shown with its source and who pays. charter never reads,
stores or relays a harness login, and never points a login at the gateway. The end-to-end
encryption promise covers live sessions, never model traffic.**

### 1. One model choice

```toml
[model]
source = "key"                         # login | key | gateway | local
provider = "anthropic"                 # who serves the model; implied by the harness for login
model = "claude-sonnet-4-5"            # as the provider names it; omitted = the harness's default
key = "vault:providers/anthropic"      # source "key" only: a vault reference, never a value
endpoint = "ollama"                    # source "local" only: a detected local endpoint (MS-17)
```

- **`source` is one of four words, and only four.** Settled by M1 and N15, which give three
  sources and a fourth. A new source is a change to this record. An extension may add an
  *endpoint* to `local` or a *provider* to `key`, but not a fifth source (E1's model-source seam is
  the provider and endpoint lists).
- **`model` may be left out.** Then the harness uses its own default for that source, which is
  today's behaviour, and the chat shows *"the harness's default"* until the harness reports the
  model it used (§6).
- **`key` and `endpoint` are references, not values.** A `key` is a vault reference
  (`vault:<vault>/<name>`), the form `secretshape` already exempts. A committed reference names
  where each member keeps their own key. It never holds anyone's key.
- **The same shape everywhere it is set**, so one reader parses it: `charter.toml`,
  `charter.local.toml`, `workspace.json`, a persona's `persona.md` front matter, and the chat
  itself (§3).

### 2. Four sources

| `source` | What it is | Who pays | Route | Shown as |
|---|---|---|---|---|
| `login` | **The harness's own login**: whatever `claude /login`, `codex login` or the harness's own flow set up. The default | the harness's account: the user's plan or the harness's own key | harness → its vendor. charter is not on the path | lock · *"your Claude plan"* (the harness and its plan, as the harness reports it) |
| `key` | **A key from a vault**: the user's own provider key (BYO, M1 source 2) | the key's owner, at the provider | harness → provider, direct from this device | lock · *"your Anthropic key"* |
| `gateway` | **Through Charter's gateway**: an org's own provider keys (M5, M8 step 2), or Charter credits where a vendor allows it (M9) | the org's key, or Charter credits | harness → Charter's gateway → provider | gateway icon · *"your org's key, via Charter"* or *"Charter credits"* |
| `local` | **A local model** on this device or this runner: Ollama, LM Studio, vLLM (N15, MS-17) | nobody | harness → a loopback or LAN endpoint | lock · *"on this machine"* |

- **Lock for direct, gateway icon for via Charter.** Settled by M3. `login`, `key` and `local`
  never pass through Charter, so all three show the lock. Only `gateway` shows the gateway icon.
- **`login`, `key` and `local` work with no account and no network to Charter.** Settled by M2,
  which keeps them off the cloud. `local` works with no network at all.
- **`gateway` waits for GT-CLOUD.** Until then, the picker does not offer it. Its tickets
  (MS-7 to MS-16) build it, and this record changes nothing in them.
- **`login` covers a harness's own key too.** Some harnesses log in with a key the harness itself
  stores, such as an API key typed into the harness's own login prompt. That is still the
  harness's login: charter does not read it, and it is not source `key`.

### 3. Where a choice is set, and which one wins

A chat's choice is the first of these that sets one:

1. **The chat**: the picker in the new-chat flow or the chat header (MS-3). Changing it starts a
   new run (ADR 0066's `switch`).
2. **The persona the chat runs as**: its preference and fallbacks, from `persona.md` (M6, MS-6).
3. **The workspace**, from `workspace.json`.
4. **This machine's override for the project**, from `charter.local.toml`.
5. **The project**, from `charter.toml`.
6. **Nothing set**: `login` with the harness's default model. That is today's behaviour.

- **Each field falls back separately.** A persona that names only a model keeps the workspace's
  source. A choice that becomes invalid at a lower level, such as a model the matrix blocks for
  this harness, is skipped with a line saying why.
- **The persona wins over the workspace.** A persona is a role, and its model is part of the role,
  as a reviewer persona that names a stronger model shows. A workspace's choice is a default for
  work that has no role. **`charter.local.toml` sits below both** because it is one machine's
  override of the project's default, not of a role's or a task's.
- **Org policy clamps all of them** (M5, C9; MS-7, after GT-CLOUD). Strictest wins. An allowlist
  can refuse a choice, and "must route via the gateway" removes `login` for the harnesses it
  covers, with X40's sentence in the picker and in the policy view.
- **Persona fallbacks stay within their source** (M6). A persona's fallback list is a list of
  models. If the first is blocked or refused, the next model on the *same* source is tried. A
  change of source is never automatic (§7 and the questions at the end).

### 4. Compiled at spawn by the harness's adapter

**Settled by M6:** compiled into each harness's config at spawn.

- **The choice is resolved once, when a run starts,** and passed to the harness's adapter
  (FD-13). The adapter turns it into that harness's flags, environment or config, the
  per-harness mapping of research 09 §6.1. A running chat is never re-routed (X23). A new choice
  is a new run.
- **The matrix decides what may be offered** (M10, MS-2). A (harness, source, model) tuple that is
  `blocked` is never offered or compiled. A `degraded` one is offered only behind its switch, with
  its breakage listed. Credits are hidden where forbidden, with the reason and BYO offered (M9,
  MS-14). Claude Code is never offered a non-Claude model.
- **A harness that cannot take a source does not get it.** A harness whose configuration cannot
  be injected per session, such as Cursor's CLI, runs on `login` only, and the picker says why.
- **A key goes into the harness's process environment, never into a file.** A harness that reads
  its configuration from a file is given a generated file that names the variable
  (`${ANTHROPIC_API_KEY}`, `{env:…}`), never the value. A key is never written to disk by charter.

### 5. What charter never does with a login

**Settled by V9:** charter never reads, copies, stores or relays a harness credential.

- **A `login` chat starts with no routing variables.** The adapter removes `ANTHROPIC_BASE_URL`,
  `ANTHROPIC_AUTH_TOKEN`, `OPENAI_BASE_URL`, Codex's `model_providers` overrides and their like
  for that harness, whatever the operator's `[chat_env] pass` lists. A subscription session is
  never pointed at the gateway, or anywhere other than its vendor. This is research 09 §6.3's
  launch rule, and MS-5's test holds it: *"no subscription token crosses charter"*.
- **charter never captures a login.** Logging in is the harness's own flow, in a shell tab (V9),
  with the harness's own files. charter only says which login is in use, from what the harness
  reports, such as its account type in a status line or a hook.
- **Smart routing never moves a `login` chat** (X23). It picks among tuples marked native or
  supported, only at spawn, and never a gateway route for a harness on a login.

### 6. Always shown

**Settled by M1:** always shown.

- **The chat header and the picker show the model, the source's icon and who pays** (§2's last
  column), from the run's resolved choice. While the model is the harness's default and the
  harness has not reported it, the header says *"the harness's default"*. A reported model then
  replaces it. If the harness reports a model other than the one chosen, the header shows the
  reported one with a warning. That is a report, so per ADR 0066 it starts a new run and is never
  acted on beyond that.
- **The first-hour words** (ADR 0072 §3). The picker says *"your key"*, never *"vault"*, and names
  the harness by its product name. "Source" itself is not shown in the first hour. The icon and
  the "who pays" line carry it.
- **Each run records its choice.** A run's `model_source` is the source word, and the run also
  records the provider and the model, from the choice and then from the harness's report (ADR 0066,
  amended below). Session records and the audit's `run.started` carry the same values (ADR 0075).

### 7. A key's path, and what it costs to keep it there

- **One path to the value.** `charterd` reads the key at spawn, as it reads every vault value
  (ADR 0068 §1), and hands it to the harness's process. **The key's approval setting applies**
  (V15): a key set to *every use* or *OS authentication* makes the chat's start wait for the
  operator, and the ask shows the chat, the harness, the key's name and that it is the chat's model
  key. The read is recorded like any other.
- **The key is in the harness's environment, and so its agent can read it.** That is the
  measurement the profile refusal was built on. Three things bound it:
  - the chat sandbox's egress presets (ADR 0067 §3) refuse every host that `model-providers`,
    `forge` and `toolchains` do not list, so the key can reach its provider and little else;
  - the key is per user, billed to its owner, and revocable at the provider;
  - `chatenv`'s refusal stands for every other route. Only the adapter, from a source of `key`,
    puts a provider key into a chat.

  A local proxy in `charterd` that held the key and gave the harness a per-chat token would keep
  the key out of the agent's reach. That proxy is a question for the operator (see the end of
  this record).
- **No fallback across sources.** A `key` whose vault entry is missing, a refused approval, a
  provider that rejects the key, or an unreachable local endpoint stops the chat's start with a
  needs-you item that names the cause and offers the other sources. charter never falls back to
  `login` on its own. A login bills a different account, often a plan with its own limits, and a
  silent move between who pays is a cost the operator did not choose (see the questions at the end).

### 8. Local models

**Settled by N15:** local models are a fourth source, and they work offline.

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
- **A `key` for a runner chat is forwarded once, when the run starts**, over the link, through
  V9's forwarding with its allowlist and step-up (ADR 0078 §7). It is held in that run's process
  environment on the runner, and in no file. An unattended runner uses its resident store (V9).

### 10. What the encryption promise covers

**Settled by M3 and C3's wording requirement.** charter's end-to-end encryption, where it exists
(shared live sessions, sync, the relay), protects content from Charter's servers. It does not cover
model traffic, and it is never described as if it did:

- **`login`, `key` and `local`:** the prompt goes from the harness to the provider or the local
  endpoint, and never through Charter. The provider sees it, under the user's own terms with that
  provider.
- **`gateway`:** Charter's gateway processes the prompt in memory, like any provider, with zero
  content retention. Only the token count, cost and model are logged, and ZDR upstreams are used
  where they exist (M3, MS-11, MS-15). The picker and the chat header show the gateway icon, and
  the gateway's own page says this in these words.

The sentence charter uses wherever encryption is described, from FR-15's site to the shared-session
view: **"End-to-end encryption protects your live sessions from Charter. Your model provider still
sees what you send it."**

## ADR 0066, amended

- **`model_source` takes one of four values:** `login`, `key`, `gateway` and `local`. The four
  descriptions (*"the profile's login, an API key, a gateway, a local model"*) become those words.
- **A run also records `provider` and `model`**: the choice resolved at its start, then the model
  the harness reports. A report that differs from the choice is ADR 0066's `switch` and starts a
  new run, as a reported model switch already does.

## ADR 0073, amended

- **A declaration gains a `[model]` table** with the one template a level-1 harness needs:
  `flag`, an argv fragment that takes `{model}` only (for example `["--model", "{model}"]`).
  With it, a declared harness can run `login` with a chosen model.
- **The other sources need an adapter.** `key`, `gateway` and `local` put a credential or a base
  URL into the harness's environment or config. A declaration may not name a variable that reads
  as a credential (ADR 0073 §5, ADR 0022), so these are adapter code, like level 2.
- **A capability, `model_sources`**, lists the sources a harness can take (ADR 0073 §6: yes, no with a
  reason, or unknown). The card and the picker read it, so a *no* is shown in words.
- **The harness × model matrix stays its own project data** (M10, MS-2), as ADR 0073 §3 says.

## ADR 0083, amended

- **A spend meter names its source.** A `login` chat's spend is what the harness reports for a
  plan, which may not be money the user is billed per token. The meter and the budget view say
  *"plan usage, as the harness reports it"* for `login`, *"billed to your key"* for `key`,
  *"credits"* or *"your org's key"* for `gateway`, and show nothing for `local`. V34c's rule, a
  budget pausing on reported spend, holds for every source, and its residual risk is stated as it
  already is.

## What changes where

The code does not change with this record.

| Where | Change |
|---|---|
| `CONTEXT.md` | Gains **Model choice** and **Model source** (in this PR) |
| `docs/plane-format.md` | No new file. The `[model]` table is added to `charter.toml`'s, `charter.local.toml`'s, `workspace.json`'s and `persona.md`'s entries when MS-3 and MS-6 write it, each **decided, not yet written** until then. Its tiers are those of the files |
| ADR 0066, ADR 0073, ADR 0083 | Amended above. Their texts are left as they are, and this record is the amendment |
| FD-13 | `ModelChoice` joins the neutral model, and the adapter compiles it |
| MS-2 | The matrix, keyed by (harness, source, model) |
| MS-3, MS-14 | The picker, the icons, the "who pays" line, hidden and degraded tuples |
| MS-4 | A key from a vault: read by `charterd` through V15, put into the environment by the adapter, never written to a file |
| MS-5 | The routing variables removed for `login`, and the test that no subscription token crosses charter |
| MS-6 | Persona preferences and fallbacks, within one source |
| MS-17 | Local endpoints, the `localhost` preset, a runner's own endpoint |
| RR | A `key` forwarded once per run start (§9) |

## What this costs

- **A key in the environment is readable by the agent** (§7). The sandbox's egress is what keeps
  it from going far. A local proxy would close that, at the cost of charter speaking each
  provider's streaming protocol between the harness and the provider.
- **No automatic fallback.** A key that hits its quota stops the next chat's start until the
  operator picks another source, even when a working login is right there.
- **A harness's default model is shown as unknown** until the harness reports it, and some
  harnesses report it late or not at all.
- **Four places a default can come from.** A chat's header says where its choice came from
  (*"from the persona"*), so the precedence of §3 can be read off the screen.

## What was rejected

- **A model choice per profile.** A profile says which program runs on this machine (ADR 0073 §5).
  The model is a property of the work and the role, which travel with the project. A profile
  already may not hold a credential.
- **Writing a key into the harness's config file.** It would put a long-lived credential on disk
  where every backup and every process of the user's can read it.
- **Gateway tokens for the direct path.** Research 09 §6.1's short-lived tokens are the gateway's.
  A direct key is the provider's, and charter cannot mint a shorter one.
- **Falling back to `login` when a key fails.** It moves the bill silently (§7).
- **Treating a harness-stored key as source `key`.** charter would have to read the harness's
  credential store to know it, which V9 forbids.

## For the operator's ruling

1. **May charter ever change a chat's source on its own when the chosen one fails?** For example,
   it could fall back from a key that hit its quota to the harness's login, or from the gateway to
   a key. It changes who pays. **Recommended: never.** The chat's start stops with a needs-you item
   naming the cause and offering the other sources. A persona's fallbacks move only between models
   on the same source.
2. **May a committed file choose `gateway`?** `charter.toml`, `workspace.json` or a persona's
   front matter naming `gateway` would make every member's chats spend credits or an org's key.
   **Recommended:** a committed file may name `login`, `key` (each member's own vault entry) or
   `local`. `gateway` comes only from the chat's own picker, `charter.local.toml`, or org policy
   (M5), so no plain merged pull request makes others spend.
3. **Should a key reach the harness through a local proxy in `charterd` rather than its
   environment?** The proxy would hold the key and give the harness a per-chat token and a
   loopback base URL, so the agent could not read the key. It forwards bytes unchanged apart from
   the credential header, and is never used for `login`. **Recommended:** environment injection
   now (MS-4, as filed), with §7's residual stated where a key is chosen. File the proxy as a
   hardening ticket for the Anthropic Messages and OpenAI Responses harnesses, to land before any
   org policy relies on keys being unreadable by agents.
