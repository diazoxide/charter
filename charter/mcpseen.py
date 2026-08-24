"""Which credentialed MCP command this operator has seen — and therefore approved.

A persona's ``personas/<name>/mcp.json`` declares an MCP server, and a ``secrets`` map on
that entry turns it into ``charter secret exec <vault> --env … --exec -- <command>``. The
mechanism is right: the value reaches the server's environment without passing through a
context window. What was missing is that **the same committed file chooses the command**
(#330). `sync-agents` then writes that argv into ``.claude/agents/<name>.md``, which the
harness loads and whose stdio servers it starts.

**Why this is consent and not an allowlist.** #317 was the same shape on a news ``check:``,
and PR #319 closed it with a list of commands a probe may run. That works there because a
``check:`` names a *charter subcommand* — a closed grammar charter defines, so "which
commands may run" has an enumerable answer, and `news._pass_through` can even read the
dangerous shape off the argparse parser rather than naming it. None of that transfers. An
MCP ``command`` is an arbitrary binary followed by arbitrary ``args``, so a list holding
the launchers real servers use (``npx``, ``uvx``, ``docker``, ``node``) is walked past by
``args`` alone — ``uvx --from git+https://… evil`` is #332's mechanism one field over —
and a list excluding them refuses every MCP server anyone actually runs. The axis with an
answer is not *what* the command is but *whether the operator has seen it*.

**The whole entry, and only what was shown.** Two properties the mechanism is worthless
without, and it shipped without both. The digest covers every key of the entry rather than
the handful charter reads, because `persona.mcp_render_entry` passes the rest through to
the harness — ``env`` did exactly that, so a committed edit could re-point an approved
server's ``PATH`` with the approval intact (#426). And an entry :func:`describe` cannot
render is not approvable at all, because the consent line IS the consent: an ``http``
server used to print a blank one under the words "read the command above" (#427).

**Machine-local and gitignored, deliberately.** Under ``STATE_DIR``, the same as
:mod:`charter.guardseen` and for a sharper reason: if the approval travelled in git, the
commit that declares the server could also declare that the server was approved, which is
the finding restored with an extra step.

**Withholding, not refusing.** An unapproved server is still written to the agent file —
only its credential is withheld. Deleting the server would break a working persona to
prevent a hypothetical, and charter's rule is additive: name the blocker, refuse the
dangerous half, and leave everything else working. The server starts and fails to
authenticate, which is a visible failure rather than a silent one.

**Nothing here raises.** A missing or corrupt marker reads as *nothing approved*, so the
failure direction is "the credential was withheld", never "sync-agents crashed" and never
"the credential was handed over because the file was unreadable".
"""

from __future__ import annotations

import hashlib
import json
import re
from pathlib import Path

from . import config

#: One file per plane, under the state dir. Never committed — see the module docstring.
FILE_NAME = "mcp-approved.json"

#: Printed in place of a consent line for an entry :func:`describe` cannot render. Such an
#: entry is reported as withheld and refused for approval, rather than silently dropped.
#: Covers both reasons: no destination at all, and a destination too big to show in full.
UNRENDERABLE = "(charter cannot show this entry in full — nothing to approve)"

#: Longest single part — command, one arg, ``type``, ``url``, one ``env`` key — shown on a
#: consent line. A part longer than this is CLIPPED with the cut announced; it is never
#: dropped, so no part can push another part off the line. See :func:`describe`.
#:
#: :data:`MAX_LINE` still bounds how many such parts fit — and an entry that overflows it
#: is refused whole rather than having a part dropped, so "no part pushes another off"
#: holds in both directions.
MAX_PART = 200

#: The narrowest terminal charter assumes, and the most rows one PRINTED consent line may
#: take on it. Their product is the hard ceiling on that line — label, decoration and
#: destination together, because all three are on the same screen.
MAX_COLS = 80
MAX_ROWS = 10

#: Longest either half of the ``persona/server`` label prints as. See :func:`label`.
MAX_NAME = 35

#: The whole label as printed: both halves and the ``/`` between them.
MAX_LABEL = MAX_NAME * 2 + 1

#: Everything charter itself puts on a printed consent line and nobody committed: the
#: ``• `` bullet `util.info` prefixes, the two-space indent, and the ``→`` with a space on
#: each side. Counted here because the ceiling below is a screen, and a screen does not
#: care who put the columns on it.
_DECORATION = len("• ") + len("  ") + len(" → ")

