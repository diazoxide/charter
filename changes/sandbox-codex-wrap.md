### Added

- **Codex chats run sandboxed on macOS.** In a project with `[sandbox]` `mode = "on"`, a Codex
  chat now starts inside the sandbox purlis writes, as an opencode chat does, instead of being
  refused (#1123).
  - **Codex's own sandbox is off inside it.** macOS applies one sandbox to a process, so Codex's
    own cannot run inside purlis's. purlis's holds what Codex's held, and what Codex's could
    not: a folder moved aside, changed and moved back, and a folder swapped for a link.
  - **A Codex home of the project's own.** A sandboxed project's Codex chats keep their Codex
    state in a home of their own, outside the project, seeded with your Codex login. They
    neither read nor write your own Codex home, so nothing they write in Codex's own state
    reaches a conversation you resume outside the sandbox. Files they write in their own
    folder, such as an instruction file, are still read by a later Codex started there.
  - **Writes.** The chat can write its own folder and a temp folder of its own. In the
    project's Codex home, only what a turn writes: its sessions, history, state, log and a
    refreshed login, and the memories, goals and queue Codex's own screen needs. So one
    sandboxed Codex chat can influence what a later sandboxed Codex chat of the same project
    loads, but never your own Codex state. It cannot write Codex's config, skills or
    plugins.
  - **Trust.** purlis marks each chat's folder untrusted for Codex, so Codex loads no
    project-local config, hooks or exec policies there and asks nothing, and it trusts exactly
    the hooks purlis arms, so purlis's own hooks run and no other does.
  - **Network.** Its traffic, Codex's own requests included, leaves through purlis's proxy,
    which carries only the hosts of the project's egress presets. Codex checks certificates
    against the system's authorities.
  - **Linux.** purlis cannot wrap Codex on Linux yet (#1040), so there such a chat is still
    refused, with a sentence that says so.

### Security

- **A Claude Code profile's program is asked its version inside purlis's sandbox.** On macOS,
  the check that a profile's program answers as Claude Code runs it in a sandbox that lets it
  write nothing but a temp folder of its own and reach no network. A program that does not
  answer in time is told apart from one that answers wrong, so the chat can be started again.
