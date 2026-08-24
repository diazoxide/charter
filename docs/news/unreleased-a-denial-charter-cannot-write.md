---
version: unreleased
headline: A denial charter cannot write is no longer an allow
---

charter's guards refuse by printing one JSON object on stdout. A hook that cannot print has
said nothing, and a `PreToolUse` hook that says nothing is an **allow** — so every way that
write could fail was a way a guard failed open.

Both spellings were in the tree at once, on the same invariant. The vault guard for
`Read`/`Grep` wrapped its whole body — the refusal included — in `except Exception: return
0`, so a `BrokenPipeError` out of `print` returned a clean zero and the vault was read. Its
sibling on Bash had no wrapper, so the same exception propagated to `charter`'s top level,
which quite correctly turns `BrokenPipeError` into exit 141 for `charter … | head`. 141 is a
*non-blocking* hook error: the tool call proceeds. Two guards on one rule, failing in
opposite directions, in a module that argues at length that they must never disagree about
what a vault is.

Reachability was low — the bodies are a few dict lookups, a `str()` and a regex — and the
direction is the point. Found by the 2026-08-24 security audit (#438).

**A decided denial now reaches the harness by some channel, or the process exits refusing.**
When the JSON write fails, charter writes the reason to stderr and exits **2**, which is the
harness's other refusal channel: it blocks the tool call and hands the model the reason.
The number matters and it is not "any failure" — every other non-zero status is a
non-blocking error and the call goes ahead, which is exactly how the 141 above let one
through.

**A write is not a delivery.** The first version of this fix asked "did the write raise?",
and on the one channel a hook actually has — a pipe — the answer is no. `print` to a pipe
block-buffers: the JSON goes into a userspace buffer, the call returns cleanly, and the
`BrokenPipeError` arrives when the interpreter flushes on the way out, which is worth exit
120 and lets the tool call through. Measured: identical behaviour to the code it replaced.
charter now flushes the verdict while the guard can still act on the answer, so "the harness
has it" is a question asked at a point where "no" can still become a refusal.

**Deciding is still allowed to fail; refusing is not.** The `except` in the vault guard was
narrowed, not deleted: a payload charter cannot parse is still an allow, still silent, still
not a broken turn. That distinction is now written into `docs/hooks.md` beside the
"hooks swallow their exceptions" rule it qualifies.

**And it holds for the guard nobody has written yet.** The fallback is not something each
call site has to remember to propagate: an undelivered denial is recorded, and the hook's
own entrypoint refuses on it. A fail-open is invisible from the outside — it looks exactly
like a guard that is present and never fires — so it is closed at the one place every guard
passes through.

Nothing to adopt: upgrading is the whole of it.
