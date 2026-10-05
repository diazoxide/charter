### Added

- **A page per harness profile in Settings.** Each `[harness.<name>]` profile in this machine's
  local settings file has its own page under Harness & profiles, with its kind, command and
  environment. Add profile asks for a name, a kind and a command. Remove and Rename are on the
  profile's page. A profile the default harness names is not removed or renamed, and the setting
  that uses it is named with a link. What the profile rules refuse is said under the field it is
  about, and a pasted secret is refused with a pointer to the vault. The last add, remove or
  rename can be undone. A new or changed command still asks for approval before its first run.
  New profile… in the default pickers opens the Add form and picks the new profile (ST-4, #1236).
