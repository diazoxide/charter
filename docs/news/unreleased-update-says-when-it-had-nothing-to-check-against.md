---
version: unreleased
headline: `charter update` says when PyPI gave it nothing to check against, instead of exiting as if you were current
---

With no `--to` and no pin to conform to, `charter update` takes the latest published version
as its target. When it ended up with no version to check against, it used the charter you
are already running as the target. It then moved the harness artifact, ran the news phase and exited 0.
That is exactly what a plane on the newest release sees, but nothing had been checked. The
refusal written for this case was one line below and could never fire.

Now it refuses, installs nothing and exits 1:

```
✗ no version came back from PyPI to check against: either it did not answer, or its answer could not be cached. Pass one explicitly: charter update --to X.Y.Z
```

The refusal gives the same two causes `charter version bump` gives for the same condition,
and like bump it does not say which one happened. The old wording guessed "offline?", but an
answer from PyPI that could not be cached ends the same way. A plane already on its
pin gets the same refusal, with or without `--bump`, because whether to stay or to propose
a bump is the question PyPI's answer decides. A plane behind its pin still conforms to it,
because PyPI has no say in that target.
