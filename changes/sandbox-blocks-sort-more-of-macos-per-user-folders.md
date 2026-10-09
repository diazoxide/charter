### Fixed

- **More sandbox blocks in macOS's per-user folders are shown, and sorted right.** A write
  refused in `/var/folders/…/0` or `…/X` was taken for the chat's own temporary folder and
  dropped. A path written through `/System/Volumes/Data` was sorted as a system folder. swiftc's
  and xcrun's own words for a refused write were not read. Each now shows as a block in macOS's
  per-user folders, and none offers a grant (#1416).
