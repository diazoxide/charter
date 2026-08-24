---
name: update
description: Update charter to a newer version and adopt what it brings — moves the CLI, this harness's artifact and the pin, then walks what this plane has not taken up. Use when asked to update or upgrade charter, whether a newer charter is published, what a version added, or how to adopt a new charter feature.
---

# Updating charter

One command converges the upgrade. Your job is the conversation around it.

```bash
charter update
```

It moves the CLI, asks this harness how its own artifact moves, proves the install took by
handing off to the new binary, and reports what the new version brought. **Read its output
and relay it** — that text is the report, not a summary of one. It is idempotent: run it
without first working out whether there is anything to do.

## Where it stops, and what you do there

**The pin.** When this plane already sits on its pin and something newer is published,
`update` proposes `charter update --bump` and stops. Moving the pin moves every teammate on
their next session, so get an explicit yes before running it.

**A host's command.** Claude Code's plugin and Codex's belong to the host, so charter names
the command rather than running it. Hand that command to the person and let them run it.

**A charter checkout.** The CLI there is the tree being edited, so `update` never installs
over it — that refusal stays. What it does instead depends on what is left to do: on the
**dev channel** it refreshes the Claude Code plugin, which lives outside the tree, and says
that is all it did; on stable, and whenever `--to` names a version, it refuses outright.
`charter version` is the read-only view either way.

## Adopt what the version brought

```bash
charter news --pending
```

Each line is one entry: its slug, what it gives you, and how to take it up. Work them **one
at a time**, and ask before each:

1. Say what it is and why it matters — `charter news --for <version>` prints the entry.
2. `adopt: charter <command>` → run it once they say yes.
3. `adopt: manual` → turn the entry's body into steps they can follow. These are the ones
   that need a judgement only they can make: adding `delegate-when:` to a persona means
   deciding what that persona should be handed, and no command can decide it.

## Report what charter could not check

An entry whose probe could not run is reported as **unchecked** — neither adopted nor
pending. Say so in those words. Charter distinguishes "you do not have this" from "I could
not tell", and collapsing them turns a probe that quietly broke into a feature the person
appears to keep declining.

## This skill is the version you are upgrading FROM

The plugin carries this text, so the new version's skill arrives with the plugin — next
session. When `update` handed you a host command to run, everything here stays the previous
version's wording until it is run and the session restarts.
