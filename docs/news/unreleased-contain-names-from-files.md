---
version: unreleased
headline: A name charter reads out of a committed file can no longer point outside the plane
---

Charter validated a name when a human typed it and never when it read one out of a file.
`valid_name` exists twice — once for personas, once for workspaces — and was called from
six places, every one of them a command handling a name someone had just typed at the
prompt. No parser called either function. On the reading side, the same kind of value was
joined onto a path or handed to git with nothing in between.

Five issues came out of that one omission. `extends:` and `[persona] default` resolved as
paths, so a reference climbing out of `personas/` became the acting persona and
contributed its `vault:`, `role:` and `tools:` from a file the plane does not contain.
`uses:` and `borrows:` did the same through `effective_tools`, whose result the PreToolUse
gate turns into an `allow` — so a persona could be granted a tool declared outside the
plane. A repo name in a workspace manifest selected the directory that `git checkout` and
a **credentialed** `git pull` then ran inside. The same field in `inventory/repos.json`
became a clone destination. And an inventory `ssh_url` was handed to `git clone`
unchanged, so `ext::sh -c '…'` — a transport that runs a command — was stopped only by
git's own `protocol.*.allow` default, which charter neither sets nor owns.

The tell that this was one omission and not five bugs is that charter already disagreed
with itself about it. `charter persona lint` checked a reference by looking for the name in
a set and reported `uses: '…' — no such persona (dangling)`, while the resolver joined the
same string onto a path and loaded it. Both ran in one session: the operator's own check
said the grant was inert while the gate was honouring it. The lint and the resolver now
share one function, so they cannot answer differently again, and a reference that is a path
rather than a name says so instead of being reported as a typo — those send you to two
different places, and only one of them has the problem in it.

**The rule is per-name-kind, because two different systems mint these names.** Charter
mints persona and workspace names, and `persona create` already enforces the alphabet, so
persona references answer to that same rule and agreement is structural rather than
maintained by hand. A *forge* mints repo names, so those get a permissive rule that forbids
traversal and separators and nothing else — `org/.github` is a repo GitHub itself tells
organisations to create, and a fix that refused to clone it would have broken working
planes in exchange for closing a hole.

Two smaller things travelled with it. A branch name from a manifest can no longer reach git
as an option: `git checkout` has options that write, and a manifest is a committed file
anyone on the team can edit. `git check-ref-format` is deliberately not what does this —
it *accepts* `refs/heads/-b`, because a leading dash is legal inside a ref, so ref grammar
and argv safety turn out to be different questions. And `_https_url` now returns an HTTPS
URL or nothing, rather than passing through whatever it could not classify.

Containment here is lexical and does not follow symlinks. That is a deliberate limit rather
than an oversight: symlinks in the files charter reads are a separate, filed problem that
covers every plane file and not just the ones named by a name, and resolving links here
would have claimed none of it while refusing planes that legitimately symlink a persona
directory today. What is checked is covered by a table-driven test across every site that
takes a name from a file, so the next join to skip containment fails in the suite rather
than shipping.
