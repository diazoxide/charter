---
name: update
description: Update charter to a newer version and adopt what it brings — the app installs the new version, then this skill walks what this plane has not taken up. Use when asked to update or upgrade charter, whether a newer charter is published, what a version added, or how to adopt a new charter feature.
---

# Updating charter

charter is a desktop app, and **the app is what moves it**. The `charter` command in this
chat ships inside the app, as does this skill, so both move together when the app does.
Nothing you run from a chat installs a new version: `charter update` is only the half about
this plane's content, and it refuses `--to` and `--bump` by name, with the reason.

## Moving to a newer version

The app checks for a newer release on its own, at launch and every few hours, and **installs
only when the operator clicks**. When one is available, the status line at the bottom of the
window offers it; its dialog has **Install**, **Check now** and the release channel.

Tell the operator that, and what it costs: **installing ends every running chat**, this one
included, and the new version runs from the next start. Do not try to install it for them —
there is no command for it, on purpose.

To see which channel this machine takes releases from, or to move it:

```bash
charter update --channel stable    # or: dev
```

## What this plane pins

```bash
charter version
```

It prints this charter's version, which is the app's, and the version the plane pins
(`[charter] version` in `charter.toml`). Exit 0 means the pin is met or there is none; 1
means drift. Relay what it says about the pin in its own words. Do not call charter current
or up to date on your own reading of it. Moving the pin moves every teammate, so it is a
change to `charter.toml` the operator makes, never one you make unasked.

## Say what the version brought

```bash
charter news                  # every version of the app, newest first, from its CHANGELOG.md
charter news --for <version>  # one version: the same notes as its release page and About
```

Relay the section for the version they are on, or the versions since the one they last knew,
in your own summary. An entry under **Fixed** that reads as a security fix is worth saying
first. When a note describes something the plane has to take up by hand, offer to walk them
through it, one at a time, and ask before each change.

`charter news --since`, `--until` and `--pending` are retired: the app writes no update
baseline and ships no adoption probes. They are refused by name.

## This skill is the version you are running

It ships inside the app, so after an update the new version's skill arrives with the next
chat the new app starts — not in this one.
