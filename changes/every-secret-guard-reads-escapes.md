### Security

- **Every secret check reads a file the way it reads, not only the way it is written.** The
  commit and push check a chat's git hooks run, a workspace repo's save, and the Edit as TOML and
  Edit as JSON editors now find a credential written with string escapes, as the project save
  does. A file that does not parse (comments, a trailing comma, JSON5, JSON lines, a key given
  twice) and a file of any name are read with their escapes decoded too. The editors and the
  project save now ask one shared check, so they refuse the same documents. A file with
  ordinary escapes and no credential is still saved (#1304).
