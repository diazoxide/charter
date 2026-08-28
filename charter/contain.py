"""One name, one directory: the containment rule for names charter reads out of files.

Charter validated a name when a human typed it and never when it read one from a
committed file (#328). `valid_name` lives in :mod:`charter.persona` and
:mod:`charter.workspace` and is called from six places, all of them commands. The reading
side — `extends:`, `uses:`, `[persona] default`, `[workspace] default`, a committed
`workspaces/.default`, a `workspace.json` repo name, an `inventory/repos.json` name —
joined the value onto a path with nothing in between, so a committed file could name a
target outside the directory charter meant to look in.

The last two arrived a round later (#442) and are the reason this list is written out
rather than described. Every argument above had already been made and shipped for the
*persona* default; the workspace twin, two rungs of the same precedence ladder, was simply
not on anybody's list — so a plane's `[workspace] default` and its committed
`workspaces/.default` reached `workspace_dir()` unchecked while the persona rung beside
them was gated. A guard that covers "the sites we thought of" is a guard that grows a new
hole every time a noun is added; the enumeration is what makes the omission visible.

:mod:`charter.docsrc` already had the answer and it was never reused: a topic is matched
against a shape before it becomes a page, because ``charter docs show ../../etc/passwd``
"must not be a file-read primitive wearing a documentation command". This module is that
idea, extracted so the reading sites share one implementation instead of four
near-misses and a fifth that forgets.

**Two questions, kept apart on purpose.**

*Shape* — "could this string name one entry in a directory?" — is :func:`segment_ok`.
*Containment* — "does joining it stay inside the base?" — is :func:`child`. Callers use
both, and the pair is deliberate rather than redundant: the shape check belongs where an
identity is decided (`inventory.merge`, beside the bare-name collision logic that already
treats the name as load-bearing), and the containment assertion belongs at every join,
because a hand-edited or PR-modified tracked file never passes through the code that
decided the identity. An identity-layer check alone is a guard the attacker walks around.

**Who gets which shape rule.** Charter mints persona and workspace names — `persona
create` and `workspace create` enforce their own `valid_name` — so those keep answering
to `valid_name`, and lint agrees with the resolver by construction rather than by a second
check kept in step by hand. A **forge** mints repo names, and `org/.github` is a real repo
GitHub itself tells organisations to create, while `MyRepo` is merely ordinary. Imposing
charter's creation-time alphabet on someone else's forge would refuse to clone both.
:func:`segment_ok` is therefore the permissive rule: it forbids traversal and separators
and nothing else.

**Two layers, and the second one resolves.** :func:`segment_ok` and :func:`child` stay
lexical: they answer a question about a *string*, and asking the filesystem would make a
traversal succeed exactly when the attacker's target happens to exist. That left every
read charter performs — not only the ones named by a name — open to a committed symlink,
which is #336 and is now :func:`file_refusal` / :func:`dir_refusal` below. The two are
deliberately separate functions: a name is refused before anything is opened, and a *path*
is refused before it is read, and neither can stand in for the other.

**What "inside the plane" had to mean.** The obvious rule — refuse a path whose `realpath`
leaves ROOT — admits #336's own demonstration unchanged, because ``.charter/`` sits
*under* ROOT (:func:`config._migrate_state_dir`), so ``personas/x/persona.md`` →
``../../.charter/vaults/devops.json`` never leaves the plane while doing precisely what the
`pretooluse-read` guard exists to stop. The boundary is therefore the directories a plane
keeps its **data** in — :func:`data_roots` — which excludes the secrets home and includes
the one part of it that is data (ephemeral memory, which lives inside ``.charter/``).

**Links are followed; escapes are refused.** Refusing every symlink would close the hole
and break a plane that legitimately links a persona directory. Resolving keeps that plane
working, and #342's reason for staying lexical was never that symlinks are acceptable —
only that doing half of this while claiming all of it would be worse than filing it.

**One assembler for report lines, because two of them drifted.** :func:`sentence` and its
path-budget twin :func:`path_sentence` are where a line of charter's own output gets its
untrusted spans contained, and they are here rather than in each module that reports.
`news` grew a private copy of exactly this (#573) rather than import the one here, which
was private and carried the wrong budget for a frontmatter value — and, proximately,
because #498 was open on this module at the time. Within one version the two disagreed
about the budget and about what to do with a field holding several things — the second
drift invisible from either side, since neither copy could see the other. That is the
argument this module's own opening paragraph makes about `valid_name`, turned on this
module. A third reporting surface —
`commands_persona`'s tables, `frame/registry`'s entry-point errors, `mcpseen` — now picks
one of two written-down budgets instead of inventing a third (#576).

**No refusal function here raises.** These checks sit under `doctor`, the status line and
SessionStart, where the rule is that a hook may cost a session its briefing and never its
turn. The single exception is :func:`writable`, which exists to raise and says so: a write
that refuses has no fallback the way a skipped read does. A refused
name is reported as data — see :data:`NOT_A_SEGMENT` and the vocabulary in
:mod:`charter.news`, which says five kinds of "no answer" five different ways for the same
reason: folding a defect in a file behind a generic message hides the defect, and somebody
has to fix it.
"""

