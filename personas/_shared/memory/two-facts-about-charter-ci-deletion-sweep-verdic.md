# Two facts about charter CI deletion sweep verdicts, measured on PR #978

_2026-09-12 01:12 · persistent_

Two facts about charter CI deletion sweep verdicts, measured on PR #978 (ce7f5d8, 2026-09-12). (1) The verdict job counts platform-deferred mutants apart from survivors: its name read "29 survivors" while the add-up gate line said "4 unpinned, 25 in masked cluster(s), 1 platform-deferred"; the deferred one was a FileNotFoundError clause, which the sweep leaves for CI to judge. (2) A shift-boundary survivor on a set comparison such as set(code) <= S (to <) cannot be pinned when set(code) can never equal S: a two-letter porcelain code against an eight-letter frozenset agreed on all 256 codes. Resolve it by saying what is meant, set(code).issubset(S), which has no boundary, rather than writing a test that cannot exist.
