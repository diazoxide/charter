# How charter updates itself, and the steps only the operator can take

charter-app updates itself with Tauri's updater, from GitHub Releases, on one of two channels.
The reasons are ADR 0042. This page is the part a person has to do by hand: generate
one keypair, store two secrets, create one release. Until they are done, nothing is published
and the app offers no updates. Every error the release workflow prints points back to one of
these steps.

**Steps 1, 2 and 4 are required. Step 3 is not** — it buys a smoother first install and costs
an Apple Developer account. Without it macOS builds are ad-hoc signed, which publishes fine and
updates fine (ADR 0042 §3, amended 2026-09-23).

| Channel | Cut from | Manifest the app reads |
|---|---|---|
| **stable** (the default) | a `v*` tag, pushed by the operator | `releases/latest/download/latest.json` |
| **dev** | any green `main` | `releases/download/dev/dev.json` |

A machine switches with `charter update --channel dev` (or `stable`). The app checks a minute
after launch and every six hours after that. It **installs only when asked**, because
installing restarts charter and charter owns every running session.

On macOS and Linux an installed update waits in place, and the title bar says **Restart to
update**. That writes down every chat and view tab each open project holds, ends the chats, and
restarts into the new version, which asks whether to reopen every session or start fresh, says
that it restarted to install an update, and has **Reopen all** as the answer in front. A chat
that is mid-turn, or one that reports no state, is named first, and the operator chooses to
restart now or wait. A charter started with `--no-restore` still puts its projects back after
the restart. On Windows the installer closes charter itself when Install is pressed; charter
writes the same records just before it does, and the installer starts it again. Nothing names
a mid-turn chat there first, and none of the Windows path has been run: nothing is ported to
Windows yet.

## 1. The updater keypair (minisign): mandatory

This is the key the app checks every update against before it installs anything. Without it
there is no updater.

```sh
cd app
npx tauri signer generate -w ~/.tauri/charter-updater.key
#   (on a machine where npx hangs: node node_modules/@tauri-apps/cli/tauri.js signer generate -w ~/.tauri/charter-updater.key)
#   Give it a password when asked.

gh secret set TAURI_SIGNING_PRIVATE_KEY          --repo diazoxide/charter < ~/.tauri/charter-updater.key
gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD --repo diazoxide/charter   # prompts; paste the password
```

**Put the private key and its password in your password manager as well.** Every installed
charter trusts this one key for good. If it is lost, no installed app can ever be updated again,
and every operator has to reinstall by hand. If it leaks, whoever has it can sign an update every
installed charter will accept. Do not rotate it casually: an app only trusts the key it was built
with.

## 2. Commit the public half

```sh
jq --arg k "$(cat ~/.tauri/charter-updater.key.pub)" '.plugins.updater.pubkey = $k' \
  app/src-tauri/tauri.conf.json > /tmp/tauri.conf.json && mv /tmp/tauri.conf.json app/src-tauri/tauri.conf.json
git switch -c updater-public-key && git commit -am "The updater's public key" && gh pr create --fill
```

The `.pub` file is one line of base64, and that line is the whole value. A test in
`app/src-tauri` accepts either the placeholder or a real minisign key and fails on anything in
between. The release workflow refuses to publish while the placeholder is there.

## 3. The Developer ID certificate: optional, and what skipping it costs

**Skip this and everything still works.** With none of the three secrets below set, the
workflow ad-hoc signs the macOS bundle instead (`codesign -s -`). That is not the same as
leaving it unsigned: on Apple Silicon a binary with no signature does not execute at all, so
signing is never optional — only *whose* signature it carries is. What an ad-hoc signature
lacks is a developer for Gatekeeper to check, and the cost lands in exactly one place, a first
install from a browser, described at the bottom of this page.

**It does not cost you the updater.** The updater's trust is the minisign key from step 1, not
Apple's. Every update is verified against that key before anything is unpacked, whoever signed
the bundle.