from __future__ import annotations

import json
import os
import stat
import unicodedata
from pathlib import Path

#: Said once, so every site refuses in the same words. A reader who hits this has a defect
#: in a committed file, not a typo, and the sentence has to be enough to act on: "no such
#: repo" would send them looking for a repo that was never the point.
NOT_A_SEGMENT = ("'{name}' is not a name — it is a path. This is read from a committed "
                 "file and joined onto a directory, so it may name one entry there and "
                 "nothing else: no '/', no '\\', no '.' or '..', and nothing absolute")

#: The separators every platform charter runs on will honour. Backslash is included on
#: POSIX too: the file is committed and shared, so the machine that *wrote* the name is
#: not necessarily the machine that resolves it.
_SEPARATORS = ("/", "\\")


def segment_ok(name: str) -> bool:
    """True when *name* could name one entry inside some directory.

    Deliberately a question about the *string*, never about the disk. Asking the
    filesystem would make a traversal succeed exactly when the attacker's target happens
    to exist, which is the one case where the answer must not change.
    """
    if not name or not isinstance(name, str):
        return False
    if name in (".", ".."):
        return False
    if "\x00" in name:
        # A NUL terminates the string inside the C library, so the name Python checked and
        # the name the kernel opened would be two different strings.
        return False
    if any(sep in name for sep in _SEPARATORS):
        return False
    # Catches a Windows drive-qualified name ("C:x") and anything else the running
    # platform considers rooted, without charter maintaining its own list of what those
    # look like.
    if os.path.isabs(name) or os.path.splitdrive(name)[0]:
        return False
    return True


def child(base, name: str) -> Path | None:
    """``base / name`` when that is a direct child of *base*, else ``None``.

    ``None`` rather than an exception because every caller is on a path that must not
    crash, and rather than a sanitised name because silently rewriting a name invents a
    second identity for the same thing and hides the defect in the file that somebody
    still has to fix.

    The normalised comparison is belt and braces over :func:`segment_ok` — which already
    forbids every separator, so the join cannot escape today. It is kept because it is the
    half that still holds if the shape rule is ever loosened to admit some new name, which
    is exactly the drift `docsrc`'s own comment warns about.
    """
    if not segment_ok(name):
        return None
    base = Path(base)
    joined = base / name
    if Path(os.path.normpath(joined)).parent != Path(os.path.normpath(base)):
        return None
    return joined


def refusal(name: str) -> str:
    """The one sentence every site uses to say why *name* was refused."""
    return path_sentence(NOT_A_SEGMENT, name=name)


# --------------------------------------------------------------------------- #
# the display layer — a committed value charter PRINTS back (#453)             #
# --------------------------------------------------------------------------- #

#: How much of a committed value charter repeats back on one report line, and a FIXED
#: marker rather than a counted one: a budget a longer input makes longer is not a budget.
#: Wide enough for a server name or a short command, narrow enough that a committed value
#: cannot own the terminal.
DISPLAY_LIMIT = 160

#: Unicode general categories with no glyph of their own, escaped by :func:`one_line`.
#: Named by CATEGORY rather than by codepoint, because a list of bad codepoints is a list
#: somebody adds one to: ``Cc`` is every control character (``\n``, ``\r``, ``\t``, NUL,
#: and the escape that starts an ANSI sequence), ``Cf`` every format character (the
#: bidirectional overrides, the zero-width joiners), ``Cs`` a lone surrogate, and
#: ``Zl``/``Zp`` the two separators that are not ``\n``.
_INVISIBLE = frozenset({"Cc", "Cf", "Cs", "Zl", "Zp"})


def one_line(value, limit: int = DISPLAY_LIMIT) -> str:
    """*value* as one line of a report, with nothing in it that can forge another.

    **The property is line structure, not trustworthiness.** Charter's own reports —
    `sync-agents`'s withheld list, `lint`'s issues, the consent line an operator reads
    before approving a credential hand-off — are lines of the form ``  <name> → <command>``,
    and every field in them comes out of a committed file. A newline in one of those
    fields writes a second line that looks exactly as much like charter's own output as
    the first, which is #453's mechanism one surface over: a value crossing into a format
    with structure without being escaped for it.

    So every character that has no glyph — see :data:`_INVISIBLE` — is replaced by its own
    escape, and the result is clipped. What this does **not** do is make the value
    trustworthy to read: ``I`` and ``l``, or a Cyrillic ``а`` and a Latin ``a``, are
    ordinary letters this returns unchanged and a reader cannot tell apart. Those cannot
    forge a line, which is the whole of what is claimed here. Where a value must be
    *bounded* rather than merely displayable — an MCP server name, which charter emits
    into YAML and into a tool-grant pattern — the bound belongs at the boundary that reads
    it, and this is what the refusal then uses to say which value it refused.

    Nor does it make the value **visible**, and that is the boundary worth naming because
    three callers crossed it (#498). :data:`_INVISIBLE` is a list of categories, so a
    value that renders as nothing without being in one of them — U+3164 HANGUL FILLER is
    ``Lo`` — comes back unchanged and correct: it forges no row. A caller whose sentence
    has to NAME something wants :func:`readable` instead. This function is not widened to
    cover it, because its other callers are printing content into a TUI and want their
    glyphs.
    """
    s = value if isinstance(value, str) else str(value)
    out = []
    for ch in s:
        if unicodedata.category(ch) in _INVISIBLE or (ch.isspace() and ch != " "):
            out.append(f"\\x{ord(ch):02x}" if ord(ch) < 0x100 else f"\\u{ord(ch):04x}")
        else:
            out.append(ch)
    rendered = "".join(out)
    return rendered if len(rendered) <= limit else rendered[:limit] + "…"


