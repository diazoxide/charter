# The plan is todos; the lifecycle belongs to the piece

A fleet needs somewhere to say *"this task is eight pieces"* before any of them exist.
Without it charter can only see pieces that were started, so it can report seven of eight
declared and never notice that an eighth was planned and never begun — which is one of the
four failure modes the work exists to close.

The plan is **workspace todos**, unchanged. A todo is intent that expires (ADR 0004), and
"piece eight, not yet begun" is exactly that. A piece may record which todo it came from;
the todo records nothing about the piece. *"Declared but claimed by nobody"* is then a query
over what already exists — **a todo with no piece** — with no new store and no new noun.

## Why not put the state on the todo

The obvious cheaper design is to grow a todo an `owner`, a `state` and a `claim` and call it
a work item. It will be proposed again, which is why this is written down.

Intent and execution have different lifetimes, and merging them destroys the shorter one. A
todo is true until somebody decides it isn't; a piece is claimed, worked, and declared
finished or abandoned, possibly several times over for one todo. Giving the todo a state
means the retry has nowhere to live except by overwriting the record of the first attempt —
so the store that was supposed to guarantee nothing is missed becomes the one that forgets.

It also breaks the direction of ADR 0004. Todos live outside memory because intent goes
stale and facts do not; hanging execution state off intent puts a third lifetime in the same
file and reintroduces the rot one level down.

## The link is one-way, and nothing closes automatically

A piece never closes its todo. Every piece of a todo declaring `done` does not mean the
intent was satisfied — that is a judgement about *coverage*, and coverage is the one thing a
program with no model cannot check. charter deciding otherwise would be asserting the plan
was correct, which is precisely the authority the design withholds from it: the agent plans,
charter enforces.

So a todo is closed by whoever closed todos before this design existed. Nothing changes
about that.

## Consequences

**The hole shrinks; it does not close.** Nothing forces an agent to write its plan down as
todos, so a plan held only in an agent's head is still invisible, exactly as ADR 0011 states.
This makes the gap *closable by the planner* rather than closed by charter, which is the
most an unmodelled control plane can offer.

**Todos gain a reader they did not have.** They have been a human-facing list; they now also
answer a structural question about a fleet. That is a reason to keep them cheap to write —
anything that makes recording a todo more ceremonious makes the plan less likely to exist,
and the plan not existing is the failure this decision is meant to prevent.
