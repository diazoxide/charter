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
cannot place fails the pull request. This file is the one in `changes/` that is not a fragment.
