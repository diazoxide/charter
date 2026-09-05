---
version: unreleased
headline: a sweep sandbox that cannot be built now quotes what the machine said, instead of reporting a full disk as a failed deletion sweep
---

`tests/test_sweep.py::…::test_no_shard_at_all_still_means_the_whole_plan` failed on CI, on
a branch whose diff touches no sweep file, and the whole of the report was this:

```
subprocess.CalledProcessError: Command '['git', 'clone', '--quiet', '--no-hardlinks',
'--no-checkout', '/tmp/sweep-gate-yfa6_l8t',
'/tmp/sweep-gate-yfa6_l8t/wd/run-2274/w0']' returned non-zero exit status 128.
```

Commit `d220a7c` passed `test (3.14)` on its `pull_request` run and failed it on its `push`
run — same tree, same Python — and a re-run of the failed job on the unchanged tree passed.

## What it was

`Sandbox.__init__` ran `git clone` and then `git checkout` with `check=True` and
`stderr=subprocess.PIPE`. That pairing looks like care and is the opposite of it:
`CalledProcessError.__str__` prints the argv and the exit status and **nothing else**. The
stderr it is constructed with rides along on the exception object and is never shown. So a
command that failed *for a stated reason* arrived as a command that failed.

Reproduced against a 20 MiB filesystem with 250 KiB left on it. The traceback is
character-for-character the one CI printed, and what `git` had actually written — collected
by this code and dropped — was:

```
fatal: failed to create directory '…/w0/.git/objects/82': No space left on device
```

None of that is charter's doing, and that is the other half of why the sentence has to
survive. Without it the only available reading is *the deletion sweep is broken*. With it,
the reader is told the box ran out of disk and the sweep never got as far as having an
opinion about anybody's code.

## The first hypothesis, and why it is refuted rather than unconfirmed

The obvious suspect was #893/#894, which landed the same day: many writers publishing
through a **shared temp name**, where `os.replace` was atomic but the name was not private.
A fixed `/tmp/sweep-gate-…` under concurrency is that shape one directory up.

It is not that, and this is a proof rather than a shrug. `tempfile.mkdtemp` creates with
`O_EXCL` and holds the directory for the life of the test, the sweep's workdir lives inside
it, and `run_dir` puts *this process's pid* under that — so no two concurrent invocations
can be handed the same sandbox path. Measured as well as argued: **1600 runs of the failing
test across 16 concurrent processes, 1600 passes.** The same 1600 pass after this change.

## What changed

Both calls go through `Sandbox._must`, which raises `NoSandbox` — a category of its own,
because every other way this tool ends badly is a statement about the branch and this one
is a statement about the machine, made before a single mutation has been measured:

```
tools.sweep.NoSandbox: the sandbox at /tmp/…/w0 could not be built — cloning the tree:
`git clone --quiet --no-hardlinks --no-checkout /tmp/… /tmp/…/w0` exited 128 — fatal:
failed to create directory '/tmp/…/objects/ad': No space left on device. That is the
machine this sweep is running on, not a verdict about any mutation: nothing had been
measured yet when it happened.
```

**No retry, no skip, no broadened `except`.** A full disk is the environment's business and
charter does not get a vote on it; being able to tell a full disk from a failed sweep is
charter's business entirely, and that is the only thing this fixes. If the next occurrence
turns out to be inodes, or a fork that would not, or something nobody has thought of, it now
names itself on the first run instead of costing a day.
