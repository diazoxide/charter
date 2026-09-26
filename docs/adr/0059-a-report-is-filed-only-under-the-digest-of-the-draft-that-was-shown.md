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

This keeps ADR 0003's point and drops its mechanism. The objection was to a flag an agent can
pass without anyone seeing what it publishes. A `--yes` that must name the draft's digest cannot
be passed that way: the agent has to print the draft first, into the same conversation the
reporter is reading. The two-step flow is still there, as two runs of one command rather than
two commands, and nothing is stored between them.

**Other decisions:**

- **Filed as the reporter, never as a token** (charter-plane ADR 0001). A chat can hold a
  plane's or a vault's token in `GH_TOKEN`, and `gh` prefers that variable to its own stored
  login. So `forge::gh_as_the_operator` hands `gh` none of `GH_TOKEN`, `GITHUB_TOKEN`,
  `GH_ENTERPRISE_TOKEN` and `GITHUB_ENTERPRISE_TOKEN`, and passes everything else a forge call
  gets, including `GH_CONFIG_DIR`, where the reporter's own login lives. When `gh` cannot
  file, charter prints the prefilled `issues/new` link, as the Python did.
- **Scrubbed by what charter can identify, visibly.** Four kinds of text are removed:
  - any line `secretshape` reads as a credential;
  - the value of any variable in the environment of eight characters or more (a terminal's
    name excepted). When a chat holds a vault's values, this is where they are;
  - the plane's path and home-directory paths;
  - the names of the plane's workspaces, clones, personas and vaults.

  Each removal leaves a placeholder that says what it was (`[env $DEPLOY_KEY]`,
  `[workspace]`), and the preview lists the categories. The reporter's read is the other half
  of the scrub.
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
  costs something. It can come back as `--on <issue>` without a store.
- **A bare `--yes`.** Rejected, for ADR 0003's reason.
- **Unlock every vault to scrub its values.** Rejected. It would prompt the keyring and call
  1Password on every draft. Vault values that are not in the environment are caught only by
  their shape.

## Consequences

`machine.rs` reserved `~/.config/charter/` partly for a reporting consent file. Nothing writes
one now.
