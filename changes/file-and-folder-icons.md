### Added

- **File and folder icons.** The explorer's tree and a branch's file tab draw each file with an
  icon for its type, and each folder with one for its name, coloured from the theme so they
  follow light and dark. The icons are an icon theme: charter's own is a small set from Material
  Icon Theme (MIT), credited with its licence in About. A project picks another under `[theme]`
  as `icons = "<extension>/<icon theme>"`, and an extension contributes one as data under
  `contributes.icon_themes` (FM-3, #1106).
- **A licence check for the app's npm packages.** CI fails when a package the app ships, or an
  asset vendored into it, is under a licence outside `deny.toml`'s allow-list (FM-3, #1106).
