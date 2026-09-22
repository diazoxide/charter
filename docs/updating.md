# How charter updates itself, and the steps only the operator can take

charter-app updates itself with Tauri's updater, from GitHub Releases, on one of two channels.
The reasons are charter ADR 0042. This page is the part a person has to do by hand: generate
two keys, store three secrets, create one release. Until they are done, nothing is published
and the app offers no updates. Every error the release workflow prints points back to one of
these steps.

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

## 3. The Developer ID certificate: macOS code signing, no notarization

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

**Keep the same team for good.** macOS lets an app replace its own bundle in `/Applications`
without asking only when the new bundle is signed by the same team. Changing teams makes every
update ask for App Management permission.

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

The build is Developer ID signed and **not notarized**. A browser marks what it downloads as
quarantined, and macOS 15 and later answer a quarantined, un-notarized app with **"charter" Not
Opened**, which offers only *Done* and *Move to Trash*. To get past it once: click *Done*, open
**System Settings → Privacy & Security**, click **Open Anyway** beside charter, and confirm. Or
run `xattr -dr com.apple.quarantine /Applications/charter.app` before the first launch.

**Updates do not meet this dialog.** The quarantine flag comes from the program that downloaded
the file, not from macOS itself. charter's updater downloads into memory, verifies the minisign
signature, unpacks the new bundle itself and renames it into place, so nothing it writes carries
the flag. This was measured on macOS 26.2 against a copy of the updater's own install code; ADR
0042 has the details.

## What an operator on Linux gets

The **AppImage** updates itself. A **`.deb`** does not: `dpkg` owns it, and the manifest
deliberately has no entry a `.deb` install would match. An Intel Mac and arm64 Linux get no
updates at all, because the release has no runner for them.
