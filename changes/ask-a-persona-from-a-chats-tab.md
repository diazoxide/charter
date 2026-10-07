### Added

- **Ask a persona from a chat's tab.** A chat tab's menu and the palette have *Ask <persona>…*,
  one entry for each persona the project has finished. It asks for a task name and what to ask,
  and starts a chat as that persona under the chat you asked from, with that persona's own
  sandbox, hosts and vaults and nothing the first chat holds. You need no dispatch grant,
  because you are the one asking. So from a `steward` chat, *Ask devops…* starts a chat that
  can use the vault tagged for `devops`, with no handoff brief to write; and the Notice for a
  vault refused to a chat's persona has *Dispatch to <persona>…*, which opens the same dialog.
  Its first message says you asked, and its report reaches the chat you launched it from on
  that chat's next turn, marked as started by you. No command a chat runs starts one. The
  project's dispatch limits hold for it, and so does an administrator's policy: a lock on all
  dispatch takes the entry off every tab, a locked pair takes it off the tabs of chats running
  as the pair's first persona, and the palette's row says who locked it (#1438).

### Changed

- **A dispatched task's first line writes the asking chat's name in a code span**:
  ``⟨task from `steward 3` · workspace alpha · …⟩``. A chat's name is something a chat can
  choose, and the line around it is purlis's own (#1438).
