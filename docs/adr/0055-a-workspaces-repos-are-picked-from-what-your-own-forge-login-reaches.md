# A workspace's repos are picked from what your own forge login reaches

**Accepted 2026-09-25**, from the operator's grill of the same day (the create-workspace flow).

A workspace holds clones, and until now the app could not put any in one: `workspace_create`
passed no repos, and nothing in the window cloned. The repos a plane could clone came from
`inventory/repos.json`, which `charter discover` rewrote from scratch on every run. The file is
tracked and pushed, and each engineer's `gh` or `glab` login reaches different repos. So one
engineer's `discover` replaced everybody's list with what their own login could see, and the
next engineer's run replaced it again. For a personal GitHub account it was worse still:
`discover` asks `users/<owner>/repos`, which GitHub answers with public repos only.

## The decision

**The picker lists what the operator's own login reaches, asked when it opens.**
`forge::list_accessible` asks as the operator (GitHub `user/repos` with
`affiliation=owner,collaborator,organization_member`; GitLab `projects?membership=true`), keeps
what is under the plane's declared owners, and drops its excludes. The answer is held in the
window and nowhere else, and a Refresh asks again. A forge that does not answer says why in its
CLI's words (`gh auth login`), and the other forges still list. A workspace with no repos can
always be made.

**The inventory only grows.** `inventory::add` merges records into what the file lists and
removes nothing. The picker adds the repos it clones that the file does not list yet, and never
overwrites a listed one: that record may carry a stack `discover` probed, where the picker's
says `"unknown"`. `discover` merges too: what this run's login could not see stays, because
another engineer's login may reach it. A repo leaves the inventory through
`[[forge]].exclude`, never because one listing lacked it. This is the one place the Rust
`discover` answers differently from the Python one, which replaced the list.

**The workspace is made first, and its repos clone after it, one at a time.** The dialog
closes at once, and the operator can start a chat while the repos land. The window asks
`clone_repo` once per repo, in turn, on a blocking thread. That is what lets each repo's own
state be drawn, lets a failed one be tried again, and keeps two clones from racing to write the
workspace's manifest.

**A workspace's settings add and remove repos with the same picker.** Ticking clones. Unticking
calls `drop_repo`, whose guard is inside the delete, as `workspace_remove`'s is: a clone holding
uncommitted or unpushed work, or one charter could not read, is refused, and so is a repo with
any worktree, because a worktree keeps its commits in the clone's object store. There is no
`force` there. Throwing work away stays `charter workspace remove --force`'s job.

## What this costs

Opening the picker costs a forge call per page per host, and it lists nothing while the
operator is offline. A repo already cloned in a workspace that the operator's login no longer
reaches is not in the picker, so it cannot be unticked there. The inventory can only be pruned
by an exclude, so a repo deleted on the forge stays listed until someone excludes it.
