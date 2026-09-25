## Install

**macOS** (Apple Silicon): download
[`charter-macos-arm64.dmg`](https://github.com/diazoxide/charter/releases/download/__TAG__/charter-macos-arm64.dmg),
open it and drag **charter** to Applications. The build is not notarized, so before the first
launch, run this once in a terminal:

```sh
xattr -dr com.apple.quarantine /Applications/charter.app
```

**Linux** (x86_64): install
[`charter-linux-x86_64.deb`](https://github.com/diazoxide/charter/releases/download/__TAG__/charter-linux-x86_64.deb)
(`sudo apt install ./charter-linux-x86_64.deb`, which also puts `charter` on your `PATH`), or run the
[AppImage](https://github.com/diazoxide/charter/releases/download/__TAG__/charter-linux-x86_64-appimage.AppImage).

**Already installed?** charter updates itself: it offers __VERSION__ within a minute of launch,
checks the release key's signature, and installs it in place. Nothing needs the Python
`charter-cp` package; the app carries its own `charter` command and its own Claude Code plugin.
On macOS, **Install `charter` command in PATH** in the command palette puts that command on your
terminal's `PATH`.

---

