"""The suite answers what CI answers about the `claude` CLI: it is not on this machine.

`charter init` installs charter's own Claude Code plugin (#881) — `claude plugin
marketplace add`, then `claude plugin install charter@charter --scope project`. Nine test
modules call `commands.cmd_init` directly, so on a developer's laptop, where `claude` IS on
`PATH`, a suite run would install and reinstall a plugin into their real Claude Code, once
per fixture plane, from throwaway temp directories that no longer exist by the time anyone
looks at `claude plugin list`.

CI has no `claude`, so CI would never have seen it. That is the shape `tests/_ttyguard.py`
already records for the suite's file descriptors — *"all three streams now answer what CI
answers, and a test that wants a different answer has to say so"* — and this is the same
move for one more fact about the machine.

**A test that wants a `claude` says so, and the idiom already exists.**
`tests/test_plugin_freshness.py` opts in seventeen times with

    mock.patch.object(plugincache, "available", return_value=True)

and stubs the seam underneath it in the same `with`. Opting in without stubbing
`plugincache.util.run` spawns the real binary, which is the one thing this file cannot
answer for: `available` is a fact about the machine, not a fake `claude`.
"""

from __future__ import annotations


def install() -> None:
    """Make `plugincache.available()` answer ``False`` for the whole run.

    Patched on the module rather than on `shutil.which`, because `which` answers for every
    binary the suite looks up — `git`, `gh`, `tmux` — and this is a statement about one of
    them. `available` is also the exact seam every existing opt-in already patches, so a
    test that wants the other answer needs to know about one name and not two.
    """
    from charter import plugincache

    plugincache.available = lambda: False
