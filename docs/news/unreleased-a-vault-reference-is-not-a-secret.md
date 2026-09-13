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

A value written as `vault:<vault>/<key>`, `charter secret get <vault> <key>`,
`op://<vault>/<item>/<field>` or `vault://<path>#<field>` now passes all four checks. It may
have a quote or a backtick on either side, but it has to be the whole value on its line, and
each name in it must be 32 characters or fewer and must not start with a known token prefix
such as `ghp_`, `sk-`, `xoxb-` or `AIza`. So a live token typed into a reference is still
refused. So is a real value beside or glued onto the reference, and a second assignment on
that line or the next. A bare `forge/token` is still refused too, because a secret can
contain a slash. One gap remains: a token of 32 characters or fewer that has none of the
listed prefixes reads as a name. The brief refusal now spells out the accepted forms.

Nothing to adopt; the check updates with charter
([#985](https://github.com/diazoxide/charter/issues/985)).
