# A handoff's consent is the harness's own prompt

`charter handoff <workspace>` opens a chat and sends it a first message. A first message is
not a suggestion: the new chat starts working on it with the operator's authority, in a
workspace that may hold clones and credentials, and nobody reads it again before it does.
The chat that wrote the brief is the same model that decided the work should move — so its
own proposal, however faithfully it quotes the brief, cannot be the consent for sending it.

**The gate is the host's own `ask` rule for `charter handoff`, and charter's hook refuses
what that prompt cannot cover.** Removing the rule is the operator's choice.

The mechanics — every refusal, every measurement and the exact tables — are in
[`docs/handoff.md`](../handoff.md) and [`docs/hooks.md`](../hooks.md). What the grill settled
is `docs/superpowers/specs/2026-09-10-chat-handoff.md`; the rulings taken on the way are that
plan's own `## Controller rulings`. This record holds the decisions that were expensive to
reach, what each one costs, and what was measured — the half that is gone first and cannot be
reconstructed from the code.

## The decision

- **The rule.** `Bash(charter handoff *)` under `permissions.ask` in `.claude/settings.json`,
  and `"charter handoff *": "ask"` under `permission.bash` in `opencode.json`
  (`commands.HANDOFF_ASK_PATTERN`). Codex has no command-pattern permissions, so there is
  nowhere to put it there.
- **Who writes it.** `charter init`, through `commands.ensure_handoff_gate`, which is
  `charter guard ask` with the pattern fixed — the same `_guard_apply` transaction, so `init`
  and the operator's own command cannot come to disagree about what "present" means. It is
  the one `permissions` entry `init` writes.
- **Who does not.** `charter update`, and everything else that runs again. A plane `init` did
  not make adopts the rule through the news entry's `adopt: guard handoff`. A writer that ran
  on every update would quietly reverse a removal, and removing the rule is the operator's
  decision; `charter doctor`'s `handoff gate` row names it missing and says so in the same
  breath.