#: Shown by :func:`readable` in place of a value whose whole rendering is ASCII spaces —
#: the empty string, or a name made of nothing else. ``""`` rather than a word, so
#: ``✗ "": no role`` reads as a persona whose name is blank rather than as a missing word,
#: and so the marker cannot be confused with a name that spells "blank". The same marker
#: `mcpseen._name` prints, because it is the same question one report over.
BLANK = '""'


def escape_char(ch: str) -> str:
    """One codepoint as an escape no other codepoint can also spell.

    Astral planes get the eight-digit ``\\U`` form rather than a long ``\\u``, because
    ``\\u1f600`` is five hex digits: U+1F600 and the two characters U+1F60 + ``0`` would
    render the same, and two values that read identically on a report line is the
    homoglyph finding with a different alphabet.

    Deliberately NOT the escape :func:`one_line` writes. That one spells a codepoint below
    U+0100 as ``\\xNN`` and everything else with a ``:04x`` — which for an astral codepoint
    produces the five hex digits above, and ``\\x`` and ``\\u`` forms of two different
    widths besides. Harmless there: `one_line` escapes only the categories that carry no
    glyph, and two *controls* that read alike are not a finding. Here the escape is what a
    reader identifies the value BY, so every form has to be fixed-width and injective.

    **Where the boundary sits is a choice and not a property.** ``<=`` could be ``<`` and
    U+FFFF would come back as ``\\U0000ffff``: still fixed-width, still injective, still
    printable ASCII, so no test reddens and none should — the claims above hold either way.
    It is written as the shorter form for every codepoint that has one, which is the reason
    to prefer this side rather than a rule anything depends on.
    """
    cp = ord(ch)
    return f"\\u{cp:04x}" if cp <= 0xFFFF else f"\\U{cp:08x}"


def escaped(text: str, *, quote: bool = False) -> str:
    """*text* as printable ASCII — every other codepoint shown as its escape, reversibly.

    **The rule is a complement, and that is the whole point.** Everything else in this
    module's display layer names a class of *bad* characters: :data:`_INVISIBLE` is five
    general categories, and a list of categories is a list of spellings that the next
    codepoint is one step outside of. This says instead what a report line **may** hold —
    U+0020..U+007E — and escapes the rest, whatever category, plane, script or combining
    class it belongs to. Nothing has to be added to it when Unicode grows.

    The argument for that complement was made in full one surface over, at
    `mcpseen._safe`, after three rounds of narrower rules were each walked past: a control
    character repaints the line, a bidi override reverses it, U+3164 HANGUL FILLER renders
    as nothing while `str.isprintable` calls it printable, and a Cyrillic ``а`` spells a
    different value that reads the same. This is that rule, extracted so `mcpseen` and
    :func:`readable` share one implementation rather than two that drift.

    **Reversible.** ``\\\\`` for a real backslash, ``\\"`` for a real quote when *quote*,
    an escape for everything outside printable ASCII, and itself for the rest — so the
    characters printed for a value determine that value. The backslash doubling is what
    makes that true and is not decoration: without it a committed name holding the six
    literal characters ``\\u3164`` reads exactly like one holding U+3164, which is one more
    pair of different values a reader cannot tell apart.

    *quote* escapes the ASCII double quote as well, which is what lets an unescaped ``"``
    be a delimiter no committed byte can spell. Off by default: a surface that delimits
    with something else does not need it, and escaping a quote nobody is using as a
    delimiter only makes an ordinary value harder to read.
    """
    return "".join("\\\\" if c == "\\" else '\\"' if (quote and c == '"')
                   else c if " " <= c <= "~" else escape_char(c) for c in text)


