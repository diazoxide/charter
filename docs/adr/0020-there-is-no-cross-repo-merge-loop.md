# There is no cross-repo merge loop

`charter change land <slug> --repo <name>` lands **one** member of a cross-repo change.
There is no `--all`. It is not a flag that defaults off and it is not a flag behind a
confirmation: **the flag does not exist, and the parser refuses it.** A test asserts that.

This is written down because it will be proposed again by everybody who has ever written
the shell loop, and because the argument against it is not obvious from the outside — from
the outside it looks like charter making you type five commands to do one thing.

## Atomicity is not on offer, so the question is what replaces it

Charter cannot make a cross-repo landing atomic. Nothing can: five repositories on
different hosts with different owners and different CI have no transaction between them,
and a merge that has already fired somebody's deploy cannot be taken back by failing the
next one. The honest guarantee is three sentences:

> Charter cannot make a cross-repo landing atomic.
> It can make the window visible, bounded and named, and it can refuse to report a partial
> landing as anything but partial.
> And it does not offer the one operation that would turn N revertible merges into one
> irreversible transition with no human between them.

`--all` is that operation. What replaces it is **legibility** — one name on every artifact
charter authors, the window named while it is open (`PARTIALLY LANDED (3 of 5)`), a change
that is never reported greener than its worst member, and archaeology that does not depend
on charter still being installed.

## Two reasons, and the second is the stronger

**ADR 0003's reasoning, applied unchanged.** That ADR rejected a `--yes` flag on `charter
report send` with one sentence: *"a flag the agent can pass is a flag the agent will pass
unprompted, which is exactly the failure being prevented."* `--all` is that flag, for an
operation whose blast radius is five repositories rather than one issue.

**`--all` would have to answer a question that has no answer.** When member 3 of 5 is
rejected mid-loop, `--all` must do *something*: stop and leave two landed, continue and
land the independents, or roll back what it did. Each of those is wrong in a case the
others handle, and charter cannot tell the cases apart — the facts that separate them are
in the head of whoever is watching. ADR 0009's rule is that charter may name a cause it
recognised and must not assert one it inferred; a flag that must guess a policy is the same
defect wearing an argument parser. **Not offering the flag is not offering a lie.**

## The obvious objection: the agent just writes the loop

It can, and nothing stops it. Three things are still true, and they are why the refusal
earns its place rather than being a speed bump.

The loop the agent writes has **no gates in it** — but the command it calls does, on every
call: the blocker gate, the check gate, the read-back. A five-iteration shell loop over
`charter change land` is five gated landings; a `--all` flag is one ungated one. **The
refusal is not of repetition, it is of a code path that batches the gates.**

Every call is traced independently, so the record shows five decisions rather than one.

And an agent that writes the loop has *chosen* to, in a session someone can read, rather
than passing a flag charter itself put on the command as the obvious way to do the obvious
thing. That difference is exactly what ADR 0003 is about, and this project has already
decided it once.

## The same refusal, one command over

`charter change revert` is bound by this too, and it is where the pressure is highest: the
moment you most want the loop is the moment you are least able to judge it. An automatic
cross-repo rollback is `--all` with the safety off, offered to an operator who has just
discovered something is wrong and does not yet know what — and the reverts themselves need
review and CI, because a revert can break a repo as effectively as the change did.

So a revert is a **new change**: its members are the landed members of the original, each
seeded with a branch carrying a revert of the sha the landing log recorded, and from there
it is pushed, reviewed, gated and landed one member at a time by the same commands with the
same refusals.

The alternative would destroy the only thing that makes the state explicable. Force-pushing
three default branches back past the merges leaves a world where the change happened, was
undone, and no repository's history mentions either — the exact failure this whole design
exists to prevent. `component-api-2` and `revert-component-api-2`, both named, both
cross-referenced, in every repo they touched, reads as a decision six months later. A
force-push reads as corruption.

## Consequences

`charter change land` takes `--repo` and lands one member per invocation. `charter change
revert` derives a record and seeds branches; it lands nothing. Neither ever force-pushes,
deletes a branch, resets a default branch, or closes a request charter did not open — those
are not flags that default off, they are argv charter never builds, and the tests read every
git invocation a change action makes rather than trusting the absence.

An operator who wants a prompt before each landing has one, for free, today:

```
charter guard ask --local 'charter change land *'
```

`charter doctor` names that command and **does not run it** (ADR 0017). `--local` is part
of the recommendation: `guard ask` writes the plane's *committed* settings by default, and
consent that travels in a commit enrols a whole team on one person's click — ADR 0003's own
reason for keeping reporting consent out of `charter.toml`. A team that genuinely wants the
prompt for everybody drops the flag, which is a decision made once and visible in a diff.

Charter adds no second human gate of its own. A consent the agent satisfies by running one
more command is theatre, and an operator prompted constantly rubber-stamps within a day —
worse than no gate. The floor that matters is `hooks._release_floor_reason`, which **denies**
`charter change land` outright under `bypassPermissions`, because an unattended `ask` falls
back to `allow` and an ask there would be indistinguishable from no guard.
