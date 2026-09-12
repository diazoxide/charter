# charter report bug scrubs every workspace, persona and vault name on thi

_2026-09-10 11:27 · persistent_

charter report bug scrubs every workspace, persona and vault name on this machine at word boundaries, so ordinary words that happen to be names here (a workspace called todos, a vault called alpha) are redacted out of prose and fixture names. There is no edit path for a draft: dry-run report.scrub on the body first, reword or pre-placeholder until it fires nothing, then draft; if a draft was over-redacted, report delete it and redraft rather than editing .charter/reports JSON.
