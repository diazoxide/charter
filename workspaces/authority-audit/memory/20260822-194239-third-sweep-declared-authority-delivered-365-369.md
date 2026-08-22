# Third sweep (declared authority) delivered: #365-#369 fixed via PR #377,

_2026-08-22 19:42 · persistent_

Third sweep (declared authority) delivered: #365-#369 fixed via PR #377, 3344 tests (+70). Three deferrals filed: #374 opencode ask_rule mistranslates MCP, #375 ask-approved carries no reason, #376 charter guard not all-or-nothing across harnesses. Left for the operator: #370 (what IS the override when a guard is wrong) and #371 (delete or narrow the clone-commit nudge — 471 asks, 97/98 approved). KEY DESIGN CATCH: _as_rule's parenthesis requirement was load-bearing BY ACCIDENT — str.startswith is a raw prefix match, so 'Globalprotect --connect' starts with 'Glob' and 'Taskwarrior add x' with 'Task'; the obvious fix (drop the paren requirement for mcp__) would have mirrored the bug. Test rule SHAPE, never a prefix.
