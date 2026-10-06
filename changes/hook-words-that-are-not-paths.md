### Fixed

- **A hook that runs `jq … // …` no longer makes a sandboxed project read-only.** The sandbox
  read every word with a `/` in a committed hook or MCP command as a script to protect, so
  jq's `//` operator denied writing `/`, and no chat could write anything. Now only a word
  that reads as a path counts as one. jq's filter text, operators, options, where output goes
  and the folders a command works in don't count. Scripts a command runs are still protected,
  however the command hands them over. If a rule would still cover `/`, the home folder, the
  project or the chat's own folder, the chat doesn't start, and the refusal names the file and
  the word (#1327).
