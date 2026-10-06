### Security

- **The first secret check now answers the way the last one does.** These checks now read
  string escapes, as the project save does: the warning on a memory or ref write, a session
  record, a repo's instructions offered as memory, a harness profile's fields, a workspace's
  settings, the git identity form, a handoff brief, the summary of a permission ask, and the
  redaction of a bug report and of the app's log. Before, an earlier check could pass a spelling
  that the save then refused. A backslash at the end of a line now joins it to the next, as in a
  TOML `"""` string, so a token split across the break is found, the commit and push check
  included. A line read without the join is still read too, and a join never turns prose such
  as a Markdown line break into a credential assignment (#1315).

### Fixed

- **The commit and push check names an escaped address once.** An escape written right before
  an email address (`\n`, `\0`) no longer lists the address twice, once as written and once as
  it reads. It is listed once, by the value it spells. Both are still listed when the allowlist
  lets one through and not the other (#1315).
- **The commit and push check stays quick on a long line or a long diff.** A line of many
  different values, or many lines a backslash joins, no longer slows it down with every value
  more (#1315).
