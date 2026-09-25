//! What a version brought, and whether THIS plane has taken it up.
//!
//! `charter/news.py`, ported. A *news entry* is a shipped, per-item note that a version
//! introduced something, carrying an optional probe for whether this plane has adopted it. Not
//! a changelog: an entry exists to be **acted on**, and one with nothing to adopt is one line.
//!
//! The properties the Python module's docstring calls load-bearing are all here, and the
//! reason each of them exists is in that docstring rather than repeated in this one. What
//! follows is only what is DIFFERENT about the Rust charter, because a port's own docstring
//! earns its place by naming the seams.
//!
//! **The corpus is history, and it is frozen** (ADR 0045). Every entry in it is a note the
//! Python charter published about one of its own releases, 0.44.0 to 0.62.1, plus the few it had
//! staged when it stopped. None of it is news about this app — the app's own release notes are
//! `CHANGELOG.md` — and nothing is added to it. It stays compiled in so `charter news` can still
//! answer what a plane pinned on the Python line skipped, read-only.
//!
//! **It ships in the binary, and the binary is the only copy.** Python resolves entries
//! packaged-copy-first with a checkout fallback; a charter that ships inside a signed app has
//! no checkout to fall back to, so `news/` is compiled in by `build.rs` and there is no second
//! source. The one risk left is that this copy drifts from what the Python charter published;
//! the differential suite checks that: `news --for <version>` is compared byte for byte against
//! the pinned Python charter, so a corpus that drifts from the oracle turns those scenarios red.
//!
//! **`--until` defaults to the newest version the corpus names, not to this binary's version**
//! — see [`history_ends`]. This is the one behavioural difference from Python in the whole
//! module and it is forced: this binary carries the app's version, which counts a different
//! line from the corpus (ADR 0045). Defaulting to it would make every range empty.
//!
//! **Every SHIPPED probe reports unchecked here, and that is a fact about this CLI rather than
//! a gap in the port.** `check:` may only name a command from [`PROBEABLE`], and of those four
//! this binary has two — `news` and `doctor` (M2.4). Every entry in the corpus names one of the
//! other two, `persona lint` or `frame-probe`, so every one of them reports *unchecked* — which
//! is the answer `charter/news.py` prescribes for a command this CLI does not register, not one
//! invented for the occasion. The machinery around it is ported whole, because the day `persona
//! lint` lands the entries start answering and the guards have to already be right.
//!
//! **One of those two lands on a sentence written for something else, and it is reported rather
//! than papered over.** [`dispatch`] tells a first token this charter does not register
//! ([`NOT_RUN`] — a fact about this machine) from a command a `check:` may not name
//! ([`UNLISTED`] — a defect in the entry). `frame-probe` is the first. `persona lint` is
//! neither, quite: this binary HAS `persona` and has no `lint` under it, so [`command_path`]
//! stops at `("persona",)`, which is not on [`PROBEABLE`], and a correct entry is told it named
//! something a probe may not run. `charter/news.py`'s `_command_path` does exactly the same
//! thing; charter's own CLI simply has `persona lint`, so it never lands there. A fourth reason
//! written here and nowhere in charter would be a fork between the two implementations, in a
//! sentence no differential scenario can reach — so the port keeps charter's three.
//!
//! **The re-entrancy marker is read and not written.** `news._ENV` travels in the environment
//! so that a charter started underneath a probe declines to probe. Setting an environment
//! variable is `unsafe` under Rust 2024 and this workspace forbids `unsafe_code`, so
//! [`probing`] READS the marker — a Rust charter spawned by a Python charter's probe still
//! declines, and still leaves the refusal mark that keeps the outer answer honest — and
//! nothing here writes one. Nothing needs to yet: neither probeable command this binary has —
//! `news` and `doctor` — starts a charter of its own. The day one does, this line has to move.

use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::scaffold::Say;
use crate::shown::{self, Slot};
use crate::version::version_key;

include!(concat!(env!("OUT_DIR"), "/news_entries.rs"));

/// The version a staged entry carries until a release stamps it.
///
/// A feature PR cannot know which version will ship it — the next release may be a patch, or
/// the PR may sit through three of them — so it does not guess. Until a stamp moves it, the
/// entry is invisible to every user-facing view: an entry naming a version that was never true
/// is the one failure staging exists to prevent.
pub const UNRELEASED: &str = "unreleased";

/// Has this plane taken an entry up?
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// The probe ran and said yes.
    Adopted,
    /// The probe ran and said no.
    Pending,
    /// The probe did not run, or ran and answered a different question. **Not** "adopted" and
    /// **not** "pending": reporting it as pending invents work, and reporting it as adopted
    /// hides the entry forever (ADR 0013 — the absence of information is not evidence of
    /// health).
    Unknown,
    /// The entry carries no `check:` at all, so there is nothing to ask.
    Informational,
}

/// The two opt-in ordering fields, and the only two.
///
/// `security:` is a CLASS — a version may ship any number of security entries, and they sort
/// above everything else. `lead:` is a POSITION, so at most one entry per version may claim
/// it. Split rather than collapsed into one `rank: <int>` because only one of the two can be
/// answered by an author working alone: "is this a security fix?" is a fact about the entry;
/// "does this go first?" is a fact about the RELEASE, which 24 authors staging for 0.52.0
/// could not each see.
pub const LEAD: &str = "lead";
/// See [`LEAD`].
pub const SECURITY: &str = "security";
const ORDERING_FIELDS: [&str; 2] = [LEAD, SECURITY];

/// Every frontmatter key a news entry may declare, and a CLOSED set: a key outside it is
/// reported by [`entry_errors`] rather than read past.
///
/// Closed because the alternative failure is silent. The frontmatter reader keeps a key
/// exactly as written, so `Security: true` is the key `Security`, the lookup for `security`
/// finds nothing, and the entry declares a security fix, sorts as though it declared nothing,
/// and leaves `charter news --for` exiting 0 with an empty stderr (charter #503).
///
/// **Loud rather than liberal, and the case is why.** Folding the lookup to lower case answers
/// `Security:` and nothing else: `securiy:`, `leads:` and `sec urity:` all parse cleanly, are
/// never looked up, and sink the entry in exactly the same silence.
///
/// The order is the order the sentences quote it in, so it is the order of the tuple in
/// `charter/news.py` and not sorted.
pub const KNOWN_FIELDS: [&str; 6] = ["version", "headline", "check", "adopt", LEAD, SECURITY];

/// The text fields, derived from [`KNOWN_FIELDS`] rather than listed again.
///
/// The two ordering fields are excluded because they are not text: `lead: 'true'` is already a
/// value [`flag`] cannot read and [`entry_errors`] already names it, and reporting it twice in
/// two vocabularies would send an author looking for two mistakes.
fn text_fields() -> Vec<&'static str> {
    KNOWN_FIELDS
        .iter()
        .copied()
        .filter(|f| !ORDERING_FIELDS.contains(f))
        .collect()
}

/// One news entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub version: String,
    pub slug: String,
    pub headline: String,
    pub check: String,
    pub adopt: String,
    pub body: String,
    /// The committed filename. Python carries a `Path`; there is no path here, and every
    /// reader of it wanted `path.name`.
    pub name: String,
    /// Declared position and class. Both default false — 24 entries do not each need a
    /// number, and an entry that says nothing sorts exactly where it always did.
    pub lead: bool,
    pub security: bool,
    /// `(field, raw)` for every ordering field whose value was not understood. Carried rather
    /// than raised, and rather than silently read as false, because false is the answer that
    /// SINKS the entry.
    pub bad: Vec<(&'static str, String)>,
    /// Frontmatter keys this entry declared that charter does not read, in the order the file
    /// wrote them. Beside `bad` and for the same reason, one half of the declaration each.
    pub unknown: Vec<String>,
}

// ------------------------------------------------------------------------------------------
// Reading an entry
// ------------------------------------------------------------------------------------------

/// `persona._frontmatter`: the frontmatter as `(key, value)` **in file order**, plus the body.
///
/// **The pairs come from [`crate::personas::frontmatter`]**, which is already that function,
/// rather than from a second copy of it here. A news entry and a persona charter are the same
/// format read by the same parser in Python, and two Rust readings of one format is how they
/// start disagreeing about a file — which this repository can already demonstrate: a THIRD
/// reading, `personas::frontmatter_value`, returns `None` at the first line without a colon
/// where this one skips it, so a persona with a blank line above `role:` has no role there and
/// has one here (charter-app #67).
///
/// What is here is the BODY, which that function does not return and a news entry needs: the
/// text after the closing fence. The split is `charter/persona.py`'s, on the first two `---`
/// **substrings** rather than on lines, so an entry whose body holds a `---` is divided exactly
/// where charter divides it.
fn frontmatter(text: &str) -> (Vec<(String, String)>, String) {
    let pairs = crate::personas::frontmatter(text);
    // `text.split("---", 2)`: at most two splits, so three parts, and the third keeps every
    // later `---` it holds. With fewer than three parts there is no frontmatter and the body is
    // the whole text — which is what `persona._frontmatter` returns for that case too.
    let body = text
        .strip_prefix("---")
        .and_then(|rest| rest.find("---").map(|cut| &rest[cut + 3..]))
        .unwrap_or(text);
    (pairs, py_strip(body).to_owned())
}

fn py_strip(text: &str) -> &str {
    crate::memstore::py_strip(text)
}

/// `dict(pairs)`: the LAST value written for a key, at the FIRST position it was written.
///
/// Python collapses the pairs into a dict, which keeps insertion order and overwrites in
/// place. Both halves matter: the value decides what is read, and the position decides where
/// an unknown key appears in the report an author is reading for the line they typed.
fn collapse(pairs: &[(String, String)]) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::with_capacity(pairs.len());
    for (key, value) in pairs {
        match out.iter_mut().find(|(k, _)| k == key) {
            Some(slot) => slot.1 = value.clone(),
            None => out.push((key.clone(), value.clone())),
        }
    }
    out
}

fn get<'a>(meta: &'a [(String, String)], key: &str) -> Option<&'a str> {
    meta.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
}

