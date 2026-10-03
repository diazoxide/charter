### Added

- **⌘P finds a file by fuzzy name** (Ctrl+Shift+P off a Mac). Once you type, a *Files* group
  follows the projects in the switcher. Each file shows its folder, branch and project, and Enter
  opens it in its file tab. The search starts in the branch picked in the explorer, or the
  project in front when no branch is picked. Tab widens it to the project, then to every open
  project when there is more than one, and the scope is shown above the files. Of two equal
  matches, the one nearer your focus comes first. ⌘P never finds files git ignores (a secret
  file git ignores included), links out of the branch, or anything inside git's own folder. With
  one project open, the key now finds files instead of doing nothing (FM-7, #1110).
