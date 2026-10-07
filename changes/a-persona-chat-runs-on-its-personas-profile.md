### Added

- **A persona can name the profile its chats start on.** A `profile:` line in a persona's
  definition names one of the project's harness profiles. The new-chat picker moves to it when
  that persona is picked, unless you already picked a harness. The persona view shows it, and
  *Set <persona>'s profile…* on its heading sets it. A chat handed to that persona starts on its
  profile, so a Claude Code chat can hand work to a persona that runs on Codex, and the brief
  reaches it the way that harness takes a first message. Name a built-in or a declared harness
  in a shared project: on a machine that does not offer the name, the chat starts on the asking
  chat's profile and says so. `profile: none` opts a persona out of a profile it inherits. A
  persona with no `profile:` whose `model:` is exactly a profile's name is read the same way.
  Claude Code and Codex profiles are tested; opencode is not yet (#1445).

### Security

- **A persona's profile is a name that is looked up, never a command that is run.** A name in
  a persona's definition that is not a built-in, a declared harness or a local profile on this
  machine starts nothing of its own. A profile whose command nobody on this machine has been
  shown and approved is not started by a handoff either; the new-chat picker shows it and asks
  (#1445).
