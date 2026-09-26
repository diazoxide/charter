# The Python oracle is frozen into recorded fixtures

Until 2026-09-23 CI ran the Python charter beside the Rust one. `uv` installed `charter-cp` from
`diazoxide/charter-plane` at commit `50d31dc`, and three jobs compared the two: every plane-writing
scenario (`tests/differential/run.py` and `doctor_scenarios.py`), 200,000 generated command lines
through the shell reader (`shellseg.py`), and 50,000 cases through the plane-root guards
(`planeroot.py`). A fourth job regenerated the fixture planes from the same commit
(`tests/fixtures/planes/generate.py --check`).

The operator ruled that *"charter-app should be standalone application without any dependency
from old charter"*, and later: *"starting from this moment - switch to new charter - and
depricate old charter and keep it as plane repo. so it means one moment we need to go freely
without old charter dependency"*. Asked why CI still ran the Python charter at all, he answered
with that ruling. So the oracle is frozen: its answers are recorded once, as files in this
repository, and CI compares the Rust `charter` against those files. Nothing in charter-app's code,
tests or CI installs Python charter or contacts `diazoxide/charter-plane` or PyPI's `charter-cp` any
more.

## What was recorded, and how

**Once, on one head where both implementations agreed.** It ran on Linux (an Ubuntu 24.04
container, git 2.43), where the unchanged differential passed all 431 scenarios on that head.
The replay was then checked there, on macOS (git 2.50), and on CI's own runner (git 2.55). The
recorder ran each
differential scenario through the differential's own `check`, both implementations and every
comparison, and wrote a row only for a scenario that passed. A scenario that failed stopped the
recording. So wherever the differential compared the two, the recorded text is Python's answer.
Where they differed by decision, the text is the app's answer and the row's `notes` say which
decision. That covers a `Divergence` citing its ADR, a rewrite, and a stream the differential
never compared.

- **`tests/fixtures/recorded/behaviour.jsonl`**: one scenario per line, sorted by name. A row
  holds:
  - the fixture plane the scenario starts from;
  - what its setup changed on top of that plane (`start`: files, modes, links, fifos, a
    repository's `.git`, a stand-in `gh`, a bare "forge");
  - the command, its whole environment, its working directory and its stdin;
  - `expect`, what the run must leave.

  The run's own directories are written as `<side>` and `<scratch>`, and the app's version as
  `<app-version>`, so a row is the same on every machine and across releases.
- **`tests/fixtures/recorded/behaviour-blobs.jsonl.gz`** holds file contents that are binary or
  longer than 2 KB, keyed by sha256 and shared between rows. Most of it is git objects.
- **The shell reader and the plane-root guards** extend the corpora that
  `fixtures/corpora/README.md` already describes, with a subset of the generated cases. That
  README says how the subset was chosen and what it reaches.

**`cargo test -p charter-cli --test recorded_behaviour` replays a row.** It copies the fixture
plane, lays `start` down, runs this build's `charter` in the recorded environment, and checks
what the differential checked of the Rust side:

- the exit status;
- stdout and stderr, byte for byte after the scenario's own masks, or up to a declared cut, or,
  for a refusal whose words were never compared, the phrase it refuses with;
- a hook's verdict, a status line's alert rows, and credentials neither stream may print;
- every entry the plane holds afterwards, with its bytes and file modes. A path the scenario
  ignores, a `.git`'s index and reflogs for example, is read through the same git questions the
  differential asked instead;
- that nothing is written beside the plane except where the command's work lands (a push to the
  stand-in forge), and that it does land there.

The whole set runs in about ten seconds, so it is not sharded.

## What changed in the freezing, on purpose

- **The fixture plane's times.** The differential copied the planes with the checkout's own
  mtimes, so which workspace a session briefing called most recent depended on the order git
  had written the files on that machine. Both sides shared the copy, so this never showed. The
  recording and the replay instead give every entry of the copy one time, the second it was laid
  out. A time a setup set on purpose, such as a crashed lock's age, is recorded and replayed.
- **A harness defect.** `_glstate_facts` masked two spellings of the plane root in set order.
  On macOS one spelling is a prefix of the other, and the Python side came out
  `/private<plane>`. The recorder masks the longest spelling first.
- **Streams the differential did not compare** (`stdout_differs`, `stderr_differs`: "not
  ported yet") are pinned to what the app prints. The app is the reference now.
- **The news corpus comparison is gone.** That comparison was the per-version
  `news-for-*-renders-the-same-notes` scenarios and the digest check, and `news/SOURCE` pinned
  the corpus to the oracle only for it. The corpus is frozen history (ADR 0045). The `news`
  command's own behaviour was still recorded then: a refusal, a range, and a plane with no
  baseline. Since #352 the corpus is gone and `news` prints the app's CHANGELOG.md, and its rows
  record that instead; ADR 0045's amendment of 2026-09-26 lists the rows that moved.
- **Checks that only ever read the Python side are gone with it.** These were a `Divergence`'s
  `python_stderr_has`, a note that Python "no longer" prints something, and the forge trap. The
  forge trap checked a setup's `origin` before the command, and that setup is now a recorded
  file that cannot move.
- **Two things that name the machine are made machine-neutral.** `doctor`'s `git` row prints
  the machine's git version, which both sides once shared; it is masked now. And `git init` on
  macOS writes `core.ignorecase` and `core.precomposeunicode` into a repository's config, so the
  replay reads a repository's config without those two keys.
- **The fuzzers' breadth.** The shell reader's 200,000 generated cases and the plane-root
  guards' 50,000 are now 2,399 and 1,710 recorded cases. They were chosen to reach every branch
  and reader feature the full runs reached. What that subset can and cannot prove is in
  `fixtures/corpora/README.md`.
- **Two masks used look-around**, which Rust's `regex` does not support. Each became a capture
  group and a replacement. The recorder checked that each rewrite gave the same text as the
  original on every stream it was applied to.

## How to change a recorded answer on purpose

A change that is meant to move a recorded answer is re-recorded from what the binary does now:

```bash
CHARTER_RECORDED_BLESS=1 cargo test -p charter-cli --test recorded_behaviour -- <scenario>…
```

This rewrites those rows, and only the fields a run can re-derive: the exit status, the
recorded stdout, stderr and alert rows, the plane's resulting tree, the git facts, and where the
command's work lands. The diff of `behaviour.jsonl` is then the change. Read it, and say in the
PR which contract moved and why. A refusal phrase or a hook verdict (`contains`, `denies`,
`says`) is the scenario's claim, not a recording, and is edited by hand.

To add a scenario, copy the row nearest to it, change what it starts from and runs, and bless
it. A row is only as good as the reading of its first recorded answer: the app is the reference
now, so nobody else will check it.

Changing a fixture plane under `tests/fixtures/planes/` moves the starting point of every row
that uses it, so it is followed by a bless of those rows, and the diff shows what that moved.

**Never re-record to make a red run green without that sentence in the PR.** Before this
record, a red scenario meant the two implementations disagreed. Now it means the app changed
behaviour that something relied on. The row cannot say whether that change was right, and the
PR has to.

## What this rules out

- Installing, fetching or running the Python charter, or `charter-cp`, in any CI job, test or
  tool of this repository.
- A required check whose name promises a comparison with Python. The gate is **recorded
  behaviour**.
- Editing a recorded row by hand to agree with a new behaviour, except for the claim fields
  above. Bless it, so what is recorded is what the binary did.