/// One entry, or `None` when the file is not one.
fn read(name: &str, text: &str) -> Option<Entry> {
    let (pairs, body) = frontmatter(text);
    let meta = collapse(&pairs);
    let version = py_strip(get(&meta, "version").unwrap_or("")).to_owned();
    if version.is_empty() {
        return None;
    }
    // The slug is the filename's, the version is the frontmatter's. Two sources for one fact
    // would drift the moment `news stamp` renamed a file and missed the field.
    let stem = name.strip_suffix(".md").unwrap_or(name);
    let slug = match stem.split_once('-') {
        Some((_, rest)) => rest.to_owned(),
        None => stem.to_owned(),
    };
    let mut flags = [false; 2];
    let mut bad: Vec<(&'static str, String)> = Vec::new();
    for (i, &field) in ORDERING_FIELDS.iter().enumerate() {
        // Present-and-empty is a different fact from absent, and `.get(…) or ""` is the line
        // that made them one: a `security:` whose value went onto the continuation line is a
        // declaration charter could not read, not a declaration that was never made.
        let raw = get(&meta, field).map(py_strip);
        match flag(raw) {
            Some(value) => flags[i] = value,
            None => bad.push((field, raw.unwrap_or("").to_owned())),
        }
    }
    let unknown = meta
        .iter()
        .map(|(k, _)| k.clone())
        .filter(|k| !KNOWN_FIELDS.contains(&k.as_str()))
        .collect();
    Some(Entry {
        version,
        slug,
        headline: py_strip(get(&meta, "headline").unwrap_or("")).to_owned(),
        check: py_strip(get(&meta, "check").unwrap_or("")).to_owned(),
        adopt: py_strip(get(&meta, "adopt").unwrap_or("")).to_owned(),
        body,
        name: name.to_owned(),
        lead: flags[0],
        security: flags[1],
        bad,
        unknown,
    })
}

/// An ordering field's declared value: `Some(true)`, `Some(false)`, or `None` for "not
/// understood".
///
/// `raw == None` means **the key is not in the frontmatter at all**, and that is false — the
/// whole of what opt-in means. Absence is not a value, so it is spelled as the absence of one;
/// every string reaching here, `""` included, is something an author typed.
///
/// **Empty is a declaration charter could not read, not a declaration never made.** charter's
/// frontmatter is flat `key: value` and drops any line without a colon, so the YAML habit of
/// putting a value on the continuation line leaves `security` present with nothing after it.
/// Folded into "absent", that entry declares a security fix, renders below the ordinary ones,
/// and the release gate exits 0 with an empty stderr (charter #486).
///
/// Present-but-unrecognised is `None`, not false, and that distinction is the point of this
/// function. The obvious spelling — a truthy set `{true, yes, 1, on}` — fails the way every
/// other guard of that shape has: the next author writes `security: Y`, or a full-width
/// `ｔｒｕｅ`, and the entry sinks silently. So the property is not "which words mean yes" but
/// **"was this value understood?"**.
fn flag(raw: Option<&str>) -> Option<bool> {
    let Some(raw) = raw else { return Some(false) };
    let folded = py_strip(raw).to_lowercase();
    match folded.as_str() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

/// The two characters a YAML habit wraps a scalar in.
///
/// A backtick is deliberately not one: a headline that is entirely one code span opens and
/// closes with a backtick and is *right* to, because that pair renders as markdown in the
/// Release body rather than as itself. These two render as themselves.
const QUOTES: [char; 2] = ['\'', '"'];

/// The quote character `value` is wrapped in, or `None`.
///
/// **charter's frontmatter is not YAML, and this is the question that says so.** It opens and
/// closes with `---`, so an author who knows YAML is *correct* to quote a headline that starts
/// with a backtick — a reserved indicator there. The reader is flat `key: value` and takes
/// everything after the first colon verbatim, so both quotes are part of the value, and the
/// value is what `render_body` puts after `### ` in a published Release (charter #902).
///
/// The rule is the whole of the matched pair and nothing narrower, and the corpus is why. The
/// tempting refinement — "and the same quote does not appear inside" — models YAML's own
/// single-quoted scalar, and one of the six headlines 0.56.0 published is exactly that, so the
/// narrower rule would miss a sixth of the entries this exists for while looking more precise.
pub fn quoted(value: &str) -> Option<char> {
    let v: Vec<char> = py_strip(value).chars().collect();
    match (v.first(), v.last()) {
        (Some(&first), Some(&last)) if v.len() >= 2 && QUOTES.contains(&first) && last == first => {
            Some(first)
        }
        _ => None,
    }
}

/// Where `e` sits among its own version's entries: 0 leads, 1 is a security fix, 2 is
/// everything else. Lower first.
pub fn rank(e: &Entry) -> u8 {
    if e.lead {
        0
    } else if e.security {
        1
    } else {
        2
    }
}

/// The word that precedes a security entry's headline, everywhere a headline is rendered.
///
/// One function rather than a literal at each call site: the Release body and the offline
/// `charter news` view are deliberately the same answer printed twice, and two copies of the
/// label is how they start disagreeing. `lead:` gets no marker — it is a position, not a kind,
/// and "this was listed first" is already visible from being listed first.
pub fn marker(e: &Entry) -> &'static str {
    if e.security { "security: " } else { "" }
}

// ------------------------------------------------------------------------------------------
// The corpus
// ------------------------------------------------------------------------------------------

/// Every entry that parses, oldest version first; staged entries last, and within one version:
/// the entry that declared `lead:`, then security fixes, then the rest.
///
/// **This sort is the whole of charter #486.** Both public views of an entry come through here
/// — the Release body via [`render_body`] and `charter news` via [`between`] — so an ordering
/// honoured by one is honoured by the other by construction rather than by two call sites
/// agreeing to sort the same way.
///
/// Within a rank the order is still the filename's, and the sort is STABLE, so an entry that
/// declares nothing lands exactly where it landed before any of this existed.
pub fn all() -> Vec<Entry> {
    let mut found: Vec<Entry> = FILES
        .iter()
        .filter_map(|(name, text)| read(name, text))
        .collect();
    found.sort_by_key(|e| (e.version == UNRELEASED, version_key(&e.version), rank(e)));
    found
}

/// Every entry that names a published version.
pub fn released() -> Vec<Entry> {
    all()
        .into_iter()
        .filter(|e| e.version != UNRELEASED)
        .collect()
}

/// The newest version the frozen corpus names — where the Python charter's line ended, and
/// `--until`'s default.
///
/// **Not this charter's version, and never compared with a pin as if it were** (ADR 0045,
/// amending ADR 0030, which used it for exactly that). This charter's version is the app's —
/// `adopt::app_version` — and it counts a different line. The range views default to this one
/// because the corpus is history: `--until` defaulting to the app's version would make every
/// range empty and `charter news` would answer "nothing new" forever.
///
/// Empty when the corpus holds no released entry, which `build.rs` makes impossible.
pub fn history_ends() -> String {
    released()
        .last()
        .map(|e| e.version.clone())
        .unwrap_or_default()
}

/// Entries newer than `lo`, up to and including `hi`.
///
/// Exclusive at the bottom because `lo` is where you already were: you have seen it.
pub fn between(lo: &str, hi: &str) -> Vec<Entry> {
    let (low, high) = (version_key(lo), version_key(hi));
    // Python catches an exception here; this one cannot raise, and a bound that is not a
    // version keys as NOT_A_VERSION — below everything — which makes `low` admit every entry
    // and `high` admit none. An empty range for an unreadable `--until` is the same answer
    // Python's `except: return []` gives.
    released()
        .into_iter()
        .filter(|e| {
            let k = version_key(&e.version);
            k > low && k <= high
        })
        .collect()
}

/// One version's entries, in [`all`]'s order.
pub fn for_version(version: &str) -> Vec<Entry> {
    all().into_iter().filter(|e| e.version == version).collect()
}

// ------------------------------------------------------------------------------------------
// What an entry declares that charter cannot honour
// ------------------------------------------------------------------------------------------

/// An ordering field whose value charter could not read, quoted back to its author.
const BAD_VALUE: &str = "{name}: `{field}: {raw}` is not a value charter reads — a news \
                         ordering field is `true` or `false`. Left unread, this entry sorts \
                         as though it never declared anything.";

/// The same field declared with nothing after the colon. Distinct from [`BAD_VALUE`] because
/// the fix is different: there is no value to correct, and quoting one back would read like a
/// rendering bug. Name the shape that produces it instead.
const EMPTY_VALUE: &str = "{name}: `{field}:` is declared with no value on that line — a news \
                           ordering field is `true` or `false`, written after the colon. \
                           charter's frontmatter is flat `key: value` and drops a line without \
                           a colon, so a value indented onto the NEXT line never reaches \
                           charter. Left unread, this entry sorts as though it never declared \
                           anything.";

/// A key that is one of charter's fields in another case. Its own sentence, because the author
/// has already written the right word and needs to be told only that the key is matched
/// exactly — pointing them at the whole list would make them hunt for a difference that is not
/// there.
const MISCASED_KEY: &str = "{name}: `{key}:` differs from `{known}:` only in case, and charter \
                            matches a frontmatter key exactly — so this entry declared no \
                            `{known}:` at all and was read as though the line were not there. \
                            Spell the key `{known}:`.";

/// Any other key charter does not read. `{fields}` is [`KNOWN_FIELDS`], built from the set
/// rather than written out, so the sentence cannot drift from what it describes.
const UNKNOWN_KEY: &str = "{name}: `{key}:` is not a field a news entry declares, so nothing \
                           read it. A news entry declares {fields}, matched exactly — a near \
                           miss parses like anything else, is looked up by nothing, and would \
                           otherwise sink this entry without a word.";

/// Two entries in one version both claiming the position only one of them can have.
const TWO_LEADS: &str = "{version}: {count} entries declare `lead: true` ({names}) — only one \
                         entry can be the one a reader sees first. Leave `lead:` off all but \
                         one; a security fix that need not be first can say `security: true` \
                         instead, which any number of entries may.";

fn fields_said() -> String {
    KNOWN_FIELDS
        .iter()
        .map(|f| format!("`{f}:`"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// What `entries` declare that charter cannot honour, as sentences. Empty is the ordinary
/// answer.
///
/// Three kinds of failure, none of them resolved quietly: an ordering field whose value was
/// not understood; a key outside [`KNOWN_FIELDS`], which is the quietest of the three because
/// an unfound key leaves no value to report; and two entries in one version both declaring
/// `lead: true`, where picking silently would hand back the accident charter #486 diagnosed.
///
/// Callers rather than this function decide the consequence: `news --for`, which IS the
/// release workflow's publish gate, refuses; the range view warns and prints on.
///
/// Every sentence is assembled through [`shown::sentence`], so a committed filename in one
/// cannot write a second line into the report it appears in (charter #502).
pub fn entry_errors(entries: &[Entry]) -> Vec<String> {
    let mut out = Vec::new();
    let mut by_name: Vec<&Entry> = entries.iter().collect();
    by_name.sort_by(|a, b| a.name.cmp(&b.name));
    for e in &by_name {
        for (field, raw) in &e.bad {
            let template = if raw.is_empty() {
                EMPTY_VALUE
            } else {
                BAD_VALUE
            };
            out.push(shown::sentence(
                template,
                &[
                    ("name", e.name.as_str().into()),
                    ("field", (*field).into()),
                    ("raw", raw.as_str().into()),
                ],
            ));
        }
        for key in &e.unknown {
            let folded = key.to_lowercase();
            match KNOWN_FIELDS.iter().find(|f| **f == folded.as_str()) {
                Some(known) => out.push(shown::sentence(
                    MISCASED_KEY,
                    &[
                        ("name", e.name.as_str().into()),
                        ("key", key.as_str().into()),
                        ("known", (*known).into()),
                    ],
                )),
                None => out.push(shown::sentence(
                    UNKNOWN_KEY,
                    &[
                        ("name", e.name.as_str().into()),
                        ("key", key.as_str().into()),
                        ("fields", fields_said().into()),
                    ],
                )),
            }
        }
    }
    // In the entries' own order, then sorted by version string, exactly as Python builds and
    // walks its dict.
    let mut leads: Vec<(String, Vec<&Entry>)> = Vec::new();
    for e in entries.iter().filter(|e| e.lead) {
        match leads.iter_mut().find(|(v, _)| *v == e.version) {
            Some(slot) => slot.1.push(e),
            None => leads.push((e.version.clone(), vec![e])),
        }
    }
    leads.sort_by(|a, b| a.0.cmp(&b.0));
    for (version, claimants) in leads {
        if claimants.len() > 1 {
            let mut names: Vec<String> = claimants.iter().map(|e| e.name.clone()).collect();
            names.sort();
            out.push(shown::sentence(
                TWO_LEADS,
                &[
                    ("version", version.as_str().into()),
                    ("count", claimants.len().into()),
                    ("names", Slot::Many(names)),
                ],
            ));
        }
    }
    out
}

/// A value the file wrapped in quotes and charter did not unwrap.
///
/// Names the character, the field and the file, and not the value: the author already knows
/// what they typed, the mistake is at both ends of it, and a 150-character headline quoted back
/// and clipped at the display limit would hide the two characters this sentence is about.
const QUOTED_VALUE: &str = "{name}: `{field}:` opens and closes with {quote} and charter kept \
                            both. This frontmatter is not YAML — it is flat `key: value`, and \
                            everything after the first colon is the value — so {quote}…{quote} \
                            publishes as {quote}…{quote}, quotes and all, in the GitHub \
                            Release and in `charter news` alike. Write the value unquoted; a \
                            backtick needs no quoting here, and a value that must really begin \
                            and end with {quote} has to be reworded, because a flat format has \
                            no way to say which pair is yours.";

/// Every value in `entries` that was written wrapped in quotes, as sentences.
///
/// **Its own function rather than a fourth branch of [`entry_errors`], and the split is not
/// cosmetic.** That function reports what an entry declared that charter *cannot honour*. A
/// quoted headline is honoured perfectly: charter reads it, renders it, and ships exactly the
/// bytes the file holds. What went wrong is upstream of charter, in what the author believed
/// the format was. Folding the two together would put this in the range view as well, where
/// every reader catching up across 0.56.0 would be scolded, forever, about six files that
/// cannot be corrected without forking the repo's copy from the Release published from it.
pub fn quoted_values(entries: &[Entry]) -> Vec<String> {
    let mut out = Vec::new();
    let mut by_name: Vec<&Entry> = entries.iter().collect();
    by_name.sort_by(|a, b| a.name.cmp(&b.name));
    for e in by_name {
        for field in text_fields() {
            let value = match field {
                "version" => &e.version,
                "headline" => &e.headline,
                "check" => &e.check,
                "adopt" => &e.adopt,
                other => unreachable!("{other} is not a text field"),
            };
            if let Some(mark) = quoted(value) {
                out.push(shown::sentence(
                    QUOTED_VALUE,
                    &[
                        ("name", e.name.as_str().into()),
                        ("field", field.into()),
                        ("quote", mark.to_string().into()),
                    ],
                ));
            }
        }
    }
    out
}

const NO_FRONTMATTER: &str = "{name}: no `key: value` frontmatter, so charter reads it as no \
                              entry at all. Every file in the news directory is an entry — \
                              give it `version:` and `headline:`, or move it out of the \
                              directory.";
const MISCASED_VERSION: &str = "{name}: `{key}:` differs from `version:` only in case, and \
                                charter matches a frontmatter key exactly — so this file names \
                                no version and is in no release, in the offline view or the \
                                published notes. Spell the key `version:`.";
const NO_VERSION_HEAD: &str = "{name}: no `version:` charter could read in its frontmatter, so \
                               this file is in no release, in the offline view or the \
                               published notes. A staged entry writes `version: ";
const NO_VERSION_TAIL: &str = "` until `charter news stamp` moves it.";

/// For a decline this version cannot explain. It says so rather than guessing at one of the
/// three above, because a file reported with a reason that is not the reason is worse than a
/// file reported with none: the reader makes the edit it names, nothing changes, and the
/// sentence has spent its credibility.
const NOT_AN_ENTRY: &str = "{name}: charter reads this file as no entry at all, for a reason \
                            this version has no sentence for. It is in the news directory and \
                            in no release.";

/// Why `name` produced no [`Entry`], as a sentence — best effort, always something.
///
/// The SET this explains comes from [`read`] returning `None`; only the wording is here. That
/// split is deliberate: a further way for `read` to decline would otherwise have to be
/// remembered in two places, and the one that gets forgotten is this one.
///
/// **Python has a fourth sentence here and this has three.** `_UNREADABLE_FILE` covers an
/// `OSError` or a `UnicodeDecodeError` from reading the file off disk. An entry compiled in as
/// a `&'static str` cannot fail either way — `include_str!` refuses non-UTF-8 at build time —
/// so the branch is not ported rather than ported dead. The day entries are read from disk
/// again, it comes back with them.
fn not_an_entry(name: &str, text: &str) -> String {
    let meta = collapse(&frontmatter(text).0);
    if meta.is_empty() {
        return shown::sentence(NO_FRONTMATTER, &[("name", name.into())]);
    }
    if let Some((key, _)) = meta
        .iter()
        .find(|(k, _)| k != "version" && k.to_lowercase() == "version")
    {
        return shown::sentence(
            MISCASED_VERSION,
            &[("name", name.into()), ("key", key.as_str().into())],
        );
    }
    if py_strip(get(&meta, "version").unwrap_or("")).is_empty() {
        let template = format!("{NO_VERSION_HEAD}{UNRELEASED}{NO_VERSION_TAIL}");
        return shown::sentence(&template, &[("name", name.into())]);
    }
    shown::sentence(NOT_AN_ENTRY, &[("name", name.into())])
}

/// Every file in the news directory that is not an entry, as sentences.
///
/// The sibling of [`entry_errors`], and it exists because the loudest version of charter
/// #503's defect is the one that never reaches that function. `Security: true` sinks an entry;
/// `Version: 0.60.0` **deletes** it — [`read`] finds no `version` and returns `None`, so the
/// file is dropped before any consumer sees it, and there is no entry to have an opinion
/// about. The release guard does not catch it either: it answers from FILENAMES, so a file
/// named `0.60.0-fix.md` satisfies "every published version ships an entry" while rendering
/// into neither the Release body nor `charter news`.
pub fn unreadable() -> Vec<String> {
    FILES
        .iter()
        .filter(|(name, text)| read(name, text).is_none())
        .map(|(name, text)| not_an_entry(name, text))
        .collect()
}

// ------------------------------------------------------------------------------------------
// Rendering a release body
// ------------------------------------------------------------------------------------------

/// The repository a rendered note links into, and the ref a staged one points at.
///
/// **History, not a dependency** (ADR 0045). Every entry here is a note the Python charter
/// published, and each is linked by tag (`v0.62.0`) to the note **as that release shipped it**
/// in the repository that published it — so the links are where the history is, and
/// repointing them at this repository would make every one of them 404. Nothing in this app
/// is published from that repository, and no new entry is written against it.
///
/// A staged entry was never released, so it has no tag, and it is linked at that repository's
/// branch exactly as the Python charter links it — the render is a preview nobody publishes, and
/// the differential compares it byte for byte with the oracle's.
pub const HISTORY_REPO: &str = "diazoxide/charter-plane";
/// See [`HISTORY_REPO`].
pub const HISTORY_BRANCH: &str = "main";

/// The most characters GitHub's create-release API accepts in a body.
///
/// **Its number, not charter's**: `POST /repos/{owner}/{repo}/releases` refuses a longer one
/// with `body is too long (maximum is 125000 characters)`, and `gh release create` forwards
/// that refusal rather than trimming to fit — so a version whose notes render past it does not
/// publish shorter notes, it publishes **none**.
pub const RELEASE_BODY_MAX: usize = 125_000;

/// How much of [`RELEASE_BODY_MAX`] [`render_body`] will actually spend. **charter's number,
/// and the 3,000 it leaves unspent is what the number is about**: something added to the body
/// that charter did not render — a preamble, a footer, a wrapper added to the publishing step
/// — landing on a body already at this budget, after an upload that cannot be undone.
///
/// Written as a subtraction a reader can check rather than as a fraction of the limit: a
/// fraction re-derives itself whenever the limit moves and never says what the remainder is
/// *for*, which is how a 15% reserve came to be guarding a 1% hazard (charter #878).
const BODY_BUDGET: usize = 122_000;

/// How long `body` is by the more conservative of the two measures GitHub might apply.
///
/// The encoded length, which for UTF-8 is never below the character count and is above it for
/// every entry charter has published — so a body under this number is under
/// [`RELEASE_BODY_MAX`] whichever unit the API's validator is actually counting.
///
/// **Measuring code points was the alternative and it was a guess.** The refusal says
/// "characters", which is evidence and not proof: the word in an error message is not the
/// expression that produced it, and the one place the difference shows up is a body within 1%
/// of the limit — i.e. exactly the bodies this exists for. Across every version charter has
/// stamped, the encoded body runs between 0.25% and 1.22% longer, so satisfying both readings
/// spends about a thousand characters of a 125,000-character allowance.
pub fn sent_length(body: &str) -> usize {
    body.len()
}

/// One entry rendered whole: its headline, its label, and the body its author wrote.
fn part(e: &Entry) -> String {
    py_rstrip(&format!("### {}{}\n\n{}", marker(e), e.headline, e.body)).to_owned()
}

fn py_rstrip(text: &str) -> &str {
    text.trim_end_matches(crate::memstore::is_python_space)
}

/// `e`'s path in the repository — the name a reader with a checkout opens.
///
/// The filename is a committed value crossing into a document with structure, which is charter
/// #502 one surface over: a name holding a newline would forge a heading in the release notes,
/// and a name holding `](https://…)` would forge a *link* in charter's own sentence, reading
/// exactly as much like charter's text as the sentence around it. The body beside it is
/// deliberately not treated this way — an entry's body IS Markdown its author wrote — but this
/// line is charter's, and the filename is a field in it.
///
/// **Percent-encoding rather than [`shown::one_line`].** That answers "can this forge a line of
/// a report?", which is not the question a Markdown document asks: it leaves `]`, `(` and a
/// backtick untouched, and it clips at 160 characters — and a clipped path is one the reader
/// cannot act on. This escapes every one of them, for every character, and never clips. It is
/// also the *same* string [`entry_url`] puts after the host, so the path a reader is shown and
/// the path the link goes to cannot come out different.
fn entry_file(e: &Entry) -> String {
    format!("docs/news/{}", quote_path(&e.name))
}

/// `urllib.parse.quote(string)` with its default `safe="/"`.
///
/// Unreserved is `A-Za-z0-9_.-~`, `/` is safe, and every other BYTE of the UTF-8 encoding
/// becomes `%XX` in upper-case hex.
fn quote_path(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for byte in name.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-' | b'~' | b'/') {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// Where `e` can be read in full, on the web.
///
/// The ref is the tag for a stamped version and the tracked branch for a staged one. A tag
/// never moves, so a link in a published release body keeps pointing at the note **as that
/// release shipped it** rather than at whatever `main` later made of it; `unreleased` has no
/// tag to point at, and its render is a preview nobody publishes.
fn entry_url(e: &Entry) -> String {
    let git_ref = if e.version == UNRELEASED {
        HISTORY_BRANCH.to_owned()
    } else {
        format!("v{}", e.version)
    };
    format!(
        "https://github.com/{HISTORY_REPO}/blob/{git_ref}/{}",
        entry_file(e)
    )
}

/// One entry as its headline and a link to the note itself.
///
/// The headline is rendered **whole** — nothing is clipped and no excerpt is invented from the
/// body. An excerpt would be the shape this function exists to refuse: a paragraph that reads
/// like the note and is not it, with no mark saying where it stopped.
fn brief(e: &Entry) -> String {
    format!(
        "### {}{}\n\nFull note: [`{}`]({})",
        marker(e),
        e.headline,
        entry_file(e),
        entry_url(e)
    )
}

/// `n` with the thousands separators Python's `{:,}` writes.
fn comma(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// The section that says, in the body itself, that the body is not all of it.
///
/// Placed at the cut rather than at the top, and one copy: a banner above the notes would be a
/// second statement of one fact, free to disagree with this one. It can be one copy because
/// the elision is *also* visible at every point it applies — every elided note keeps its own
/// heading, in its own place in the order, with [`brief`]'s link directly under it.
///
/// It states the arithmetic because "some notes are linked" is the sentence a reader has no way
/// to check. These numbers they can.
///
/// The rule is written only when something is above it to be ruled off: a body that begins with
/// `---` is a body some renderers read as frontmatter.
fn elision(shown_count: usize, total: usize, whole: usize) -> String {
    let listed = total - shown_count;
    let rule = if shown_count > 0 { "---\n\n" } else { "" };
    format!(
        "{rule}## {listed} of these {total} notes are listed by headline only\n\n\
         Rendered whole, {total} notes come to {} characters, and GitHub refuses a release \
         body over {}.\n\n\
         **Every note this version shipped is in this list.** {shown_count} are above in full; \
         the {listed} below are a headline and a link. No note was dropped, and no note's text \
         was cut short — the text of each one below is in the note it links to, which ships in \
         the wheel and in the repository as well.",
        comma(whole),
        comma(RELEASE_BODY_MAX),
    )
}

/// One version's entries as the body of a GitHub Release.
///
/// The shipped entry is the single source for both the offline suggestion and the public
/// notes, so the two cannot drift: one is printed from the other. Order comes from [`all`], not
/// from this function, and the label from [`marker`], which the offline view calls too.
///
/// **And the result is bounded, because the far end of it refuses a long one.** Three
/// properties decide the shape, and each rules out an easier one:
///
/// **Nothing is dropped and nothing is truncated.** Cutting the string at the limit is the
/// obvious fix and it is the "convincing empty" this codebase refuses everywhere: a body that
/// ends mid-sentence with a dozen notes simply absent reads exactly like a release that shipped
/// a dozen fewer things.
///
/// **The cut is one point, not a per-entry decision.** A greedy fill would give an ordinary
/// note its body while a security note above it lost one, which is charter #486's defect
/// wearing a size limit.
///
/// **And *k* is measured against the order [`all`] already decided.** There is no second rule
/// in here that promotes security entries: the order that decides what leads is the order that
/// decides what keeps its body.
///
/// Each candidate is **built and then measured**, rather than measured by adding up part
/// lengths: deriving the length is a second answer to "how long is this?", free to drift from
/// the string actually returned by a separator's width.
///
/// A body that cannot be brought under the limit even with every note brief is returned **as it
/// is**, not silently cut: the caller refuses to print it.
pub fn render_body(version: &str) -> String {
    let entries = for_version(version);
    let whole: Vec<String> = entries.iter().map(part).collect();
    let body = whole.join("\n\n");
    if sent_length(&body) <= BODY_BUDGET {
        return body;
    }
    let briefs: Vec<String> = entries.iter().map(brief).collect();
    // The character count, not `sent_length`: this number goes into a sentence that says how
    // long the notes are, and Python writes `len(body)` there.
    let whole_chars = body.chars().count();
    let mut smallest = String::new();
    for k in (0..entries.len()).rev() {
        let mut candidate: Vec<&str> = whole[..k].iter().map(String::as_str).collect();
        let cut = elision(k, entries.len(), whole_chars);
        candidate.push(&cut);
        candidate.extend(briefs[k..].iter().map(String::as_str));
        let candidate = candidate.join("\n\n");
        if sent_length(&candidate) <= BODY_BUDGET {
            return candidate;
        }
        smallest = candidate;
    }
    smallest
}

// ------------------------------------------------------------------------------------------
// Probing: has THIS plane adopted an entry?
// ------------------------------------------------------------------------------------------

/// Anything a shell would treat as syntax — `charter/news.py`'s `_SHELLISH`, character for
/// character and no wider.
///
/// Present only as a belt to the braces: nothing here is ever passed to a shell, so this
/// rejects an entry whose AUTHOR believed it might be — which is a broken entry either way, and
/// better reported than silently truncated.
///
/// Not widened with the other controls, tempting as that is: a set that refuses MORE than
/// charter's would make an entry unchecked here and probed there, and the two charters would
/// disagree about one committed file.
const SHELLISH: [char; 13] = [
    ';', '|', '&', '<', '>', '$', '`', '(', ')', '\\', '\n', '"', '\'',
];

/// Every command path a `check:` may name.
///
/// An entry chooses from here rather than from the whole CLI, because what makes a probe safe
/// is a property of the command: it reads rather than acts, and any argv it hands to another
/// program is charter's rather than the entry's. Neither half can be read off the parser —
/// clap cannot be asked whether a command writes — so this is a list a human keeps.
///
/// A path is the subcommand tokens and nothing else. Flags are the entry's to choose; a deeper
/// subcommand is a different command and needs its own line, or listing `news` would silently
/// list `news stamp`, which renames files.
///
/// Adding one has two questions to answer, and the second is the one that gets skipped: does it
/// change this machine, and does its exit code actually mean "this plane has the thing?".
/// `version` fails the second — it always exits 0 — which is why the most obviously harmless
/// command in the CLI is not here.
///
/// **Two of these four are not commands this binary has yet**, and that is the list being
/// charter's rather than this CLI's: it says what an entry may NAME, and an entry naming a
/// command this charter does not register is separately refused by [`tokens`] with a different
/// sentence, because the two are different findings. The list does not shrink to match the CLI,
/// for the same reason it is not derived from the parser: it is the rule an entry's AUTHOR is
/// held to, and that rule is the same on every charter.
pub const PROBEABLE: [&[&str]; 4] = [
    &["doctor"],
    &["news"],
    &["persona", "lint"],
    &["frame-probe"],
];

/// The subcommand tree of the charter that is asking, as [`Dispatch`] hands it over.
///
/// charter asks argparse; this asks clap. Neither answer belongs in core — the tree is the
/// CLI's own shape — so it is passed in, which also lets a test declare a tree of its own
/// rather than depend on whichever commands happen to exist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandTree {
    pub name: String,
    pub children: Vec<CommandTree>,
}

impl CommandTree {
    /// A leaf.
    pub fn leaf(name: &str) -> Self {
        CommandTree {
            name: name.to_owned(),
            children: Vec::new(),
        }
    }

    /// A command with subcommands.
    pub fn with(name: &str, children: Vec<CommandTree>) -> Self {
        CommandTree {
            name: name.to_owned(),
            children,
        }
    }

    fn child(&self, name: &str) -> Option<&CommandTree> {
        self.children.iter().find(|c| c.name == name)
    }
}

/// What a probe needs from the charter running it.
///
/// A trait rather than a closure because [`dispatch`] has to be able to call back INTO the
/// command it is running — `news --pending` probes, and an entry may name `news` — and a
/// closure that calls the function that holds it is not a thing Rust lets you write.
pub trait Dispatch {
    /// This charter's subcommands.
    fn tree(&self) -> &CommandTree;
    /// Run `charter <tokens>` in this process, discarding its output; its exit code, or `None`
    /// when there is no usable one (the command could not be parsed, or it panicked).
    fn run(&self, tokens: &[String]) -> Option<i32>;
}

/// Would a shell read anything in `argv` as syntax?
///
/// Its own function because two callers need the same answer for different purposes:
/// [`tokens`] refuses on it, and [`dispatch`] has to know *which* of `tokens`' two refusals
/// fired to say whose defect it is (charter #321).
pub fn shell_syntax(argv: &str) -> bool {
    !argv.is_empty() && argv.chars().any(|c| SHELLISH.contains(&c))
}

/// `argv` as a charter subcommand's tokens, or `None` if it is not one.
///
/// Two refusals, both structural rather than advisory: anything a shell would read as syntax,
/// and any first token that is not a registered subcommand. `charter` is implied and must not
/// be written, so an entry cannot reach a different binary.
///
/// They are not the same defect, and [`dispatch`] tells them apart before it reports one. Shell
/// syntax can never run on any machine in any version; an unregistered first token is a command
/// *this* charter does not have.
pub fn tokens(argv: &str, tree: &CommandTree) -> Option<Vec<String>> {
    if argv.is_empty() || shell_syntax(argv) {
        return None;
    }
    // `str.split()` with no argument: any run of whitespace, and no empty pieces — where
    // "whitespace" is Python's, which is four separator controls wider than Rust's.
    let parts: Vec<String> = argv
        .split(crate::memstore::is_python_space)
        .filter(|p| !p.is_empty())
        .map(str::to_owned)
        .collect();
    let first = parts.first()?;
    tree.child(first)?;
    Some(parts)
}

/// The subcommand path `tokens` names, or `None` when it cannot be read off.
///
/// Leading tokens are consumed while they name a subcommand at the level reached so far. The
/// walk then stops at the first token that does not — a flag, or an argument — and that is the
/// case worth stating: **argparse reads *past* a flag to find the subcommand behind it**, so
/// `news --pending stamp 9.9.9` runs `news stamp`. A walk that just stopped would score it as
/// plain `news` and let a rename through a list that never named it.
///
/// clap is stricter than argparse here and would refuse that line rather than running the
/// rename — so this guard is defensive in the Rust CLI where it was load-bearing in the Python
/// one. It is ported anyway, because the property being defended is "this walk can say what the
/// tokens name", and a guard kept only while the parser happens to agree with it is a guard
/// that disappears the day the parser changes.
pub fn command_path(tokens: &[String], tree: &CommandTree) -> Option<Vec<String>> {
    let mut at = tree;
    let mut path: Vec<String> = Vec::new();
    let mut rest: &[String] = tokens;
    while let Some(head) = rest.first() {
        if at.children.is_empty() {
            break;
        }
        if let Some(child) = at.child(head) {
            at = child;
            path.push(head.clone());
            rest = &rest[1..];
            continue;
        }
        if rest.iter().any(|t| at.child(t).is_some()) {
            return None;
        }
        break;
    }
    Some(path)
}

/// May a `check:` name `charter <argv>`?
///
/// Public because it is the entry author's rule, not only the dispatcher's: a suite holds every
/// shipped `check:` to it, so an entry naming a command a probe may not run fails the PR that
/// adds it rather than the machine that installs it.
///
/// `adopt:` is deliberately NOT held to this. It is the line a human is told to run, once, on
/// purpose — `adopt: browser install` installs a browser, which is the point — where `check:`
/// runs unprompted on every plane that upgrades. Same grammar, opposite rules, and reading one
/// as the other is how the restraint gets widened back out.
pub fn probeable(argv: &str, tree: &CommandTree) -> bool {
    let Some(tokens) = tokens(argv, tree) else {
        return false;
    };
    let Some(path) = command_path(&tokens, tree) else {
        return false;
    };
    PROBEABLE.iter().any(|listed| {
        listed.len() == path.len() && listed.iter().zip(&path).all(|(a, b)| *a == b.as_str())
    })
}

/// How many [`dispatch`] calls are in flight.
///
/// Plain process state rather than a thread-local: a thread-local reads its default in every
/// new thread, so a probing command that fanned its own work out to a pool would walk straight
/// past the guard — which is the one failure this exists to make impossible. The global's
/// failure mode is the opposite and far cheaper: two probes racing in different threads would
/// make each other `unknown`, which is wrong but bounded, honest, and not reachable today.
static DEPTH: AtomicUsize = AtomicUsize::new(0);

/// Why the outermost dispatch has no answer to give, when it has none.
static REFUSED: Mutex<Option<&'static str>> = Mutex::new(None);

/// The same guard, in a form that survives `exec` — `<pid>:<path>`.
///
/// **The PID** is the process running the probe, which is what keeps the marker from becoming
/// the worse bug: a copy that turns up somewhere charter did not put it names a process, and
/// one that no longer exists guards nothing. Believed, a scrap of stale environment would turn
/// every probe on that machine into `unknown` for good and say nothing about why.
///
/// **The path** is where a descendant that was refused leaves a mark, because bounding the loop
/// is not the same as answering honestly: a `check:` whose command exits 0 while the charter it
/// spawned quietly declined would report the entry ADOPTED.
///
/// This binary READS this variable and never writes it — see the module docstring.
pub const PROBE_ENV: &str = "CHARTER_NEWS_PROBE";

/// The PID of a process ABOVE this one that is running a probe, or `None`.
///
/// Three ways the marker means nothing, and each of them is a defect if believed. It is not a
/// PID at all — debris, or somebody's guess at the name. It is **this** process's. Or it names
/// a process that has exited, in which case the probe it stood for is gone too.
fn outer_probe() -> Option<u32> {
    let raw = std::env::var(PROBE_ENV).unwrap_or_default();
    let head = raw.split(':').next().unwrap_or_default();
    if head.is_empty() || !head.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let pid: u32 = head.parse().ok()?;
    if pid == 0 || pid == std::process::id() {
        return None;
    }
    // **[`crate::process::alive`], and not an answer of this module's own** (charter-app#102).
    // This used to carry its own pair of arms, and off POSIX they said the opposite of
    // `glstate`'s: every marker read as live, so a stale one would have suppressed the check
    // for ever — the exact failure the paragraph above says the PID is here to prevent. On
    // POSIX the two also read an unplaceable errno differently, which no run could provoke and
    // nothing had noticed. One question, one answer, and the reason is written once where the
    // answer is.
    //
    // What it costs THIS caller is named there rather than hidden: off POSIX a descendant
    // would not decline a mutation inside a probe it cannot see. That is a round of the check
    // being wrong; believing a stale marker is every round on that machine being wrong, with
    // nothing to clear it.
    crate::process::alive(pid).then_some(pid)
}

/// Is an entry's `check:` running right now — here, or in a process above this one?
///
/// Public, because the answer is not only this module's business. A probe asks whether this
/// plane has something; anything that would answer by CHANGING the machine has to decline, and
/// say so through [`refuse_mutation`] so the entry comes back unchecked rather than pending.
pub fn probing() -> bool {
    DEPTH.load(Ordering::SeqCst) > 0 || outer_probe().is_some()
}

/// Leave word, for the process whose probe this is, that a descendant was refused.
///
/// The only channel there is, and it never raises: a path that cannot be written costs the
/// outer probe its honesty — it falls back to reading an exit code — and must not cost the
/// caller anything at all.
fn mark_refusal() {
    if outer_probe().is_none() {
        // This process's own probe. It already knows — the flag it reads is in memory — and
        // writing a file to tell ourselves would put the read path through the disk to learn
        // what it just decided.
        return;
    }
    let raw = std::env::var(PROBE_ENV).unwrap_or_default();
    let Some((_, mark)) = raw.split_once(':') else {
        return;
    };
    if mark.is_empty() {
        return;
    }
    let _ = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(mark);
}

/// Record that a command declined to run because a probe is in flight.
///
/// Stopping the mutation is the easy half. The command still has to return SOMETHING, and
/// whatever it returns is not an answer to "has this plane adopted this entry?" — so the exit
/// code is withheld here exactly as a re-entered one is, and the entry reports `unknown`.
/// Reporting `pending` instead would invent a chore on a plane that may already have adopted
/// the entry.
pub fn refuse_mutation() {
    set_refused(Some(MUTATES));
}

fn set_refused(why: Option<&'static str>) {
    if let Ok(mut slot) = REFUSED.lock() {
        *slot = why;
    }
}

fn refused() -> Option<&'static str> {
    REFUSED.lock().ok().and_then(|slot| *slot)
}

/// Five ways to have no answer, said five ways.
///
/// "Did not run here" points the reader at their own machine, which is right for a check this
/// CLI could not resolve and wrong for an entry whose `check:` can never run anywhere — folding
/// those together hides the second behind the first, and the second is a defect in the entry
/// that somebody has to fix.
const NOT_RUN: &str = "`charter {check}` did not run here, so this entry is unchecked — \
                       neither adopted nor pending";
const PROBES: &str = "`charter {check}` probes news itself, so its exit code answers a \
                      different question than this entry's — unchecked, neither adopted nor \
                      pending. A `check:` has to name a command that does not probe";
const IN_FLIGHT: &str = "`charter {check}` was not run: a probe is already in flight, and a \
                         probe never runs from inside a probe — unchecked here";
const MUTATES: &str = "`charter {check}` changes this machine rather than reading it, so it \
                       was not run — a `check:` asks whether this plane already has something \
                       and cannot be the thing that goes and gets it. Unchecked, neither \
                       adopted nor pending";
const UNLISTED: &str = "`charter {check}` is not a command a `check:` may name. A probe reads, \
                        and the argv it hands anything else is charter's rather than this \
                        entry's, so an entry picks from a short list of read-only commands \
                        instead of from the whole CLI. Unchecked, neither adopted nor pending";

/// Run `charter <argv>` in this process. Exit code, or `None` if there is no usable one.
///
/// `None` is the whole reason this returns an `Option` rather than an `i32`: a probe that did
/// not run — or that ran and answered a different question — must not be reported as an answer.
///
/// **The guard is two halves, and only one of them is about the loop.** Refusing the nested
/// call bounds the recursion; clearing the outer call's exit code is what keeps the result
/// honest. A `doctor` inside a `doctor` exits 0 whenever nothing is broken, so an outer probe
/// that read that code would report the entry ADOPTED and never offer it again — the entry
/// hidden by the very bug it triggers. Bounded is not the same as correct (ADR 0013).
///
/// Three refusals, and they are not the same question: is this a charter subcommand at all; is
/// a probe already running; and is this a command a `check:` may name at all. The first of
/// those is really two, and reading its bare `None` as one was charter #321 — so the reason is
/// taken from [`shell_syntax`] rather than from the `None`, which cannot carry it.
pub fn dispatch(argv: &str, d: &dyn Dispatch) -> Option<i32> {
    if DEPTH.load(Ordering::SeqCst) == 0 {
        // Cleared on the way IN, not on the way out, and before the early returns below: every
        // top-level dispatch has to start clean, or a refusal recorded while probing the
        // previous entry is read as this entry's answer.
        set_refused(None);
    }
    let tree = d.tree();
    let Some(tokens) = tokens(argv, tree) else {
        if shell_syntax(argv) {
            // The one of `tokens`' two refusals that is the ENTRY's defect: a `check:` carrying
            // shell syntax can never run — not here, not on any machine, not in any version —
            // so "did not run here" would send its author to look at a laptop that has nothing
            // wrong with it (charter #321). The other refusal, a first token this CLI does not
            // register, really is a fact about here, and keeps `NOT_RUN`.
            set_refused(Some(UNLISTED));
        }
        return None;
    };
    if probing() {
        set_refused(Some(PROBES));
        mark_refusal();
        return None;
    }
    if !probeable(argv, tree) {
        // After the re-entrancy guard rather than before it, so the marking is reached on
        // exactly the paths it was reached on before.
        set_refused(Some(UNLISTED));
        return None;
    }

    // Released through `Drop`, which is this language's `finally` and is needed for the same
    // reason: released only on the happy path, the guard would stay armed for the life of the
    // process the first time a `check:` unwound, turning every later probe into `unknown` with
    // no way to tell why.
    let held = Depth::hold();
    let code = d.run(&tokens);
    drop(held);
    // Python also puts the environment marker back here. There is no marker to put back: this
    // binary never sets one (see the module docstring).
    if refused().is_some() { None } else { code }
}

struct Depth;

impl Depth {
    fn hold() -> Self {
        DEPTH.fetch_add(1, Ordering::SeqCst);
        Depth
    }
}

impl Drop for Depth {
    fn drop(&mut self) {
        DEPTH.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Has this plane adopted `entry`? `(status, why)`.
pub fn probe(entry: &Entry, d: &dyn Dispatch) -> (Status, String) {
    if entry.check.is_empty() {
        return (Status::Informational, String::new());
    }
    // Read before dispatching: inside another probe — this process's, or one it was spawned by
    // — this one is refused before it runs, and blaming THIS entry's `check:` for probing would
    // be a guess. The command already in flight is the one that probes, and it may not be this
    // one; across a process boundary it is not even in the list.
    let in_flight = probing();
    let code = dispatch(&entry.check, d);
    match code {
        None => {
            let why = if in_flight {
                IN_FLIGHT
            } else {
                refused().unwrap_or(NOT_RUN)
            };
            // Through `shown::sentence` rather than a plain format: `check:` is frontmatter and
            // this is printed as a line of `charter news --pending`'s own report.
            (
                Status::Unknown,
                shown::sentence(why, &[("check", entry.check.as_str().into())]),
            )
        }
        Some(0) => (Status::Adopted, String::new()),
        Some(_) => (Status::Pending, String::new()),
    }
}

// ------------------------------------------------------------------------------------------
// The command
// ------------------------------------------------------------------------------------------

/// Everything `charter news` said, and where each line goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    /// stdout, verbatim, newline-terminated as `print` leaves it.
    pub out: String,
    /// stderr, as glyph-prefixed lines.
    pub said: Vec<Say>,
    pub code: u8,
}

impl Report {
    fn new() -> Self {
        Report {
            out: String::new(),
            said: Vec::new(),
            code: 0,
        }
    }

    /// An empty report for a command that wraps one of these — see [`crate::adopt`].
    pub fn new_public() -> Self {
        Report::new()
    }

    /// Append `other`'s output and lines, and take the worse of the two exit codes.
    ///
    /// The worse and not the later: a wrapper that said nothing went wrong must not turn a
    /// refusal it printed into an exit 0.
    pub fn absorb(&mut self, other: Report) {
        self.out.push_str(&other.out);
        self.said.extend(other.said);
        self.code = self.code.max(other.code);
    }

    fn print(&mut self, line: &str) {
        self.out.push_str(line);
        self.out.push('\n');
    }
}

/// One entry, one line, in the shape the reader acts on.
///
/// Deliberately not JSON. The skill is a Claude Code artifact and opencode gets none, so this
/// text is the only thing an agent on another harness has — a machine surface would become the
/// parsed one, and this the unchecked one.
///
/// Every span an entry owns goes through [`shown::line`]. The slug, the headline and the
/// `adopt:` command are all frontmatter or a filename, this is two lines of charter's own report
/// with a structure a reader parses by eye, and charter #502 is the same value crossing into the
/// same kind of line one function over. The `·` and the indent are charter's; a third of each
/// would be the entry's.
fn news_line(e: &Entry) -> String {
    let adopt = shown::line(&e.adopt);
    // Asked of the CONTAINED value, not the raw one. They agree today; the question this line
    // needs answered is "is there a command to show the reader", and a value that renders as
    // nothing is not one.
    let how = if adopt.is_empty() {
        "adopt: manual (see the entry)".to_owned()
    } else {
        format!("adopt: charter {adopt}")
    };
    format!(
        "  {} · {}{}\n      {how}",
        shown::line(&e.slug),
        marker(e),
        shown::line(&e.headline)
    )
}

/// Said once, where a probe would otherwise be run against nothing.
///
/// "Has this plane adopted it?" has no subject outside a control plane. Running the probes
/// anyway would spend a subprocess each to report whatever a plane-less charter happens to exit
/// with — an answer to a question nobody asked, indistinguishable in the output from one that
/// means something.
const NO_PLANE: &str = "no control plane here, so charter cannot tell which of these this \
                        plane has adopted — run `charter news --pending` from inside one";

/// `charter news --for <version>`: one version's entries as its release notes.
///
/// **THE release gate.** charter's `release.yml` runs this and refuses to tag on a non-zero
/// exit, and its announce job pipes this same stdout into `gh release create --notes-file`. So
/// an ordering charter cannot honour is caught before a Release exists to be wrong.
pub fn for_release(version: &str) -> Report {
    let mut report = Report::new();
    // Both halves, behind one gate. `entry_errors` speaks for files that BECAME entries;
    // `unreadable` speaks for files in the news directory that did not, which is where a
    // miscased `Version:` lands. Asked over the whole directory rather than for this version,
    // because a file with no readable version has no version to be filtered by and the release
    // being cut is when somebody wants to hear about it.
    let mut problems = entry_errors(&for_version(version));
    problems.extend(unreadable());
    if !problems.is_empty() {
        // "the news for", not "the news entries for": half of what this gate catches is a file
        // that never became an entry, and a sentence naming only entries would send the reader
        // looking for one that does not exist.
        report.said.push(Say::Err(format!(
            "the news for {version} declares something charter cannot honour:"
        )));
        for why in problems {
            report.said.push(Say::Info(format!("  {why}")));
        }
        report.code = 1;
        return report;
    }
    // A second refusal rather than more lines in the first, because the reader is being told a
    // different thing. Everything above is a declaration charter could not read; this is one it
    // read exactly as written, from an author who believed the frontmatter was YAML.
    let quoted = quoted_values(&for_version(version));
    if !quoted.is_empty() {
        report.said.push(Say::Err(format!(
            "the news for {version} quotes a value charter does not unquote:"
        )));
        for why in quoted {
            report.said.push(Say::Info(format!("  {why}")));
        }
        report.code = 1;
        return report;
    }
    let body = render_body(version);
    if body.is_empty() {
        report
            .said
            .push(Say::Err(format!("no news entry for {version}.")));
        report.code = 1;
        return report;
    }
    // The claim nobody was making (charter #665). Everything above answers "do these entries
    // render?", which was never the failing question. What the announce job then does with the
    // string is POST it to an API that refuses a body over `RELEASE_BODY_MAX` outright, after an
    // upload PyPI will not take back.
    //
    // `+ 1` is the newline printed below. It is a character in the file the workflow redirects
    // into and a character GitHub counts, and measuring the string rather than the file is the
    // off-by-one that would show up only at the ceiling — the one place nobody gets a second try.
    let sent = sent_length(&body) + 1;
    if sent > RELEASE_BODY_MAX {
        report.said.push(Say::Err(format!(
            "the release notes for {version} come to {} characters and GitHub refuses a release \
             body over {} — `gh release create` fails with `body is too long`, and in release.yml \
             that happens in `announce`, after the PyPI upload.",
            comma(sent),
            comma(RELEASE_BODY_MAX)
        )));
        // Said because the reader's obvious next move — "link more of them" — is the one that
        // cannot work here. `render_body` already links every note it can, so reaching this
        // means the HEADLINES alone are over the limit.
        report.said.push(Say::Info(format!(
            "  charter already lists every note for {version} by headline and a link, and the \
             headlines alone do not fit. Shorten them, or split the release."
        )));
        report.code = 1;
        return report;
    }
    report.print(&body);
    report
}

/// `charter news --pending`: every entry, any version, whose probe says this plane has not
/// adopted it.
pub fn pending_report(d: &dyn Dispatch, has_plane: bool) -> Report {
    let mut report = Report::new();
    if !has_plane {
        report.said.push(Say::Warn(NO_PLANE.to_owned()));
        return report;
    }
    let (mut shown_count, mut unchecked) = (0usize, 0usize);
    for e in released() {
        let (status, why) = probe(&e, d);
        match status {
            Status::Pending => {
                report.print(&news_line(&e));
                shown_count += 1;
            }
            // Said, not swallowed. A probe that could not run is the one case where silence
            // would be read as "nothing to adopt" — the shape ADR 0013 exists to refuse. The
            // slug is the committed filename with its version prefix cut off, and `why` already
            // carries the entry's `check:` contained.
            Status::Unknown => {
                report
                    .said
                    .push(Say::Warn(format!("{}: {why}", shown::line(&e.slug))));
                unchecked += 1;
            }
            Status::Adopted | Status::Informational => {}
        }
    }
    if shown_count == 0 {
        if unchecked > 0 {
            // NOT the ✓ line, which claims every probe reported adopted — the one thing an
            // unchecked entry did not say. A green tick printed under the warning it
            // contradicts is how the warning stops being read.
            let plural = if unchecked == 1 { "y" } else { "ies" };
            report.said.push(Say::Warn(format!(
                "nothing pending, but {unchecked} entr{plural} could not be checked — which is \
                 not the same as nothing to adopt."
            )));
        } else {
            report.said.push(Say::Ok(
                "nothing pending — every entry with a probe reports adopted.".to_owned(),
            ));
        }
    }
    report
}

/// `charter news [--since X] [--until Y]`: the range view, for a reader catching up.
///
/// `since` and `until` are the flags as typed, with `""` for "not given". The default for
/// `until` is chosen BEFORE the strip and not after, which is `(args.until or …).strip()` in
/// Python: `--until "  "` is a bound the operator typed, strips to nothing, is not a version,
/// and gives an empty range — where no flag at all gives [`history_ends`].
pub fn range_report(since: &str, until: &str, d: &dyn Dispatch, has_plane: bool) -> Report {
    let mut report = Report::new();
    let since = py_strip(since);
    let until = if until.is_empty() {
        py_strip(&history_ends()).to_owned()
    } else {
        py_strip(until).to_owned()
    };
    if since.is_empty() {
        // No baseline is not a range. Replaying every entry ever written as though it were news
        // would be charter presenting old text as new — so it says what it can and points at the
        // view that IS honest without one.
        report.said.push(Say::Info(
            "no baseline recorded, so there is no range to report.".to_owned(),
        ));
        report.said.push(Say::Info(
            "  what this plane has not adopted:  charter news --pending".to_owned(),
        ));
        return report;
    }
    let entries = between(since, &until);
    if entries.is_empty() {
        report
            .said
            .push(Say::Ok(format!("nothing new between {since} and {until}.")));
        return report;
    }
    if !has_plane {
        report.said.push(Say::Warn(NO_PLANE.to_owned()));
    }
    // Warned, not refused. This view is a reader catching up, and withholding the range over a
    // malformed `security:` line would lose them the other nineteen entries to protect them from
    // one being in the wrong place. `--for` is where refusing belongs, because that is the call
    // that becomes a published Release.
    let mut warnings = entry_errors(&entries);
    warnings.extend(unreadable());
    for why in warnings {
        report.said.push(Say::Warn(why));
    }
    for e in &entries {
        let status = if has_plane {
            probe(e, d).0
        } else {
            Status::Informational
        };
        // Version and headline are both frontmatter, and this is a report line whose two-space
        // column a reader parses by eye (charter #502).
        report.print(&format!(
            "{}  {}{}",
            shown::line(&e.version),
            marker(e),
            shown::line(&e.headline)
        ));
        // Only an entry with something to DO gets the action line. An informational entry — a
        // patch note, usually — exists to say there is nothing to take up, so printing "adopt:
        // manual" beneath it invents a chore out of the line that denies one.
        if status == Status::Pending {
            report.print(&news_line(e));
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    /// [`DEPTH`] and [`REFUSED`] are process state, on purpose (their own docstrings say why),
    /// and `cargo test` runs these in threads of one process. Every test that dispatches takes
    /// this first, so two of them cannot make each other `unknown`.
    static PROBE_TESTS: Mutex<()> = Mutex::new(());

    struct NoCommands;
    impl Dispatch for NoCommands {
        fn tree(&self) -> &CommandTree {
            static TREE: std::sync::OnceLock<CommandTree> = std::sync::OnceLock::new();
            TREE.get_or_init(|| CommandTree::with("charter", vec![]))
        }
        fn run(&self, _tokens: &[String]) -> Option<i32> {
            unreachable!("nothing is probeable against an empty tree")
        }
    }

    /// A charter whose `news` command probes, which is what `charter news --pending` is.
    struct ProbesItself;
    impl Dispatch for ProbesItself {
        fn tree(&self) -> &CommandTree {
            static TREE: std::sync::OnceLock<CommandTree> = std::sync::OnceLock::new();
            TREE.get_or_init(|| CommandTree::with("charter", vec![CommandTree::leaf("news")]))
        }
        fn run(&self, tokens: &[String]) -> Option<i32> {
            assert_eq!(tokens[0], "news");
            // The sweep: one entry, and its `check:` names the command doing the sweeping.
            let inner = entry(
                "0.60.0-inner.md",
                "---\nversion: 0.60.0\nheadline: h\ncheck: news --pending\n---\nb\n",
            );
            let (status, why) = probe(&inner, self);
            assert_eq!(status, Status::Unknown);
            assert!(why.contains("a probe is already in flight"), "{why}");
            // `charter news --pending` exits 0 with nothing pending, which is the exit code
            // that would report the OUTER entry adopted if it were believed.
            Some(0)
        }
    }

    fn entry(name: &str, text: &str) -> Entry {
        read(name, text).expect("the fixture is an entry")
    }

    #[test]
    fn the_corpus_parses_and_nothing_in_it_is_refused() {
        // The shipped corpus is the fixture with the most authors. Everything in it must read,
        // and it is the differential suite that proves the RENDERING matches charter's.
        assert!(FILES.len() > 300, "the corpus did not compile in");
        assert!(unreadable().is_empty(), "{:?}", unreadable());
        assert!(
            entry_errors(&all()).is_empty(),
            "{:?}",
            entry_errors(&all())
        );
        assert!(!all().is_empty());
    }

    #[test]
    fn the_order_is_oldest_first_with_staged_entries_last() {
        let versions: Vec<String> = all().iter().map(|e| e.version.clone()).collect();
        let staged_at = versions.iter().position(|v| v == UNRELEASED);
        if let Some(at) = staged_at {
            assert!(versions[at..].iter().all(|v| v == UNRELEASED));
        }
        let released: Vec<&String> = versions.iter().filter(|v| *v != UNRELEASED).collect();
        for pair in released.windows(2) {
            assert!(
                version_key(pair[0]) <= version_key(pair[1]),
                "{} came before {}",
                pair[0],
                pair[1]
            );
        }
    }

    #[test]
    fn within_a_version_lead_comes_first_and_security_before_the_rest() {
        // 0.53.0 has a `lead:` and six `security:` entries, which is the only shape that
        // exercises all three ranks at once.
        let ranks: Vec<u8> = for_version("0.53.0").iter().map(rank).collect();
        assert!(!ranks.is_empty());
        for pair in ranks.windows(2) {
            assert!(pair[0] <= pair[1], "{ranks:?} is not in rank order");
        }
        assert_eq!(ranks[0], 0, "the entry declaring `lead:` is not first");
    }

    #[test]
    fn an_ordering_value_charter_cannot_read_is_named_rather_than_read_as_false() {
        let e = entry(
            "0.60.0-x.md",
            "---\nversion: 0.60.0\nheadline: h\nsecurity: yes\n---\nbody\n",
        );
        assert!(!e.security);
        assert_eq!(e.bad, vec![("security", "yes".to_owned())]);
        let said = entry_errors(&[e]);
        assert_eq!(said.len(), 1);
        assert!(said[0].contains("`security: yes` is not a value charter reads"));
    }

    #[test]
    fn a_field_with_nothing_after_the_colon_gets_the_other_sentence() {
        let e = entry(
            "0.60.0-x.md",
            "---\nversion: 0.60.0\nheadline: h\nsecurity:\n  true\n---\nbody\n",
        );
        assert_eq!(e.bad, vec![("security", String::new())]);
        let said = entry_errors(&[e]);
        assert!(said[0].contains("declared with no value on that line"));
        assert!(
            said[0].contains("a value indented onto the NEXT line never reaches charter"),
            "{}",
            said[0]
        );
    }

    #[test]
    fn a_miscased_key_is_reported_and_not_folded() {
        let e = entry(
            "0.60.0-x.md",
            "---\nversion: 0.60.0\nheadline: h\nSecurity: true\n---\nbody\n",
        );
        assert!(!e.security, "a miscased key must not be read");
        assert_eq!(e.unknown, vec!["Security".to_owned()]);
        let said = entry_errors(&[e]);
        assert!(said[0].contains("differs from `security:` only in case"));
    }

    #[test]
    fn a_key_that_is_not_a_near_miss_is_still_reported() {
        let e = entry(
            "0.60.0-x.md",
            "---\nversion: 0.60.0\nheadline: h\nsecuriy: true\n---\nbody\n",
        );
        let said = entry_errors(&[e]);
        assert!(said[0].contains("is not a field a news entry declares"));
        // The list of fields comes from the set rather than from a sentence.
        assert!(
            said[0].contains("`version:`, `headline:`, `check:`, `adopt:`, `lead:`, `security:`")
        );
    }

    #[test]
    fn two_entries_cannot_both_lead_one_version() {
        let a = entry(
            "0.60.0-a.md",
            "---\nversion: 0.60.0\nheadline: a\nlead: true\n---\nx\n",
        );
        let b = entry(
            "0.60.0-b.md",
            "---\nversion: 0.60.0\nheadline: b\nlead: true\n---\nx\n",
        );
        let said = entry_errors(&[a, b]);
        assert_eq!(said.len(), 1);
        assert!(
            said[0].contains("0.60.0: 2 entries declare `lead: true` (0.60.0-a.md, 0.60.0-b.md)")
        );
    }

    #[test]
    fn a_filename_cannot_forge_a_second_line_of_the_report() {
        // charter #502: the ordering VALUE was contained and the committed FILENAME beside it
        // was not, so a filename holding a newline forged an extra line of charter's report.
        let e = entry(
            "0.60.0-a\nEVIL: charter says nothing is wrong.md",
            "---\nversion: 0.60.0\nheadline: h\nsecurity: yes\n---\nbody\n",
        );
        let said = entry_errors(&[e]);
        assert_eq!(said.len(), 1);
        assert_eq!(said[0].lines().count(), 1, "{}", said[0]);
    }

    #[test]
    fn a_miscased_version_deletes_the_entry_and_is_reported_for_it() {
        assert!(read("0.60.0-x.md", "---\nVersion: 0.60.0\nheadline: h\n---\nb\n").is_none());
        let said = not_an_entry("0.60.0-x.md", "---\nVersion: 0.60.0\nheadline: h\n---\nb\n");
        assert!(said.contains("differs from `version:` only in case"));
        let none = not_an_entry("stray.md", "just prose, no frontmatter\n");
        assert!(none.contains("no `key: value` frontmatter"));
        let blank = not_an_entry("0.60.0-x.md", "---\nversion:\nheadline: h\n---\nb\n");
        assert!(blank.contains("A staged entry writes `version: unreleased`"));
    }

    #[test]
    fn a_quoted_value_is_reported_and_never_unquoted() {
        let e = entry(
            "0.56.0-x.md",
            "---\nversion: 0.56.0\nheadline: '`charter doctor`''s row'\n---\nb\n",
        );
        // Honoured exactly as written — nothing unquotes it.
        assert_eq!(e.headline, "'`charter doctor`''s row'");
        let said = quoted_values(&[e]);
        assert_eq!(said.len(), 1);
        assert!(said[0].contains("`headline:` opens and closes with '"));
        // And a headline that is entirely one code span is NOT quoted: a backtick is not a
        // quote character here, because that pair renders as markdown.
        let ok = entry(
            "0.56.0-y.md",
            "---\nversion: 0.56.0\nheadline: `charter news`\n---\nb\n",
        );
        assert!(quoted_values(&[ok]).is_empty());
    }

    #[test]
    fn the_last_line_of_a_repeated_key_wins_at_the_first_position() {
        // `persona.parse` builds a dict, so two lines with one key collapse to the LAST value
        // while keeping the FIRST position (charter #509 is that the drop is silent).
        let e = entry(
            "0.60.0-x.md",
            "---\nnope: 1\nversion: 0.60.0\nnope: 2\nheadline: h\n---\nb\n",
        );
        assert_eq!(e.unknown, vec!["nope".to_owned()]);
        assert_eq!(e.headline, "h");
    }

    #[test]
    fn the_body_is_everything_after_the_second_marker() {
        let e = entry(
            "0.60.0-x.md",
            "---\nversion: 0.60.0\nheadline: h\n---\nfirst\n\n---\n\nsecond\n",
        );
        assert_eq!(e.body, "first\n\n---\n\nsecond");
    }

    #[test]
    fn a_body_over_the_budget_keeps_every_note_and_links_the_rest() {
        // 0.54.0 is 86 notes and does not fit; 0.53.0 is 13 and does.
        let elided = render_body("0.54.0");
        assert!(sent_length(&elided) <= BODY_BUDGET);
        assert!(elided.contains("notes are listed by headline only"));
        for e in for_version("0.54.0") {
            assert!(
                elided.contains(&format!("### {}{}", marker(&e), e.headline)),
                "{} lost its heading",
                e.slug
            );
        }
        // A security note is never demoted while an ordinary one keeps its body.
        let whole = render_body("0.53.0");
        assert!(!whole.contains("listed by headline only"));
        assert_eq!(
            whole,
            for_version("0.53.0")
                .iter()
                .map(part)
                .collect::<Vec<_>>()
                .join("\n\n")
        );
    }

    #[test]
    fn a_link_points_at_the_tag_for_a_release_and_the_branch_for_a_staged_entry() {
        let released = entry("0.60.0-x.md", "---\nversion: 0.60.0\nheadline: h\n---\nb\n");
        assert_eq!(
            entry_url(&released),
            "https://github.com/diazoxide/charter-plane/blob/v0.60.0/docs/news/0.60.0-x.md"
        );
        let staged = entry(
            "unreleased-x.md",
            "---\nversion: unreleased\nheadline: h\n---\nb\n",
        );
        assert_eq!(
            entry_url(&staged),
            "https://github.com/diazoxide/charter-plane/blob/main/docs/news/unreleased-x.md"
        );
        // A filename that would forge a link in charter's own sentence is percent-encoded.
        let nasty = entry(
            "a](https://evil.example).md",
            "---\nversion: 0.60.0\nheadline: h\n---\nb\n",
        );
        assert_eq!(
            entry_file(&nasty),
            "docs/news/a%5D%28https%3A//evil.example%29.md"
        );
    }

    #[test]
    fn a_range_is_exclusive_at_the_bottom_and_inclusive_at_the_top() {
        let versions: Vec<String> = between("0.61.0", "0.62.0")
            .iter()
            .map(|e| e.version.clone())
            .collect();
        assert!(!versions.contains(&"0.61.0".to_owned()));
        assert!(versions.contains(&"0.62.0".to_owned()));
        assert!(between("0.62.1", "0.62.1").is_empty());
        // A staged entry is in no range.
        assert!(
            !between("0.44.0", &history_ends())
                .iter()
                .any(|e| e.version == UNRELEASED)
        );
    }

    #[test]
    fn the_history_ends_at_the_newest_released_entry_whatever_the_apps_version() {
        let shipped = history_ends();
        assert!(version_key(&shipped).is_version());
        assert_eq!(
            shipped, "0.62.1",
            "the corpus is frozen; nothing is released into it"
        );
        for e in released() {
            assert!(version_key(&e.version) <= version_key(&shipped));
        }
    }

    #[test]
    fn a_probe_never_runs_from_inside_a_probe_and_the_outer_code_is_withheld() {
        let _serial = PROBE_TESTS.lock().unwrap_or_else(|e| e.into_inner());
        // charter #311, both halves. Refusing the nested call bounds the recursion; clearing
        // the outer call's exit code is what keeps the result honest — `news --pending` exits 0
        // whenever nothing is pending, and an outer probe that read that 0 would report this
        // entry ADOPTED and never offer it again.
        let outer = entry(
            "0.60.0-outer.md",
            "---\nversion: 0.60.0\nheadline: h\ncheck: news --pending\n---\nb\n",
        );
        let (status, why) = probe(&outer, &ProbesItself);
        assert_eq!(
            status,
            Status::Unknown,
            "a 0 from a nested sweep was believed"
        );
        assert!(why.contains("probes news itself"), "{why}");
        // And the guard is released: a second probe is not poisoned by the first.
        assert_eq!(DEPTH.load(Ordering::SeqCst), 0);
        assert_eq!(probe(&outer, &ProbesItself).0, Status::Unknown);
    }

    #[test]
    fn a_command_that_mutates_declines_on_its_own_account() {
        let _serial = PROBE_TESTS.lock().unwrap_or_else(|e| e.into_inner());
        // The frightening half of charter #314 was never in a child: a `check:` naming an
        // installer runs in the process that IS the probe, at the depth the counter permits by
        // design. So the command declines itself, and refusing is only half of it — a command
        // that declined has no exit code worth reading.
        set_refused(None);
        refuse_mutation();
        assert_eq!(refused(), Some(MUTATES));
        set_refused(None);
    }

    #[test]
    fn a_check_naming_a_command_this_charter_does_not_have_is_unchecked() {
        let _serial = PROBE_TESTS.lock().unwrap_or_else(|e| e.into_inner());
        // Every shipped probe names `persona lint` or `frame-probe`, and this binary has
        // neither — so the honest answer is `unknown`, and it says which machine it is about.
        let e = entry(
            "0.60.0-x.md",
            "---\nversion: 0.60.0\nheadline: h\ncheck: persona lint\n---\nb\n",
        );
        let (status, why) = probe(&e, &NoCommands);
        assert_eq!(status, Status::Unknown);
        assert!(
            why.contains("`charter persona lint` did not run here"),
            "{why}"
        );
    }

    #[test]
    fn a_check_carrying_shell_syntax_is_the_entrys_defect_and_says_so() {
        let _serial = PROBE_TESTS.lock().unwrap_or_else(|e| e.into_inner());
        let e = entry(
            "0.60.0-x.md",
            "---\nversion: 0.60.0\nheadline: h\ncheck: doctor && rm -rf /\n---\nb\n",
        );
        let (status, why) = probe(&e, &NoCommands);
        assert_eq!(status, Status::Unknown);
        assert!(
            why.contains("is not a command a `check:` may name"),
            "{why}"
        );
        assert_eq!(why.lines().count(), 1);
    }

    #[test]
    fn an_entry_with_no_check_is_informational() {
        let e = entry("0.60.0-x.md", "---\nversion: 0.60.0\nheadline: h\n---\nb\n");
        assert_eq!(probe(&e, &NoCommands).0, Status::Informational);
    }

    #[test]
    fn the_probeable_list_is_what_an_entry_may_name_and_the_tree_is_what_exists() {
        let tree = CommandTree::with(
            "charter",
            vec![
                CommandTree::leaf("news"),
                CommandTree::leaf("secret"),
                CommandTree::with("persona", vec![CommandTree::leaf("lint")]),
            ],
        );
        assert!(probeable("news --pending", &tree));
        assert!(probeable("persona lint --only stale", &tree));
        // Registered, and not a command a probe may run.
        assert!(!probeable("secret exec vault curl evil.example", &tree));
        // Listed, and not a command this charter has.
        assert!(!probeable("doctor", &tree));
        // `charter` is implied and must not be written.
        assert!(!probeable("charter news", &tree));
        // A subcommand sighted past a flag means this walk cannot say what the tokens name.
        let deeper = CommandTree::with(
            "charter",
            vec![CommandTree::with("news", vec![CommandTree::leaf("stamp")])],
        );
        assert_eq!(
            command_path(
                &["news".into(), "--pending".into(), "stamp".into()],
                &deeper
            ),
            None
        );
        assert!(!probeable("news --pending stamp 9.9.9", &deeper));
    }
}
