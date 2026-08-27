---
version: unreleased
headline: charter's own test suite no longer reaches your 1Password vault, and no longer stops to wait for you to type
---

Two tests in charter's suite ran the operator's **real `op` binary against their real
1Password vault**. `charter vault add --provider 1password` finishes by asking the provider
whether the vault is healthy, and for a 1Password vault that shells out — so the vault names
those fixtures spell, `Eng` and `Engineering`, were being looked up on whatever machine ran
the suite, as whoever was signed in. Measured by putting a logging stand-in for `op` first
on `$PATH` and running the whole suite: four invocations, from two tests, and none anywhere
else in 6636.

They had passed for as long as they had because an unauthenticated `op` **fails fast** — it
exits non-zero in well under a second, charter reports a vault it could not read, and the
assertions (about a refusal to migrate, and about an account pin not travelling) held for
the right reason by luck. `op` does not fail fast when it thinks a human could be asked. It
**prompts**, and the suite then sits in `subprocess.communicate` behind a biometric dialog:
measured at 15 minutes inside a fresh tmux pane, with `python3 -m unittest discover -s tests`
never finishing. Running charter's own tests should not touch anybody's actual credentials,
and it certainly should not park behind a fingerprint reader.

**The second half is the same failure with no vault in it.** Run the suite from an ordinary
terminal — which is where anyone working on charter actually runs it — and it hung, forever,
in `test_frame_launcher`. charter's workspace picker opens on **two** independent questions,
`sys.stdin.isatty() and sys.stdout.isatty()`, deliberately, because `charter claude <
/dev/null` at a terminal has one and not the other. The launcher's test helpers pinned the
second of that pair and left the first to whatever shell the suite was launched from. Under
a pipe — CI, an agent's shell, `... | grep` — the answer was "nobody is there" and the suite
went green. Under a terminal, it stopped at charter's own prompt and waited for a human who
was never going to type. Measured with `input()` replaced by a recording raise and the suite
run under a pty: **122 tests reached that prompt**, in one module, on a tree whose CI had
been green throughout.

A hang is the worst outcome available. It is not a pass, not a fail, and not a report — and
the environment that reports the suite's health is precisely the one that cannot see it,
because CI's stdin is never a terminal.

**Both are fixed as a class, on the shape the plane guards already settled.** 0.52.0 refused
writes into your real `.charter/`; 0.53.0 refused reads of the `[update] channel` your own
`charter.toml` declares; the entry beside this one removes charter's variables from the
suite's environment and refuses an undeclared read of one. This is the same move twice more:

* **The credential CLI is refused at the spawn.** A test that runs `op`, `vault` or the
  browser lane's `npx` now fails, by name, on the line that reached for it, having read
  nothing. Which CLIs count is asked of charter's own resolver table rather than listed, so
  a provider added there is covered on the commit that adds it. The two tests that were
  reaching drive a fake instead — the pattern five sibling modules in the suite already use.
* **The terminal is answered, and one question about it is refused.** All three streams
  report what CI reports, before charter is imported at all, so `input()` ends rather than
  blocks and charter's own colour decision no longer depends on where the run happened. On
  top of that, a test that asks whether stdin is a terminal without saying which answer it
  wants is refused: the launcher's helpers now state both halves of the pair, and the
  refusal is what makes those two lines load-bearing rather than decoration.

Removal alone would only have silenced the red. Every one of those 122 tests would still
have been asserting against a terminal-ness it never chose, and the next test that *means*
"there is a human here" would have got `False` and passed vacuously — which is not a
hypothetical: `test_mcp_approval` already records an instance going the other way, where
ANSI codes from a real terminal joined a transcript it derives its expectations from and a
mutation that dies under a pipe "reported OK under a pty".

**What this run still cannot promise.** The suite now gives the same verdict from a pipe and
from a terminal for the two classes above, and a full run under a pty no longer hangs. It is
not yet identical: 28 tests across eight modules still fail only under a terminal, and every
one of them traces to `os.get_terminal_size()` — the size of the window the run happens to
be in, not whether it is one. That is issue #544 and it is not fixed here. What the
measurement adds to it: the pty those 28 were run under had never been given a window size,
so `get_terminal_size()` answered **0 columns**, and `tui.term_width` guards `$COLUMNS <= 0`
while doing nothing about a tty that reports zero — the same defect one rung down its own
ladder, and reachable outside a test by any terminal whose size was never set. Give the same
pty a size and all 28 pass.

None of this reaches you unless you run charter's own test suite. Nothing to adopt.
