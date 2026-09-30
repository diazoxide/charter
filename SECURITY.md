# Security

## Reporting a vulnerability

Please report a vulnerability privately, through GitHub's
[private vulnerability reporting](https://github.com/diazoxide/charter/security/advisories/new),
and not in a public issue.

## Checking that a download is a real charter build

Every file a release publishes, on the stable channel and the `dev` prerelease alike, carries
a signed [SLSA build provenance](https://slsa.dev/spec/v1.0/provenance) attestation. It says
the file came out of this repository's `release.yml` on a GitHub-hosted runner, and names the
commit the workflow ran at. With the [GitHub CLI](https://cli.github.com/) installed:

```sh
gh attestation verify charter-macos-arm64.dmg \
  --repo diazoxide/charter \
  --signer-workflow diazoxide/charter/.github/workflows/release.yml \
  --deny-self-hosted-runners
```

It works the same for the `.deb`, the AppImage, the `.app.zip` and the updater archives. A pass
prints the workflow and the commit. A file that was changed after it was built, or built
anywhere else, fails. To pin a stable release to its tag, add
`--source-ref refs/tags/vX.Y.Z`.

- **This is not what the updater checks.** The installed app trusts an update because the
  operator's minisign key signed it
  ([ADR 0042](docs/adr/0042-charter-updates-itself-and-nothing-it-cannot-verify-reaches-it.md)).
  Provenance answers a different question: which build made this file.
- **A dev build names the workflow's commit.** A dev build runs after `ci` passes on `main`, so
  the commit in its attestation is `main`'s head when that run started. The commit it was built
  from is in the `dev` release's notes. The two differ only when something merged in between.
- **A build started by hand is not attested.** It is published nowhere.
- **Releases made before provenance was added have none.**

## What charter's chat sandbox is, and what it is not

A plane can put every chat charter starts in a sandbox ([ADR 0067](docs/adr/0067-a-chat-runs-in-a-sandbox-charter-compiles-for-its-harness.md)):
`[sandbox]` with `mode = "on"` in its `charter.toml` ([the plane format](docs/plane-format.md)).

**What it covers.** Chats that charter starts, on a harness charter compiles the policy for.
Today those are Claude Code and Codex. A sandboxed chat reaches only the hosts of the plane's egress
presets. It is always denied four classes: a vault's storage, charter's integrity state, the
powers that belong to a person, and, on a runner, the runner's own internals. A chat that
cannot be given all of this does not start. A plane can turn the sandbox on, but never off.

**What it does not cover.**

- **Agents charter did not start.** For those, charter can export each harness's managed
  settings. It does not enforce them.
- **Charter itself.** Charter reads and writes files in a chat's directory by name. Until the
  `openat` rewrite of charter's core and its external review land
  ([ADR 0028](docs/adr/0028-containment-checks-a-path-and-does-not-hold-it.md)), a sandboxed
  chat that races charter's own file access may get charter to do what the chat cannot. So the
  sandbox is a boundary between a chat and the machine, not between a chat and charter.
- **opencode chats.** Charter does not sandbox them yet, so a plane with the sandbox on does
  not start them.
- **What a person does inside a chat.** A harness's own screen can offer the person at it a
  way to widen its sandbox, such as Codex's `/permissions` picker, which offers full access.
  Charter neither sees nor records that choice.
- **Codex on Linux.** Codex's sandbox has been measured holding the policy on macOS only.
  On Linux charter compiles the same profile, and it has not been measured there yet.
- **Codex's temp directories.** A sandboxed Codex chat can also write the system's shared
  temp directories, as Codex's own workspace-write sandbox allows.
- **The `PreToolUse` guard.** It is a guard against mistakes, not against an attacker with
  shell access as your user. The sandbox is what enforces. The guard explains a refusal.
