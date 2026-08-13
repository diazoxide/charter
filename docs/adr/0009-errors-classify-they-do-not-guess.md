# Errors classify, they do not guess

A charter error may name a cause it **recognised**. It must not assert a cause it has not
verified. Where nothing was recognised, it says so, offers candidates *as* candidates, and
tells the reader how to see the underlying tool's own output.

This is a rule about error text, which sounds too small for an ADR. It is here because the
rule is invisible in the code that follows it — a well-written wrong guess and an earned
diagnosis are the same three lines — and because removing a confident, helpful-sounding
sentence looks like a regression to anyone who has not read the incident that produced it.

## Why it matters

The 1Password provider's write failure used to end with: *check that a service-account
token has WRITE access to this vault; a read-only token fails here with 1Password error
(101)*.

That sentence was true once. It was written after a real incident where a token could read
a vault and not write to it, and where the previous, vaguer message ("check `op` is signed
in and the vault exists") had sent someone to check two things that were both fine. It was
a good fix for that failure. It was then applied to **every** write failure, regardless of
what `op` had actually said.

In #78 it met a template `op` had declined to parse. charter reported a permissions
problem. The reporter audited their tokens — and one of the two genuinely *was* read-only,
so the wrong answer briefly looked confirmed. The two causes were only separated by testing
each token by hand against a piped `op` call.

The general shape: **a vague error keeps you looking; a confident wrong one tells you to
stop.** That makes an unearned diagnosis worse than no diagnosis, not better — which is the
opposite of how it feels to write one.

This is the third instance of one family in this codebase. #55: `doctor` said vaults were
"healthy" when it had tested reachability. #75: a test covered charter-created items and
was read as covering adopted ones. Each time the statement was accurate about a narrower
thing than the reader took it for. An error message is the sharpest version, because it
does not merely permit the wrong inference — it supplies it.

## What this means in practice

`_diagnose(stderr) -> str | None` matches `op`'s stderr against signatures and returns a
**fixed constant** per signature. Failing to match is a normal outcome.

Three properties do the work:

**Constants only.** stderr is matched against and never interpolated into the result. The
rule that charter errors never carry provider output — because a resolver's stdout *is* the
secret and its stderr can echo what it was given — therefore holds by the shape of the
function rather than by anyone remembering to scrub. There is no path along which the text
could reach a message.

**Every signature carries provenance.** Each entry names where its wording was observed: a
local reproduction on op 2.34.0, a real incident, a specific issue. Two signatures that
would obviously be useful — item-not-found and session-expired — are *absent*, because
their wording could only have been guessed. Inventing them is the same error one level down.

**It degrades to silence, not to a wrong answer.** Matching another CLI's English is
fragile: `op` may reword an error in any release. What makes that acceptable is the
direction of the failure — an unmatched signature costs precision and can never produce a
false diagnosis. The behaviour it replaces failed the other way, being precise and unearned.

The `write` flag survives for one job: ordering the candidates when nothing matched, where
the path is genuinely the only remaining information. Where a signature matched, it wins
regardless of path.

## Consequences

Some failures now say less than they did. That is the point, and it will read as a
regression to someone comparing messages side by side without the incident in hand.

`reference.py` is deliberately **not** changed. Its resolver failure already lists causes in
order rather than naming one — it is the model being copied here, not a defect. Extending
the classifier to it would mean inventing signatures for HashiCorp `vault`, which nobody has
observed, and that is precisely what this ADR forbids.

The bar for adding a signature is a source, not a plausible-sounding string. A signature
without provenance recreates the failure this ADR exists to prevent, with the added
durability of looking deliberate.
