---
version: unreleased
headline: A value glued to a short flag is read as one value again — `env -Sfoo=1 cat <vault>` printed a vault and is denied
security: true
---

`env -Sfoo=1 cat .charter/vaults/x.json` printed a fabricated vault on a real plane while
`cat .charter/vaults/x.json` on the same plane was denied. One wrapper word, one flag, and
the guard that exists to keep a credential out of a transcript had nothing to say.

The parse is the whole of it. `env` packs a command into one token two ways —
`--split-string=<command>` and the glued short `-S<command>` — and `hooks._split_env_chdir`
read the long form's rule first. So a packed command carrying its **own** `=` was split at
the wrong one: `-Sfoo=1 cat …` yielded `1` as the program, the `cat` was never named, and no
guard downstream had a reader to object to. getopt hands a short option everything glued
after it, `=` included; the rule for the long form was answering about a token that was
never a long form.

**Four of its five neighbours were denied the whole time, and that is the part worth
recording.** Measured against the shipped hook, one row at a time:

```
cat .charter/vaults/x.json                           -> DENY   (control)
env -Sfoo=1 cat .charter/vaults/x.json               -> ALLOW  <-- the bypass
env -S "cat .charter/vaults/x.json"                  -> DENY
env --split-string=foo=1 cat .charter/vaults/x.json  -> DENY
env -S "foo=1 cat .charter/vaults/x.json"            -> DENY
env FOO=1 cat .charter/vaults/x.json                 -> DENY
```

Anyone probing this by hand would very likely have stopped at the third row and concluded
the guard held. It took the one form combining a glued short flag with a value that itself
contains an `=` — which is exactly the form a real `env -S` setting a variable takes. All
six rows are now one table in `tests/test_guard_attached_option_values.py`, driven by one
loop, because the five that already denied are what make the sixth's regression visible: a
later refactor that trades one spelling for another has to redden something.

**And its siblings, because a bug with a shape has them.** Every flag whose value may be
attached went through the same two rules in the same wrong order, and `env -C` — the chdir
whose value is what makes a later relative operand resolve — was live in the same way:
`env -Cx=y/../.charter/vaults cat x.json` was allowed and printed the vault, after the
`mkdir x=y` a shell does without being asked twice. `sudo -D` and `xargs -a` are the same
parse. The two rules are now disjoint rather than merely ordered — a glued short form never
starts with `--`, and the `=` rule now requires it — so the ordering cannot quietly come
back, and all three wrappers are walked across all four spellings in the tests.

The same parser is what charter's own test harness uses to decide whether a test is about to
spawn charter against your live plane. It had been repairing this ordering on its way in
since the defect was filed; that repair is deleted with this fix, which makes its removal a
second regression test — the suite stays green on production's parse alone.

Two neighbours found while measuring this one are **not** fixed here, and were filed rather
than quietly absorbed: [#556](https://github.com/diazoxide/charter/issues/556), a value
attached to a *bundled* short option (`env -iC<dir> cat x.json`, which relocates into the
vault directory and is allowed), and
[#555](https://github.com/diazoxide/charter/issues/555), `env` accepting assignments to
names that are not shell identifiers while the parser stops at the first token it cannot
read as one (`env a-b=1 cat <vault>`). Both were live, both were unchanged in either
direction by this fix, and both are a different question from the one #547 asked — a fix
that widens its own scope until it is answering three questions is how a guard change stops
being reviewable. *(Both are closed in this same release, by their own change: see "An
option is its letter, and an assignment is whatever will do the assigning".)*
`SECURITY.md`'s position is unmoved: guard rails, not guarantees — four
rounds of adversarial review established that deciding what a shell will execute, without
executing it, is not winnable in a Python tokeniser. This was a mis-ordering with a correct
answer, and it has one now.

Nothing to adopt: upgrading is the whole of it.

[#547](https://github.com/diazoxide/charter/issues/547), found by external review and
confirmed on a plane built by `charter init`.
