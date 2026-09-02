---
version: unreleased
headline: A byte charter cannot decode is one odd character in a listing, not a traceback out of a quit
---

`frame/tmuxctl.run` is the one place charter promises that a tmux which misbehaves degrades
down the path a refusal does. It caught two ways for a tmux command to end badly — a wedged
server (`subprocess.TimeoutExpired`) and a tmux that could not be started at all (`OSError`)
— and there was a third it did not catch:

```
UnicodeDecodeError: 'utf-8' codec can't decode byte 0xff in position 15: invalid start byte
```

`capture_output=True, text=True` with no `errors=` decodes the pipe **strictly**. The raise
happens inside `subprocess.run`, before charter has a return code to branch on, and
`UnicodeDecodeError` is a `ValueError` — so neither existing clause saw it, and it came out
of the function whose docstring says it never raises. `_probe`, the module's other captured
child, had the same hole: it catches `OSError` and `subprocess.SubprocessError`, and
`version()` gates the whole launch, so a `tmux` on `$PATH` answering in another encoding was
a traceback in place of a frame.

## Which tmux output can actually do this, measured rather than assumed

#828 files the sharp caller as `_capture_transcript`, reading a harness pane during a quit,
on the grounds that a pane holds arbitrary bytes. **Measured on tmux 3.7c, that path does
not reach charter** — under `LANG=C.UTF-8` and again under `LC_ALL=C`, a pane that prints
`\377` is stored in tmux's own screen as U+FFFD, and `capture-pane -p -e -N` hands back
valid UTF-8. tmux sanitises it before charter ever sees it.

Two other paths do, both measured on the same binary:

* **A user option round-trips its bytes untouched.** `set-option -w @charter_chat` with a
  raw `\377` in it comes back out of `list-windows -a -F '#{@charter_chat}'`, and out of
  `display-message -p`, exactly as it went in. That listing is `_chat_seats` — what
  `charter frame-quit` asks before it kills anything — and §3.3 is why it is not
  hypothetical: one tmux server serves every plane on the machine, so charter reads windows
  it did not create, and the agent inside a harness pane can reach the same socket.
* **tmux's own stderr echoes the raw bytes of an argument it refuses**:
  `invalid window name: BAD\377NAME`. That is `report_failure`'s input, so the decode could
  take charter down *while it was reporting a failure*.

## What it does now, and the two shapes that were not chosen

Both captured children decode with `tmuxctl.DECODE_ERRORS`, which is `"replace"`: a byte
charter cannot read becomes U+FFFD and the rest of the answer arrives intact. A chat id with
one in it fails `_FRAME_ID_RE` and its row is dropped, which is what that listing already
does with rows it cannot read; a transcript keeps its other two thousand lines.

**Not a third invented return code beside `TIMED_OUT` and `COULD_NOT_RUN`.** Those two say
*charter never got an answer*. Here tmux answered — rc 0, the whole listing — and only
charter could not read one byte of it. Reporting that as a refusal would name tmux in a
failure message for a command it ran correctly, and would cost `_capture_transcript` a whole
transcript over one byte.

**And not `errors="surrogateescape"`, which is what #828 suggests.** It stops the raise in
`run` and moves it: a lone surrogate has no UTF-8 encoding at all, so it raises
`UnicodeEncodeError` on any strict encode a caller performs later — `sys.stdout` under a
normal `LANG=en_US.UTF-8` is exactly that (measured: `sys.stdout.errors` is `strict` there,
and writing one raises), as is any `write_text`. A function documented as never raising has
to hand back a value that does not raise either, or the promise covers only its own frame.
The round trip it is chosen for is never cashed anywhere either: the one caller that
persists the text encodes it with `errors="replace"` first, which writes `?` — 0x3F,
measured — so the bytes are lost at the sink regardless, and lost as a character
indistinguishable from a question mark the agent really printed. U+FFFD at least says
something unreadable was there, which is what a terminal shows for the same byte anyway.

## The floor one module over keeps its job and changes its reason

`_capture_transcript`'s `text.encode("utf-8", "replace")` is #810 group C's odd survivor,
pinned last release by a case that hands it a capture holding a lone surrogate. #828 reads
it as dead code to delete once the decode is settled. It is not dead — it is *unreachable in
production and observable in the suite*, deliberately, and the test that pins it says so.
What changed is the sentence underneath: it used to be a floor for the day the read path
stopped raising, and it is now a floor for the day the read path stops replacing. Both
spellings of that coupling — the comment on the encode and the docstring on the case — now
name `tmuxctl.DECODE_ERRORS`, so a change to one is a change somebody makes on purpose.

Nothing to adopt.
