---
version: unreleased
headline: A line that names where a credential lives, `token: vault:forge/token`, is no longer refused as a credential
---

When `charter handoff` refuses a brief that looks like it holds a secret, it tells you to name
where the credential lives. The rule behind that refusal then refused the answer. A line such
as `api_key = vault:forge/api-token` or ``token: `charter secret get forge token` `` looked
like a credential being assigned, so the brief was refused again. The PostToolUse warning on a
memory file, `charter save` and `charter persona memory-sync` use the same rule and refused it
the same way.

A value written as `vault:<vault>/<key>` or `charter secret get <vault> <key>` now passes all
four checks. It may have a quote or a backtick on either side, but it has to be the whole
value on its line. A real value beside or glued onto the reference is still refused, and so is
a second assignment on that line or the next. A bare `forge/token` is still refused too,
because a secret can contain a slash. The brief refusal now spells out both accepted forms.

Nothing to adopt; the check updates with charter
([#985](https://github.com/diazoxide/charter/issues/985)).