def readable(value, limit: int = DISPLAY_LIMIT) -> str:
    """*value* as one line a reader can **read the value back off**. Never blank.

    A different question from :func:`one_line`, and a second function rather than a wider
    one, because `one_line` has some ninety callers and the property most of them want is
    the one it promises: a committed value cannot forge a second ROW. Those callers print
    workspace names, persona roles, component titles and forge text into a TUI, and a
    non-ASCII value there is ordinary content that should reach the screen as its glyphs.
    Widening `one_line` would escape every one of them to make three call sites legible.

    So this is for the three surfaces where the value is an **identifier** — a name a
    sentence tells somebody to go and fix, a path a sub-agent is told to run. There, being
    able to tell which one it is beats keeping the glyph, and `one_line` cannot deliver it:
    it decides on :data:`_INVISIBLE`, a list of five categories, and U+3164 HANGUL FILLER
    (``Lo``), U+2800 BRAILLE PATTERN BLANK (``So``), U+115F and U+1160 (``Lo``) are on none
    of them, are not `isspace`, and survive `strip`. A persona directory named with three
    of them linted as ``✗ : no role`` — a row naming no persona (#498).

    **Blankness is decided, not enumerated.** After :func:`escaped`, the string holds only
    U+0020..U+007E, and the ASCII space is the only member of that range that renders as
    nothing. So "this renders as nothing" is exactly "this is spaces", which is a question
    about the whole class rather than a sample of it, and :data:`BLANK` stands in when the
    answer is yes. That is why the escape comes first and the emptiness test second; the
    other order is the growing list this exists to avoid.

    **What it costs, said out loud.** A legitimately non-ASCII value prints as its escapes
    here. That is a real loss and it is bounded to these surfaces on purpose: charter mints
    persona names itself (`persona.valid_name` — a lowercase letter or digit, then
    ``[a-z0-9._-]``), so a name this is asked about is ASCII by construction and comes back
    byte-identical, and the same holds for the ordinary `bin/` script name. Nowhere a person
    writes prose — a `role:`, a memory, a workspace title — goes through this.

    Clipped with ASCII dots rather than ``…``, so the promise "what comes back is printable
    ASCII" holds for a clipped value too and a caller never has to special-case the marker.
    """
    # `str(value)` unconditionally, and not `value if isinstance(value, str) else …`: for a
    # string `str` hands the same object straight back, so the type test in front of it was
    # a branch that could not change an answer. `one_line` above carries the longer form;
    # this one does not copy it. The coercion ITSELF is load-bearing and is pinned by a
    # test — a report surface that raises on a `Path` tells its reader less than one that
    # prints the path, and "nothing here raises" is this module's rule.
    shown = escaped(str(value))
    if len(shown) > limit:
        shown = shown[:limit] + "..."
    # `.strip(" ")` and not `.strip()`, and the two are EQUIVALENT here — deliberately, and
    # checked: after `escaped` the string is U+0020..U+007E, so a bare `strip` has no other
    # whitespace left to find and no test can tell them apart. Naming the space is what says
    # WHY the question is decidable at all; a bare `strip` would read as "whatever Python
    # calls whitespace", which is the category list this function exists to get away from.
    return shown if shown.strip(" ") else BLANK


#: How much of any ONE path a refusal sentence — or a generated brief — repeats back.
#: Larger than `DISPLAY_LIMIT`, because these name a PATH and a plane's paths are
#: legitimately long, and a clipped path is one the reader cannot act on. Still a fixed
#: number, for the reason `DISPLAY_LIMIT` is one: a budget the input can grow is no budget.
PATH_DISPLAY_LIMIT = 1024


#: What :func:`sentence` writes BETWEEN the elements of a field holding several things.
#:
#: Charter's own separator, for the same reason the template is charter's own text: a line
#: naming several committed things is a line with structure, and a separator taken from one
#: of the things would let that thing restructure the line. Written once, so two report
#: surfaces cannot separate their lists differently.
SEQUENCE_SEPARATOR = ", "


def _slots(fields: dict, limit: int) -> dict:
    """Every field bounded to one line, ready to be substituted into a template.

    The **budget** is the caller's, and it is the only thing :func:`sentence` and
    :func:`path_sentence` differ by. Everything else — that a field is contained at all,
    and that a field holding several things is contained element by element rather than
    after the join — is the same question wherever a report line is assembled, so it is
    answered here and cannot be answered twice.

    **Containing the elements and not the join, because the join is the sentence.** A
    sentence that names a list exists to name every entry in it; clipping the joined string
    drops the last entries and leaves a line that reads as though those entries were never
    there. Per element, a long entry is clipped and its neighbours still arrive.

    **"Several things" is a property, not a type.** The test is *is this one string, or is
    it something that can be iterated* — not a list of the container classes somebody
    happened to pass. ``(list, tuple)`` is a spelling: a caller handing a `set`, a
    `frozenset`, a `dict_keys` or a generator to a sentence naming several files falls off
    the end of it and gets Python's own `repr` of the container printed into charter's
    prose, brackets and quotes and all. `str` and `bytes` are iterable and are one thing,
    which is why they are named as the exception rather than the containers being named as
    the rule.
    """
    shown = {}
    for key, value in fields.items():
        if isinstance(value, (str, bytes)) or not hasattr(value, "__iter__"):
            shown[key] = one_line(value, limit=limit)
        else:
            shown[key] = SEQUENCE_SEPARATOR.join(one_line(v, limit=limit) for v in value)
    return shown


