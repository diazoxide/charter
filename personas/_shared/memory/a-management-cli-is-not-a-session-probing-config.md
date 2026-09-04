# A MANAGEMENT CLI IS NOT A SESSION — probing config discovery with one yi

_2026-09-04 00:10 · persistent_

A MANAGEMENT CLI IS NOT A SESSION — probing config discovery with one yields a confident FALSE NEGATIVE. codex mcp list ignores a project .codex/config.toml entirely; claude plugin marketplace list reports "No marketplaces configured" even for a valid absolute extraKnownMarketplaces entry, because registering it is something a SESSION does at trust time. Both made me conclude a feature was absent when it was present. Measure with a real session (codex exec, opencode run, claude interactive) and diff against a control directory. Second trap, same day: asking a model whether it can see a sentinel is NOT a measurement unless you check the trace for tool calls — mine "found" the sentinel by running sed on the file whose name I had just put in the question. Three false greens in one session, all this shape.