#: Ceiling on the DESTINATION half — what :func:`describe` may return. An entry with so
#: many parts that even their clipped forms do not fit is one the operator cannot be shown
#: in full, so it is not renderable and (via :func:`fingerprint`) not approvable.
#:
#: This is a SCREEN, not a byte count, and that is the whole reason it exists. The
#: operator answers the prompt printed *under* this line, so a line taller than the
#: terminal has already scrolled the command it names off the top by the time the
#: question is asked. Round two set it to 2000 — twenty-five rows of an 80-column tty —
#: and nine args of 200 padding columns each fit inside it with the destination out of
#: view. Escaping (see :func:`_safe`) makes such padding visible; it does not make it
#: short, so the ceiling has to be the screen itself.
#:
#: The label and the decoration are SUBTRACTED rather than ignored, which is the round
#: after that one: a ceiling that bounds the part charter was looking at and not the line
#: it prints is bounded by whatever the attacker puts in the other part. A committed
#: server name of a hundred thousand characters printed twelve hundred rows in front of a
#: destination that was itself comfortably inside the ceiling.
MAX_LINE = MAX_COLS * MAX_ROWS - MAX_LABEL - _DECORATION

#: Two or more ASCII spaces. After :func:`_safe` escapes every codepoint outside printable
#: ASCII, the ASCII space is the ONLY character that can still reach a consent line and
#: render as nothing — so collapsing this run is the whole blank class, not one member.
_SPACE_RUN = re.compile(" {2,}")


def path() -> Path:
    return Path(config.STATE_DIR) / FILE_NAME


def needs_consent(vault: str | None, entry: dict) -> bool:
    """Would rendering *entry* hand *vault*'s value to the command a committed file names?

    The one question that decides whether there is anything to consent to, asked in one
    place so the approve path, the withheld report and the digest cannot disagree about
    which servers are in scope. Kept separate from :func:`fingerprint` because a digest of
    ``None`` now means "no approval can exist", which includes entries that ARE in scope
    and must still be reported.
    """
    if not vault or not isinstance(entry, dict):
        return False
    secrets, files = entry.get("secrets"), entry.get("secret_files")
    return bool((isinstance(secrets, dict) and secrets)
                or (isinstance(files, dict) and files))


def _canon(value):
    """Untrusted JSON as something :func:`json.dumps` renders deterministically.

    Recursive and total, rather than a list of fields: the WHOLE entry is digested, so a
    key charter does not know about yet cannot fall outside the fingerprint. ``env`` was
    exactly that key (#426) — copied verbatim into the generated agent file by
    `persona.mcp_render_entry`, handed to ``execvpe``, and invisible to the digest, so a
    committed edit could add ``NODE_OPTIONS`` or re-point ``PATH`` on an already-approved
    server without lapsing the approval.

    A value JSON cannot carry is tagged rather than stringified, so an exotic object
    cannot digest as the plain string that happens to be its ``repr``.
    """
    if isinstance(value, dict):
        return {str(k): _canon(v) for k, v in value.items()}
    if isinstance(value, (list, tuple)):
        return [_canon(v) for v in value]
    if value is None or isinstance(value, (str, bool, int, float)):
        return value
    return ["<not-json>", repr(value)]


def fingerprint(vault: str | None, entry: dict) -> str | None:
    """What the operator is being asked to approve, as one digest.

    ``None`` when no approval can exist for this entry, which is two cases and both mean
    "render it without the vault wrapper":

    * **Nothing to consent to** — no ``secrets`` and no ``secret_files``, or no vault. The
      entry is passed through untouched by `persona.mcp_render_entry`, so no credential is
      at stake and requiring approval would be a prompt about nothing.
    * **Nothing to show** — :func:`describe` cannot render a destination for it, so the
      operator would be approving a blank line (#427). An entry nobody can be shown is not
      an entry anybody can approve. "Cannot be shown" is two properties and both are
      decided on the ESCAPED line, not on the raw one: it renders as nothing (only ASCII
      spaces survive :func:`_safe`), or it does not fit on the screen the question is
      asked on (:data:`MAX_LINE`). Round two decided the first on ``str.isprintable``,
      which is true of U+3164 HANGUL FILLER — so a line blank on every terminal got a
      real digest and was approvable.

    **Every field of the entry is in here**, which is the point: approving a server by
    name — or by five of its fields — lets a later commit re-point the same name at a
    different binary, a different endpoint, or a different environment while the approval
    stays valid. The vault (whose secrets) and the entry in full (where they go) both
    change the digest.
    """
    if not needs_consent(vault, entry) or not describe(entry):
        return None
    material = json.dumps({"vault": str(vault), "entry": _canon(entry)},
                          sort_keys=True, ensure_ascii=False)
    return hashlib.sha256(material.encode("utf-8")).hexdigest()


