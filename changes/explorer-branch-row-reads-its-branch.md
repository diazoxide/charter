### Changed

- **A branch's row in the explorer reads its branch and its repo.** It used to show the name of
  the branch's folder, with the branch beside it. It now reads _`fix/login` in svc_, and its
  folder's path is in the tooltip. A folder with no branch checked out still reads as the
  folder's name. A typed letter in the tree finds a row by its branch (#1102).
