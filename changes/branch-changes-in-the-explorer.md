### Added

- **What a branch changed, marked in the explorer.** In the explorer's tree, each file a branch
  changed, added, deleted or renamed against the branch it was cut from, committed or not,
  carries a mark, and each folder holding one says how many. A renamed file names where it came
  from, and a deleted file is drawn where it was. The marks follow agents as they write anywhere
  in the branch, even in a folder you never opened, with no refresh. *Changed only* collapses a
  branch's files to what it changed, every folder of it open. A *Filter files* box narrows the
  tree to names that contain what you type, and Esc clears it. A repo's own folder, which was cut
  from nothing, is marked with what isn't committed yet (FM-4, #1107).

### Security

- **Reading a branch's changes runs no filter program the repo's config names.** The explorer
  reads a branch's changes by itself as agents write, so a filter driver set in the repo's own
  git config is turned off for those reads. Content passes through unfiltered. A driver charter
  can't turn off makes the read refuse rather than run it (FM-4, #1107).
