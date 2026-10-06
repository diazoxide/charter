### Changed

- **The app is purlis now: `purlis.app`, bundle identifier `dev.purlis.app`.** At its first launch
  the app moves its log folder from `dev.charter.app` to `dev.purlis.app` before it writes a log
  there, and holds its keychain items again under the new identity without asking: a vault whose
  items macOS would ask about keeps working and waits for **Finish moving**, which asks once per
  secret. Nothing moves while another charter is running, and purlis does not start while the
  old app still runs. macOS treats the renamed app as a new one, so it asks again for permission
  to show notifications. An install that updated itself keeps the folder name `charter.app`; a
  fresh `purlis.app` replaces it, and the old `charter.app` can be deleted. On Linux the
  one-per-user lock and the D-Bus name follow the new identifier, and the `.deb` is the `purlis`
  package, which replaces the `charter` package. Updates, the release assets
  (`purlis-macos-arm64.dmg`, `purlis-linux-x86_64.deb`, …), About's links and `charter report`
  use `purlis/purlis`; installed builds still find new releases through GitHub's redirect from the
  old address (RN-9, #1267).
