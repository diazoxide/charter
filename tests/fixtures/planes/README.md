# Fixture planes

Two control planes, written by the Python charter itself, for the tests to read. The format they are in is specified in
[`docs/plane-format.md`](../../../docs/plane-format.md); this directory is the "fixture planes" that spec calls for (ADR 0025,
spec decision 13).

| Plane | What it is |
| --- | --- |
| `minimal/` | What `charter init` leaves behind, and nothing else |
| `daily/` | A plane in use: a LIVE workspace with a clone, memory, todos and a snapshot; a second workspace left local; a second persona with its own and shared memory; a vault registry; and the session state a harness run leaves in `.charter/` |

**Nothing here is hand-written, and nothing regenerates it any more.** Every byte is what the
Python charter (pinned at `50d31dc`) wrote when `generate.py` ran it; on 2026-09-23 the Python
oracle was retired (ADR 0046) and the generator with it, so these planes are DATA now — the
starting point every recorded scenario in `tests/fixtures/recorded/` is replayed from, and what
`crates/charter-core/tests/fixture_planes.rs` reads. Change one only on purpose, in a PR that
says why, and re-record the scenarios that start from it
(`CHARTER_RECORDED_BLESS=1 cargo test -p charter-cli --test recorded_behaviour`): every
recorded answer assumes these exact bytes.

```bash
git add -f tests/fixtures/planes    # a plane's .gitignore hides its own files
```

## What was pinned, and why

charter records who did a thing and when, and both would otherwise change on every run and
on every machine. The generator pinned:

| Thing | Pinned to | How |
| --- | --- | --- |
| Clock | 2026-03-02 09:00 UTC, +1 minute per command | `time-machine`, started in a `sitecustomize.py` on `PYTHONPATH` — charter reads the wall clock directly, so no environment variable reaches it |
| Timezone | `TZ=UTC` | Memory and todo *filenames* use local time while the timestamps inside them are UTC; without this they disagree by the offset |
| Hostname | `fixture-host` | `socket.gethostname` is patched in the same `sitecustomize.py`; it lands in `_dispatch`/`_skills`/`pieces` filenames and has no environment override |
| User | `fixture` (`$USER`), `Fixture User` (git) | `workspace.json.updated_by` comes from one or the other depending on the writer |
| Session id | `fixture-session-1` | `$CHARTER_SESSION_ID`; otherwise a run inherits the operator's real session |
| `PATH` | `/usr/bin:/bin` | With `claude` on `PATH`, charter installs its plugin and writes `enabledPlugins`; without it, it writes its own `hooks.PreToolUse` block. Both shapes are legal — pinning keeps the fixture from depending on whose machine ran it |

## What the fixtures deliberately leave out

- **File modes.** charter writes `.charter/` as `0700` and the files in it as `0600`, and
  git carries only the executable bit — so on any fresh clone or CI checkout they come back
  `0755`/`0644`, and the generator's `--check` did not compare modes. A test about modes has
  to create the files and let charter write them; reading a mode off a checked-out fixture
  measures git, not charter.
- **Empty directories.** A fresh plane has `inventory/` and `workspaces/` with nothing in
  them, and git cannot carry an empty directory. They are part of the format, so each plane
  records its own in `<plane>.empty-dirs` beside it. A test that needs the directories
  themselves creates them from that file, as the recorded-behaviour replay does.
- **Every `.git` directory**, the plane's own and each clone's. Git will not track a path
  inside a `.git` directory, so a fixture that kept one could not be committed. This also
  drops `<clone>/.git/info/exclude`, which charter writes and the format specifies: a test
  that needs that block should make a clone and let charter write it.
- **`.charter/cache/**`** — keyed by absolute path, so it carries the generator's scratch
  directory, which differs on every run. The format marks these `internal`.
- **`.charter/fingerprint.key`** — random key material, regenerated per plane. It could
  never match a regeneration, and a fixture has no business carrying key bytes.
- **Anything needing a forge, a network, a tty or tmux**: `charter discover`
  (`inventory/repos.json`), `charter version bump`, `charter change land`
  (`changes/log/<host>.jsonl`), a real profile launch, and the 1Password vault provider.
  `docs/plane-format.md` documents each of those files; a test that needs one writes it
  through the same writer charter uses.

## A rule for editing a fixture

Keep fixture titles ASCII. Memory and todo filenames are slugs of the title, and a title
with an accented character is stored NFD on macOS and NFC on Linux — the fixture would then
drift between the machine that generated it and CI, with nothing in the diff to say why.

## Secrets

The vault registry in `daily/` is the registry's *shape*, and the one value in it is the
literal `fixture-not-a-secret`. No fixture ever carries a credential, and the format spec
records the registry's shape only, never contents.