If you do have an Apple Developer account, set all three and the workflow uses them. Set
*some* of the three and the workflow refuses the publish, because a half-configured
certificate silently signs with something nobody chose.

1. On developer.apple.com: **Certificates → + → Developer ID Application**. Create it and
   download it into Keychain Access.
2. In Keychain Access, export that certificate *with its private key* as `charter-devid.p12`,
   with a password.
3. Then:

```sh
security find-identity -v -p codesigning    # copy the "Developer ID Application: … (TEAMID)" line

base64 -i charter-devid.p12 | gh secret set APPLE_CERTIFICATE --repo diazoxide/charter
gh secret set APPLE_CERTIFICATE_PASSWORD --repo diazoxide/charter                     # the .p12 password
gh secret set APPLE_SIGNING_IDENTITY --repo diazoxide/charter \
  --body "Developer ID Application: Your Name (TEAMID)"
```

**If you adopt a team, keep it.** macOS lets an app replace its own bundle in `/Applications`
without asking for App Management permission when the new bundle is signed by the same team.
Changing teams is what makes every update start asking.

The obvious worry about ad-hoc is that it has no team at all, so nothing can match. **That was
measured on macOS 26.2 (Apple Silicon) rather than guessed, and it does not happen**: a replica
of the updater's install path, running inside an ad-hoc-signed app in `/Applications`,
replaced its own bundle with a differently-hashed ad-hoc one and was never asked for App
Management — `tccutil` afterwards reported no entry for the app at all. The run is described
in ADR 0042 §3, including what it does *not* cover. Moving from ad-hoc to a real team later is
the same one-time reinstall any team change is, so adopting a certificate later is not blocked
by anything here.

**Not set, on purpose:** `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID`, `APPLE_API_*`. Those
are notarization, which ADR 0042 declines. The consequence is below.

## 4. The dev channel's release: once

GitHub has no release without a tag, and nothing in CI creates a tag. So you create the one the
dev channel hangs on, once:

```sh
gh release create dev --repo diazoxide/charter --prerelease --target main \
  --title "dev channel" --notes "The rolling dev channel. Not a release."
```

It **must** stay a prerelease. GitHub's `latest` pointer skips prereleases, and that is the only
thing keeping stable machines off dev builds. The workflow checks this before every dev
publish. After this, every green `main` replaces this release's assets. The tag itself never
moves.

## Cutting a stable release

What a release brought is written in `CHANGELOG.md` (Keep a Changelog), under `## [Unreleased]`,
as it merges. Cutting the release is one PR and one tag:

```sh
# 1. one PR, merged:
#    - CHANGELOG.md: rename `## [Unreleased]` to `## [X.Y.Z] - YYYY-MM-DD`, put an empty
#      `## [Unreleased]` above it, and point the link references at the bottom at the new
#      tag (`[Unreleased]: …/compare/vX.Y.Z...HEAD`, `[X.Y.Z]: …/releases/tag/vX.Y.Z`)
#    - [workspace.package] version in Cargo.toml AND "version" in app/src-tauri/tauri.conf.json
#      set to X.Y.Z
git switch main && git pull
git tag vX.Y.Z && git push origin vX.Y.Z      # this, and only this, publishes a stable release
# 2. afterwards, bump both version fields to the NEXT version and merge, so dev builds are
#    X.Y.(Z+1)-dev.N
```

The `## [X.Y.Z]` section is the release: it becomes the GitHub release's body and the notes in
`latest.json`, and About Charter in that build shows the same section out of the copy compiled
into it. The release page puts **how to install** above it (`.github/release-install.md`, with the
tag and version filled in), because the page is where a first install starts. Lead the section
with a short paragraph saying what the release is, before `### Added`: that paragraph is the
headline on the release page and in About alike. So the workflow refuses a tag whose version has no section in `CHANGELOG.md`, or an
empty one, before it builds anything, beside refusing a tag that disagrees with `Cargo.toml`. To
see what a tag would publish, run `cargo run -p changelog -- X.Y.Z` on the merged `main`.