def sentence(template: str, **fields) -> str:
    """One line of charter's own report, with **every** field in it bounded to one line.

    *template* is a literal in charter's own source — charter's sentence, with ``{}`` slots.
    Everything substituted into it is treated as a value out of a committed file or off a
    filesystem somebody else's commit created, and goes through :func:`one_line`.

    **The containment is at the assembly, not at the slots.** A report line is charter's
    own output and the fields in it are not; a newline in one of them writes a second line
    that looks exactly as much like charter's output as the first. `news.entry_errors`
    contained the ordering *value* and interpolated the committed *filename* three inches
    away raw, so an entry named ``0.60.0-a\\nEVIL: charter says nothing is wrong.md``
    printed two lines where charter emitted one, the second being the author's sentence in
    charter's voice (#502). The value was contained because the value was what that commit
    was about — not because the filename had been judged safe.

    Wrapping each span individually fixes the spans somebody enumerated and leaves the door
    open at the shape #502 predicted: a further untrusted span turning up in one of these
    sentences. A field added to a template tomorrow is contained by having been passed
    here, which is a property a reviewer checks by reading the call site rather than one
    they have to remember.

    **Two functions rather than one with a ``limit=`` keyword, and the reason is this
    signature.** ``**fields`` makes the template's slot names and this function's own
    parameter names one namespace, so a template that ever grows a ``{limit}`` slot would
    have its value silently eaten as the budget and then raise ``KeyError`` out of
    `str.format` — inside the module whose rule is that nothing here raises. The budget is
    named by which function you call instead, so no slot name can collide with it, and the
    two budgets charter has are written down where each other can see them.

    What this does not reach is a call site that builds its sentence with an f-string
    instead of calling this. Nothing in the language stops that, so
    `tests/test_a_news_entry_cannot_forge_a_report_line.py` plants a line break in every
    field a news entry owns and asserts each report is still the number of lines charter
    meant to write.
    """
    return template.format(**_slots(fields, DISPLAY_LIMIT))


def path_sentence(template: str, **fields) -> str:
    """:func:`sentence` at :data:`PATH_DISPLAY_LIMIT` — a sentence that names a PATH.

    The refusals in this module name resolved paths, and a plane's paths are legitimately
    long: at :data:`DISPLAY_LIMIT` the reader gets a clipped path they cannot act on, which
    is the one thing a refusal exists to give them. Everything else is :func:`sentence`.

    Formatted here rather than at the twelve `.format` calls it replaces, because twelve is
    how many places the thirteenth gets forgotten in — the same argument that put the name
    bound inside `persona.mcp_servers` instead of at each of its consumers (#453).
    """
    return template.format(**_slots(fields, PATH_DISPLAY_LIMIT))


def json_line(obj, *, sort_keys: bool = False) -> str:
    """*obj* as JSON on exactly **one physical line**, whatever strings it holds.

    The property is ``len(json_line(x).splitlines()) == 1`` for every ``x``, and the thing
    that delivers it is ``ensure_ascii=True``. That is why this is a named function rather
    than a keyword remembered at each call site: on a line-delimited surface the flag is
    not a formatting preference, it is the whole of the escaping, and `ensure_ascii=False`
    reads like a harmless "keep the é" at every site where it is wrong.

    **What JSON escapes on its own is not the set of line breaks.** ``\\n``, ``\\r`` and the
    rest of the C0 controls are covered by the standard's own string rules and would
    survive `ensure_ascii=False` — but U+2028 LINE SEPARATOR, U+2029 PARAGRAPH SEPARATOR
    and U+0085 NEL are none of those. They pass through raw, and each one is a line break
    to `str.splitlines`, to a YAML 1.1 reader, and to a JavaScript parser before ES2019.
    That is #453 with a different spelling of "newline": the boundary that bounds a server
    NAME stops them, so the emission that is supposed to hold *on its own* was resting on
    it, and a committed VALUE — which no boundary bounds — added a physical line to a
    generated agent's frontmatter with no bypass needed at all.

    Escaping every non-ASCII codepoint answers for all three without naming any of them,
    and for the next codepoint some standard decides is a line break, because the answer
    does not depend on knowing which codepoints those are. The residue is ASCII, where the
    only line breaks are ``\\n`` and ``\\r`` and JSON already escapes both.

    Round-trips exactly — ``json.loads(json_line(x)) == x`` for anything JSON can carry,
    lone surrogates included, which `ensure_ascii=False` cannot even encode to UTF-8 to
    write out. Escaped, never dropped: the reader of the file charter wrote gets the value
    the committed file declared, spelled in a way that cannot restructure the file.

    Compare :func:`one_line`, which bounds a value being **displayed** and mangles it to do
    so. This one bounds a value being **serialised**, and preserves it exactly.
    """
    return json.dumps(obj, ensure_ascii=True, sort_keys=sort_keys)


# --------------------------------------------------------------------------- #
# the resolving layer — a PATH charter is about to read (#336)                 #
# --------------------------------------------------------------------------- #

class Refused(Exception):
    """A path charter declined to write to, carrying the sentence that says why.

    Deliberately **not** a `ValueError`. The write sites sit under `except ValueError`
    handlers that already mean "the text you passed was empty", and three more swallow it
    bare — inheriting from it would let a containment refusal be caught by a clause
    written about something else and disappear, which is the one outcome worse than the
    corruption this exists to stop.

    Caught once, in `cli.main`, beside `util.ProcTimeout` and for the same stated reason:
    *a child that outlived its budget is a condition, not a bug*. So is a committed file
    that redirects a write. Reaching the `except Exception` below it would file a crash
    report against charter for a defect in the plane's own data, and send the reader
    looking for the bug in the wrong repository.

    The refusal functions themselves still never raise — this is thrown by *callers* that
    have nothing useful to do with a refusal except stop.
    """


