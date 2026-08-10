# Nothing publishes without a human "yes" — and there is no `--yes` flag

Detection and drafting of a report are automatic. Publishing never is. `charter report
bug|gap` drafts, redacts and prints the payload; `charter report send <id>` publishes.
The second command *is* the consent — there is deliberately no flag that collapses the two
into one, and adding one would defeat the entire design.

charter has no interactive prompt anywhere: `util.py` carries only `info/ok/warn/err`,
because charter runs inside hooks and agent sessions where blocking on stdin would hang.
So consent could not be a y/n prompt, and the two-command split turns that constraint into
the mechanism: the reporting agent drafts, shows the Reporter the payload in conversation,
and can only publish after the Reporter answers. An upstream issue is public, attributed to
a real person, and awkward to retract; an agent filing them unattended under the Reporter's
name is the kind of thing that gets a tool uninstalled once.

## Considered Options

A `--yes` flag on a single command was rejected: a flag the agent can pass is a flag the
agent will pass unprompted, which is exactly the failure being prevented. An interactive
path gated on `isatty()` was rejected too — it would put the safeguard only in front of
humans at a terminal, which is almost never the case, leaving the agent path ungated.

## Consequences

The automation on offer relieves the Reporter of *writing* a report, not of *approving*
one. That is the intended trade. Filing consent is additionally asked once per human and
stored in user-level config — never in `charter.toml`, which is committed and would enrol
a whole team on one person's say-so.
