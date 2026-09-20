# The browser lane

Driving a browser splits cleanly in two, and only one half is charter's.

| | owner |
| --- | --- |
| How to drive a page — snapshots, clicking, network mocking, tracing | Playwright |
| Where credentials come from, and how parallel workers stay isolated | charter |

Charter ships the `charter:browser` skill for its half and **none** of Playwright's pages.
`@playwright/cli` is Apache-2.0 and charter is MIT: redistributing the generated reference
would put a second licence, with its attribution obligations, into every wheel — for content
charter neither wrote nor maintains. It would also pin a pre-1.0 package that publishes
frequently to *charter's* release cadence, so a Playwright fix would wait on a charter
release to reach anyone.

```bash
charter browser install                 # generate Playwright's reference into this plane
charter browser install --version 0.1.19
```

## The paths it leaves behind

Three, of different kinds. Charter decides one and states the others — see
[ADR 0017](adr/0017-charter-ignores-what-carries-credentials.md) for why the line falls
there.

| path | what it is | posture |
| --- | --- | --- |
| `.playwright-cli/` | traces, snapshots, screenshots | **ignored by charter** |
| `.claude/skills/playwright-cli/` | the generated reference | yours to decide |
| `.playwright/` | `cli.config.json`, project config | yours to decide |

### `.playwright-cli/` — ignored, and not your call

The CLI's **output** directory: `.playwright-cli/trace` plus snapshots, screenshots and
PDFs. It appears when something writes there, not at install, which is why it tends to show
up as an unexplained `??` entry some time after the command that caused it.

Not a session, despite the name — a session lives in
`~/Library/Caches/ms-playwright/daemon/<hash>/<name>.session`, outside any repo. What makes
it credential material is **traces**: a trace records network requests with their headers and
bodies, and DOM snapshots of the pages that produced them. So a trace taken while
`charter secret exec` was logging in holds the login POST — the one thing the credential
bridge exists to keep out of reach.

Charter appends it to the plane's `.gitignore` and says so. Committing it would put that in
everyone's clone, and into the forge's history, where a later `git rm` does not reach it.
That is credential material, so charter does not ask.

`charter doctor` checks the same thing, because the install-time write only helps a plane
that runs the install *after* this landed — and the directory appears well after the command
that caused it, so it reads as ordinary untracked noise until something commits it.

### `.claude/skills/playwright-cli/` and `.playwright/` — stated, entirely your call

The generated reference, and the workspace config directory `install` creates for
`cli.config.json`. Charter leaves both untracked and ignores neither, because both answers
are defensible and the decision is the plane's. For the reference:

- **Commit it** — a fresh clone works with no `npx` round trip and no network. The cost is
  a tree holding ten pages that a later `install` rewrites without warning, so an edit to
  them looks durable right up until it is not.
- **Leave it untracked** — the tree stays honest about what is generated. The cost is a
  generator run per clone, and the reference simply being absent until someone does it.

`.playwright/cli.config.json` is ordinary project configuration — plenty of teams would
commit it, and charter has no opinion.

Whichever you pick, pick it once and write it down. Regenerate rather than edit: the pages
are Playwright's, and `install` is the only supported way to change them.

## Credentials

The bridge itself — `charter secret exec --dotenv`, one session per worker, and the failure
modes that silently produce a bogus login — lives in the `charter:browser` skill, which
ships with the plugin and so versions with the CLI rather than with a plane's copy of it.
Your harness loads it by name; `charter doctor` reports it missing. For the vault side of
it, see [docs/secrets.md](secrets.md).
## Sessions belong to a version

State lives under `~/Library/Caches/ms-playwright/daemon/<hash>/<name>.session`, and the
hash keys on the *installation*, not the working directory. Two commands that resolve
different versions talk to different daemons, and the second reports `The browser 'owner' is
not open, please run open first` while the first browser is alive and still logged in. Pin
the version explicitly in every command.
