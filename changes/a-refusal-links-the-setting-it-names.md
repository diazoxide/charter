### Fixed

- **A refusal in Settings links the setting it is about, whatever its key.** The core now hands
  each standing refusal of `charter.toml`, `charter.local.toml` and a workspace's settings over
  with the key it is about, so Settings no longer reads the key out of the sentence. An
  extension whose id holds a dot (`my.ext`) now gets its link too (#1292).
- **A workspace folder that is a link is refused in its own words.** Saving a workspace's
  settings, or any other write of its `workspace.json`, used to say the workspace "was renamed
  or removed" when its folder is a link to another folder. It now says that the folder is a
  link, and that purlis writes `workspace.json` only in a real folder. The link is never
  followed (#1292).
