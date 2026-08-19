---
version: unreleased
headline: Call the API as the user your browser just logged in as
---

The browser lane got you authenticated and stopped there. The step after — reading the
token that session now holds, so the same run can check that the UI and the API agree —
had no supported shape, and the obvious workaround was the one Playwright's own reference
documents:

```bash
TOKEN=$(playwright-cli --raw cookie-get session_id)   # the leak
```

Command substitution puts the token in a shell variable, in a transcript, with nothing
redacting it. That is the outcome the lane exists to prevent.

A browser session is just another place a credential lives, so it is now a **reference**,
resolved like `op://` and `vault://`:

```bash
charter secret set qa API_TOKEN --value 'browser://owner/localstorage/access_token'
charter secret exec qa --env TOKEN=API_TOKEN -- \
  curl -sH "Authorization: Bearer $TOKEN" https://api.example.test/me
```

`browser://<session>/localstorage/<key>` and `browser://<session>/cookie/<name>`. No new
command and no new flag — every consuming path (`--env`, `--file`, `--dotenv`, redaction,
`charter persona secret exec`) already worked, and works here unchanged.

Charter builds the invocation, so the version pin is right by construction — the mistake
that otherwise reports `not open` against a browser that is alive and still logged in.
Override it per vault with `{"version": "0.1.19"}` when a session was opened at another
version. Whole storage state is deliberately not readable this way: a dump is a credential
blob nobody declared, and the redactor cannot scrub what it cannot name.
