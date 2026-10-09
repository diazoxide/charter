# Changelog fragments

A pull request that people using purlis would notice adds one file here, `changes/<slug>.md`,
instead of editing [`CHANGELOG.md`](../CHANGELOG.md). Name it for the change
(`browse-worktree-files.md`), so two pull requests never write the same file.

The file is one or more of Keep a Changelog's headings, `### Added`, `### Changed`,
`### Deprecated`, `### Removed`, `### Fixed` or `### Security`, each with its entry under it,
written exactly as it will read in `CHANGELOG.md`:

```markdown
### Added

- **Read any file of a worktree in purlis.** A worktree's menu in the explorer has *Browse the
  files of …* (RC-5, #706).
```

Release prep runs `node tools/changelog-fold.mjs`, which puts each entry first under its heading
in `## [Unreleased]` and deletes the fragment. A dev build folds them into its own copy before it
is built, so About purlis shows them. CI's tool tests fold every fragment here, so one the script
cannot place fails the pull request.

`STANDING.md` is not a fragment either. It is a standing notice, written like one, that every
fold puts under `## [Unreleased]` unless it is there already, and never deletes, so every
version's notes carry it until a pull request removes the file. It announces the end of the
rename's compatibility window until 1.0 (ADR 0091); the pull request that closes the window
removes it. Apart from these two files, everything here is a fragment.
