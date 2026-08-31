---
version: unreleased
headline: a forge body is the text you wrote — charter refuses a `gh issue create` whose backticks the shell would run
---

An agent filing an issue wrote ``--body "… `env -u PYTHONSAFEPATH` …"``, meaning the
backticks as a markdown code span. Inside double quotes they are command substitution. The
shell ran `env` and pasted sixty-four variables into the body of a **public** issue — four
1Password service-account tokens, each decoding to the master unlock key, a GitLab PAT, and
the session's own variables. It was live for forty minutes, and it was found because a
different agent happened to read the issue.

**Redaction does not undo that.** A forge keeps public edit history, so rotation was the
only remedy, and rotation is the operator's work rather than the agent's.

Nineteen other issues filed the same night used the same ``--body "…"`` shape with
backticks in them and were harmless — because the backticked text was not a runnable
command. **The pattern was wrong in all twenty and nineteen were lucky.** What got
published was not decided by what the agent meant to say; it was decided by what the
operator's shell exports, which charter had an opinion about nowhere.

## The one argument where markdown and the shell collide

A body is the only place an agent routinely writes prose *about* commands, and a code span
and a command substitution are the same character. That is why this is not an exotic slip
waiting for an exotic fix:

```bash
gh issue create --body "run `env` first"     # the shell runs env; gh publishes the output
gh issue create --body 'run `env` first'     # one character, and it publishes the text
```

So charter now refuses the first. A `gh`/`glab` command whose purpose is to publish prose —
`issue create|comment|edit`, `pr create|comment|edit|review`, `release create|edit`, `gist
create|edit`, and glab's `issue`/`mr` `create|note|update`, `release create`, `snippet
create` — is denied when a command substitution the shell would **run** stands on its line.
The denial names the remedy: `--body-file <path>`, or `--body-file -` with a **quoted**
heredoc.

**The unquoted heredoc is covered too, and that mattered more than the `--body` case —
because the rule written after the incident had a hole in it.** The working rule says to use
`--body-file -` with `<<'BODY'`. Those quotes are the entire rule, and nothing was enforcing
them: an agent who learns the rule and forgets them writes `<<BODY`, which expands exactly
as `--body "…"` does. Anyone who has read the rule is therefore *more* likely to meet this
guard on the heredoc path than on the one the incident used, and a guard that covered only
`--body` would have steered every agent onto a path it did not watch. `<<'BODY'`,
`<<"BODY"`, `<<\BODY` and `<<-'BODY'` are allowed with a body full of backticks; `<<BODY`
and `<<-BODY` are not.

## What it claims, which is less than you might read into it

It reads the **shape** of a command line. The value is out of its reach in both directions:
at `PreToolUse` the substitution has not run, and by `PostToolUse` the issue is already
public — which is also why matching bodies against the environment was not an option that
existed rather than one that was rejected. This is a refusal of a shape, **not** a promise
that a credential stays off a forge, and the denial says so in as many words.

Three limits, stated rather than left to be discovered:

* a **`--body-file` whose file already holds the same text** is not covered — nothing
  expands on that path, so there is no shape to see;
* **`git commit -m "… `x` …"`** is out of scope — and not because it matters less. On the
  axis this whole issue turns on, *can it be undone*, a commit message is **worse** than an
  issue body: a body is replaced in one call, while a pushed commit needs a history rewrite,
  and a rewrite reaches neither forks nor existing clones nor the forge's caches. It is out
  because the commit surface has not been verified the way the `gh`/`glab` verbs were, and
  because it is dense with the character the guard keys on: most commit messages on
  charter's own `main` carry a backtick — 26 of 30 consecutive ones when this was
  measured — and inside `-m "…"` every one of those is live. A guard that fires constantly
  on legitimate work gets switched off, and then it covers nothing. Filed separately as
  [#711](https://github.com/diazoxide/charter/issues/711) so it is a decision and not an
  omission;
* the check is scoped to the **whole Bash call**, not to the body argument, so
  `cd "$(git rev-parse --show-toplevel)" && gh pr create --body-file b.md` is refused as
  well. Narrowing it means deciding which argument a substitution lands in, which means
  putting a shell inside the guard — the failure the leak guard has already documented four
  times. Run the substitution in a separate Bash call; each is judged alone.

And a fourth, which is the nearest one and worth knowing before you rely on this:
**`gh api` is not covered.** `gh api repos/o/r/issues -f body="…"` publishes the same issue
without ever spelling a noun and a verb, and so does a user-defined `gh alias`, or a program
name that arrives in a variable. Covering `gh api` means deciding which of its invocations
write, which is a surface nobody has verified the way the nineteen verbs were — so it is
stated rather than half-covered.

It also closes, for this one command family, a bypass the secret-leak guard has always
listed as open: a **quoted** command substitution. `gh issue create --body "$(cat <vault>)"`
walks past the leak guard — shlex keeps it as one word and no vault predicate looks
inside — and stops here. Not because this guard knows anything about vaults; it does not
look inside the substitution at all.

Unlike the plane guards beside it, this one is **not gated on there being a control
plane**. What it refuses is a fact about the shell, not a policy this plane holds.

## Checked against a shell, not against a belief about one

A fix for a class of bug is unusually likely to contain that bug, and a shell parser
written to guard shell expansion is the worst case of it. So the part that is new
reasoning — *would this substitution actually run* — is a **differential test**:
twenty-eight spellings are executed by `bash` with a sentinel file as the oracle for
whether the substitution ran, and charter's answer has to match on every one.

It earned its keep before the branch was pushed. A here-string, `<<<"… `x` …"`, was being
re-read as a heredoc whose delimiter was the quoted word — classifying a live substitution
as an inert body, a fail-open in the one direction that matters. Review did not catch that.
`bash` did, on the first run.

The oracle had to be fixed first, too. The first version substituted `echo MARK` and asked
whether `MARK` came back on stdout — but the *unexpanded* text contains `MARK` as well, so
every case read as "it expanded", including the ones that plainly had not. Replacing it with
a marker the source cannot contain then exposed a second confusion: two cases where the
shell really had run the substitution and stdout could never show it, because a heredoc
whose file descriptor is superseded by a second heredoc is still expanded, and a
substitution feeding a command that then errors has still run. The oracle is now a sentinel
file — *did it execute* rather than *did its output arrive*.

**This is a defect class charter already has a name for, and it is worth naming here:
an assertion that sits on the path which already satisfies it.** `assertRaises(SomeError)`
is the usual shape — it pins a type, and the type is not the property. A marker that matches
itself is the same error one level out: the check passes on the state it is meant to detect
*and* on the state it is meant to reject, so it discriminates nothing while looking like
evidence. A guard's test is exactly where that hides, and an oracle wrong in the permissive
direction is how a fail-open guard gets "fixed" into place and stays there.

## Adopting it

Nothing to do. It arrives with the plugin's `PreToolUse` hook. If you write forge bodies
with `--body-file` already, you will never see it.
