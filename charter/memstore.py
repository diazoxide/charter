"""A tiny per-file **memory store**: one markdown file per fact, plus a ``MEMORY.md``
index, in a directory. Keyword search, near-duplicate detection, and forget-by-slug — a
small, programmatically-explorable "DB" rather than one ever-growing file, so concurrent
writers don't conflict on a shared log.

Shared by **persona** memory (role knowledge) and **workspace** memory (task journal) so
both behave the same. A store can be ``timestamped`` — filenames get a
``YYYYMMDD-HHMMSS-`` prefix so a directory listing (and the index) orders chronologically;
that's the default for the workspace journal, where "when" matters. Persona memory keeps
its slug-only names (addressed by slug in ``forget``/``recall``).

Each memory file:  ``# <title>\\n\\n_<stamp> · <kind>_\\n\\n<body>\\n``
Never store secrets here — memory is committed + shared; secrets live only in the vault."""

from __future__ import annotations

import datetime
import re
from pathlib import Path

_SLUG_RE = re.compile(r"[^a-z0-9]+")


def slug(title: str) -> str:
    s = _SLUG_RE.sub("-", (title or "").lower()).strip("-")
    return s[:48] or "note"


def index_path(mem_dir: Path) -> Path:
    return mem_dir / "MEMORY.md"


def _now() -> datetime.datetime:
    return datetime.datetime.now()


def ensure_index(mem_dir: Path, header: str) -> Path:
    """Create the dir + MEMORY.md (with *header*) if missing; return the index path."""
    mem_dir.mkdir(parents=True, exist_ok=True)
    idx = index_path(mem_dir)
    if not idx.exists():
        idx.write_text(header if header.endswith("\n") else header + "\n")
    return idx


def write(mem_dir: Path, text: str, title: str | None = None, *, timestamped: bool = False,
          kind: str = "persistent", index: bool = True,
          stamp: datetime.datetime | None = None) -> Path:
    """Write one memory file into *mem_dir* and (unless ``index=False``) append it to the
    index. ``timestamped`` prefixes the filename with ``YYYYMMDD-HHMMSS-`` for chronological
    ordering. Returns the path. Raises ValueError on empty text — secrets must never pass here."""
    text = (text or "").strip()
    if not text:
        raise ValueError("empty memory")
    title = (title or text.splitlines()[0]).strip()[:72]
    mem_dir.mkdir(parents=True, exist_ok=True)
    now = stamp or _now()
    prefix = now.strftime("%Y%m%d-%H%M%S-") if timestamped else ""
    base = slug(title)
    p = mem_dir / f"{prefix}{base}.md"
    i = 2
    while p.exists():  # same title (and same second) → keep both
        p = mem_dir / f"{prefix}{base}-{i}.md"
        i += 1
    p.write_text(f"# {title}\n\n_{now.strftime('%Y-%m-%d %H:%M')} · {kind}_\n\n{text}\n")
    if index:
        index_append(index_path(mem_dir), p.name, title)
    return p


def index_append(idx: Path, filename: str, title: str) -> None:
    if not idx.exists():
        idx.parent.mkdir(parents=True, exist_ok=True)
        idx.write_text("# Memory Index\n\n")
    with idx.open("a") as f:
        f.write(f"- [{title}]({filename})\n")


_STAMP_RE = re.compile(r"^_(\d{4}-\d{2}-\d{2})[ T]", re.M)


def memory_date(text: str, filename: str = ""):
    """The date a memory was recorded — from its in-body ``_YYYY-MM-DD …_`` stamp (universal,
    written by :func:`write`), falling back to a ``YYYYMMDD-`` filename prefix. ``date`` or None."""
    import datetime
    m = _STAMP_RE.search(text or "")
    if m:
        try:
            return datetime.date.fromisoformat(m.group(1))
        except ValueError:
            pass
    fm = re.match(r"(\d{4})(\d{2})(\d{2})-", filename or "")
    if fm:
        try:
            return datetime.date(int(fm.group(1)), int(fm.group(2)), int(fm.group(3)))
        except ValueError:
            pass
    return None


