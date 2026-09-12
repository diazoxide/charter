# gh pr merge --squash WITHOUT --body writes the branch's commit messages

_2026-09-11 11:30 · persistent_

gh pr merge --squash WITHOUT --body writes the branch's commit messages into main's history: the squash commit's subject is the PR title plus (#N), and its body concatenates every branch commit's message (verified on PR 965's merge 3286a4f, whose body carries the fix commit's full message). So when a commit message already pushed on the branch carries a claim a later review proved false (PR 966: aeca22e's message claimed the runtime plane guard catches a PATH-resolved tmux; #967 says it does not), merge with an explicit, reviewed summary instead: gh pr merge <N> --squash --match-head-commit <sha> --subject '<title> (#N)' --body-file <file>. Force-pushing to rewrite the message is the wrong fix, because it breaks every SHA the reviews and ledger pinned.
