### Changed

- **Work a chat needs an answer from has one route, `purlis dispatch`.** A chat told to
  dispatch to a persona could run a reporting handoff instead, because the handoff skill
  taught `--report` as the way to get an answer and `--persona` for another persona. The
  handoff skill, the persona skill, `purlis docs show handoff`, `purlis handoff --help` and
  the briefing of a chat the app started (as where such work goes, not as a reason to give
  work away) now say one thing in the same words: work a chat
  needs an answer from, for its own persona or another, is `purlis dispatch` (with `--in
  workspace:<name>` when it must run elsewhere), and a handoff is fire-and-forget. The
  refusals that named `purlis handoff <workspace> --persona <name>` as the way to dispatch to
  a persona (a vault a chat may not use, `purlis persona use` inside a chat) name `purlis
  dispatch --to <name>` (#1515).
- **`purlis handoff --report` is carried out as a task.** It is still taken, and it sends the
  app the ask `purlis dispatch --in workspace:<workspace>` sends, so the new chat is a task of
  the asking chat in every respect: `purlis dispatch list` shows it, `wait`, `tell`, `answer`
  and `cancel` work on it, the window shows it as a task, and its report is delivered as a
  task's is. The result says so and names `purlis dispatch` as the route from now on. A task
  with no `--name` is called `handoff to <workspace>`; it leaves no todo and no `handoff` row
  in the dispatch log, and no extension is told a handoff was created; and `--report` with
  `--create`, or into a workspace that is not there, is refused with the two commands that do
  it, because a task works in a workspace that exists (#1515).

### Fixed

- **A reporting handoff's report could sit unread.** It was left for the asking chat's next
  turn and nothing started that turn, `purlis dispatch wait` refused its chat and `list` did
  not show it. As a task, its report starts the asking chat's turn where purlis may type into
  that chat, and can be waited for (#1515).
- **`purlis handoff --help` is no longer refused for lacking a heredoc.** purlis's Bash hook
  lets `--help` and `-h` through as the word right after `handoff`, where nothing is fed to
  the command and the call holds no other handoff. Every other refusal stands: from a
  sub-agent, inside a string a shell runs, with a spelling purlis cannot read, and beside a
  file, a pipe, a here-string or a live substitution (#1515).
