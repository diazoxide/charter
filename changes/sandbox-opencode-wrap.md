### Added

- **opencode chats run sandboxed on macOS.** In a project with `[sandbox]` `mode = "on"`, an
  opencode chat now starts instead of being refused. opencode has no sandbox of its own, so
  charter runs the whole harness inside a profile it writes, which starts from "deny
  everything".
  - **Writes.** The chat can write its own directory and a temp directory of its own. Of
    opencode's own files, it can write only what a turn writes: its sessions and log. Its state
    is kept in the chat's own temp directory.
    It cannot write opencode's credentials, plugins or packages.
  - **Snapshots.** A sandboxed opencode chat makes no snapshots (#1075).
  - **Network.** Its traffic leaves through a proxy charter runs for that chat, including
    opencode's own requests to its model provider. The proxy carries only the hosts of the
    project's egress presets.
  - **Keychain.** The profile also keeps the chat away from the keychain, so a project with a
    keyring vault can run sandboxed opencode chats.
  - **Hook reports.** A report charter's hooks could not hand to the app is shown in the chat's
    window, not dropped.
  - **Linux.** Charter cannot wrap opencode on Linux yet (#1040), so there such a chat is still
    refused, with a sentence that says so (SD-2, #695).

### Security

- **A sandboxed chat no longer writes what a program run later loads.** This holds for Claude
  Code and opencode chats in every sandboxed project. Such a chat can no longer write any of
  these, at any depth of any directory it may write:
  - git's config and hooks, in every clone, worktree and submodule;
  - `.husky/` and `.githooks/`, the directory a `core.hooksPath` names, and every script a
    harness's project config runs;
  - shell startup files;
  - `.mcp.json`;
  - Claude Code's, Codex's, opencode's and editors' project config;
  - `charter.toml` and `charter.local.toml`.

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
    whether by its path or through a link.

  The chat then runs the program's real path, the one that was checked. The Claude Code check
  takes the program's own answer, so it catches a profile that is not Claude Code by mistake,
  not one written to pass. Approving the profile is what vouches for its program.
- **A project whose `charter.toml` cannot be read starts no chat.** Charter cannot tell whether
  such a project runs chats sandboxed. So it says so and starts nothing, rather than starting
  the chat unsandboxed.
- **A sandboxed project no longer starts Codex.** Codex's own sandbox cannot be kept to the
  rules charter compiles while it runs. So until charter runs Codex inside its own sandbox
  (#1123), a sandboxed project refuses a Codex chat with a sentence that says so. A person can
  still start it without the sandbox, and that choice is audited.
