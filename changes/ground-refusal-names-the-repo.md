### Fixed

- **The sandbox reads more spellings of a protected script and of a chat's own folder.** A
  script named in a value attached to its option (`-bf./tools/x.awk`, `--from-file=./x.jq`,
  `-javaagent:./agent.jar`) is protected the same as one given as a separate word. On macOS
  a folder reached under its data-volume name (`/System/Volumes/Data/Users/…`) counts as the
  same folder. That holds when a chat's ground is checked, and in the rules of every
  sandbox purlis compiles. On a volume that ignores letter case, a folder that doesn't
  exist yet and differs from a chat's ground only in case counts as that ground. When a chat
  is refused because a config names its ground, the refusal now names the repo or workspace
  the config sits in (#1356).
