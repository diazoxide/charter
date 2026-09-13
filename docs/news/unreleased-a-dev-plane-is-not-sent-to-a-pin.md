---
version: unreleased
headline: On the dev channel, `charter version bump` refuses to write the pin session start would refuse, and `version sync` stops recommending it
---

Session start refuses a plane that pins `[charter] version` and also declares
`[update] channel = "dev"`, because those ask for two different charters. Two commands
still led there. `charter version bump` checked no channel. It installed the target, wrote
the pin and, with `--push`, committed it, so every teammate on the plane later got a
refusal about a `charter.toml` they had not edited. And `charter version sync` on a dev
plane with no pin said `Pin one with: charter version bump --push`.

Now `version bump` on a dev plane refuses before it asks PyPI, installs anything or writes
the lock, with `--to` or without:

```
✗ refusing to pin a control plane that declares `[update] channel = "dev"`: a pin and the dev channel ask for two different charters. Nothing was installed or written.
•   to pin a release, drop `[update] channel = "dev"` from the plane's `charter.toml` and bump again
•   to stay on `main`, pin nothing and move this charter onto it:  charter update
```

On a pin-less dev plane, `version sync` says the plane follows `main`. It then names the
same next step `charter version` names on this channel: `charter update`, or a `git pull`
when the charter you run is a clone you are working in. The stable channel is unchanged.
