### Added

- **A sandbox block in an opencode chat raises a Notice too.** When a shell command an opencode
  chat ran fails because its sandbox refused it, the chat's tab offers Allow for this chat,
  Always allow and Keep blocked, and the block is counted in `purlis doctor`. opencode hands
  back a command's output and errors as one stream, so purlis reads the whole of a failed
  command's output, not only its errors as for Claude Code. Because of that, an opencode Notice
  pre-fills a host or folder only from the sandbox's own report, and otherwise asks you to type
  it (#1353).