def _read() -> dict:
    try:
        doc = json.loads(path().read_text())
    except (OSError, ValueError):
        return {}
    return doc if isinstance(doc, dict) else {}


def approved(persona_name: str) -> set[str]:
    """Fingerprints this operator has approved for *persona_name*. Never raises."""
    entry = _read().get(persona_name)
    return {f for f in entry if isinstance(f, str)} if isinstance(entry, list) else set()


def approve(persona_name: str, fingerprints) -> None:
    """Record *fingerprints* as this persona's approved set, REPLACING what was there.

    Replacing rather than adding is what makes the record self-pruning: a server the
    persona no longer declares stops being approved the next time the operator approves,
    so a stale entry cannot come back to life under a re-added server name.
    """
    doc = _read()
    doc[persona_name] = sorted({f for f in fingerprints if f})
    p = path()
    p.parent.mkdir(parents=True, exist_ok=True)
    p.write_text(json.dumps(doc, indent=2, ensure_ascii=False) + "\n")


def _escape(ch: str) -> str:
    """One codepoint as an escape no other codepoint can also spell.

    Astral planes get the eight-digit ``\\U`` form rather than a long ``\\u``, because
    ``\\u1f600`` is five hex digits: U+1F600 and the two characters U+1F60 + ``0`` would
    render the same, and two different commands that read the same on a consent line is
    the homoglyph finding with a different alphabet.
    """
    cp = ord(ch)
    return f"\\u{cp:04x}" if cp <= 0xFFFF else f"\\U{cp:08x}"


def _safe(text: str) -> str:
    """*text* as printable ASCII — every other codepoint shown as its ``\\uXXXX`` escape.

    Every field here comes out of a committed file and the line IS the consent, so the
    question this answers is not "is this character printable" but "does what the operator
    reads still say what would run". Two earlier spellings of this guard each matched
    something narrower than that question, and each was walked past — in order:

    * a ``\\r`` or an ``ESC[2K`` in ``args`` repaints the line and a U+202E bidi override
      reverses it — caught by ``str.isprintable``, which is where round one stopped;
    * ``str.isprintable`` is nonetheless **true** for U+3164 HANGUL FILLER, U+2800 BRAILLE
      PATTERN BLANK and U+115F/U+1160. All are ``isspace() == False``, survive ``strip``,
      and render as nothing on every terminal, so a ``command`` of three of them was a
      line blank to the reader and truthy to charter — round one's ``"   "`` one spelling
      on, and round two's regex over the ASCII space did not reach it;
    * a U+0301 combining acute is printable and is neither, and repaints the rows above
      and below the line; Cyrillic ``а``/``с`` are printable and are neither, and spell an
      endpoint that reads identically to the ASCII one it re-points to, so an operator
      re-asked about a homoglyph cannot see what changed.

    No list of codepoints answers that, because the next spelling is always one codepoint
    further out. The class that does is the **complement**: printable ASCII is what a
    consent line may contain, and everything outside it — any category, any plane, any
    combining mark, any lookalike — is shown as its escape rather than its glyph. MCP
    commands, args, urls and env keys are ASCII in practice; anything else on a consent
    line is a reason to show the escape, not the glyph.

    Emptiness is then decided on the **escaped** form, which is what turns "renders as
    nothing" from a growing list into a decidable question: the escaped string holds only
    U+0020..U+007E, and the ASCII space is the only member of that range that renders as
    nothing. Collapsing runs of it and stripping the ends therefore returns ``""`` **if
    and only if** the part was nothing but ASCII spaces. That is an argument about the
    whole class, not a sample of it — and `tests/test_mcp_approval.py` checks it by
    sweeping every codepoint Python can hold rather than by listing four.

    The backslash escapes to ``\\\\`` for the same reason the astral form is eight digits:
    so that **every** ``\\uXXXX`` on a consent line is a codepoint that was really there.
    Without it, a committed ``command`` holding the six literal characters ``\\u3164``
    reads exactly like one holding U+3164 — one more pair of different commands the
    operator cannot tell apart by reading the line. A Windows path shows as
    ``C:\\\\Users\\\\x``; that is the cost, and it is unambiguous.
    """
    out = "".join("\\\\" if c == "\\" else c if " " <= c <= "~" else _escape(c)
                  for c in text)
    return _SPACE_RUN.sub(" ", out).strip()


