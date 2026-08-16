# Grilled — the open tracker, 2026-08-16

**Status:** decisions, then implementation. Written against `main` @ 0.29.1 with eight issues open.

A grilling session is an interview, and there was nobody to interview: this ran overnight with
the operator asleep and explicit instruction to decide and build. So the questions below are
put and answered in one pass, and every answer records **what would change it** — because an
unattended decision that hides its own reversibility is worse than one that was never made.

Two issues are **not** implemented, on purpose. That is argued below rather than assumed.

---

## Round 0 — what is actually on the tracker

Eight open. Six actionable, and two of those are the same bug:

| | |
|---|---|
| #137 / #154 | report redaction scrubs the vault alias, leaves the vault name beside it |
| #155 | no way to discard a drafted report |
| #156 | `doctor` is green while a clone in a non-active workspace is stale |
| #157 | nothing *prevents* an agent leaving `main` in the plane root |
| #140 | a clone that ships `charter.toml` silently becomes the plane you are operating on |
| #127, #124 | **parked**, with written unpark triggers |

❓ **Q1 — are #137 and #154 the same issue?**
Both were split out of #128. Both say: the alias is redacted while the real vault name and the
`OP_*_SERVICE_ACCOUNT_TOKEN` survive in the same code block.

➡️ **Yes, one bug.** Keep **#137** — it was filed first, carries the options analysis, and is
labelled `bug`. #154 carries the better evidence: the actual half-redacted command. Close #154
as a duplicate *after* lifting its command block into #137, so the fix is written against the
concrete artefact rather than the summary of it. Both close on the fix.

❓ **Q2 — does the parking on #127 and #124 still hold?**
Each was parked with reasoning and an explicit unpark trigger. #127: a second reporter, or one
machine needing two planes on genuinely incompatible versions. #124: a workspace lost on a
machine that is not the maintainer's, or a common terminal supplying no pane id.

➡️ **Both hold; neither is implemented.** No trigger has fired, and nothing in tonight's batch
fires one. Implementing them anyway would be overriding a documented decision because an
unattended agent had time — which is the worst reason available. The instruction was "implement
all issues"; the honest reading is *all issues that are open work*, and a parked issue with a
stated trigger is a decision already taken, not a backlog item.
**Reverses if:** the operator says park-no-longer. That is one sentence from them and a fresh
session; it is not something to infer from silence.

---

## Round 1 — the two that argue with each other

#157 wants **prevention** because a warning did not hold. #140 documents a silent failure whose
cheapest fix is **a warning**. Deciding them separately would produce an incoherent pair, so
they are decided together.

❓ **Q3 — #157: may charter refuse a git command in the plane root, when ADR 0008 chose signal
over refusal?**

ADR 0008 did not reject prevention. It deferred it, and named its own bar:

> Preventing it outright — refusing commands that would operate in the root — is the real
> answer and is deliberately not what ships first. Which commands count is a judgement that
> wants evidence.

#157 is that evidence: six branch switches in one session, `doctor` printing the correct warning
on every run in between, and the agent rationalising past it each time. Plus the operator's note
that two background agents in one working tree clobber each other through `git checkout`.

➡️ **Yes — and this is following ADR 0008, not contradicting it.** The bar it set has been met,
by exactly the consumer charter is built for. ADR 0008 gets an amendment recording that, so the
next reader does not have to reconstruct why the posture changed.

❓ **Q4 — which commands count?** This is the judgement ADR 0008 said wanted evidence, so it is
scoped to what the evidence shows and no further.

➡️ **Only what moves HEAD between branches, and only in the plane root.** `git switch`,
`git checkout <ref>`, `git checkout -b/-B`. Not `git commit` — `charter save` commits in the
root by design and advancing HEAD on the branch you are on is not the failure. Not
`git checkout -- <path>`, which restores a file and never moves HEAD.

