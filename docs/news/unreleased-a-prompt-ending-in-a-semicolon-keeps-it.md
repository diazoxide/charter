---
version: unreleased
headline: `charter claude "…;"` gives the harness its trailing `;`, and a launch too long for tmux says how long
---

tmux reads any argument that ends in `;` as its own command separator. So
`charter claude "run the tests;"` started the harness on `run the tests`; a `;`-ending
argument in the middle turned the arguments after it into a tmux command run on charter's
server; and a directory or `$CHARTER_ROOT` ending in `;` made tmux refuse the launch with
an error that never named it — `unknown command: -P` for the directory, `unknown command: -e`
for the root while charter's tmux server was running, and `error connecting to …` before it
was. Charter now escapes a trailing `;` as `\;` — tmux's own
spelling of a literal one — in every argument, directory and identity value it hands tmux,
on charter's own server and inside a tmux you already had.

The middle-argument case was a latent surface, not a vulnerability: only arguments you type
yourself reach a harness's argv today. If you had been typing `\;` to get a `;` through, you
now get the backslash too — drop it.

A launch whose arguments take tmux past its 16,364-byte command limit used to print the
whole command back beside tmux's own refusal. It now names the limit, how many bytes the
launch's arguments carried, and the fix: put long text in a file and pass its path.
