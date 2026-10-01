# Security

## Reporting a vulnerability

Please report a vulnerability privately, through GitHub's
[private vulnerability reporting](https://github.com/diazoxide/charter/security/advisories/new),
and not in a public issue.

## Checking that a download is a real charter build

Every build file a release publishes (the `.dmg`, `.app.zip`, `.deb`, AppImage, the
updater archives with their `.sig` files, and the SBOMs), on the stable channel and the `dev` prerelease
alike, carries a signed [SLSA build provenance](https://slsa.dev/spec/v1.0/provenance)
attestation. It says the file came out of this repository's `release.yml` on a GitHub-hosted
runner, and names the commit the workflow ran at. With the [GitHub CLI](https://cli.github.com/)
installed, for a stable release tagged `<tag>`:

```sh
gh attestation verify charter-macos-arm64.dmg \
  --repo diazoxide/charter \
  --signer-workflow diazoxide/charter/.github/workflows/release.yml \
  --source-ref refs/tags/<tag> \
  --deny-self-hosted-runners
```

For a `dev` build, leave out `--source-ref`, or give `refs/heads/main`. A pass prints the
workflow and the commit. A file that was changed after it was built, or built anywhere else,
fails.

- **The update manifest is not attested.** `latest.json` and `dev.json` carry no provenance.
  The installed app does not need it to: it checks each build's own minisign signature, made
  by the operator's key, before it installs anything
  ([ADR 0042](docs/adr/0042-charter-updates-itself-and-nothing-it-cannot-verify-reaches-it.md)).
  Provenance answers a different question: which build made this file.
- **A dev build names the workflow's commit.** A dev build runs after `ci` passes on `main`, so
  the commit in its attestation is `main`'s head when that run started. The commit it was built
  from is in the `dev` release's notes. The two differ only when something merged in between.
- **A build started by hand is not attested.** It is published nowhere.
- **Releases made before provenance was added have none.**

## What a release is made of: the SBOM, and `cargo audit bin`

Each release carries a [CycloneDX](https://cyclonedx.org/) SBOM for each platform:
`charter-macos-arm64.cdx.json` and `charter-linux-x86_64.cdx.json`. The Linux SBOM covers the
`.deb` and the AppImage, which ship the same binaries. It is read off the built bundle, not the
source. The Rust crates come from the binaries themselves, and the web front end's packages
come from `app/package-lock.json`, production packages only. Like every build file, it carries
provenance, so check it with `gh attestation verify` first. Then list what it names:

```sh
jq -r '.components[] | "\(.purl // .name)"' charter-macos-arm64.cdx.json
```

Any scanner that reads CycloneDX can check it against advisories, for example
`grype sbom:charter-macos-arm64.cdx.json`.

**Every Rust binary charter ships carries its own dependency list.** The app, the `charter`
command beside it and the built-in extensions are built with
[`cargo auditable`](https://github.com/rust-secure-code/cargo-auditable), which puts the exact
crates and versions each one was compiled from into the binary. So you can check an installed
copy against the [RustSec](https://rustsec.org/) advisory database with no SBOM at hand:

```sh
cargo install cargo-audit --locked
# macOS
cargo audit bin /Applications/charter.app/Contents/MacOS/* \
  /Applications/charter.app/Contents/Resources/extensions/persona-statistics/bin/persona-statistics
# Linux (.deb)
cargo audit bin /usr/bin/charter* \
  /usr/lib/charter/extensions/persona-statistics/bin/persona-statistics
```

Builds published before this change carry neither.

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
