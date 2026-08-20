# Statusline glyph rule settled (diazoxide/charter#309, commit 8c93847): ⚡

_2026-08-20 13:48 · persistent_

Statusline glyph rule settled (diazoxide/charter#309, commit 8c93847): ⚡ means EXACTLY ONE thing — a dispatch is running — because that fact renders in two places (persona chip + strip aggregate) and both must read as the same fact. The prompt-cache gauge therefore gave up the bolt and became 'cache NN%' (dim label, coloured number, same shape as 'ctx NN%'). Regression guard: tests/test_statusline_gauge.py::test_the_bolt_on_the_strip_means_a_dispatch_and_nothing_else asserts the rendered strip contains exactly one ⚡ and that it is the aggregate.
