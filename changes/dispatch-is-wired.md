### Added

- **A dispatch to another persona waits for your answer, and starts when you allow it.** Where
  no dispatch grant covers the pair, the task is held: `purlis dispatch` says `held for the
  person` and exits 0, the Notice on the asking chat's tab shows the brief, and **Allow**
  starts exactly that task on the brief you read. **Keep blocked** starts nothing. Either way
  the asking chat is told on its next turn. A second dispatch across the same pair while you
  are being asked is not queued beside the first. The Notice says that a grant covers the
  helper sub-agents those chats run too (#1437).
- **Dispatch runs under the limits you set.** The limits in force for a dispatch are the
  project's, for the asking chat's workspace and persona, lowered by your own and capped by an
  administrator's policy, read afresh each time. A refusal names the limit and the count it
  stands at. A persona's two own limits are counted across every chat of the project: the
  tasks the chats running as it wait on, and the chats running as it at once. A limit of 0
  says where dispatch was switched off (#1439, #1440).
- **`purlis dispatch --profile <profile>`** starts the task on another of the project's
  harness profiles, and the `dispatch` tool takes `profile`. With none named, a task starts on
  its persona's own profile where the definition names one, else on the asking chat's, as a
  handoff does. Where the persona's own profile is not offered on this machine, the chat
  starts on the asking chat's and both chats are told so (#1436, #1445).

### Changed

- **A dispatch with no grant is no longer an error.** `purlis dispatch` used to answer `needs a
  grant` and exit 1. It now holds the task for your answer, says so on stdout and exits 0.
- **A chat on no harness profile can dispatch to a persona that names one.** It is refused only
  where there is no profile at all to start the new chat on.

### Fixed

- **Closing a chat in the middle of a chain no longer splits its lineage in two.** Each chat a
  dispatch starts keeps the id of the chat you started, and a lineage is counted by it, so the
  lineage limit holds when a chat between two others closes or is started again (#1436).

### Security

- **An Allow starts the brief you read, and no other.** The app keeps the held dispatch as it
  was asked, by its own number, and starts that one. A brief sent afterwards across the same
  pair is never started by the Allow of the first.
- **A chat nobody is at is refused in one place.** A chat whose harness runs with its
  permission prompts off, by what its harness reports or by the command it was started with,
  dispatches to another persona only under a grant that already stands for you or the project,
  and only from inside the sandbox. A grant made for one chat never counts for it, nothing of
  its ask is held, and no Notice is raised (#1446).
- **The persona chat takes its folder from the asking chat, and nothing else.** Not the hosts
  or folders you allowed that chat alone, not its start without the sandbox, not the grants it
  holds, and not a profile whose own command switches the prompts off.
