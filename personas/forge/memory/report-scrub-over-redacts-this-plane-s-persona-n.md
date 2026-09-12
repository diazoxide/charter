# report scrub over-redacts: this plane's persona names (forge, reddit, re

_2026-09-11 03:24 · persistent_

report scrub over-redacts: this plane's persona names (forge, reddit, release, statusline, steward) are matched as whole words anywhere in report/gap text, so a literal source filename that happens to equal a persona name (e.g. charter/statusline.py) gets replaced with [persona] mid-sentence. charter report bug prints '_Scrubbed before drafting: ...' when this fires -- always check that line and reword (e.g. describe the file instead of naming it) rather than send with a stray placeholder.
