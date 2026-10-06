### Fixed

- **Plain-file and reference vaults no longer look empty after your project's state folder
  moves.** A vault records its file as `.charter/vaults/<name>.json`, and moving the state folder
  to `.purlis/` left that record pointing at nothing, so the vault read as empty and the next
  `secret set` could start a second file. A vault file under the state folder is now read and
  written in the folder the project has, under either name, with nothing to rewrite. A vault file
  that is missing after values were written to it is now refused with its path, instead of being
  read as an empty vault (#1285).
