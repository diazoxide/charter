### Added

- **Settings' standing refusals link to the setting they are about.** Each line Settings shows
  under "charter does not take this from the files as they stand" that names a setting now has a
  Go to button, which shows that setting's group and puts the focus on the setting. The Saving
  view's line about a pull-request mode on an origin with no known forge links to Settings ›
  Saving › Mode (NO-7, #1232).
- **Edit as JSON at the Workspace level.** Settings at a workspace's level has an Edit as JSON
  link for its `workspace.json`, beside the Project level's Edit as TOML. charter checks the text
  before saving it, and refuses to save over a file that changed on disk since you opened it
  (NO-7, #1232).
