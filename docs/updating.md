# How charter updates itself, and the steps only the operator can take

charter-app updates itself with Tauri's updater, from GitHub Releases, on one of two channels.
The reasons are charter ADR 0042. This page is the part a person has to do by hand: generate
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

## 1. The updater keypair (minisign): mandatory

This is the key the app checks every update against before it installs anything. Without it
there is no updater.

```sh
cd app
npx tauri signer generate -w ~/.tauri/charter-updater.key
#   (on a machine where npx hangs: node node_modules/@tauri-apps/cli/tauri.js signer generate -w ~/.tauri/charter-updater.key)
#   Give it a password when asked.

gh secret set TAURI_SIGNING_PRIVATE_KEY          --repo diazoxide/charter-app < ~/.tauri/charter-updater.key
gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD --repo diazoxide/charter-app   # prompts; paste the password
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

base64 -i charter-devid.p12 | gh secret set APPLE_CERTIFICATE --repo diazoxide/charter-app
gh secret set APPLE_CERTIFICATE_PASSWORD --repo diazoxide/charter-app                     # the .p12 password
gh secret set APPLE_SIGNING_IDENTITY --repo diazoxide/charter-app \
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
gh release create dev --repo diazoxide/charter-app --prerelease --target main \
  --title "dev channel" --notes "The rolling dev channel. Not a release."
```

It **must** stay a prerelease. GitHub's `latest` pointer skips prereleases, and that is the only
thing keeping stable machines off dev builds. The workflow checks this before every dev
publish. After this, every green `main` replaces this release's assets. The tag itself never
moves.

## Cutting a stable release

```sh
# 1. set [workspace.package] version in Cargo.toml AND "version" in app/src-tauri/tauri.conf.json to X.Y.Z; merge it
git switch main && git pull
git tag vX.Y.Z && git push origin vX.Y.Z      # this, and only this, publishes a stable release
# 2. afterwards, bump both to the NEXT version and merge, so dev builds are X.Y.(Z+1)-dev.N
```

The workflow refuses a tag that disagrees with `Cargo.toml`, and a test holds `tauri.conf.json`
to `Cargo.toml`.

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

## What an operator on Linux gets

The **AppImage** updates itself. A **`.deb`** does not: `dpkg` owns it, and the manifest
deliberately has no entry a `.deb` install would match. An Intel Mac and arm64 Linux get no
updates at all, because the release has no runner for them.