#: Said once, like :data:`NOT_A_SEGMENT`, and for the same reason: the reader has a defect
#: in a committed file. It names both ends because "refused" without the target is
#: unactionable — the whole point is that the path charter opened is not the path it read.
#:
#: ``{verb}`` is read/write rather than two near-identical constants: which operation was
#: redirected is the one word that differs, and it is the word that tells the reader
#: whether they are looking at a leak or at corruption.
NOT_PLANE_DATA = ("'{name}' resolves to '{target}', outside the directories a control "
                  "plane keeps its data in ({roots}). A committed symlink there redirects "
                  "the {verb}, so charter follows a link that lands inside them and "
                  "refuses one that leaves")

#: A path charter cannot examine at all (vanished mid-listing, a broken link, no
#: permission). Refused rather than raised — every caller here is on a path that must not
#: crash — and said in its own words so it is not read as a containment failure.
UNREADABLE = "'{name}' cannot be examined ({error})"

#: Not a file at all: a FIFO, a device, a socket, a directory. Named for what it is,
#: because "could not be read" would send the reader looking for a permissions problem
#: when what they have is a path that blocks for ever or yields for ever.
#:
#: A FIFO blocks a *writer* just as completely as a reader — ``open(fifo, "a")`` waits for
#: a reader to appear and never stops waiting — so this is one sentence for both sides,
#: and the write side has no `hooks.json` timeout above it to end the wait.
NOT_A_FILE = ("'{name}' is not a regular file (it is {kind}). Charter opens plane data at "
              "names a committed file can occupy, so an entry that blocks or never ends "
              "would take the {verb} with it")

#: The bound on one plane file charter reads whole. **1 MiB, and it is meant never to fire
#: on anything a human wrote**: the largest persona charter in charter's own plane is
#: 6.8 KB, its largest memory index 5 KB, its largest document under `docs/` 34 KB. The
#: number is not tuned to those — a cap sitting just above real content fires on the first
#: long runbook somebody curates — it is set where nothing an editor produces can reach it
#: and no single read can cost anything. `os.lstat` reports it in the syscall the
#: containment check already makes, so the bound is free.
MAX_BYTES = 1_048_576

TOO_LARGE = ("'{name}' is {size} bytes, over the {cap}-byte bound on one plane file. "
             "Nothing a memory, todo, ref or persona charter is meant to hold comes near "
             "that, so this is a defect in the file rather than a limit to raise")

#: `stat` module names for the shapes that are not files, so a refusal says which.
_KINDS = ((stat.S_ISDIR, "a directory"), (stat.S_ISFIFO, "a FIFO"),
          (stat.S_ISSOCK, "a socket"), (stat.S_ISCHR, "a character device"),
          (stat.S_ISBLK, "a block device"))


def data_roots() -> tuple[Path, ...]:
    """The directories a control plane keeps readable data in.

    Read from :mod:`charter.config` **at call time**, never captured at import: the test
    harness re-points the plane with `config.use`, and a root captured at import would
    quietly contain against the developer's real checkout.

    ``PERSONA_STATE_DIR`` is here because ephemeral memory is data charter is *supposed* to
    read, and it lives under the secrets home. That is the whole reason this is a list of
    data directories rather than "the plane, minus ``.charter/``".
    """
    from . import config
    return (config.PERSONAS_DIR, config.WORKSPACES_DIR, config.PERSONA_STATE_DIR)


def within_data(path) -> bool:
    """True when *path* **resolves** inside one of :func:`data_roots`.

    Both ends are resolved. The roots have to be too: on macOS a temp plane lives under
    ``/var/folders/…``, which is itself a link to ``/private/var/…``, so comparing a
    resolved path against an unresolved root refuses every read in the test harness and
    on any plane behind a linked mount. Resolving the roots is also what keeps a plane
    that *relocates* ``personas/`` or ``workspaces/`` behind a link working, which is the
    same legitimate case :func:`file_refusal` preserves one level down.

    **The fast path is one ``lstat``, and it is exact.** ``persona.load`` asks this of
    ``personas/<name>`` on every call, and four ``realpath``s (20µs) there doubled a
    status-line sweep. When a path's own parent *is* a data root and the path itself is
    not a link, it cannot have moved relative to that root — whatever the root resolves
    to, this resolves inside it — so the answer is already known. Anything else (a deeper
    base, a link, a lexical outsider) still pays the full resolve.
    """
    try:
        target = os.path.abspath(path)
        roots = [os.path.abspath(r) for r in data_roots()]
        if os.path.dirname(target) in roots and not stat.S_ISLNK(os.lstat(target).st_mode):
            return True
    except (OSError, ValueError):
        pass                                   # vanished or unreadable — ask the slow path
    try:
        target = os.path.realpath(path)
        for r in data_roots():
            root = os.path.realpath(r)
            # `+ os.sep` so a sibling named like a root ("personas-old") is not a child.
            if target == root or target.startswith(root + os.sep):
                return True
    except (OSError, ValueError):
        return False
    return False


