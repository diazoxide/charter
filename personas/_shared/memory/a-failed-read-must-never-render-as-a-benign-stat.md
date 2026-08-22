# A failed read must never render as a benign state: onepassword._fields r

_2026-08-22 10:05 · persistent_

A failed read must never render as a benign state: onepassword._fields returned {} for EVERY non-zero 'op item get', so a rate-limited vault printed 'has no secrets' (#322) and set()'s read-modify-write piped back a template holding only the new key, dropping every sibling. Fixed by PROVING absence — list the vault under its own identity; a listing that fails IS the answer — not by matching op's English. Same species as doctor's _NOT_CHECKED_HINT and #331's dead perms branch.
