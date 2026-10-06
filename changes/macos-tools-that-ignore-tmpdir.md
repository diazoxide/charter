### Fixed

- **Swift and clang builds work in a wrapped opencode chat.** A wrapped chat's `TMP`, `TEMP`,
  `xcrun`'s cache and clang's module cache now point into its own temp directory, so `swiftc`
  and `clang -fmodules` no longer fail on macOS's per-user cache folder. `mktemp` without a
  path, and Swift or Objective-C programs that use `NSTemporaryDirectory`, still ask the system
  for its per-user temp folder, which no sandboxed chat may write; `mktemp -p "$TMPDIR"` works
  (#1120).
- **A sandboxed Claude Code chat's block notice names macOS's per-user folders.** A block in
  the per-user temp folder says so and gives `mktemp -p "$TMPDIR"` as the way round. A block in
  the per-user cache folder says Swift and clang builds cannot write it there yet, instead of
  reading as "a temporary folder" (#1120).
