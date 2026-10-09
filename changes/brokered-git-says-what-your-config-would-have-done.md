### Added

- **A clone or worktree the app makes for a chat says when your git config would have
  mattered.** It reads none of your git config, by design. A checkout whose `.gitattributes`
  names a filter, the top one or a folder's, now says so (up to eight by name, the rest
  counted): for Git LFS, that the files arrived as pointer files and that `git lfs pull`
  fetches them. A clone that git reports could not reach its host, where your own
  `http.proxy`, certificate setting, extra header or `url.<base>.insteadOf` would have changed
  its route, names the key (never its value) and says to clone it in your terminal. A clone
  the forge answers as not there names none (#1550). `docs/plane-format.md` explains the
  difference (#1413).
