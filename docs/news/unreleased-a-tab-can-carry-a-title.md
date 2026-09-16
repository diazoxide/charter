---
version: unreleased
headline: A chat tab can carry a title — rename it from the tab menu or F2, name it at `+`, and a handoff titles its chat from the brief
---

A chat tab was its bare id. On a plane with four chats in one workspace, `api.1` through
`api.4` told you how many you had and nothing about which was which — and there was no rename
anywhere in the frame.

**A tab can now carry a name you chose.** The id is untouched: it goes on doing every piece
of linking charter does — the harness session, the transcript, the quit record, the click map,
`charter frame-chat` — and a chat with no title behaves exactly as it did before.

**Four ways to set one.**

- **`chat: rename` in the tab menu.** Press `-` on the chat strip, or right-click any tab, and
  the middle row opens a one-line input in the pane you are already looking at.
- **`chat: rename` in `F2`**, which renames the chat the palette was opened in.
- **A title row at `+` and on a workspace tab.** The profile selector a new chat opens at
  carries `title: (none) — Enter to name this chat` as its last row, so the tab is named
  before anything starts.
- **A handoff names its own chat** from the first line of the brief it was opened on — the
  chat you did not open now reads as what it was opened to do.

Enter on an empty input takes the title off again, and `Esc` cancels the naming and nothing
else — at `+` it comes back to the profile list rather than closing the chat you are making.

**Where it shows.** The chat strip, in place of the id; and after the id in the tab menu's
heading, the quit and `chat: close` confirmation rows, the choice an ended tab offers, and
`F2 → chat`'s list. Nowhere else: a workspace is not a chat.

**Claude Code is started under `<title> · <id>`**, so a titled chat is what you see in its
prompt box and in `claude --resume`'s picker. Charter passes the name with `--name` at the
next start or resume and **never types `/rename` into a running harness** — so a rename you
make now reaches Claude Code the next time that chat starts or resumes, and the notice says
so. **Codex and opencode take no name at launch** and never see the title: neither has a flag
charter could pass, so their tabs carry the title in charter's own surfaces only.

**A quit records the title and `charter reopen` puts it back**, before the harness starts, so
a restored plane comes back under the names you gave it.

**One gesture moved.** The tab menu's cursor opens on the first row that can run, and that
is now `chat: rename` rather than `chat: close` — so `-` then `Enter` opens the rename input
instead of the close warning, and close is one `down` below it. A press that used to be one
keystroke from a confirmation is now two, which is the direction that guard points.

**Limits.** A title is one line of printable text, at most 60 characters. A longer one is
refused with its length rather than cut, and a control byte is refused rather than escaped —
charter says the rule and renames nothing. A press on a tab still resolves to the chat and
never to its words, so the strip's click map is unchanged. A chat with a long title makes the
strip wider, which can cost it a row or push a tab behind the overflow count; `F3` is the
gesture for more rows.