- **Where it has to be.** Claude Code reads project settings from the session's own
  directory and does not walk up, and a framed chat stands in `workspaces/<ws>/`. The plane's
  restrictive rules therefore ride into every generated workspace and clone settings file
  (`claude_code.RESTRICTIVE_BUCKETS = ("ask", "deny")`, #942/#948). `allow` never travels:
  a restriction only ever adds a prompt, while a grant copied sideways puts a permission in
  force where nobody clicked for it.
- **What charter's hook adds.** `hooks._handoff_refusal` (A7) refuses a handoff from a
  sub-agent, from an unattended run, in a spelling the rule was measured not to match, and
  with a stdin the prompt cannot show.
- **No depth limit.** A handed-off chat may hand off again, because every hop needs its own
  yes.

## Why not the alternatives

- **A hook `ask` cannot be the gate.** `hooks._ask` answers `allow` when the payload says
  `permission_mode: bypassPermissions`, because a hook `ask` floors the decision at a prompt
  the host cannot lift and would hang an unattended run. So the one mode where a handoff most
  needs a human is the mode where a hook `ask` is not one. opencode has no ask channel at
  tool time at all — its `tool.execute.before` can allow or throw, which is that harness's
  own `ask-decisions` deficit.
- **A charter-side confirmation on stdin.** There is no terminal inside a Bash tool call, and
  stdin is already carrying the brief.
- **A quiz in the conversation.** It is the model's own proposal. The skill still shows the
  brief in full, because the operator reading it before the prompt appears is what makes the
  prompt answerable — but it is a procedure, not consent.
- **A charter-side rule list.** [ADR 0014](0014-policy-that-fits-a-pattern-belongs-to-the-host.md):
  policy that fits a command pattern belongs to the host, and a second engine cannot win
  against one whose ask rule prompts even when the hook returned `allow`.
- **`--brief-file`.** A prompt that shows a path is an approval of a path. The brief arrives
  as one quoted heredoc so the prompt shows the exact text the new chat is sent.
- **A depth limit.** Nothing to buy: every hop already needs a yes.

## The decisions somebody would otherwise undo

### The prompt is consent for one spelling, and it is not a security boundary

Claude Code says of its own Bash rules that one *"isn't a security boundary around the
program"*
([permissions#bash-rule-limits](https://code.claude.com/docs/en/permissions#bash-rule-limits)),
and the measurement agrees: on Claude Code 2.1.268, against a stand-in model API under a
throwaway `HOME`, `charter 'handoff'`, `python3 -m charter handoff`, a path to charter,
`charter $'handoff'`, `charter {handoff,}`, `charter hando?f`, and a handoff inside `eval`,
`bash -c` or a `bash <<'EOF'` body all ran with **no prompt**, while the canonical spelling
asked in every mode including `bypassPermissions`. The tables are in `docs/handoff.md`.

So A7 refuses the spellings it can recognise that the rule was measured not to match. That
is a rule a model can follow, not a wall: A7 reads a command's words and is not a shell, and
it does not see a handoff run by an interpreter, through a variable, from a script file, or
behind a shell hidden by a name charter cannot know (`r() { bash; }; r <<'EOF'`).

**What it costs, stated rather than discovered.** `python3 -m charter handoff` is refused —
and that is the spelling `CONTRIBUTING.md` gives for running a checkout. Allowing it would
run a handoff the host never asked about. Re-measured on this branch, 2026-09-12:
`python3 -m charter handoff beta <<'BRIEF'` and `charter 'handoff' beta <<'BRIEF'` both
answer `handoff-spelling`.

### A heredoc body is judged by its own opener and by its own pipeline

A brief is a heredoc body. So is a commit message, a document, a `tee` input — and so is a
script. Charter has to decide which bodies are commands, for two guards at once (A7 and the
secret-leak guard share one plan), and the two fail opposite ways, so the unit had to be
right rather than convenient.

**The rule.** A body is read as commands when its opener's own program is a shell or an
interpreter (`bash`, `python3`, one of those behind `env`/`nohup`, `ssh` where the remote
shell runs it), when charter cannot resolve the opener to a name at all (`${RUNNER} <<'EOF'`,
`$(which bash) <<'EOF'`), or when an executor stands downstream of it **in the same
pipeline**. Everything else hands its body on as data.

**Why the pipeline and not the line or the word.** Line-scoping refused ordinary commands —
22 of them, and `git commit -F -` inside `( … )` refused this repo's own commit messages.
Word-scoping let `cat <<'A' | bash` through, which is a script. The pipeline is what a shell
actually uses: measured 2026-09-12 on GNU bash 3.2.57 and zsh 5.9, `cat <<'A' | bash` runs
the body in both, and `cat <<'A'; …` runs the command after the `;` and never the body. Nine
review rounds converged on it; each round's finding is recorded at its own line in
`charter/hooks.py` rather than here.

Re-measured against A7 on this branch, 2026-09-12, each with a handoff in the body:
`cat <<'A' | bash` → `handoff-shell-string`; `cat <<'A' > notes.md` → allowed;
`git commit -F - <<'EOF'` → allowed.

**What it costs.** When the word naming the program is itself a variable or a substitution,
charter treats the body as something that could run, so a brief-shaped body under
`${EDITOR} <<'EOF'` is refused although the program is a pager. A fail-safe that switched off
for a friendly-looking name would not be one. Two shapes are named limits rather than fixed:
a `'` in a non-reader's heredoc body leaves a shell string unparseable and lets a later
`eval '…'` through, and the scan does not honour `#` comments.

### The brief's fence carries a per-render marker, and the brief keeps its lines

A chat that reopens with no conversation is shown its brief as a quoted block. The brief is
attacker-influenced by construction — it is whatever another chat was told to work on — so
the quotation has to be one it cannot close.

The first shape bought that with line structure: escape every newline, and a value that
cannot start a line cannot write the line that ends the quotation. It works, and it costs the
feature its purpose — a 12 KB document flattened onto one line is materially harder for the
chat to work from, and being worked from is the only thing the brief is for.

**So the fence carries a marker minted at the render** (`hooks._BRIEF_OPEN`,
`_brief_token`): six `os.urandom` bytes, hex. `charter handoff` wrote the brief at an earlier
moment — possibly days and a restart ago — and nothing rewrites it, so its author cannot know
the marker. A brief may spell `⟨/brief⟩` as often as it likes and close nothing. The size is
against a bound charter already holds: a guess costs at least a dozen bytes of a brief capped
at 12,288, so about a thousand guesses against 2^48 markers, and no bigger brief or faster
machine moves that.

**What it costs.** The block is not reproducible byte-for-byte between renders, so nothing
may compare two of them for equality; `os.urandom` rather than `random`, because a `fork`
hands both sides of `random` the same stream and a predictable marker is the property gone.
Escaping happens at the render and never on the way into storage, so what was recorded stays
the text the operator approved: measured on this branch, `\n` survives as a line break while
`\r`, `\x1b[` and U+2028 are escaped.

### The arrivals record is a directory of empty files

A handoff opens a chat nobody is looking at, so the workspaces strip marks where it landed
until somebody looks. The mark is plane-scoped state, and a plane runs many charter processes
at once.

**One file listing the marked names made every writer a read-modify-write over the whole
set**, and it lost marks three ways: two handoffs landing together, a handoff landing while a
switch cleared, and a switch clearing while a handoff landed. A lost mark is exactly the
invisibility the record exists to end — the chat is open and nothing points at it.

Measured 2026-09-12 on macOS 25.2.0 (APFS), Python 3.14.4, two threads released from one
`threading.Barrier`, 400 rounds each: a read-modify-write over one file (read the set, add
the name, atomic replace — the shape charter's other set-files use) **lost a mark in 400 of
400 rounds**; a directory with one empty file per mark lost **0 of 400**. The suite pins the
directory shape at 60 rounds on each of the three interleavings
(`tests/test_a_handoff_lands_at_the_front_of_the_strip.py`).

A lock was considered and refused: a plane is shared by processes that can be killed at any
moment, so a stale lock nothing clears is a worse failure than the one it fixes — and it
would still be a fix by timing. One file per name makes recording and clearing independent
operations on different paths, so there is no interleaving to get wrong.

**What it costs.** A name is now joined onto a path, so `workspace.valid_name` has to be
asked again at the join — #442 arriving through a record instead of a listing. The mark is
the file's existence, so there is nowhere to put a timestamp, and nothing may later ask when
a workspace was marked without changing the shape.

### Duplicate detection differs by caller, because the callers prefer opposite errors

A handoff records a todo in the target workspace. `charter ws todo` records one too, and both
ask "is this already on the list" — with the same metric and the opposite answer when it is
ambiguous.

- **`charter ws todo` refuses a duplicate.** A false duplicate costs the operator a rephrase;
  a missed one leaves two near-identical todos, and closing one leaves its twin looking
  outstanding. It wants the catching rule: whole texts, Jaccard over words longer than three
  characters, threshold 0.5.
- **A handoff records nothing and opens the chat anyway.** A false duplicate silently drops a
  real todo and the work goes invisible; a missed one is a second todo beside a second chat,
  which may be exactly what was approved. It wants the careful rule: first lines only, and
  where fewer than three words overlap — too thin to be evidence in either direction —
  identity of the normalised first line decides.

**What was measured, re-measured on this branch 2026-09-12.** Every handoff todo ends in the
same 18-word provenance sentence, which `memstore.wordset` reduces to **nine** comparable
words once it drops everything three characters or shorter — eight fixed, plus the source
chat and workspace, which two handoffs out of one chat also share. Over whole texts,
`Fix the widget` and `Ship the release` — briefs with no word in common — score **0.750**
(0.769 when the source chat and workspace are different words), so the second handoff into a
workspace recorded nothing at all, silently. Over first lines they score **0.000**. And the
overlap is no better in the other direction on text this short: one shared word out of two is
**0.5** exactly, so `Fix the widget` swallowed `Break the widget`,
while `fix the bug` has no comparable word at all and two of them agree on nothing.

**What it costs.** Moving the threshold to 0.51 answers one of those pairs and neither of the
others, so there is no number here to tune — which is why the fallback is identity rather
than a second threshold. Two first lines differing only past `memstore.TITLE_MAX` (72
characters) are stored as the same title and read as one todo: stated rather than engineered
around. And punctuation is not collapsed, so `Fix the widget!` is a second todo beside
`Fix the widget`.

## Consequences, including what they cost

- **`charter init` writes a `permissions` key**, which it never did before. The note in
  `commands.cmd_guard_ask` that said so is corrected rather than deleted, because it is still
  the rule for every other rule: charter is the editor, the operator is the author.
- **Every `charter guard ask` rule now reaches a workspace chat**, not only the handoff rule.
  That is the prerequisite fix this record consumed (#942/#948), and it changes what a plane's
  existing rules do: a rule written months ago starts prompting in chats where it silently did
  not. Workspaces pick it up at their next launch, or at `charter workspace reinit --all`.
- **Codex has no prompt in front of a handoff at all.** Its hook payload still carries
  `agent_id` and `permission_mode` — measured on codex-cli 0.147.0, `codex exec` reporting
  `bypassPermissions` under every approval setting tried except `--approve-for-me` — so a
  sub-agent's or an unattended handoff is refused, and any other Codex handoff runs without
  asking. Named as a `handoff-gate` deficit, so `doctor` says it rather than leaving it to be
  found.
- **opencode cannot refuse a sub-agent's or an unattended handoff.** The payload charter's
  plugin builds carries neither field, so the `ask` rule in `opencode.json` is the whole gate
  there.
- **The gate is in force where charter writes settings and nowhere else** — the plane root, a
  workspace directory, a checkout's own root. A chat rooted in `docs/`, in `personas/<p>/`, or
  deep inside a checkout is not gated, and `doctor` says so rather than implying otherwise.
- **A declined prompt is still a cleared routing mark.** `hooks.pretooluse` clears
  `routing: require`'s pending mark for a handoff that passed A7, before the host's prompt is
  answered — the same thing `pretooluse_dispatch` does for a declined Agent call.
- **The tally cannot answer "who handed off what".** A handoff's row is
  `{event, ts, placement, created}` and nothing else: no workspace name, because a LOCAL
  workspace's name must not reach a committed file, and no persona, no brief. It carries no
  agent, so `charter persona stats` counts no dispatch for one.

## Considered and rejected

- **A hook `ask` as the gate** — lifted to `allow` under `bypassPermissions`, absent on
  opencode.
- **A charter-side confirmation on stdin** — no terminal inside a Bash tool call.
- **A model quiz as the consent** — the proposer cannot be the approver.
- **`--brief-file`** — the prompt would show a path, and a path is not a text.
- **A depth limit on handoffs** — every hop already needs a yes.
- **`charter update` writing the rule** — it would take back the operator's removal at the
  next update, and there is no stamped version to write it only once against until the
  release that ships the entry.
- **A lock around the arrivals record** — a stale lock file nobody clears is worse than the
  race, and it is still a fix by timing.
- **A fixed brief fence defended by escaped newlines** — unforgeable, and it destroys what
  the brief is for.
- **Widening the duplicate threshold to 0.51** — answers one measured pair and neither of the
  other two.
