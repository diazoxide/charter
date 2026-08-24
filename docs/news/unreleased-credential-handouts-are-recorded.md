---
version: unreleased
headline: charter trace can now answer "which command received the prod token"
---

`charter trace` recorded guard denials, tool approvals, memory writes, and `secret-warn` —
the scanner that spots a credential in a file about to be committed. It recorded nothing
about the credentials charter itself handed out. So after the fact there was no answer to
the first question anybody asks: *which command received the prod token*. `charter secret
audit` looks like the place to find it and is not; it is a rotation-age report.

Every route by which a value leaves charter's own process now writes one line:

- `secret-exec` — a child's environment or a temp file, in all three launch modes
  (captured, `--stream`, `--exec`).
- `secret-cp` — a file on disk, with the destination, which is the thing you have to go and
  delete.
- `secret-reveal` — `secret get --reveal`, a terminal.

Each line carries the vault, the key **names**, and for `exec` the environment variable
names and `argv[0]`. `charter trace --summary` gives them their own section beside guard
denials rather than a number in a tally.

**No line carries a value, and that is a rule about shapes rather than a list of fields.**
Whatever is recorded is stripped of every value the call resolved, at any depth, before
anything is written — so the field somebody adds next year is covered by the rule that is
already there. The rest of `argv` is deliberately not recorded: charter never substitutes a
secret into a command line, but you might have typed one there, and a file whose purpose is
to hold no values must not copy a line that might.

Two things it does not claim. It cannot say what a command *did* with a credential once it
had it — a secret delivered to a process is that process's from then on. And `secret get`
without `--reveal` records nothing, because nothing left: it prints a length and a digest.

Recording two of the three routes would have been worse than recording none. Grepping for
`secret-exec` and `secret-cp`, finding nothing, and concluding the token never left the
vault is a false answer from a record that looks complete.
