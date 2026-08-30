# The deletion sweep's mutant was not always the edit it printed: ast node

_2026-08-30 11:06 · persistent_

The deletion sweep's mutant was not always the edit it printed: ast node spans exclude the parentheses a programmer wrote, so splicing a sub-node's source text back re-associates the expression. Measured 144 of 8,903 expression mutations in charter/ + tools/ (#655/#680). Fix is tools/sweep.py:parenthesised — spell any replacement whose top AST node binds loosely (LOOSE/TIGHT name every ast.expr subclass, asserted in the suite).
