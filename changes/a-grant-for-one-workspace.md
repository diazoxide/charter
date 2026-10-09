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
  holds in every workspace, and so does Keep blocked for a chat. At the project's root there
  is no workspace to limit a grant to, so the question and its answers say an Allow for you
  or for the project holds in any workspace. A question asked again for another workspace
  says where you already allowed the pair. Grants you already have hold in any workspace, as
  before (#1505).

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
  **Remove**, and **Count it again** for the workspace that is there now. `purlis workspace
  remove` and `purlis workspace rename` count the old name gone themselves. **A grant does not
  follow a rename**: after `purlis workspace rename runners ci` the grants "in runners" cover
  nothing until you set each one's workspace again in Settings, and the rename says how many
  it left. No grant is kept for a workspace that is not there yet: allowing a handoff that
  makes its workspace starts that one dispatch, and the next one asks. **What this does not
  catch:** a workspace's folder removed and made again by hand, or by a pull, with no
  dispatch and no look at Settings in between is not noticed. Every grant, revoke and change of a grant's workspace is recorded in
  purlis's event log with the workspace it holds in (#1505).
- **An Allow that starts one dispatch is for the dispatch you read.** Where an Allow starts a
  dispatch no grant covers (the list of pairs you said never to does not read, or the
  workspace is not there yet), it starts that dispatch with that brief and no other, and
  nothing of it is left once that dispatch has returned, or once you revoke the grant, say
  never, or change its workspace (#1505).
- **If you run two versions of purlis on one project:** a version from before this change that
  rewrites this machine's record (`app/sandbox.json`) drops the grants you limited to a
  workspace and what you accepted of the project's. Nothing widens: those dispatches ask
  again (#1505).