def files(mem_dir: Path) -> list[Path]:
    """Every memory file in *mem_dir* (sorted — chronological when timestamped)."""
    if not mem_dir.exists():
        return []
    return sorted(p for p in mem_dir.glob("*.md") if p.name != "MEMORY.md")


#: A memory FILENAME is a bare slug — no slash, space or colon. Matching only that
#: shape keeps a title containing a URL-ish `](…md` fragment (API paths do) from being
#: mistaken for a listed file.
_INDEX_LINK_RE = re.compile(r"\(([A-Za-z0-9][\w.-]*\.md)\)")


def index_drift(mem_dir: Path) -> dict[str, list[str]]:
    """Compare MEMORY.md's links against the files actually present.

    Returns ``{"dangling": [...], "unindexed": [...]}`` — links pointing at a file
    that isn't there, and files no link points at. Both are reachable without any
    concurrency bug: MEMORY.md is append-heavy and edited by many agents and
    humans at once, so a merge resolved by taking one side drops the other's
    line while its file survives.

    Cheap by design (one read + one glob, no file contents): `doctor` calls this
    for every memory base, and runs from the SessionStart hook.
    """
    actual = {p.name for p in files(mem_dir)}
    idx = index_path(mem_dir)
    listed = set(_INDEX_LINK_RE.findall(idx.read_text())) if idx.exists() else set()
    return {"dangling": sorted(listed - actual), "unindexed": sorted(actual - listed)}


def index_size(mem_dir: Path) -> int:
    """How many memories a base holds — the growth signal `doctor` reports.

    Counts files, not index lines: the files are the truth, and a base mid-drift
    would otherwise report a number that disagrees with `index_drift`.
    """
    return len(files(mem_dir))


def entries(mem_dir: Path) -> list[tuple[Path, str, str]]:
    """(path, title, text) per memory; title = first '# ' line, else the file stem."""
    out = []
    for p in files(mem_dir):
        try:
            text = p.read_text()
        except OSError:
            continue
        title = p.stem
        for ln in text.splitlines():
            if ln.startswith("# "):
                title = ln[2:].strip()
                break
        out.append((p, title, text))
    return out


#: Words carrying no signal, dropped so they cannot outvote the term that mattered.
#: Scoring is raw `count()` with no IDF, so before this "for" in a long memory could beat
#: a single exact hit on the one word the user actually searched for.
_STOPWORDS = frozenset("""
a an the and or but if then than that this these those of in on at to for from by with
without as is are was were be been being it its it's do does did done have has had
you your we our i my me they them their he she his her not no yes so such can could
should would will shall may might must about into over under again more most some any
""".split())


def _terms(s: str) -> list[str]:
    """Query tokens worth scoring.

    The length floor is **2**, not 3. At 3 it silently discarded every two-character
    token, so `recall "S3"`, `"CI"`, `"db"`, `"PR"`, `"TZ"` and `"v2"` each returned a
    confident "No memories match" against a corpus that contained them — and search is
    the only route to memories beyond the ten titles the session briefing lists, so a
    false negative here is indistinguishable from the fact not existing.

    Single characters stay out: they match nearly everything and rank nothing.
    """
    return [t for t in re.split(r"\W+", (s or "").lower())
            if len(t) >= 2 and t not in _STOPWORDS]


def dropped_terms(s: str) -> list[str]:
    """Tokens in *s* that :func:`_terms` discarded. Lets a caller tell "nothing matched"
    from "nothing was searched for" — printing the first when it is really the second is
    how a query like `recall "in CI"` reported an empty corpus."""
    raw = [t for t in re.split(r"\W+", (s or "").lower()) if t]
    return [t for t in raw if len(t) < 2 or t in _STOPWORDS]


