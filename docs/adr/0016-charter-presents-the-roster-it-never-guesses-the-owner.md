# charter presents the roster; it never guesses the owner

Personas advertise what they accept. `delegate-when` is prose, written by whoever created
the persona, rendered into the generated sub-agent description that a router reads. What
no field has ever expressed is the other direction — when the persona currently *acting*
should hand work away — and the tally says the gap costs something real: on this plane,
three dispatches ever, all to one persona, while three others advertised correct triggers,
linted green, and were never dispatched once.

The obvious way to close it is for charter to match the incoming prompt against every
`delegate-when` and name a winner: *"this looks like `statusline`'s work."* That is the
design this ADR refuses, before the code that would host it exists.

## The decision

When charter helps route work, it injects **facts it owns** and nothing else: which
personas exist, what each one's `delegate-when` claims, when each was last dispatched, and
whether anything has been dispatched this turn. The model reads that and routes. charter
never names the owner.

## Why

**charter cannot justify the claim.** A keyword overlap between a prompt and a prose
advert is not evidence of ownership. ADR 0009 already draws this line for errors — *they
classify, they do not guess* — and `curate.py` holds it for memory curation, which
nominates duplicates with stdlib string distance and leaves every judgement to the agent.
Routing is the same shape: cheap heuristics, expensive mistakes.

**A confident wrong answer is worse than silence.** The roster block rides the
commitment-point gate, which exists because a nudge that becomes wallpaper stops being
read. The first time charter says "this belongs to `release`" about a status-line bug, the
block joins the set of things people skim past — and it takes the honest half of the
message, the part that says nothing was dispatched, with it.

**A second source of truth would drift.** The alternative to matching prose is a
`triggers:` field of regexes beside the advert. Then a persona claims its remit twice, in
two languages, and the copy nobody edits is the one routing reads. `delegate-when` is
already maintained because it is the description a human sees when choosing.

**Path ownership contradicts the rule the roster teaches.** `owns: charter/statusline.py`
is the other tempting matcher, and steward's own charter rejects it in prose: *route on
the work, not the file — a version string inside `tests/test_plugin.py` is `release`'s.*

## Consequences

**The `require` level cannot name a culprit.** With no owner asserted, the strongest thing
a tool-time check may say is *"you are editing and nothing was dispatched this turn,"*
listing the roster. That is a weaker sentence than "you should have delegated to
`statusline`" and it is the one charter can actually stand behind.

**Routing quality depends on the adverts.** A persona with a vague `delegate-when` routes
badly, and charter cannot compensate. `persona lint` already warns on a missing one; that
warning is now load-bearing rather than tidy.

**The roster costs context on work-shaped prompts.** Name plus advert plus a date, for each
persona, on prompts the commitment gate already classifies as work. That is the price of
not guessing, and it scales with roster size — a plane with forty personas will need a
narrowing rule (`routes-to:` prioritises; it deliberately does not restrict, because a
restriction silently hides personas created after it was written).

**This ADR ships before the mechanism it governs.** Deliberately. The matcher is the
obvious next improvement to a roster block — the kind of change that arrives as a small,
plausible patch long after everyone has forgotten why the block only ever states facts.
