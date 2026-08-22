# charter guard's _as_rule: the parenthesis requirement in the old carve-o

_2026-08-22 19:39 · persistent_

charter guard's _as_rule: the parenthesis requirement in the old carve-out was load-bearing by accident — startswith(_RULE_TOOLS) is a raw prefix match, so 'Globalprotect --connect' hits 'Glob' and 'Taskwarrior add x' hits 'Task'. Any fix must test rule SHAPE (Tool(pattern) | bare tool name | bare mcp__ name), never a prefix, or it mirrors the bug it fixes (#365).
