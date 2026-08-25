# sidebar
PR: https://github.com/diazoxide/charter/pull/523
Branch: frame-sidebar-headings-todos-version

## Blocking

### 1

(C) `docs/news/unreleased-the-sidebar-gets-headings-and-your-todos.md:95-97` claims the todo list "is empty on the very first paint" at launch and attributes it to #512. Verified false against the shipped code: `gather.read(fid)` falls through to a live `scan()` on a cold cache — which is exactly what `gather.discard`'s docstring says deleting the file restores — so with no cache file present, `slots.render("right", fid)` returns a fully populated `▪ todos 3` section (and `_bottom` returns a fully populated repo table). Narrow or drop the paragraph in the same commit; the implementer's `left_open` field carries the same unverified claim.

