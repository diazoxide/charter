### Fixed

- **The sandbox protects more of the scripts a project's configs run.** A script named
  without a `/` is protected where its program reads it as code: `bash hook.sh`,
  `awk -f sum.awk`, `python3 -m tools.x` and an MCP server's `node server.js`. The same
  holds when a launcher runs it (`uv run server.py`, `npx tsx server.ts`,
  `env node server.js`). A runner's own subcommands (`bun test`, `deno task`) and a Python
  package's folder stay writable. A file named without its extension is protected under
  each name its runner tries (`node -r ./hook` covers `hook.js`, `ruby -r./x` covers
  `x.rb`). Also covered: the value of an assignment (`BASH_ENV=./x.sh`, git's
  `-c core.hooksPath=./h`), each value after a compiler's `-Wl,`, and both readings of
  `-f./a=b.awk`. A folder named only after an `=` (`CARGO_TARGET_DIR=./target`) stays
  writable, unless the variable loads code (`BASH_ENV`, `NODE_OPTIONS`, `LD_PRELOAD`,
  `GIT_SSH_COMMAND` and the like), whose value is always protected. A later-letter value is matched to the project through a link, its data-volume
  name or another letter case. The folder of the socket a chat reports on is protected
  under each of its names. A start whose sandbox profile would be too long to hand over is
  refused with a reason and a way out, not a spawn error (#1418).
