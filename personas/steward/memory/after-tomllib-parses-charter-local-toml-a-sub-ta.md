# After tomllib parses charter.local.toml, a sub-table written [harness.cl

_2026-09-11 19:01 · persistent_

After tomllib parses charter.local.toml, a sub-table written [harness.claude.alt] and an inline table written enviroment = { CLAUDE_CONFIG_DIR = ... } under [harness.claude] are the same thing: a dict-valued key inside the profile's table. So any rule that treats nested tables as dotted profile names must still validate a parent that has keys of its own WITH those nested tables present, or a typo'd env table is silently dropped and the profile launches the default account (review 13). harness-profiles Task 1 settled it: a parent holding only sub-tables declares nothing (built-in kept, children refused by dotted spelling); a parent with its own keys is refused by UNKNOWN_PROFILE_KEY for the nested table. Found 2026-09-11 when the first F4 implementation broke the review-13 and ruling-37 tests.
