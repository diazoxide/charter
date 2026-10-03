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
`charter-macos-arm64.cdx.json` and `charter-linux-x86_64.cdx.json`. What it lists, exactly:

- **The Rust crates of the three charter binaries**: the app, the `charter` command beside it
  and the built-in `persona-statistics` extension. They are read out of the built binaries
  themselves (see `cargo auditable` below): on macOS from the `.app`, on Linux from the `.deb`.
  The AppImage carries the same three binaries, but it also bundles system libraries
  (WebKitGTK and what it needs), and the SBOM does not list those. Nor does it list the macOS
  system frameworks the app links.
- **The web front end's npm packages**, from `app/package-lock.json`, production packages
  only. The front end is compiled into the app, so these are the packages it was built from,
  not ones found in the download.

Like every build file, the SBOM carries provenance, so check it with `gh attestation verify`
first. Then list what it names:

```sh
jq -r '.components[] | "\(.purl // .name)"' charter-macos-arm64.cdx.json
```

Any scanner that reads CycloneDX can check it against advisories, for example
`grype sbom:charter-macos-arm64.cdx.json`.

**Every Rust binary charter ships carries its own dependency list.** The app, the `charter`
command beside it and the built-in extensions are built with
[`cargo auditable`](https://github.com/rust-secure-code/cargo-auditable), which puts the exact
crates and versions each one was compiled from into the binary. It is the first third-party
compiled tool in the release job that holds the signing keys, so it is pinned to one exact
version and built from its own lockfile (`cargo install --locked --version`). `cargo install`
cannot pin a checksum for the crate it installs; crates.io never replaces a published
version, and Cargo checks every crate it downloads against the index. So you can check an installed
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

## The repository's supply-chain score

The [OpenSSF Scorecard](https://scorecard.dev/) grades this repository once a week
(`.github/workflows/scorecard.yml`): pinned actions, token permissions, branch protection and
the rest. It reports and never gates a change. Its findings go to the repository's code
scanning, where maintainers read them. They are not yet published to the public Scorecard API.

## What charter's chat sandbox is, and what it is not

A plane can put every chat charter starts in a sandbox ([ADR 0067](docs/adr/0067-a-chat-runs-in-a-sandbox-charter-compiles-for-its-harness.md)):
`[sandbox]` with `mode = "on"` in its `charter.toml` ([the plane format](docs/plane-format.md)).
A project charter makes has it on. A project made before the sandbox existed keeps running as
it did, and charter offers once to turn it on.

**What it covers.** Chats that charter starts, on a harness charter compiles the policy for.
Today those are Claude Code, through its own sandbox, and opencode on macOS. A sandboxed
project does not start Codex at all until charter can hold it (#1123), except on a person's
audited opt-out.
opencode has no sandbox of its own, so charter runs the whole harness inside a profile it
writes, and the harness's traffic leaves through a proxy charter runs on the loopback interface
for that chat. A sandboxed chat reaches only the hosts of the plane's egress
presets. It is always denied five classes: a vault's storage, charter's integrity state, the
powers that belong to a person, on a runner the runner's own internals, and writing what a
program run later outside the sandbox loads, such as git's config and hooks, shell startup
files and each harness's and editor's project config. A chat that
cannot be given all of this does not start. Nor does a chat whose folder, or a folder between
it and the plane, is a link. Nor does a chat whose program is a relative path, or lies anywhere a
sandboxed chat may write (the plane, the chat's folders, the system temp folders), as written
or after its links, or whose command names a file in such a place, such as a script handed to
an interpreter. Nor does a Claude Code profile whose program does not answer as Claude
Code: a harness's own sandbox binds only that harness. That answer is the program's own, so it
catches a profile that is not Claude Code by mistake, not one written to pass; the person's
approval of the profile is what vouches for the program. The same approval is what catches a
path no word spells, in any encoding: inline code or inline config that builds the path of a
script in the plane, and a program that, by its own logic, reads and runs a file a chat can
write without being handed it. The check of a command's words is a lint that catches mistakes,
not a boundary. The boundary for every sandboxed harness, Claude Code included, is to be
charter's own wrap around the whole harness, which is not built yet for Claude Code (#1123). A plane can turn the sandbox
on, but never off.

**The opt-out is one chat's, and it is recorded.** A person can start one chat without the
sandbox from the window's new-chat picker, and nothing else can: there is no CLI word, file or
setting for it, and it is for a new chat only. A relaunched or resumed chat does not inherit
it: it starts sandboxed, or not at all where the sandbox cannot be applied (a Codex chat, a
refused program). Continuing an opted-out chat's conversation without the sandbox is not
offered yet (#1098). That chat's tab says
it runs without the sandbox. Charter records the start in its event log on this machine
(`trust.sandbox.off`), with the reason the person typed, and records the sandbox coming back
on (`trust.sandbox.on`). Charter also counts on this machine how many new chats started without
it, and never sends that count anywhere.

**What it does not cover.**

- **Agents charter did not start.** For those, charter can export each harness's managed
  settings. It does not enforce them.
- **Charter itself.** Charter reads and writes files in a chat's directory by name. Until the
  `openat` rewrite of charter's core and its external review land
  ([ADR 0028](docs/adr/0028-containment-checks-a-path-and-does-not-hold-it.md)), a sandboxed
  chat that races charter's own file access may get charter to do what the chat cannot. So the
  sandbox is a boundary between a chat and the machine, not between a chat and charter.
- **Windows.** Charter has no sandbox backend on Windows yet (#565), so every chat there
  starts without the sandbox. Its tab says so, and the start is recorded with charter, not the
  person, as the one who started it unsandboxed.
- **opencode on Linux.** Charter cannot wrap opencode on Linux yet (#1040), so a plane with
  the sandbox on does not start opencode chats there.
- **What opencode keeps for itself.** A wrapped opencode chat can also write what an opencode
  turn writes: its sessions database, log and storage. Its state is kept in the chat's own
  temp directory, never in opencode's. opencode's
  config, its cache and parts of its data directory hold code or settings a later opencode
  loads: its credentials file and its snapshot repositories. So none of those is writable,
  except the one config file a first run writes, and a sandboxed opencode chat makes no
  snapshots (#1075).
- **opencode's hook reports.** A wrapped opencode chat's hooks run inside the sandbox, so what
  they report is the chat's own claim, never proof that a hook ran (#1069). A report the app
  does not take is shown in the chat's window rather than kept for later.
- **Codex in a sandboxed project.** Codex's own sandbox resolves its paths again at every
  command, so a running chat could move what charter's rules name, and none of the classes
  would hold. Until charter runs Codex inside its own compiled sandbox, as it runs opencode
  (#1123), a sandboxed project refuses Codex. A person can still start it without the sandbox,
  and that choice is audited.
- **A directory moved into place, for Claude Code.** Claude Code's own sandbox keeps a chat
  from writing a clone's git config and hooks, but cannot keep it from moving a whole `.git`
  into place (#1065).
- **Temp directories.** A wrapped opencode chat gets a temp directory of its own. Some macOS
  tools ignore `TMPDIR` and are refused (#1120).
- **An embedded bare repository.** A chat can write a directory that git takes for a bare
  repository, and a git run *inside* it later reads its config. No protected name is in it,
  so the sandbox cannot tell it apart. Setting git's `safe.bareRepository = explicit` closes
  this (#1100).
- **What a person does inside a chat.** A harness's own screen can offer the person at it a
  way to widen its sandbox, such as Codex's `/permissions` picker, which offers full access.
  Charter neither sees nor records that choice.
- **The `PreToolUse` guard.** It is a guard against mistakes, not against an attacker with
  shell access as your user. The sandbox is what enforces. The guard explains a refusal.
