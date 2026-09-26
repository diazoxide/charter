# A report is filed only under the digest of the draft that was shown

**Accepted 2026-09-26**, by the operator's ruling on charter#363: reports file to
`diazoxide/charter` under the reporter's own identity, a saved panic record is offered as a
draft bug, and the command line comes before any button in the window.

The Python charter's reporting surface had eight verbs: `bug`, `gap`, `list`, `show`, `delete`,
`consent`, `send` and `comment`. It kept drafts on disk, asked for consent once per person, and
filed only through `send`. Its ADR 0003 (charter-plane, *"Nothing publishes without a human
'yes' — and there is no `--yes` flag"*) gave the reason for the two steps: *"a flag the agent
can pass is a flag the agent will pass unprompted."*

## The decision

**`charter report bug|feature` drafts, scrubs and prints the draft on every run. It files only
on the reporter's yes**: `y` at a prompt when both standard input and standard error are a
terminal, or `--yes <digest>`, where the digest is the one the preview of that same draft
printed. The digest is twelve hex characters of SHA-256 over the repository, the title and the
body, so `--yes` files only the exact bytes that were on screen. If one character changes, the
digest no longer matches and nothing is sent.

**What the digest does, and what it does not.** It makes the filed bytes the shown bytes: an
agent must print the exact draft into the conversation before `--yes` can file it, and nothing
it changes afterwards can be filed under the old yes. It does **not** make the yes a human's.
An agent can run the preview and then `--yes <digest>` in one turn with nobody answering, which
is ADR 0003's objection, and it still holds. What stands in front of that is the harness's own
permission prompt on the second run. A plane that wants a report to always ask can add
`charter guard ask 'charter report *--yes*'`. The operator asked for `--yes` knowing that; this
record is where the residual is written down.

Every bare run is the dry run the issue asked for: the preview is the exact title and body
`gh` would be handed. The one thing sent before a yes is the duplicate search, whose query
(words of the scrubbed title) is printed before it goes.

**Other decisions:**

- **Filed as the reporter, never as a token** (charter-plane ADR 0001). A chat can hold a
  plane's or a vault's token in `GH_TOKEN`, and `gh` prefers that variable to its own stored
  login. So `forge::gh_as_the_operator` hands `gh` none of `GH_TOKEN`, `GITHUB_TOKEN`,
  `GH_ENTERPRISE_TOKEN` and `GITHUB_ENTERPRISE_TOKEN`, and passes everything else a forge call
  gets, including `GH_CONFIG_DIR`, where the reporter's own login lives. When `gh` cannot
  file, charter prints the prefilled `issues/new` link, as the Python did, leaving the body out
  when it would make the link too long to open. `GH_CONFIG_DIR` still passes: it is where the
  reporter's own login lives, so a chat that points it at another account's configuration
  files as that account. That residual is the reporter's own environment, and is named here.
- **Scrubbed by what charter can identify, visibly.** Four kinds of text are removed:
  - any line `secretshape` reads as a credential;
  - the value of any variable in the environment of eight characters or more (a terminal's
    name excepted). When a chat holds a vault's values, this is where they are;
  - the plane's path and home-directory paths;
  - the names of the plane's workspaces, clones, personas and vaults — except charter's own
    words (`charter`, `charter-app`, `charter-plane`, `steward`) and the placeholders' words,
    which identify nobody.

  Each removal leaves a placeholder that says what it was (`[env $DEPLOY_KEY]`,
  `[workspace]`), and the preview lists the categories. The reporter's read is the other half
  of the scrub. A described report is prose, so it has no field allowlist: the scrub, and the
  test that no workspace or repo name reaches `gh`, stand where the issue asked for an
  allowlist test. Only a panic draft is a closed set of fields.
- **A panic is a closed set of fields.** From the app's `panics.log`, the draft keeps where it
  panicked (an absolute path is cut to its last three parts), the message (scrubbed, and
  marked as free text), and the charter version the record names. The app now writes that
  version into every record. The thread and the backtrace are dropped.
- **Duplicates are searched with `gh search issues --repo diazoxide/charter`.** `gh api
  search/issues` answers 404 on this repository. The candidates are shown for the reporter to
  judge, and they never stop a filing.
- **`feature`, with `gap` as its alias.** The prose guard's `report gap` row covers both words.

## Considered options

- **Port the eight verbs.** Rejected for this version. The draft store, the consent file, the
  caps and expiry exist to carry a draft from one command to another. A draft that is rebuilt
  from its input on every run, and filed under its own digest, needs none of them.
  `report comment` (adding a reproduction to an existing issue) is the one verb whose loss
  costs something: the preview lists possible duplicates, and charter cannot yet comment on
  one. It can come back as `--on <issue>` without a store.
- **A bare `--yes`.** Rejected, for ADR 0003's reason.
- **Unlock every vault to scrub its values.** Rejected. It would prompt the keyring and call
  1Password on every draft. Vault values that are not in the environment are caught only by
  their shape.

## Consequences

`machine.rs` reserved `~/.config/charter/` partly for a reporting consent file. Nothing writes
one now.

## Amendment, 2026-09-26: the ask rule is written by default

The operator ruled on #363 (D11): keep `charter report --yes <digest>`, and have charter write
an **ask** permission rule for `charter report *--yes*` by default, so the harness always asks
the operator before a report is filed. The residual named above, an agent previewing and then
filing in one turn, is now met by a prompt every plane has, not one a plane has to add.

- **`charter init` writes it** beside the handoff rule, in each harness's own syntax:
  `Bash(charter report *--yes*)` in `.claude/settings.json`'s `permissions.ask`, and
  `"charter report *--yes*": "ask"` in `opencode.json`'s `permission.bash`. As with the
  handoff rule, it goes into every harness or none: a file charter cannot read stops both
  writes.
- **`charter reinit` adds it** to a plane made before this. It appends to the existing lists
  and touches no other rule, and it mentions the rule only when it added it.
- **Workspace layers carry it at once.** When `init` or `reinit` writes the rule, every
  workspace layer charter generates is rewritten through the same writer `charter guard ask`
  uses (#449), so a chat started in a workspace is asked too.
- **`charter doctor`'s `ask rules` row warns when it is missing** from the settings a chat
  started in that directory reads, or from `opencode.json`, and names the harnesses that lack
  it. `charter doctor --fix` adds it through `charter guard ask`'s writer. Removing it stays the
  operator's choice: the row warns and never fails, and only `reinit` and `--fix` put it back,
  both of which the operator runs.
- **Codex has no equivalent.** Codex's `.rules` files (`prefix_rule`) match a command's
  arguments as a prefix, in order, so they cannot say "`--yes` anywhere after
  `charter report`". A `prefix_rule(["charter", "report"], decision = "prompt")` would ask
  before every preview as well, and a prefix ending in `--yes` misses a `--yes` that follows
  another flag. So charter writes nothing for Codex. There, the digest and Codex's own
  approval policy are what stand in front of a filing.

The rule matches `--yes` anywhere after `charter report`, so both `--yes=<digest>` and a
`--yes <digest>` that follows `--title` get the prompt. Answering `y` at the terminal prompt
is unaffected, because the person typing it is already the operator.
