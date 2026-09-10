---
version: unreleased
headline: `charter claude "…;"` gives the harness its trailing `;`, and a launch too long for tmux says how long
---

tmux reads any argument that ends in `;` as its own command separator. So
`charter claude "run the tests;"` started the harness on `run the tests`; a `;`-ending
argument in the middle turned the arguments after it into a tmux command run on charter's
server; a second chat, or any chat inside a tmux you already had, opened in a directory
ending in `;` failed with `unknown command: -P`; and a `$CHARTER_ROOT` ending in `;` failed
every launch with `unknown command: -e`. Charter now escapes a trailing `;` as `\;` — tmux's own
spelling of a literal one — in every argument, directory and identity value it hands tmux,
on charter's own server and inside a tmux you already had.

The middle-argument case was a latent surface, not a vulnerability: only arguments you type
yourself reach a harness's argv today. If you had been typing `\;` to get a `;` through, you
now get the backslash too — drop it.

A launch whose arguments take tmux past its 16,364-byte command limit used to print the
whole command back beside tmux's own refusal. It now names the limit, how many bytes the
launch's arguments carried, and the fix: put long text in a file and pass its path.
