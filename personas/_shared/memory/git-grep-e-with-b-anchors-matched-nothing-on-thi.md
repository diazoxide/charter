# git grep -E with \b anchors matched NOTHING on this macOS machine (2026-

_2026-09-10 11:58 · persistent_

git grep -E with \b anchors matched NOTHING on this macOS machine (2026-09-10): a pattern wrapped in \b returned empty, while the same alternation with a trailing [^A-Za-z_0-9] instead found the real call site. git grep -E uses POSIX ERE, where \b is not a word boundary. An empty result from a \b pattern is not evidence of absence. Use git grep -P, an explicit character class, or git show <rev>:<path> | grep.