def _not_plane_data(path, verb: str = "read") -> str:
    # The names go in as a SEQUENCE, not as a string this function joined: joining first
    # and containing second bounds the whole list at one budget, so the last root drops out
    # of a sentence whose job is to say which directories are the plane's. The join is
    # `path_sentence`'s (:data:`SEQUENCE_SEPARATOR`), and the rendering is unchanged.
    roots = sorted(Path(r).name for r in data_roots())
    try:
        target = os.path.realpath(path)
    except (OSError, ValueError):
        target = path            # unresolvable — say what was asked for, never raise here
    return path_sentence(NOT_PLANE_DATA, name=path, target=target, roots=roots, verb=verb)


#: Said once, like the two above. Names the resolved target because that is the whole
#: point: `"~/../../etc/charter-worktrees"` reads as a home-relative path and resolves to
#: `/private/etc/charter-worktrees`, and a reader who is only shown what they wrote cannot
#: see what they got.
NOT_PLANE_ADJACENT = ("'{name}' resolves to '{target}', which is neither inside the "
                      "control plane ({root}) nor beside it. This is read from a committed "
                      "file and directories get created there, so it may name a place "
                      "under the plane or a single sibling of it — '../charter.worktrees', "
                      "the documented shape — and nothing further afield")


def plane_adjacent(root, path) -> bool:
    """True when *path* is at/under *root*, or a **direct child** of *root*'s parent.

    A different boundary from :func:`within_data`, and deliberately so. That one answers
    "may charter READ this", and its roots are the directories a plane keeps data in.
    This one answers "may a committed file send charter's directory CREATION here", and
    the honest answer cannot be "inside the plane": relocating the worktree root exists
    precisely to get worktrees out of anywhere a build tool globs from, and the shape
    ``config.worktrees_root_for`` documents is ``"../charter.worktrees"`` — a sibling.

    So the boundary is the plane and its own doorstep. A direct child of the parent, not
    anything under the parent: a plane usually lives beside other checkouts, and "under
    the parent" would let a committed value plant a worktree root inside a colleague's
    repo. One sibling directory is the documented case and the whole of it.

    Never raises, like everything here — an unresolvable path is simply not adjacent.
    """
    try:
        target = Path(os.path.realpath(path))
        base = Path(os.path.realpath(root))
    except (OSError, ValueError):
        return False
    if target == base or base in target.parents:
        return True
    return target.parent == base.parent and target != base.parent


def plane_adjacent_refusal(root, declared) -> str | None:
    """Why a committed *declared* path may not be used as a root under *root*, or ``None``.

    Takes the value **as written** so the message can show both what was declared and what
    it resolved to — the gap between the two is usually the defect.
    """
    if not declared:
        return None
    p = Path(str(declared)).expanduser()
    p = p if p.is_absolute() else (Path(root) / p)
    if plane_adjacent(root, p):
        return None
    try:
        target = os.path.realpath(p)
    except (OSError, ValueError):
        target = str(p)
    return path_sentence(NOT_PLANE_ADJACENT, name=declared, target=target, root=root)


def dir_refusal(directory, verb: str = "read") -> str | None:
    """Why charter must not list — or write inside — *directory*, or ``None``.

    Separate from :func:`file_refusal` because a listing pays this **once** while paying
    the per-file check N times, and because it is what catches the variant the file check
    structurally cannot see: when the *directory* is the link, every file inside it is an
    ordinary regular file that no per-file check has anything to object to.

    That variant is worse on the write side, where charter creates the directory it is
    about to write into: ``mkdir(parents=True, exist_ok=True)`` accepts a symlink to an
    existing directory without complaint, so a committed link at ``memory/`` silently
    relocates every file written under it. Hence *verb* — the same question, asked before
    the ``mkdir`` rather than before the listing.
    """
    if not within_data(directory):
        return _not_plane_data(directory, verb)
    return None


def file_refusal(path) -> str | None:
    """Why charter must not read *path*, or ``None``.

    Three questions, **one syscall**, and that is why both halves of #336 close at the
    same gate. ``lstat`` says whether this is a link (containment), whether it is a
    regular file (a FIFO blocks the read for ever, a device never ends) and how big it is
    (the bound) — and it says all of it *without opening anything*, which is the property
    a deadline around the read could never have given: there is nothing to time out,
    because nothing is opened.

    Cheap on purpose. These run under the status line and SessionStart, where the read is
    already the cheapest thing in the frame and a guard costing more than it guards gets
    reverted the first time somebody profiles it. The expensive answer
    (:func:`within_data`, which resolves) is asked only when the entry **is** a link — a
    path that is not one cannot have moved relative to the directory it was listed from,
    which the caller checked once with :func:`dir_refusal`.

    Not a TOCTOU guard, and not sold as one: the attacker here holds a commit, not a
    process racing the read.
    """
    return _path_refusal(path, missing_ok=False, verb="read")


