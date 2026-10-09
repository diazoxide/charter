### Added

- **A clone or worktree the app makes for a chat says when your git config would have
  mattered.** It reads none of your git config, by design. A checkout whose `.gitattributes`
  names a filter, the top one or a folder's, now says so: for Git LFS, that the files arrived
  as pointer files and that `git lfs pull` fetches them. A clone that cannot reach its host
  where your own `http.proxy`, certificate setting, extra header or `url.<base>.insteadOf`
  would have changed its route names the key (never its value) and says to clone it in your
  terminal; a clone that fails for another reason names none (#1550). `docs/plane-format.md`
  explains the difference (#1413).