def search(dirs, query: str, limit: int = 8) -> list[tuple[Path, str, int]]:
    """Keyword-rank memories across one or more *dirs* against *query* (stdlib, no
    vectors). Title hits weigh 3×. Returns [(path, title, score)] best-first.

    Terms match on a word BOUNDARY (``\\bterm``) rather than as bare substrings, so
    "version" no longer scores inside "conversion" and a short token like "db" no longer
    hits every "dbname" in the corpus. Prefixes still count — "version" matches
    "versioned" — which is the cheap half of stemming without the maintenance of the
    other half.
    """
    terms = _terms(query)
    if not terms:
        return []
    pats = [(t, re.compile(rf"\b{re.escape(t)}\w*")) for t in terms]
    ents = []
    for d in dirs:
        ents += entries(d)
    scored = []
    for p, title, text in ents:
        low, tl = text.lower(), title.lower()
        score = sum(3 * len(pat.findall(tl)) + len(pat.findall(low)) for _t, pat in pats)
        if score:
            scored.append((score, str(p), p, title))
    scored.sort(key=lambda x: (-x[0], x[1]))
    return [(p, title, score) for score, _s, p, title in scored[:limit]]


def wordset(text: str) -> set[str]:
    """The comparable words in *text* — the tokenizer :func:`duplicates` scores with.

    Public because near-duplicate detection has a second caller (the todo store, which asks
    "would this new text duplicate an existing one?" rather than "which stored pairs
    duplicate each other?"). Both must tokenize identically or the shared threshold means
    two different things.
    """
    return {w for w in re.split(r"\W+", text.lower()) if len(w) > 3}


def duplicates(dirs, threshold: float = 0.5) -> list[tuple[float, Path, str, Path, str]]:
    """Near-duplicate memory pairs by Jaccard word overlap ≥ *threshold* across *dirs*."""
    ents = []
    for d in dirs:
        ents += entries(d)
    sets = [(p, title, wordset(text)) for p, title, text in ents]
    dupes = []
    for i in range(len(sets)):
        pa, ta, sa = sets[i]
        for j in range(i + 1, len(sets)):
            pb, tb, sb = sets[j]
            if not sa or not sb:
                continue
            jac = len(sa & sb) / len(sa | sb)
            if jac >= threshold:
                dupes.append((jac, pa, ta, pb, tb))
    dupes.sort(key=lambda x: -x[0])
    return dupes


def resolve(mem_dir: Path, ident: str) -> Path | None:
    """Find a memory file by full filename or slug — matching ``<slug>.md`` or, for a
    timestamped store, ``<prefix>-<slug>.md``. First match wins."""
    name = ident if ident.endswith(".md") else f"{ident}.md"
    p = mem_dir / name
    if p.exists():
        return p
    hits = [q for q in files(mem_dir) if q.name == name or q.name.endswith(f"-{name}")]
    return hits[0] if hits else None


def forget(mem_dir: Path, ident: str) -> bool:
    """Delete one memory (by filename or slug) and drop its index line. True if removed."""
    p = resolve(mem_dir, ident)
    if not p or not p.exists():
        return False
    p.unlink()
    _drop_index_line(mem_dir, p.name)
    return True


def _drop_index_line(mem_dir: Path, filename: str) -> None:
    idx = index_path(mem_dir)
    if idx.exists():
        keep = [ln for ln in idx.read_text().splitlines() if f"({filename})" not in ln]
        idx.write_text("\n".join(keep) + ("\n" if keep else ""))


def body(text: str) -> str:
    """A memory's content with the ``# title`` and ``_stamp_`` header lines stripped and
    whitespace normalized — the basis for EXACT-duplicate comparison (same fact, maybe
    recorded twice with different timestamps/titles)."""
    lines = (text or "").splitlines()
    kept = [ln for ln in lines
            if not ln.startswith("# ") and not re.match(r"^_.*·.*_\s*$", ln.strip())]
    return re.sub(r"\s+", " ", " ".join(kept)).strip().lower()


def archive(mem_dir: Path, ident: str) -> Path | None:
    """Move one memory into ``<mem_dir>/archive/`` (out of the active glob, so it drops
    from search/recall/stats) and remove its index line — a **reversible** retire that
    keeps the file on disk + in git history. Returns the new path, or None if not found."""
    p = resolve(mem_dir, ident)
    if not p or not p.exists():
        return None
    dest_dir = mem_dir / "archive"
    dest_dir.mkdir(parents=True, exist_ok=True)
    dest = dest_dir / p.name
    i = 2
    while dest.exists():
        dest = dest_dir / f"{dest.stem}-{i}{dest.suffix}"
        i += 1
    p.rename(dest)
    _drop_index_line(mem_dir, p.name)
    return dest
