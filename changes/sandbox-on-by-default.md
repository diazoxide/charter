### Added

- **New projects run their chats sandboxed.** A project that `purlis init` or the app makes
  now has `[sandbox]` with `mode = "on"` in its `charter.toml`. Its sandboxed chats can reach
  model providers, your forges and package registries (SD-2, #1056).
  - **Existing projects.** A project made before this keeps running as it did. The first time
    you open it, a notice offers to turn the sandbox on. "Turn the sandbox on" adds the same
    block to `charter.toml`, and "Keep it off" changes nothing. The notice does not come back
    either way.
  - **One chat without the sandbox.** The new-chat picker has "start without the sandbox", for
    that one chat, with a box for your reason. The chat's tab says it runs without the
    sandbox. A relaunched or resumed chat does not inherit the choice: it starts sandboxed, or
    not at all where the sandbox cannot be applied (a Codex chat, or a program the sandbox
    will not bind). Only a new chat can start without the sandbox; continuing an opted-out
    chat's conversation without it is not offered yet (#1098). If the sandbox cannot be
    applied on this machine, the picker shows why, and its button says "Start without the
    sandbox".
  - **Installing what is missing.** On Linux, if bubblewrap or socat is missing, the picker
    shows your distribution's install command. "Type it in a shell tab" opens a shell at the
    project root with the command typed. It needs sudo, so you run it yourself.
  - **Programs the sandbox will not bind.** A profile whose program is anywhere a chat can
    write or is named by a relative path, or a Claude Code profile whose program does not
    answer as Claude Code, is refused in a sandboxed project; the picker says why before you
    start it. It checks a profile's program only once you have approved the profile.
  - **Codex.** purlis cannot keep a Codex chat inside its sandbox yet (#1123), so in a
    sandboxed project the picker says so and a new Codex chat starts only without the sandbox.
  - **Windows.** There is no sandbox on Windows yet (#565), so chats there start without it
    and their tabs say so.
  - **Recorded.** Each start without the sandbox is a `trust.sandbox.off` event in purlis's
    event log on this machine. The sandbox coming back on for that chat is `trust.sandbox.on`.
    The chat's session record says `sandbox: off`.
  - **How often.** `purlis doctor` and Project settings show how many new chats in the project
    started without the sandbox on this machine. The count is never sent anywhere.
