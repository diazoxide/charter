### Changed

- **A repo's own folder is marked against its upstream.** With no recorded base, the change
  markers, "Changed only" and the cockpit's ahead and behind count from the branch it follows
  (`origin/main`, say), not only from its last commit. An upstream that is not one plain remote
  and branch is no base, as before (#1130).

### Fixed

- **An untracked folder the change markers cannot open costs only itself.** It is marked as one
  entry, and the rest of the branch's changes are still shown (#1130).
