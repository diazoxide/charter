## Install

**macOS** (Apple Silicon): download
[`purlis-macos-arm64.dmg`](https://github.com/purlis/purlis/releases/download/__TAG__/purlis-macos-arm64.dmg),
open it and drag **purlis** to Applications. The build is not notarized, so before the first
launch, run this once in a terminal:

```sh
xattr -dr com.apple.quarantine /Applications/purlis.app
```

purlis was called charter before. If `charter.app` is in Applications, delete it once
`purlis.app` is there: purlis replaces it and will not start while the old app runs. An install
that updated itself keeps the folder name `charter.app` and needs nothing.

**Linux** (x86_64): install
[`purlis-linux-x86_64.deb`](https://github.com/purlis/purlis/releases/download/__TAG__/purlis-linux-x86_64.deb)
(`sudo apt install ./purlis-linux-x86_64.deb`, which also puts `purlis` on your `PATH`), or run the
[AppImage](https://github.com/purlis/purlis/releases/download/__TAG__/purlis-linux-x86_64-appimage.AppImage).

**Already installed?** charter updates itself: it offers __VERSION__ within a minute of launch,
checks the release key's signature, and installs it in place. Nothing needs the Python
`charter-cp` package; the app carries its own `charter` command and its own Claude Code plugin.
On macOS, **Install `charter` command in PATH** in the command palette puts that command on your
terminal's `PATH`.

**Checking a download:** every build file here carries a signed build provenance attestation
(`latest.json`, the update manifest, does not; the updater checks each build's own signature).
[`SECURITY.md`](https://github.com/purlis/purlis/blob/__TAG__/SECURITY.md#checking-that-a-download-is-a-real-charter-build)
gives the `gh attestation verify` command. What each build is made of is in its CycloneDX SBOM
(`purlis-<platform>.cdx.json`), and every Rust binary in it can be checked with
`cargo audit bin`; [`SECURITY.md`](https://github.com/purlis/purlis/blob/__TAG__/SECURITY.md#what-a-release-is-made-of-the-sbom-and-cargo-audit-bin)
says how.

---

