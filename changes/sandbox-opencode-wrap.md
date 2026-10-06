### Added

- **opencode chats run sandboxed on macOS.** In a project with `[sandbox]` `mode = "on"`, an
  opencode chat now starts instead of being refused. opencode has no sandbox of its own, so
  purlis runs the whole harness inside a profile it writes, which starts from "deny
  everything".
  - **Writes.** The chat can write its own directory and a temp directory of its own. Of
    opencode's own files, it can write only what a turn writes: its sessions and log. Its state
    is kept in the chat's own temp directory.
    It cannot write opencode's credentials, plugins or packages.
  - **Snapshots.** A sandboxed opencode chat makes no snapshots (#1075).
  - **Network.** Its traffic leaves through a proxy purlis runs for that chat, including
    opencode's own requests to its model provider. The proxy carries only the hosts of the
    project's egress presets.
  - **Keychain.** The profile also keeps the chat away from the keychain, so a project with a
    keyring vault can run sandboxed opencode chats.
  - **Hook reports.** A report purlis's hooks could not hand to the app is shown in the chat's
    window, not dropped.
  - **Linux.** purlis cannot wrap opencode on Linux yet (#1040), so there such a chat is still
    refused, with a sentence that says so (SD-2, #695).

### Security

- **A sandboxed chat no longer writes what a program run later loads.** This holds for Claude
  Code, opencode and Codex chats in every sandboxed project. Such a chat can no longer write any of
  these, at any depth of any directory it may write:
  - git's config and hooks, in every clone, worktree and submodule;
  - `.husky/` and `.githooks/`, the directory a `core.hooksPath` names, and every script a
    harness's project config runs;
  - shell startup files;
  - `.mcp.json`;
  - Claude Code's, Codex's, opencode's and editors' project config.

  As a result, a sandboxed opencode chat can no longer create a git repository, or clone one,
  inside its directory. What Claude Code's own sandbox still lets through is listed in
  `SECURITY.md` (#1065).
- **No sandboxed chat starts in a linked folder.** If the chat's folder, or any folder between
  it and the project, is a link, the chat is refused with a sentence that says so.
- **A sandboxed chat starts only on the program it was compiled for.** Each of these is refused
  with a sentence that says so:
  - a Claude Code profile whose program does not answer as Claude Code;
  - a program named by a relative path;
  - a program that lies anywhere the chat could write, such as the project or a temp folder,
    whether by its path or through a link;
  - a command that names a file in such a place anywhere in a word, such as a script handed
    to an interpreter, a flag's value or a `file:` URL.

  The chat then runs the program's real path, the one that was checked. The Claude Code check
  takes the program's own answer, so it catches a profile that is not Claude Code by mistake,
  not one written to pass. Approving the profile is what vouches for its program. It also
  vouches for a path no word spells, in any encoding, and for a program that reads and runs
  such a file without being handed it. The check of a command's words catches mistakes; it is
  not the boundary. For opencode and Codex, the boundary is purlis's own wrap around the whole
  harness; Claude Code is not inside it yet (#1150). A command word over 4 KiB is refused.
- **A project whose `charter.toml` cannot be read starts no chat.** purlis cannot tell whether
  such a project runs chats sandboxed. So it says so and starts nothing, rather than starting
  the chat unsandboxed.
- **Codex runs inside purlis's own sandbox.** Codex's own sandbox could not be kept to the
  rules purlis compiles while it runs, so purlis now runs Codex inside its own sandbox, as it
  runs opencode (see *Codex chats run sandboxed on macOS*).