def write_refusal(path) -> str | None:
    """Why charter must not **write** to *path*, or ``None``.

    #348 gated every read of plane data and left the write side untouched, so a committed
    link at a name charter writes redirected the write instead of the read (#349). The
    same three questions apply — is this link contained, is it a file at all, is it a
    sane size — and each of them matters *more* here: an append to a credential store
    corrupts it where a read merely leaked it, ``write_text`` through a **dangling** link
    creates the target wherever it points, and ``open(fifo, "a")`` blocks for ever with a
    human sitting at the command rather than a hook timeout overhead.

    **One rule differs, and it is the whole reason this is a second function.** On a read,
    a path that is not there is a refusal. On a write it is the ordinary case — the file
    is about to be created — so ENOENT answers ``None``. Getting that backwards refuses
    every first write on a fresh plane, and getting it *too* right (treating "not there"
    as "nothing to check" before the link is resolved) misses the dangling-link case
    entirely, because ``exists()`` is false for exactly the link that is most dangerous.
    Both halves are held by tests that fail in opposite directions.

    **The parent is checked here, not by the caller.** On the read side that split is
    right — a listing pays :func:`dir_refusal` once and the per-file check N times. A
    write has no such loop, and the directory variant is the one a per-file check
    structurally cannot see: when ``memory/`` is the link, ``MEMORY.md`` inside it is an
    ordinary regular file with nothing to object to, and every ``mkdir(exist_ok=True)`` in
    charter accepts a symlink to a directory without complaint. Folding it in costs one
    resolve on a path nobody writes in a loop, and makes it impossible for the fifteenth
    caller to remember the file and forget the directory.

    What this is not: a check on whether the *value* being written belongs there. It
    answers where the bytes will land, nothing more.
    """
    return (dir_refusal(Path(path).parent, "write")
            or _path_refusal(path, missing_ok=True, verb="write"))


def writable(path) -> Path:
    """*path*, when charter may write there — else raise :class:`Refused`.

    The one place in this module that raises, and the exception is deliberate rather than
    an oversight in the "nothing here raises" promise above: that promise is about the
    *refusal* functions, which run under `doctor`, the status line and SessionStart and
    must return data. This is for callers on a command path, where a refusal has no
    sensible fallback — a read that refuses can skip the file, but a write that refuses
    and carries on leaves `charter persona remember` printing ✓ over a fact it never
    recorded, which is the same class of lie as the corruption being prevented.

    Callers that must *not* raise — the hook-driven tallies, which already promise a hook
    may never break a turn — ask :func:`write_refusal` directly and decline to write.
    """
    refusal = write_refusal(path)
    if refusal:
        raise Refused(refusal)
    return Path(path)


def _path_refusal(path, *, missing_ok: bool, verb: str) -> str | None:
    """The body :func:`file_refusal` and :func:`write_refusal` share.

    One implementation with the difference named, rather than two functions kept in step
    by hand — the divergence between two checks that read the same file and answered
    differently is what #328 and #342 were both about, and it is not a mistake worth
    making again inside the module that exists to stop it.
    """
    if not str(path):
        # ENOENT means "about to be created" only for a path that COULD be created, and
        # the empty path never can. Without this the write side answered "nothing to
        # object to" for it and handed the caller an unhandled `OSError` one line later —
        # a refusal turned back into a crash, which is the bug #348 shipped and fixed.
        return path_sentence(UNREADABLE, name=path, error="empty path")
    try:
        st = os.lstat(path)
    except FileNotFoundError:
        if missing_ok:
            return None       # about to be created — there is nothing there to object to
        return path_sentence(UNREADABLE, name=path, error="No such file or directory")
    except OSError as e:
        return path_sentence(UNREADABLE, name=path, error=e.strerror or e)
    except ValueError as e:
        # `os.lstat` raises ValueError, NOT OSError, on a path holding a NUL — the one
        # input shaped to get past a check (`segment_ok` refuses it for the same reason).
        # "Nothing here raises" is this module's promise; catching only OSError broke it.
        return path_sentence(UNREADABLE, name=path, error=e)
    if stat.S_ISLNK(st.st_mode):
        # Asked BEFORE `os.stat`, and that order is load-bearing on the write side: a
        # dangling link has no target to stat, so a containment check placed after the
        # stat would never run on the one link that can create a file out of nothing.
        if not within_data(path):
            return _not_plane_data(path, verb)
        try:
            # Follows the link. Still a `stat`, so a FIFO on the other end answers here
            # rather than blocking.
            st = os.stat(path)
        except FileNotFoundError:
            if missing_ok:
                return None   # a contained link naming a file charter is about to create
            return path_sentence(UNREADABLE, name=path, error="No such file or directory")
        except OSError as e:
            return path_sentence(UNREADABLE, name=path, error=e.strerror or e)
        except ValueError as e:
            return path_sentence(UNREADABLE, name=path, error=e)
    if not stat.S_ISREG(st.st_mode):
        kind = next((k for test, k in _KINDS if test(st.st_mode)), "not a file")
        return path_sentence(NOT_A_FILE, name=path, kind=kind, verb=verb)
    if st.st_size > MAX_BYTES:
        return path_sentence(TOO_LARGE, name=path, size=st.st_size, cap=MAX_BYTES)
    return None
