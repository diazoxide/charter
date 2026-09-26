---
name: browser
description: Drive a real browser for a task that needs one — a web login, an end-to-end check, verifying a UI, reaching something behind auth — with credentials pulled from a vault so charter hands the password to the browser instead of you typing it into the conversation. Use when browser automation needs to authenticate, or when several workers each need their own logged-in session.
---

# Driving a browser with vault credentials

Two different projects own the two halves of this:

- **How to drive a page** belongs to Playwright: snapshots, clicking, network mocking,
  tracing. Generate its reference into this plane once:

  ```bash
  charter browser install          # writes .claude/skills/playwright-cli/
  ```

  charter ships none of those pages. They are Apache-2.0, and they change far more often
  than charter releases. To update them, run that command again rather than editing them.
  It runs `npx`, so it needs Node.js. `--version` pins an exact version.

  The command also gitignores `.playwright-cli/`, which is where traces and snapshots land.
  A trace holds the network traffic of whatever it recorded, logins included. Whether to
  commit the generated pages and `.playwright/cli.config.json` is the plane's choice.

- **Where credentials come from, and how parallel workers stay isolated** is what this
  skill covers.

## One session per worker

Each worker passes its own `-s=<name>`. Sessions hold independent cookies, localStorage,
IndexedDB, cache and tabs, so N workers can each be logged in as a different user at once.

```bash
npx @playwright/cli@<version> -s=owner  open https://example.test/
npx @playwright/cli@<version> -s=viewer open https://example.test/
npx @playwright/cli@<version> list          # live sessions
npx @playwright/cli@<version> -s=owner close
```

**Pin the version in every command.** A session belongs to the *version* that opened it.
Two commands that resolve different versions look at different daemons. The second then
reports `The browser 'owner' is not open` while the first browser is still alive and logged
in.

## Credentials: never typed, never printed

`charter secret exec --dotenv` resolves vault keys into one 0600 temp file and points
`PLAYWRIGHT_MCP_SECRETS_FILE` at it. You then refer to a secret **by name**. Playwright
substitutes the value and scrubs it from the output it captures.

That is a net, not a boundary. A step that transforms or forwards the value is not scrubbed:
a screenshot, an `eval`, a POST. The credential goes wherever you send it.

Three rules for the flow:

- **Open the session inside the bridge.** `playwright-cli` reads
  `PLAYWRIGHT_MCP_SECRETS_FILE` once, when the session starts. Setting it later, on a `fill`
  against a session that is already open, fails silently: the literal string `PASS` is typed
  into the field. Wrap the whole flow in one `charter secret exec`, from `open` through the
  last `fill`.
- **Wait for each field before filling it.** `open` returns before an identity provider's
  redirect lands. A `fill` that runs too early fails with "does not match any elements",
  which looks like a wrong selector.
- **`not open` is not a cue to re-open.** A second `open` with the same `-s=<name>` starts a
  new browser and orphans the logged-in one. The dotenv file is gone once `charter secret
  exec` exits, so the new browser cannot be filled either. Re-run the whole flow instead.

```bash
charter secret exec <vault> \
  --dotenv PLAYWRIGHT_MCP_SECRETS_FILE=USER:<user-key> \
  --dotenv PLAYWRIGHT_MCP_SECRETS_FILE=PASS:<pass-key> \
  -- bash -c '
    P="npx @playwright/cli@<version> -s=owner"
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

To read the **active persona's** vault instead of naming one, use `charter persona secret
exec`. A different fixture account is a different pair of keys, not a different mechanism.

Reading a token *out of* a logged-in session through the vault (a `browser://` reference)
is not in this version yet. Do not stand in for it with `TOKEN=$(… cookie-get …)`: that is
command substitution into the transcript, with nothing to redact it.

## Hard rules

- **Never read a filled secret back.** Do not evaluate `el.value`, take a screenshot of a
  filled password field, or dump the dotenv file.
- Never type a credential in directly, even "just once to test". Use the bridge.
- Never commit a session directory, a storage-state file or a trace. Each one holds live
  cookies or authenticated traffic, which is the credential in another form.
