---
headline: The vault's headline claim now says what charter guarantees, which is narrower than what it said
---

charter's front page said *"the model never sees the value"*. `skills/secrets/SKILL.md` —
the page loaded into the model's own context — said it harder: *"In every case the value is
injected into the subprocess and redacted from its output, so a command that echoes it
still cannot leak it into the transcript."*

Both are false, and not by a clever exploit. Redaction is a string replacement over the
output charter captured, so a command that *transforms* the value defeats it through the
ordinary path:

```
charter secret exec demo --env T=T -- sh -c 'echo "$T"'            # ***
charter secret exec demo --env T=T -- sh -c 'printf %s "$T" | base64'   # the value, in base64
```

`--exec` and `--stream` capture nothing at all and therefore redact nothing, which
`docs/secrets.md` has always said and `charter/secrets/base.py` has always called *"a
defence-in-depth net."* The defect was never the mechanism. It was that **the strength of
the claim rose as the reader's ability to check it fell**: correct on the page you reach
after going looking, unqualified on the front page, and strongest of all in the file read
by the one reader who cannot go and check.

**The guarantee, stated the way it is actually kept.** *On the paths that consume a value
— `secret exec` with `--env`/`--file`/`--dotenv`, and the MCP launcher — charter never
prints the value into the conversation, and everywhere else charter prints it only into a
destination you named yourself.* Where the value goes after that is a property of the
command you asked charter to run. Read `charter secret exec <vault> -- <cmd>` with the same
suspicion you would read `<cmd>` holding the credential directly, because that is what it
is. That sentence, and the paragraph that bounds it, now stand in `SECURITY.md`, in the
README, in `docs/secrets.md`, `docs/mcp.md`, `docs/hooks.md` and `docs/git-policy.md`, and
in both model-facing skills.

**The first draft of that sentence was itself false, which is the whole point of the
issue.** It read *"charter never prints the value into the conversation"*, full stop — and a
reviewer whose only instruction was to break it ran three words of charter's own CLI:

```
$ charter secret cp demo TOKEN /dev/stdout
FAKE-NOT-A-REAL-SECRET-9931✓ Wrote 'demo/TOKEN' to /dev/stdout (0600). Value not shown.
```

That is charter's own process writing the credential into the transcript, with no child
command anywhere in it, and then reporting that it did not. `charter secret get --reveal
--force` is the second route. Both are open ([#421](https://github.com/diazoxide/charter/issues/421),
[#422](https://github.com/diazoxide/charter/issues/422)) and being fixed separately; the
sentence is bounded here rather than left to depend on a fix that has not landed. A
guarantee whose truth is scheduled is not a guarantee.

**The model gets two new hard rules**, because it is the reader that acts on this file
rather than filing an issue about it: never pass a secret to a command whose recipient you
did not choose, and never `secret cp` to anything but a real file path you named.
`/dev/stdout` and friends put the value straight into this conversation.

**Two guard descriptions stopped overstating their own reach.** `docs/hooks.md` described
the secret-leak guard as catching "a command whose argv would put a vault's contents into
the transcript", which is a semantic property no name-based check has; `docs/secrets.md`
said the hook "denies `--reveal` outright". Both now describe what the guard is: a check on
names in the argv it can see, which closes the accidental roads and not a chosen one.

**The prose test stopped exempting anything that says "charter".** Round one asked whether
the word appeared anywhere in the sentence, and the same reviewer rescued four of this
file's own historical claims by pasting it into a subordinate clause — *"Because charter
resolves it in its own process, the model never sees the value"* passed. A guard against
overclaiming that is silenced by mentioning the project's name is not a guard. Now the
question is positional and asked of the clause the absolute word sits in: the bound has to
be in the *same sentence*, naming charter is necessary and never sufficient, and a promise
whose subject is the model or the value itself fails whatever else the sentence says. The
verb lists grew for the same reason — "the value is **stripped** from every output" and
"the value never **enters** an agent's context" were both synonyms away from the words the
first draft knew.

**Nothing about the vault changed, and nothing needs adopting.** What changed is that the
promise on the front page is now bounded where the code bounds it — and
`tests/test_claims.py` fails the build if a future sentence anywhere in these pages makes
an absolute claim about where a value ends up without a bound beside it.
