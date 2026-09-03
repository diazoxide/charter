# Going through tests/_tmuxreap.name() is NOT enough to make a tmux socket

_2026-09-01 14:28 · persistent_

Going through tests/_tmuxreap.name() is NOT enough to make a tmux socket reapable — the SLUG can leave the namespace. Measured on charter at 2e6eecb: test_a_planes_frame_really_reads_that_way builds its slug from the tmux binary filename, which at the 3.2 floor is 'tmux-3.2', and charter-frame-reads-in-tmux-3.2-<pid> has a '.' that _OURS ([a-z0-9] segments, pid last) rejects — owns() False, two live unreapable servers per floor test, from a caller that HAD followed the rule. The AST census over tests/ could never catch it (computed slug). Fix belongs in the producer: name() now refuses a slug whose result owns() would not recognise. Corollary: any 'go through the one helper' rule needs the helper to check its own output, or the rule only moves the defect one call deeper.