**And the remedy must stay executable.** `doctor`'s hint says *"Put the root back:
`git -C <plane> checkout main`"*. A guard that blocks the fix it recommends is a trap, so
checking out the plane's **default branch** is always allowed. That single carve-out is what
makes this a guard rather than a cage.

❓ **Q5 — #140: prefer the ancestor plane, name the nesting, or refuse?**

The issue offers all three and marks option 1 *"most correct, and the most invasive: `ROOT`
resolution is load-bearing everywhere."*

➡️ **Name it (option 2). Do not change resolution.** Two reasons, and the second is the real one:

1. `ROOT` resolution is under every command and every test. Rewriting it unattended, overnight,
   is the single highest-blast-radius change available in this repo.
2. **The ambiguity is genuine.** Sometimes the inner plane *is* the one you mean — charter's own
   dogfooding clones charter into a workspace, and that clone is a legitimate control plane you
   might want to manage. #157's ambiguity is not genuine: there is exactly one right answer
   about switching branches in a shared tree. Prevention fits where the answer is unambiguous;
   naming fits where it is not.

That is what keeps Q3 and Q5 coherent rather than arbitrary: **prevent the unambiguous, name the
ambiguous.** ADR 0013's second rule already says the naming half — *a divergence charter can see,
charter names*.
**Reverses if:** naming turns out not to hold either, the way #157's warning did not. Then
option 3 (refuse state-writing commands unless the plane is named explicitly) is next, and
option 1 after that.

---

## Round 2 — the rest

❓ **Q6 — #156: where should staleness be reported, and can it be trusted?**
The reporter picks `doctor`, and is right: it is what people run to be told what is wrong.

The subtlety nobody has raised: **behind-ness needs a fetch.** `@{u}` is the last-known remote
state, so without fetching, `doctor` sees only what a previous `sync`/`fetch` left behind.

➡️ **Report from existing remote-tracking refs. No fetch, no network.** `doctor` runs from the
SessionStart hook and the module's contract forbids network there. The consequence is stated in
the check itself: it can **under**-report, never fabricate — which is the acceptable direction,
and the same discipline ADR 0009 applies to error text. Scope is every workspace, since being
scoped to the active one is the bug.

❓ **Q7 — #137: which redaction rule?**
Options were: redact everything charter knows; redact only the unambiguous; or say what was
scrubbed.

➡️ **Redact the whole cluster, and say what was scrubbed.** Half a policy is what produced the
bug — `[redacted]` next to `VolatiCloud Marketing` costs readability and buys nothing.

Between "all" and "only the unambiguous": a vault name is frequently a **company name**, which is
precisely the class of identifier this exists to keep off a public tracker. Dropping bare names
would leak the thing most worth protecting.

The prose-mangling cost is real and is paid down two ways rather than argued away:
**per-category placeholders** (`[workspace]`, `[persona]`, `[vault]`, `[token-env]`) so the
maintainer can still follow the shape of a command, and a **scrub summary** so the Reporter's
mandatory read under ADR 0003 has something to check against instead of having to notice
absences.

❓ **Q8 — #155: may `report delete` remove a report that was already sent?**

➡️ **Drafts freely; a sent report needs `--force`.** A sent report is the local record of what
went out under the Reporter's identity, and the tracker copy is not under charter's control.
Deleting it silently would leave no trace of a thing that exists publicly.

---

## The frontier is empty

Everything above is decided. What remains is execution, and the sequence exists to keep review
honest rather than to satisfy dependencies — nothing here blocks anything else.

1. **#157** — the plane-root guard, plus the ADR 0008 amendment.
2. **#156** — `doctor` sees every workspace's clones.
3. **#140** — a nested plane is named. Adjacent to #156 because both touch `doctor`.
4. **#137 / #154** — coherent redaction.
5. **#155** — `report delete`. Adjacent to #4 because both touch `report`.
6. **0.30.0** — four version sites, tag, verify the publish.

Each lands as its own PR, rebased on `main`, **with CI confirmed green before merge** — that
last clause is written down because skipping it once already put `main` red in this session.
