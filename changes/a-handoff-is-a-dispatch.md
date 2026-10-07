### Changed

- **A handoff is a dispatch: its consent is the dispatch grant, on every harness.** Your
  harness's permission prompt no longer consents to `purlis handoff`. The app decides a handoff
  as it decides a dispatched task. To the asking chat's own persona it opens at once and asks
  nobody. To another persona it needs a dispatch grant: the first time, nothing opens and a
  Notice on the asking chat's tab shows the brief in full, and you allow the pair for this
  chat, for you on this machine or for the project, or keep it blocked. The command returns at
  once saying the handoff is held, and on Allow the chat opens then, in the workspace the
  handoff named, which is created then and not before. Later handoffs across the pair open
  with no prompt. Nobody approves the brief: it is the new chat's first message, it is in the
  dispatch's record, and the chat that gets it is told who asked and still asks you for every
  command that asks (#1444). What a project author will notice:
  - `purlis init` writes no ask rule for a handoff, and a Claude Code chat the app starts
    carries an `allow` for one, as it does for a dispatch: by each flag the line can start
    with (`--name`, `--report`, `--persona`, `--create`, `--vision`) and for
    `purlis handoff report`, never a bare wildcard. So the spelling to write puts a flag first,
    `purlis handoff --name "<task>" <workspace>`. A line that starts with the workspace is
    still a handoff; Claude Code asks about it first, as it does for any command it has no
    rule for. An `ask` or `deny` of your own still wins.
  - A project made earlier still carries the rule `init` wrote, so its harness asks a second
    time before every handoff. `purlis doctor` says so on its `handoff gate` row, and
    `purlis doctor --fix handoff-rule` removes exactly that rule from `.claude/settings.json`
    and `opencode.json`, says what it removed, and names every rule about a handoff it left
    because you wrote it (a `deny`, an `allow`, another spelling, anything in
    `.claude/settings.local.json`). It runs only by name and commits nothing: a teammate on an
    older purlis is still asked by that rule and by nothing else, so commit the change once
    they have updated.
  - `purlis handoff` with no `--persona` now runs the new chat as the asking chat's own
    persona, as a dispatched task does. It used to start it as the project's default persona.
  - A handoff is held to the project's dispatch limits, its depth among them (3 unless the
    project sets another), and to an administrator's dispatch locks. A chain of handoffs had
    no depth limit before.
  - A handed-off chat starts with its own persona's hosts and vault, with nothing to allow on
    its tab. It no longer starts holding the asking chat's grants, and a chat that still holds
    another persona's is refused a handoff until you allow it its own.
  - `purlis guard handoff` is retired: it writes nothing and says what consents now. To have
    your harness ask as well, run `purlis guard ask 'purlis handoff*'`, which purlis holds
    under every spelling. The glob `init` used to write, `purlis handoff *`, is read as the
    retired rule: as an `ask` it covers the plain spelling only. A `deny` on it is yours and
    is held under every spelling.
  - `purlis doctor --fix rename-plane` gives a carried handoff ask no `purlis` twin.
  - **An older purlis in a shared project.** Once the rule is removed and committed, a
    teammate on a purlis older than this change is refused the `purlis handoff` spelling by
    their own hook, their `charter handoff` runs with no prompt and no grant, and their
    `init` and `guard handoff` write the rule back.
  - A handoff that was held and then allowed leaves no todo in its workspace yet.
- **A handoff from a Codex chat works.** purlis's hook refused every handoff from a chat whose
  harness reported its permission prompts off, and Codex reports that for nearly every run.
  The hook no longer reads it. The app weighs it instead, from its own mark on the chat: **a
  Codex chat is always taken as a chat nobody is at**, so it is never asked for a grant. To
  its own persona it hands off; to another it needs a grant that already stands and a
  sandboxed project; in a project with no sandbox it hands off to its own persona only
  (#1444).
- **purlis's handoff guard refuses less, and says why in its own words.** It still refuses a
  handoff from a helper sub-agent, one inside a string, a heredoc or a substitution a shell
  runs, one whose two words the shell would rewrite, and a brief the shell would change (a
  pipe, a file, a here-string, an unquoted heredoc, a live substitution). It no longer refuses
  a handoff for being spelt another way than a harness rule matched: a path to purlis,
  `python3 -m`, a `VAR=` prefix or a wrapper, a quoted word and two spaces are read as the
  handoff they are (#1444).

### Security

- **A handoff from a chat nobody is at is no longer refused outright; the app decides it.**
  purlis's hook used to refuse every handoff from a run with its harness's permission prompts
  off. What stands in its place: the app marks such a chat from its harness's reports, its
  profile's command and its own arguments, and never asks anyone on its behalf. It hands off to
  another persona only under a grant you made for yourself or the project, and only from inside
  a sandbox. To its own persona it hands off unasked, which is new, and is bounded: each handoff
  it opened that still works counts toward its running-per-chat limit, the lineage and depth
  limits hold, and it cannot create a workspace (`--create` is refused) (#1444).
- **A pre-allowed dispatch or handoff carries no rider.** A Claude Code chat is handed an
  `allow` for `purlis dispatch` and `purlis handoff`. Where a call runs one of them beside any
  other command, purlis's hook now answers `ask` for the whole call and names the other
  command, and it asks where it cannot read the call. This holds for the dispatch rules that
  were already there as well. Whether Claude Code would have asked by itself is not measured
  (#1444).
