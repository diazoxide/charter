### Fixed

- **A sandbox Notice about macOS's per-user folders reads true for each of them.** A write
  refused in `/var/folders/…/0` or `…/X` was said with the temporary folder's sentence; the
  sentence now names the per-user folders and gives `mktemp -p "$TMPDIR"` as the way round for
  the temporary one. The cache folder's Notice no longer names one harness. A Swift program's
  refused write, which Foundation reports as `NSCocoaErrorDomain Code=513` with the file's path,
  is now read as a block (#1416).
