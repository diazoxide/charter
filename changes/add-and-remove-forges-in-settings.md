### Added

- **Add and remove forges in Settings.** Settings › Forges at the Project level has Add forge,
  which opens a small form (kind, owner, host, repos never listed) and writes a new `[[forge]]`
  block to `charter.toml`, and a Remove on each block. What charter would refuse is said under
  the field it is about, and nothing is written. A forge is not removed while something needs
  it: a catalogued repo on its host, or a save mode that opens a request there. Those are named,
  with a link to the setting. The last add or remove can be undone (ST-3, #1227).