def _clip(text: str, budget: int) -> str:
    """*text* cut to *budget* characters with the cut ANNOUNCED, never silently.

    Used per part rather than on the finished line. Truncating the finished line is what
    let a committed ``args`` of 600 characters produce a consent line naming neither the
    ``env`` it sets nor the ``url`` it points at, because both are appended after ``args``
    — so the important half of the line was the half that got cut.

    The marker is ASCII, like everything else :func:`_safe` lets through, so that "a
    consent line is printable ASCII" needs no footnote reading "except charter's own
    ellipsis". A claim with an exception is a claim its test has to carve a hole in, and
    the hole is where the next spelling goes.
    """
    return text if len(text) <= budget else (
        text[:budget] + f"... (+{len(text) - budget} more chars)")


def _name(part) -> str:
    """One half of a :func:`label`: printable ASCII, never wider than :data:`MAX_NAME`.

    Clipped with a FIXED marker rather than with :func:`_clip`'s counted one. The count is
    what the operator needs for a destination — how much of the command is off the line —
    and it is exactly wrong here, because its own width grows with the input it describes.
    A budget that a longer input makes longer is not a bound, and a bound is the only
    thing standing between a committed name and the rows it costs.

    A half that renders as nothing shows as ``""`` rather than as an invisible gap, the
    same convention :func:`describe` uses for an ``env`` key, so ``reddit/""`` reads as a
    server whose name is blank instead of as a missing word.
    """
    shown = _safe(str(part))
    if len(shown) > MAX_NAME:
        shown = shown[:MAX_NAME - 3] + "..."
    return shown or '""'


def label(*parts) -> str:
    """The ``persona/server`` a consent line puts in front of the destination.

    :func:`describe` has been hardened three times, and this half of the same printed line
    reached the terminal untouched on all three — which is this fix's own lesson arriving
    at its own expense. Each round put the guard on the FIELD that was attacked rather
    than on the SURFACE it is printed on, so the next spelling only had to move one field
    over. It moved here.

    The ``server`` half is the live one: it is a key of a committed ``mcp.json``, so it is
    an arbitrary JSON string, of arbitrary length, in any script. Confirmed end to end
    through ``sync-agents --approve-mcp`` before this existed — a server named with three
    U+3164 HANGUL FILLERs printed ``reddit/ → uvx some-reddit-mcp`` with nothing between
    the slash and the arrow; one carrying an ANSI erase wiped charter's own words standing
    beside it and repainted the row from column zero; a bidi override reversed the line;
    and a name of a hundred thousand characters printed twelve hundred rows before the
    destination reached the screen, with :data:`MAX_LINE` satisfied throughout because
    :func:`describe` never saw the name.

    The ``persona`` half is *already* contained: it is a directory under ``personas/``, and
    `persona.reference_ok` refuses any reference outside ``[a-z0-9][a-z0-9._-]*`` (#328),
    so a persona whose name is not printable ASCII resolves to nothing and never reaches
    this line. It goes through the same escape anyway, because charter joins guards rather
    than choosing between them — this one still holds if that alphabet is ever widened.
    And the half of it that is not hypothetical: `valid_name` bounds the ALPHABET and not
    the LENGTH, so only the clip here keeps a 255-character persona directory off four
    rows of the screen the question is asked on.
    """
    return "/".join(_name(p) for p in parts)


