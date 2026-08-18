---
name: browser
description: Drive a real browser for a task that needs one — a web login, an end-to-end check, verifying a UI, reaching something behind auth — with credentials pulled from a vault so the password never enters the conversation. Use when browser automation needs to authenticate, or when several workers each need their own logged-in session.
---

# Driving a browser with vault credentials

Two halves, owned by different projects:

- **How to drive a page** — snapshots, clicking, network mocking, tracing — is
  Playwright's. Generate its reference into this plane once:

  ```bash
  charter browser install          # writes .claude/skills/playwright-cli/
  ```

  Charter ships none of those pages: they are Apache-2.0 and they change far more often
  than charter releases. Regenerate with that command rather than editing them.

- **Where credentials come from, and how parallel workers stay isolated** — this skill.

## One session per worker

Each worker passes its own `-s=<name>`. Sessions hold independent cookies, localStorage,
IndexedDB, cache and tabs, so N workers can each be logged in as a different user at once.

```bash
npx @playwright/cli@<version> -s=owner  open https://example.test/
npx @playwright/cli@<version> -s=viewer open https://example.test/
npx @playwright/cli@<version> list          # live sessions
npx @playwright/cli@<version> -s=owner close
npx @playwright/cli@<version> kill-all      # reap stale processes
```

Pin the version explicitly in every command. The tool is pre-1.0 and its behaviour moves.

## Credentials — never typed, never printed

`charter secret exec --dotenv` resolves vault keys into one 0600 temp file and points
`PLAYWRIGHT_MCP_SECRETS_FILE` at it. You then refer to a secret **by name**; Playwright
substitutes the value and redacts it from output, so it never reaches the transcript.

> **Open the session *inside* the bridge.** `playwright-cli` reads
> `PLAYWRIGHT_MCP_SECRETS_FILE` once, when the session daemon starts. Setting it later, on
> a `fill` against an already-open session, **fails silently** — the literal string `PASS`
> is typed into the password field and no error is raised. Wrap the whole flow, `open`
> through the last `fill`, in a single `charter secret exec`.

> **Wait for each field before filling it.** `open` returns as soon as the first response
> lands, not when the page you are logging in to exists — an SPA typically redirects to an
> identity provider afterwards. Filling immediately fails with
> `"#username" does not match any elements`, which reads as a **wrong selector** and sends
> you hunting for a better one when the flow was already right. It is not free to get wrong
> here: `charter secret exec` shreds the dotenv file when it exits, so the still-open
> session can no longer be filled with a secret, and recovering means re-running the whole
> flow rather than the failed step. The example waits so it never needs to.

```bash
charter secret exec <vault> \
  --dotenv PLAYWRIGHT_MCP_SECRETS_FILE=USER:<user-key> \
  --dotenv PLAYWRIGHT_MCP_SECRETS_FILE=PASS:<pass-key> \
  -- bash -c '
    P="npx @playwright/cli@<version> -s=owner"

    # `open` returning is not the page existing. Poll for the element itself rather than
    # sleeping a fixed amount: a redirect to an IdP can take a moment or several.
    wait_for() {
      for _ in $(seq 1 15); do
        $P eval "() => !!document.querySelector(\"$1\")" 2>/dev/null | grep -q true && return 0
        sleep 2
      done
      echo "TIMEOUT waiting for $1" >&2; return 1
    }

    $P open https://example.test/
    wait_for "#username" || exit 1
    $P fill "#username" USER
    wait_for "#password" || exit 1
    $P fill "#password" PASS
    $P click "button[type=submit]"
    $P snapshot
  '
```

The second `wait_for` is not redundant: an identity provider may ask for the username on
one screen and the password on the next, so `#password` can be absent at the moment the
first field is submitted.

Use `charter persona secret exec` to read the **active persona's** vault instead of naming
one. A different fixture account is a different pair of keys, not a different mechanism.

To avoid re-authenticating on every run, save the logged-in state once and reload it — see
`storage-state` in the generated Playwright reference.

## Hard rules

- **Never read a filled secret back.** No evaluating `el.value`, no screenshot of a filled
  password field, no dumping the dotenv file. Substitution and redaction exist precisely to
  keep the value out of the conversation; reading it back defeats both.
- Never type a credential in directly, even "just once to test". Use the bridge.
- Never commit a session directory or a storage-state file — they carry live cookies, which
  are the credential in another form.
