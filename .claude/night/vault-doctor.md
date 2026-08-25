# vault-doctor
PR: https://github.com/diazoxide/charter/pull/493
Branch: vault-doctor-470-471-472
Weaker: None found. Checked deliberately, input by input, since `private_mkdir` replaces `mkdir(parents=True, exist_ok=True)` at 30 sites: an existing directory is a no-op on both; a **file** in the way raises `FileExistsError` on both (mine re-raises explicitly after `is_dir()`, which is what `exist_ok=True` does and what every caller writing into the path afterwards depends on); a symlink to a directory is accepted by both; a parent that is a file raises `NotADirectoryError` on both; every routed site keeps its original `except OSError`/`except Exception`, and every exception `private_mkdir` raises is an `OSError` subclass. Modes are strictly tighter, never looser. No `contain.writable`/`dir_refusal` gate moved relative to its mkdir. `doctor` gained a note; no status was weakened. `persona list`/`stats` gained bounds; nothing lost one.

## Bypass

`charter persona remember <persona> "<text>" --ephemeral` bypasses the 0700 walk entirely and, on a fresh clone, is what creates `.charter/` — leaving the plane's state directory at 0755 under `umask 022` and 0777 under `umask 000`. The path reaches the writer as a parameter (`persona.remember` -> `ephemeral_dir()` -> `memstore.write` -> bare `mkdir(parents=True, exist_ok=True)` at charter/memstore.py:54), which is the one spelling `tests/_statedirscan.py` says it cannot see — and the reason it gives for that being safe ("`.charter/` itself is 0700") is false on this exact flow.

## Blocking

### 1

(A) `charter persona remember <p> "note" --ephemeral` still leaves `.charter/` at the umask default, on the exact defect #470 names. `persona.remember` (charter/persona.py:1400) resolves `ephemeral_dir()` = `.charter/persona-state/ephemeral/<session>/<ns>` and passes it to `memstore.write`, which creates it with a bare `mem_dir.mkdir(parents=True, exist_ok=True)` (charter/memstore.py:54, :76; same shape at :99 and :416). On a fresh clone `.charter/` is gitignored and absent, so that mkdir is what creates the state directory itself.

REPRODUCTION (branch extraction, `charter init`-built plane, the same fixture `TheCliDecidesIt` uses):
    umask 022; mkdir /tmp/plane && cd /tmp/plane
    PYTHONPATH=<branch> python3 -m charter init --forge github --owner acme
    rm -rf .charter                       # a fresh clone has none
    PYTHONPATH=<branch> python3 -m charter persona remember steward "note" --ephemeral
    stat -f "%Sp %N" .charter             # -> drwxr-xr-x  .charter

Adding one case to the branch's own sweep helper makes it explicit:
    def test_ephemeral_memory_gets_there_first(self):
        self._sweep("ephem", ("persona", "remember", "steward", "note", "--ephemeral"))
  -> AssertionError: 45 != 0 : under umask 0o22, `charter persona remember steward note --ephemeral` left `.charter` at 755
  -> AssertionError: 3 != 1 : the umask still decides it: [('0o0', '777'), ('0o22', '755'), ('0o77', '700')]

CONSEQUENCE. Under `umask 022` the whole chain is other-traversable and the memory file itself lands 0644 — I read `.charter/persona-state/ephemeral/<sid>/steward/an-ordinary-scratch-note.md` at `-rw-r--r--` under `drwxr-xr-x .charter`. Every later file written directly into `.charter/` then sits in a directory any account on the machine can list (`vaults.json`, `guard-seen.json`, the fingerprint key); I confirmed a subsequent `charter vault add` + `secret set` leaves `.charter` at 755 with `vaults.json` inside it, and `doctor` correctly reports the loose directory — which is charter telling the operator about a directory charter itself just made. Under `umask 000` the state directory is 0777, i.e. world-writable, so another account can plant files in it.

This also refutes the reasoning `tests/_statedirscan.py` gives for leaving parameter-passed paths uncovered: "What keeps those from being an exposure is that the level they hang off — `.charter/` itself — is 0700." On this flow it is not.

FIX: route the ephemeral branch through the walk — `config.private_mkdir(d)` in `persona.remember` before `memstore.write` when `ephemeral` is true (or inside `memstore` gated on the target being under `STATE_DIR`; it must stay conditional, because `memstore.write` is also handed the committed `personas/<n>/memory`). Then add the sweep case above so the fourth writer is pinned like the other three.

