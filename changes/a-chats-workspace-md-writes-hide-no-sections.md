### Changed

- **A chat's vision or section entry can no longer hide the rest of `workspace.md`.** A text
  that opens an HTML comment or a code fence and does not close it is refused, because a Markdown
  viewer would hide every section after it. A chat's write that would grow `workspace.md` past
  64 KiB is refused too, since every later chat in the workspace reads the whole file. The
  refusal says how to keep it short. Writes that shrink a longer file still go through (#1598).
