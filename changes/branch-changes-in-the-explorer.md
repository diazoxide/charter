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

- **The explorer reads a branch's changes without starting git.** It reads them by itself as
  agents write, so it no longer starts git in a branch's folder for them, and nothing that
  folder's git config names can run from that read. The read runs in a short-lived copy of
  charter itself, which is stopped if it runs too long or uses too much memory, so a branch built
  to hang or swell the read costs that copy and not the app (FM-4, #1107).
- **A git read through charter's git runner no longer fetches an object the repository
  lacks.** Such a read now fails instead of fetching the object on its own (FM-4, #1107).
