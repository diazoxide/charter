# charter report comment <id> --on N posts report.render(rec) through gh i

_2026-09-10 12:20 · persistent_

charter report comment <id> --on N posts report.render(rec) through gh issue comment: the whole drafted body plus the charter/Python/OS footer, with the scrubber applied at draft time like an issue. The 72-character title cap does not apply, since comment_on never calls title(rec). It has no --dry-run: the draft printout (or charter report show) is the preview, and the command prints the issue URL, not the comment URL, so fetch that with gh issue view --json comments.
