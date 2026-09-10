---
version: unreleased
headline: `charter claude "…;"` gives the harness its trailing `;`, and a launch too long for tmux says how long
---

tmux reads an argument that ends in `;` as its own command separator and drops the `;`
silently, so `charter claude "run the tests;"` started the harness on `run the tests`.
Charter now escapes it as `\;`, tmux's spelling of a literal one, for every argument it
starts a harness with — on charter's own server and inside a tmux you already had.

A launch whose arguments take tmux past its 16,364-byte command limit used to print the
whole command back beside tmux's three words. It now names the limit, how many bytes the
launch's arguments carried, and the fix: put long text in a file and pass its path.
