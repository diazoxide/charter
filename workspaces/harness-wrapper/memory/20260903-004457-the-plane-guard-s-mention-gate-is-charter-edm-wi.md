# The plane guard's mention gate is `charter|edm` with NO word boundaries 

_2026-09-03 00:44 · persistent_

The plane guard's mention gate is `charter|edm` with NO word boundaries (tests/_planeguard.py). `edm` is three characters: it collides with 1 tempfile name in 8,442 (6 positions x 37^-3 over the 8 random chars) and, deterministically, with tests/_isolation.py's own `edm-test-` plane prefix. #830 was that collision plus shlex being unable to lex bash's $'...' ANSI-C quoting: gate passes on the substring, string will not lex, 'undecidable' answers charter, and a `bash -c printf` is refused against the real plane. Rule: a gate documented as 'can only cause misses' must never be what decides a fail-closed path — give the undecidable exits the boundaried word test.
