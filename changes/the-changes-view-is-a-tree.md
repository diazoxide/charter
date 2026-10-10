### Changed

- **The Changes view reads like an editor's source-control view.** Each repo is a heading with
  the branch it is on, and under it are rows for its changes, its branches (each branch one row
  further in), its pipeline and any column an extension adds. It used to be a wide table that
  scrolled sideways in the side. The branches are drawn with the same tree guides as the
  explorer. The view is now one Tab stop that takes the arrow keys, so ⌃⇧G (Ctrl+Shift+G) puts
  the keyboard in it, and Shift+F10 on a repo's row opens that repo's menu (#1701, #1682).
- **An empty Changes or Search view says what goes there.** With no workspace focused, with no
  repos in the workspace, or with a search that matched nothing, the view says what it would
  list and what to try (#614).

### Removed

- **Search no longer has a view tab.** The left side's Search view is the only place a search
  is shown (#1701).
