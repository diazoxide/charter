### Added

- **A dispatch grant can be limited to one workspace.** When a chat asks to dispatch to
  another persona for a task that works in a workspace, the question now says which, and an
  Allow for you or for the project holds for work in that workspace only. *In any workspace*
  is a choice under the answers for when you mean the wider grant. The workspace is the one
  the task works in, wherever the asking chat is: the one named with `--in workspace:<name>`,
  the one a worktree is cut in, the one a handoff moves into, else the asking chat's own. An
  Allow for one chat covers that chat's tasks in that workspace. Settings › Project › Dispatch
  shows where each grant holds and lets you narrow or widen your own, and a project grant's for
  everyone, each asked first. "Any persona" can be limited the same way. A never is not: it
  holds in every workspace. Grants you already have hold in any workspace, as before (#1505).

### Changed

- **An Allow for one chat is for that task's workspace.** A chat you allowed to dispatch to a
  persona is asked again when it sends that persona to work in another workspace (#1505).
- **A chat nobody is at** starts a chat as another persona in another workspace only under a
  grant that names the pair and covers the workspace it goes into: a grant limited to any
  other workspace, its own included, does not carry it across (#1505).

### Security

- **A grant for one workspace is never read as a grant for all of them.** The project's file
  writes it as `{ to = "devops", in = "runners" }` in the persona's list, and your own are kept
  under their own keys, so an older purlis reads no grant there, and an entry that does not
  read grants nothing. A teammate's limited grant waits in Settings for your Accept. A grant
  counts only while its workspace is there, and a workspace made later under the name of one
  purlis saw removed inherits none: Settings shows the grant as covering nothing, with
  **Remove**, and **Count it again** for the workspace that is there now. **What this does not
  catch:** a workspace removed and made again with no dispatch and no look at Settings in
  between is not noticed. Every grant, revoke and change of a grant's workspace is recorded in
  purlis's event log with the workspace it holds in (#1505).
