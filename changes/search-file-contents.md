### Added

- **⌘⇧F searches the content of your files** (Ctrl+Shift+F off a Mac, outside a chat). It opens a
  Search tab on the branch picked in the explorer, or the workspace or project in front, and you
  can widen it to the workspace, the project, or every open project. Literal or regular
  expression, match case and whole word. Hits stream in grouped by project, branch and file,
  with the matching lines and a count per file; a large result stops at a page and _Show more_
  carries on, and a new query stops the one before. ↓ steps from the box into the hits, ↑/↓ move
  through them, and Enter opens the hit in its branch's file tab at its line. The search never
  looks inside what git ignores, links, vaults, files named like a credential, binary files or
  files past the preview's 2 MiB. A file with a line longer than 256 KiB is listed as not
  searched. Search tabs are not reopened at the next launch (FM-8, #1111).
