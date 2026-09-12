# A guard that ends a heredoc body exactly where bash ends it will ALLOW s

_2026-09-11 21:40 · persistent_

A guard that ends a heredoc body exactly where bash ends it will ALLOW some calls an earlier regex-based guard refused, and that is a FIX, not a regression. Measured 2026-09-11 (bash 3.2.57, zsh 5.9): with 'charter handoff b <<BRIEF && bash <<A', a vault read sitting after a ' BRIEF' line (leading space, so not a terminator) or after a line continuation that eats the terminator is INSIDE the brief and no shell ever runs it. Before claiming a verdict change is a regression, run the case under a real shell with a marker file and see whether the read executes.
