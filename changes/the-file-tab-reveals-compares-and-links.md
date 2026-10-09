### Added

- **A search jump reveals its file in the file tab's tree.** The folders above the file a jump
  picked open, and its row is scrolled into view (#1137).

- **Show what changed in a file's own tab.** A file opened in a tab of its own offers *Show
  what changed* where the branch changed it, as the Files tab's preview does (#1189).

- **Copy path and Reveal beside a file in the file tab.** The header over a file, in the Files
  tab's preview and in a file's own tab, copies the file's path in the branch or shows it in your
  file manager, as a tree row's menu does (#1143).

### Changed

- **A file's comparison reads again when its branch moves.** While the explorer watches the
  branch, the comparison tab compares again by itself, keeping the comparison on screen until the
  new one comes; *Compare again* still asks at any time (#1189).
