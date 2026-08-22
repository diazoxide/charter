# Smoke-testing a release: a refusal proves nothing without a benign twin.

_2026-08-22 12:28 · persistent_

Smoke-testing a release: a refusal proves nothing without a benign twin. Cutting 0.48.0, three of six smoke 'passes' were vacuous — 'secret list' errored with 'no vault named' (registry shape is {"vaults":{...}}, not flat), 'persona show' exited 2 on argparse not containment, and a traversing 'extends:' was refused only because charter's frontmatter parser does NOT strip quotes (extends: "parent" silently never resolves; unquoted works). Always assert the precondition was met AND run the legitimate twin.
