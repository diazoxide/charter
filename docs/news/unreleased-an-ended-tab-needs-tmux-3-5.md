---
version: unreleased
headline: Keeping a chat's tab when its harness ends needs tmux 3.5 — below that an exit can go unreported, and charter now says so instead of promising a tab it may not get
---

**A harness that ends keeps its tab** — resume, start fresh, or close — and all of it rests
on tmux telling charter that the pane's program finished. Up to and including **tmux 3.4**,
that message can go missing: a lost `SIGCHLD` means tmux never runs the pane's `pane-died`
hook, so charter is never told the harness ended. The tab is not kept. The pane stops where
the harness left it, with no resume row and nothing on screen saying it is over, and the chat
goes on looking live everywhere else because as far as charter knows it is.

**This is a tmux bug, fixed upstream in tmux 3.5, and not something charter can work
around** — charter learns about an exit through that hook and has no second way to hear about
it. It is intermittent rather than constant, which is the part worth knowing in advance:
measured with charter taken out of the picture, tmux 3.4 under load missed 8 of 60 exits; on
one image 3.4 missed 5 of 40 and 3.5 missed 0 of 40.

**Ubuntu LTS ships 3.4**, so `apt` on that release will not move you off it — a newer tmux
has to come from a backport, a build, or Homebrew. macOS gets 3.7c from Homebrew, so a Mac
already has this.

**Two cases differ.** A chat opened *inside a tmux you are already in* never uses that hook
— charter watches the pane itself there — so it keeps its tab on any tmux and loses only the
exit code, which can turn a clean `/exit` into a crash drawer. And `charter frame -- <cmd>`,
the escape hatch, closes its window through the same hook rather than offering a choice, so
below 3.5 a missed exit leaves it attached with nothing to end the session. That one is a
hang rather than a missing tab.

**Nothing is switched off below 3.5.** Charter launches the same way, arms the same hook, and
keeps the tab every time the hook does fire. What changed is that charter stops promising it
and says so, on the two surfaces you can ask on demand rather than at a launch:

- **`charter doctor` has a row of its own, `ended tab`**, and it answers three ways: the tmux
  it measured and that an exit is reported; the tmux it measured and that one can be missed,
  with what that costs; or that it could not read a tmux version here at all and therefore
  cannot say either way. It never reports the third as one of the first two.
- **`charter <harness> --probe` and `charter frame-probe`** name it as a standing limit
  alongside the 3.2 floor and the 3.3 resize hook.

A tab that was missed is closed the ordinary way, the palette's `chat: close`, which asks for
confirmation because charter still believes there is a harness in there to stop.

The mechanism, the measurements and the remedy are in `docs/frame.md` under *What it needs*
and *When a harness ends*. It is the same finding as #1116, which is why charter's CI has
run the real-tmux tests on tmux 3.5a since #1120.