def describe(entry: dict) -> str:
    """Where the credential would go, as the line that asks the operator to look at it.

    Names and keys only — a ``secrets`` map holds vault KEY names, never values, so this
    is safe to print; and it IS printed, because the question the operator is answering is
    "which credential goes where", and the line used to answer only the second half.

    **Which credential, not just which command.** ``secrets`` and ``secret_files`` are what
    `persona.mcp_render_entry` turns into ``--env VAR=<vault key>`` and
    ``--file VAR=<vault key>``, so the vault key named there decides *what value* the
    command receives. They were in the digest and not on the line, which is finding three
    of round three — a homoglyph re-point that lapses the approval and re-asks under a line
    that reads the same — moved to the field that chooses the credential rather than the
    destination: editing ``{"REDDIT_CLIENT_ID": "client-id"}`` to
    ``{"REDDIT_CLIENT_ID": "aws-root-key"}`` in a committed ``mcp.json`` produced a
    byte-identical consent line and a different fingerprint, so the operator was correctly
    re-asked and correctly could not tell why. Being re-asked under an unchanged line is
    not consent, it is a second chance to make the same mistake.

    ``""`` when the entry has no destination to show, which :func:`fingerprint` turns into
    "not approvable" — so a line the operator cannot read is a line nobody can consent to.
    Two ways to get there:

    * **Nothing named.** No ``command``, no ``args``, no ``url``. An ``http``/``sse``
      server has no command, and building the line from ``command`` + ``args`` alone
      rendered it as an EMPTY string under the words *"Read the command above"* (#427).
      Falling back to ``url`` fixes the common case; ``""`` for the rest is the general
      one. **A part that renders as nothing does not count as naming something** — and
      after :func:`_safe` that is decidable rather than enumerable: every part comes back
      as printable ASCII with all else escaped, so it is blank exactly when it held
      nothing but ASCII spaces. Round one tested ``""`` and missed ``"   "``; round two
      tested ``"   "`` and missed U+3164 HANGUL FILLER, which is printable, is not
      whitespace, survives ``strip``, and shows as nothing. Neither is a special case
      now — they are the same case, asked of the escaped form.
    * **Too much named.** So many parts that even their clipped forms exceed
      :data:`MAX_LINE` — :data:`MAX_ROWS` rows of an :data:`MAX_COLS`-column terminal.
      Charter will not print a page of destination and call it a line the operator read,
      and it will not print half of one either. Fail closed: withheld. The ceiling is a
      screen because the operator answers the prompt printed UNDER this line: round two
      set it to 2000 characters, twenty-five rows, and nine args of 200 padding columns
      fit inside it with ``uvx evil-server`` scrolled off the top. Escaping makes padding
      visible, which is necessary and not sufficient — visible padding scrolls a line just
      as far as invisible padding does, so the length that is refused has to be the length
      that does not fit.

    **Every part is named; only its contents can be shortened.** Round one clipped the
    FINISHED line at 600 characters, and both the ``[type url]`` and the ``(env: …)``
    suffixes are appended after ``args`` — so ~600 characters of plausible ``args`` in a
    committed file produced a consent line naming neither the ``env`` it set nor the
    ``url`` it pointed at, while the approved render still carried both to ``execvpe``.
    The comment that stood here claimed this was impossible because "the destination is at
    the FRONT of the line"; that was true only of ``command``, and false of every field
    that decides where a url-transport entry connects or which binary ``PATH`` resolves.
    Each part now gets its own :data:`MAX_PART` budget and the suffixes are built from
    already-clipped parts, so nothing appended to this line can be pushed out of it.

    ``env`` keys are shown because they choose the destination as surely as ``command``
    does: ``PATH`` decides which binary ``execvpe`` finds, ``NODE_OPTIONS`` decides what
    it loads (#426).

    The rule the three suffixes are instances of, said once so a fourth field does not
    have to be found the way these were: **everything the digest covers that changes what
    the vault hands over, or where it lands, is on the line.** ``env`` chooses the binary,
    ``url`` chooses the endpoint, ``secrets`` and ``secret_files`` choose the credential.
    A field that lapses an approval without changing the line spends the operator's second
    look on a line they have already read.
    """
    def _shown(x) -> str:
        # `or '""'`: a name that renders blank is still a name the harness would use, so
        # it is printed as an empty string rather than left as an invisible gap in a list.
        # Applied only to what actually reaches the line — an `env` VALUE never does, and
        # escaping a megabyte nobody will read is work done on an attacker's behalf.
        return _clip(_safe(str(x)), MAX_PART) or '""'

    if not isinstance(entry, dict):
        return ""
    raw = entry.get("args")
    argv = list(raw) if isinstance(raw, (list, tuple)) else ([raw] if raw else [])
    parts = [str(entry.get("command") or "")] + [str(a) for a in argv]
    dest = " ".join(p for p in (_clip(_safe(p), MAX_PART) for p in parts) if p)
    url = _clip(_safe(str(entry.get("url") or "")), MAX_PART)
    if url:
        shown = f"{_clip(_safe(str(entry.get('type') or 'http')), MAX_PART)} {url}"
        dest = f"{dest}  [{shown}]" if dest else shown
    if not dest:
        return ""
    env = entry.get("env")
    if isinstance(env, dict) and env:
        dest += "  (env: " + ", ".join(sorted(_shown(k) for k in env)) + ")"
    for field, shown_as in (("secrets", "vault"), ("secret_files", "vault file")):
        m = entry.get(field)
        if isinstance(m, dict) and m:
            # `VAR=key`, the shape `mcp_render_entry` builds — so the line reads like the
            # `secret exec` argv the answer authorises rather than like a summary of it.
            pairs = sorted(f"{_shown(var)}={_shown(key)}" for var, key in m.items())
            dest += f"  ({shown_as}: " + ", ".join(pairs) + ")"
    # Not truncated: refused. A line this long does not fit on the screen the question is
    # asked on, and cutting it would put us back where round one was — deciding which half
    # of the destination the operator gets to see. The digest covers every byte either way.
    return "" if len(dest) > MAX_LINE else dest
