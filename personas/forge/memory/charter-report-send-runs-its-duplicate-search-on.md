# charter report send runs its duplicate search only on the REAL send, nev

_2026-09-10 14:51 · persistent_

charter report send runs its duplicate search only on the REAL send, never in --dry-run: a first-line match refuses with rc 2 and prints 'this may already be reported' (2026-09-10, #942 was matched against #855, a CLOSED and already-fixed issue). The dry run is therefore not a preview of whether send will succeed. --new is the only way past it, and it is the right answer when the flagged issue is a different defect; report comment <id> --on N is the answer when it is not.