Between releases a test (`about.rs`) holds the crate version to the changelog: it has its own
section, or it is newer than every released version and `## [Unreleased]` is there. A test also
holds `tauri.conf.json` to `Cargo.toml`. A dev build shows `[Unreleased]` in About, as a dev build
of the next version; its release note stays the one-line "dev build of <sha>".

## What a first-time installer sees on macOS

Either way the build is **not notarized**, and either way a browser marks what it downloads as
quarantined, so a first launch is refused once. What differs is only how it is refused.

**The one instruction that works in both cases**, before the first launch:

```sh
xattr -dr com.apple.quarantine /Applications/charter.app
```

Quarantine is the flag Gatekeeper assesses. Remove it and there is nothing left to refuse.

**With a Developer ID (step 3 done).** macOS 15 and later answer a quarantined, un-notarized
app with **"charter" Not Opened**, which offers only *Done* and *Move to Trash*. Click *Done*,
open **System Settings → Privacy & Security**, click **Open Anyway** beside charter, and
confirm.

**Ad-hoc signed (step 3 skipped).** Gatekeeper has no developer to check, so it refuses a
quarantined copy outright. The **Open Anyway** route may work here too, but **that has not been
measured** and the dialog an ad-hoc bundle raises is not necessarily the one above — macOS has
a separate, blunter *"is damaged and can't be opened"* wording it uses for apps it cannot
attribute to anyone. Use the `xattr` command and the question does not arise.

**Updates do not meet any of this, ad-hoc included.** The quarantine flag comes from the
program that downloaded the file, not from macOS itself. charter's updater downloads into
memory, verifies the minisign signature, unpacks the new bundle itself and renames it into
place, so nothing it writes carries the flag. Measured on macOS 26.2 (Apple Silicon) against a
replica of the updater's own install code, running inside an ad-hoc-signed app in
`/Applications`: a quarantine flag on the old bundle did not survive onto the new one, no App
Management permission was requested, and the replaced bundle launched and updated again. ADR
0042 §3 has the table and the limits.

## The `charter` command in a terminal

The app ships its own `charter`, and every chat the app starts finds that one first on its
`PATH`. A terminal does not, until it is put there:

- **macOS**: run **Install `charter` command in PATH** from the command palette. It links
  `/usr/local/bin/charter` to the `charter` inside `charter.app`, the way VS Code's "Install
  'code' command in PATH" does, and macOS asks for an administrator's password when that
  directory is not yours. A link rather than a copy, so the command follows every update the
  app installs. It never replaces a `charter` somebody else put there — the Python charter,
  most likely — and says so instead; remove that one first if you want this one.
- **Linux**: the `.deb` installs `/usr/bin/charter`. The AppImage runs from a mount point that
  changes at every launch, so there is nothing stable to link to.

## What a chat brings with it

Nothing has to be installed into Claude Code or Codex. The bundle carries its own Claude Code
plugin, `charter-app` (`Contents/Resources/plugin` on macOS, `/usr/lib/charter/plugin` on
Linux), and each Claude Code chat the app starts loads it for that session alone with
`--plugin-dir`: charter's hooks, its Bash guard, and the `handoff`, `working-in-a-clone`, `update`, `persona`, `secrets` and `browser` skills. The same chat turns the Python charter's `charter@charter` plugin off for
itself, so a plane whose settings enable that plugin for your terminal sessions does not give
an app chat two sets of hooks. A Codex chat is armed the same way with `-c` flags. A `claude`
or `codex` you run in a terminal is untouched, and an update to the app updates all of it.

## What an operator on Linux gets

The **AppImage** updates itself. A **`.deb`** does not: `dpkg` owns it, and the manifest
deliberately has no entry a `.deb` install would match. An Intel Mac and arm64 Linux get no
updates at all, because the release has no runner for them.
