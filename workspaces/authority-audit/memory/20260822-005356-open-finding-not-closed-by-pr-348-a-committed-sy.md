# Open finding, NOT closed by PR #348: a committed symlink redirects chart

_2026-08-22 00:53 · persistent_

Open finding, NOT closed by PR #348: a committed symlink redirects charter's WRITES as well as its reads. personas/<x>/memory/MEMORY.md is a FIXED name, so a link there makes 'charter persona remember' append its index line through the link — demonstrated on 0.47.2 and on the #336 branch (the write paths ensure_index/index_append/_drop_index_line/write do not go through memstore.files). Pointed at .charter/vaults/<n>.json it corrupts a credential store from a commit. Fix shape: reuse contain.file_refusal at the write sites, with ENOENT reading as 'nothing to object to'.
