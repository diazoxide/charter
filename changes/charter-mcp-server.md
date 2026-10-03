### Added

- **Every chat the app starts gets charter's MCP server, whatever harness it runs.** Claude
  Code, Codex and opencode chats get the same tools, for that chat alone, beside your own MCP
  servers: `todo_list`, `todo_add` and `todo_done`, `memory_search` and `memory_add`,
  `session_record_list` and `session_record_read`, `change_status`, and `ask_operator`, which
  puts a question to you through the harness's own prompt. Each tool acts on the workspace the
  chat works in, and none of them reads a vault or a forge (HP-7, #674).
