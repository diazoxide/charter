//! What a panel is, and what a stranger's extension may contribute as one.
//!
//! **This is the contribution contract, and it is [`crate::extension`]'s vocabulary widened by
//! exactly one word.** Until now an extension could contribute `themes` and declare a `program`
//! nothing runs. A panel is the second entry in that vocabulary, and it is admitted on the same
//! four properties charter ADR 0041 requires of a declarative extension point — the properties
//! the theme already had before the registry existed:
//!
//! 1. **The vocabulary is closed and charter decides it.** A panel fills in values for names
//!    charter published — [`Block`], [`Row`], [`Mark`], [`Detail`] — and it cannot introduce
//!    one. An unknown key is refused at [`declared`] rather than ignored, because a key charter
//!    ignored is a key the operator was not shown when he consented.
//! 2. **The value space is not a program.** Text, a member of an enumeration, a bounded list.
//!    Nothing here is evaluated by anything.
//! 3. **charter chooses the consumer.** A panel says what it holds. It never says *where* — no
//!    region, no side, no width, no markup. [`Panel::order`] is a hint charter sorts by, and
//!    charter puts the result in the region ADR 0038 gives it.
//! 4. **charter parses and re-emits, never interpolates.** What comes off the disk is read into
//!    the typed values below and a fresh value is written out from them. The manifest's bytes
//!    never reach the window as markup; the window draws text nodes out of a `Row`, and React
//!    escapes them, so there is no path from a declared string to an element.
//!
//! # What a panel may NOT do, and the one that matters
//!
//! **A declared panel has no [`Row::runs`].** That field names a row of `app/src/actions.ts`'s
//! catalogue — charter's own verb, with the operator's own authority — and it is populated only
//! by charter's own contributions, in Rust, in this process. An extension that declared one is
//! refused by name at [`declared`], and the refusal says why rather than dropping the field:
//!
//! **what an extension puts in the window is data, not a verb.** A row that ran a charter verb
//! on a click would be charter acting on an extension's say-so, through a path with no hook, no
//! prompt and no grant — which is ADR 0041's *"a second door beside the one being built"*,
//! opened by a panel.
//!
//! **That is still true now that the executor exists** ([`crate::executor`], ADR 0041 stage 2),
//! and the reason is worth saying because the tempting reading is the opposite. The executor
//! runs an extension's *own* program, once, when the operator opens one of its views, and what
//! comes back is data in this vocabulary ([`answered`]) — which refuses `runs` exactly as a
//! manifest does. The grant that would lift it is *this extension may offer this catalogue
//! row*, consented per extension, per row, and nobody has asked for it yet: ADR 0041's rule is
//! that a capability is written down for a plugin that wants it, never ahead of one.
//!
//! So the asymmetry is real and is written down rather than smoothed over: **charter's own
//! panels can put a charter verb on a row and a stranger's cannot.** It is not a property of
//! being charter; it is a property of being code that is already in the process.
//!
//! # Where the numbers in a chart come from
//!
//! [`Block::Chart`] is the third kind of block, and it is admitted **only in an answer**, never
//! in a manifest. charter ADR 0043 refused a chart block while there was no executor, for a
//! reason that still holds for a manifest: *a declared chart is a chart of numbers the extension
//! wrote down at install time*, and stale statistics under the word *statistics* are worse than
//! none. A program the executor asks when the operator opens a view answers with numbers read
//! at that moment, so the chart is current, and that is the whole difference. [`declared`]
//! has no key a chart could arrive in; [`answered`] accepts one.
//!
//! # And the one that is not about execution at all
//!
//! ADR 0041 keeps *deception* apart from *code execution*, and a panel is where the two most
//! easily get confused. A row is words on the operator's screen inside his own window. It can
//! lie, and no parser here stops it. What this module does about that is narrow and stated:
//! charter draws the contributing extension's id on the panel (ADR 0041 item 5 — *show what is
//! in force, after approval and not only at it*), the vocabulary has no way to say *where* so a
//! panel cannot cover a consent surface, and [`Row::runs`] is refused so a lie cannot be wired
//! to an action. What is left — a panel that says something untrue — is the same class as a
//! theme that paints *needs you* like idle, and it is the install-time decision's to carry.

use std::collections::BTreeSet;

/// The most panels one extension may contribute.
///
/// The region is one column in a window that has three other regions in it, and every panel in
/// it costs height that the needs-you queue — the one surface ADR 0038 says must never be
/// competed with — is sharing. A bound is what keeps "an extension contributed a panel" from
/// becoming "an extension took the region".
pub const MOST_PANELS: usize = 8;

/// The most rows one declared panel may hold.
///
/// The window bounds what it *draws* on its own ([`crate::panel`]'s consumer loads a page at a
/// time), so this is not the drawing budget — it is the budget for what charter holds in memory
/// per approved extension, from a file that is read at every launch.
pub const MOST_ROWS: usize = 500;

/// The most one row's words may be. A row is one line on screen; the window shortens it to fit
/// and the whole of it goes in the card the row opens.
pub const MOST_TEXT: usize = 1 << 10;

/// The most one row's card may say.
pub const MOST_DETAIL: usize = 8 << 10;

/// The mark a panel or a row carries, as a closed set of names charter owns.
///
/// **Names and never files.** ADR 0041's third crossing is *a reference to a file* — a font, an
/// image, an `@import` — because every one of them is a read of a path the extension chose. An
/// icon is exactly the shape that invites one, so the vocabulary is a word from this list and
/// the window maps it to a glyph it already ships.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mark {
    /// The open circle a todo carries.
    Todo,
    /// A person charter can run a chat as.
    Persona,
    /// A clone of a repository.
    Repo,
    /// A worktree charter cut.
    Piece,
    /// Something that wants reading.
    Note,
    /// Something that is wrong.
    Trouble,
    /// No claim about what the row is. What a declared panel gets when it says nothing.
    #[default]
    Dot,
}

impl Mark {
    /// The word charter publishes for it, which is the word a manifest writes.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Todo => "todo",
            Self::Persona => "persona",
            Self::Repo => "repo",
            Self::Piece => "piece",
            Self::Note => "note",
            Self::Trouble => "trouble",
            Self::Dot => "dot",
        }
    }

    /// Every word in the vocabulary, for a refusal that lists what was allowed instead of
    /// leaving the author to guess.
    pub fn every() -> [Self; 7] {
        [
            Self::Todo,
            Self::Persona,
            Self::Repo,
            Self::Piece,
            Self::Note,
            Self::Trouble,
            Self::Dot,
        ]
    }

    /// The mark that word names, or nothing.
    pub fn parse(word: &str) -> Option<Self> {
        Self::every().into_iter().find(|it| it.as_str() == word)
    }
}

/// How a row or a note reads, as a closed set of meanings rather than a colour.
///
/// **A meaning and never an appearance**, which is property 3 in the small. A theme decides what
/// `trouble` looks like (`app/src/theme/`) and a panel decides what *is* trouble; a vocabulary
/// that let a panel say *red* would be a panel choosing charter's palette, and ADR 0041's floor
/// — *the consent surfaces and the state colours are charter's* — would have no way to hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tone {
    #[default]
    Plain,
    /// The one its list treats as the default — the plane's default persona, and whatever
    /// answers the same question in a contributed list.
    Default,
    /// Something is wrong. Drawn as an alert, so a screen reader is told rather than shown.
    Trouble,
}

impl Tone {
    /// The word charter publishes for it.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Plain => "plain",
            Self::Default => "default",
            Self::Trouble => "trouble",
        }
    }

    /// Every word in the vocabulary, for a refusal that lists what was allowed.
    pub fn every() -> [Self; 3] {
        [Self::Plain, Self::Default, Self::Trouble]
    }

    /// The tone that word names, or nothing.
    pub fn parse(word: &str) -> Option<Self> {
        Self::every().into_iter().find(|it| it.as_str() == word)
    }
}

/// What opens when a row is opened.
///
/// **A closed set, and that is what keeps the card one pattern rather than two.** The persona
/// card (charter-app#173) is a popover anchored to the row it is about; the operator's word for
/// what a todo should do when it is clicked was *"like persona description"*. So there is one
/// popover and the vocabulary says what goes in it, rather than a second surface per panel.
///
/// [`Self::Persona`] is charter's own — it costs a `persona_details` call against the plane —
/// and a declared panel gets [`Self::Text`], which is its own longer words and no read of
/// anything. That is the same asymmetry [`Row::runs`] draws, one notch smaller: charter's
/// contribution may name a consumer that runs code, a stranger's may only carry data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Detail {
    /// The row's own words, in full. The only kind a declared panel may use.
    Text(String),
    /// What this plane says this persona is — `charter_core::personas::details`, read when the
    /// card is opened and never cached, because a definition is a file an operator edits.
    Persona(String),
}

/// What a panel or a view is ABOUT, as a closed set of subjects charter publishes.
///
/// **A subject and never a place.** ADR 0043's property 3 is that a panel never says *where*;
/// a subject says what a thing concerns, and charter decides where things about it are offered.
/// charter's own personas panel is about [`Self::Personas`]; an extension's view about
/// personas is therefore offered on that panel's heading and inside a persona's card — both of
/// which are charter's choices, made in the window, not the extension's.
///
/// **It is also what decides what charter hands a program** ([`crate::handed`]). A view about
/// personas is told this plane's persona names and when each memory was written, and nothing
/// else; a subject this list does not have is a view charter has nothing to hand, so the
/// manifest naming it is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Subject {
    /// The plane's personas and what each one remembers.
    Personas,
}

impl Subject {
    /// The word charter publishes for it, which is the word a manifest writes.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Personas => "personas",
        }
    }

    /// Every word in the vocabulary, for a refusal that lists what was allowed.
    pub fn every() -> [Self; 1] {
        [Self::Personas]
    }

    /// The subject that word names, or nothing.
    pub fn parse(word: &str) -> Option<Self> {
        Self::every().into_iter().find(|it| it.as_str() == word)
    }
}

/// How a chart is drawn, as a closed set of two.
///
/// **Two, because the two questions statistics answer are two**: how things compare
/// ([`Self::Bars`] — one bar per thing) and how one thing moves over a sequence
/// ([`Self::Columns`] — one column per step, in the order given). A pie, a line and a scatter
/// are each a third word, added the day a producer wants one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Shape {
    /// Horizontal bars, one per point, labelled at the start.
    #[default]
    Bars,
    /// Vertical columns, one per point, in the order given, labelled underneath.
    Columns,
}

impl Shape {
    /// The word charter publishes for it.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Bars => "bars",
            Self::Columns => "columns",
        }
    }

    /// Every word in the vocabulary, for a refusal that lists what was allowed.
    pub fn every() -> [Self; 2] {
        [Self::Bars, Self::Columns]
    }

    /// The shape that word names, or nothing.
    pub fn parse(word: &str) -> Option<Self> {
        Self::every().into_iter().find(|it| it.as_str() == word)
    }
}

/// One magnitude in a chart: a label, a whole number, and a short note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Point {
    pub label: String,
    /// **A whole number, and not a float**, because every statistic a producer has asked for
    /// is a count, and a count cannot be `NaN`, negative or `1e308`. A float is the day a
    /// producer wants a ratio, and it arrives with its own refusals.
    pub value: u32,
    /// Drawn quietly beside the value — a share, a date. Never a colour: see [`Tone`].
    pub note: Option<String>,
}

/// A chart: a title, a shape, what its values count, and the points.
///
/// **Numbers and words, and nothing that says how they look.** charter draws the bars from
/// theme tokens (`app/src/theme/`), scales them to the largest value itself, and writes every
/// label as a text node. A producer cannot choose a colour, a width, a font or an axis — ADR
/// 0041's four properties, one block further on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chart {
    pub title: String,
    pub shape: Shape,
    /// What the values count — `memories`. Said once, beside the title, rather than per point.
    pub unit: Option<String>,
    pub points: Vec<Point>,
}

/// One row of a list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// What this row is called inside its panel. Unique within it, because it is what the
    /// window remembers when it remembers which card is open.
    pub key: String,
    /// The words. One line on screen; the window shortens what does not fit rather than
    /// wrapping, and the card carries the whole of it.
    pub text: String,
    /// A short trailing note — a date, a count. Drawn quietly beside the words.
    pub note: Option<String>,
    pub mark: Mark,
    pub tone: Tone,
    /// What opens when the row is opened, if anything. A row with none is not a button.
    pub detail: Option<Detail>,
    /// The catalogue row this runs when it is pressed, by id (`app/src/actions.ts`).
    ///
    /// **Never populated from a manifest.** See this module's header: a declared row that ran a
    /// charter verb would be an executor with no executor behind it. charter's own
    /// contributions set it, in Rust, and the window looks the id up in the catalogue — so a
    /// row cannot invent a verb even here, and an id the catalogue has stopped offering draws
    /// as no button at all rather than as a dead one.
    pub runs: Option<String>,
}

/// What a list says when it has no rows.
///
/// **It is part of the contract rather than the window's default**, because the sentence is the
/// panel's to write: *"Nothing to do"* and *"No personas on this plane"* are different claims
/// and a shared default would make them the same one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Empty {
    /// The line in the middle of the empty panel.
    pub headline: String,
    /// A sentence under it, when there is more to say than the headline.
    pub body: Option<String>,
    /// The catalogue row the empty state offers as a way out, by id.
    ///
    /// Charter's own, for [`Row::runs`]'s reason — an empty state with a button is an action,
    /// and an action is a verb.
    pub offer: Option<String>,
}

/// One part of a panel's body.
///
/// Three kinds. The first two are what a panel has needed since the contract was written: a
/// list of things, and a sentence about why there is no list. **The third, [`Self::Chart`],
/// arrived with the first producer that wanted one** — the persona statistics view, which is
/// the executor's first consumer — and not before it, which is ADR 0041's rule about
/// capabilities invented for hypothetical extensions, applied to a vocabulary. It is accepted
/// in an answer ([`answered`]) and has no way into a manifest ([`declared`]); the module header
/// says why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Block {
    /// A bounded, searchable list of rows. The window draws at most a page of them, scrolls
    /// inside the panel, offers more, and shows a search once there are more than a page.
    List { rows: Vec<Row>, empty: Empty },
    /// A sentence. A [`Tone::Trouble`] one is what charter could not do, in its own words, and
    /// is drawn as an alert.
    Note { text: String, tone: Tone },
    /// Magnitudes, drawn by charter. Only ever answered, never declared.
    Chart(Chart),
}

/// Who contributed a panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum By {
    /// charter itself, in this process. The only contributor whose rows may carry a verb.
    Charter,
    /// An approved extension, by id.
    Extension(String),
}

/// A panel, which is a title, an ordering and a body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Panel {
    /// What this panel is called by whoever contributed it. One segment.
    pub id: String,
    /// What the heading says.
    pub title: String,
    /// Where it goes among the others, low first. **A hint and not a position**: charter sorts
    /// by it and breaks ties by contributor and id, so two extensions that both said `10` get a
    /// stable order rather than the order the disk was read in.
    pub order: i32,
    pub mark: Mark,
    pub blocks: Vec<Block>,
    pub from: By,
    /// What this panel is about, when it is about a subject charter publishes. charter's own
    /// personas panel is, and the views about the same subject are offered on its heading.
    /// **Never from a manifest**: a declared panel that claimed a subject would be choosing
    /// which of charter's surfaces a stranger's view is offered on, and that is charter's to
    /// choose.
    pub about: Option<Subject>,
}

impl Panel {
    /// The handle the window holds this panel by, unique across every contributor.
    ///
    /// **Namespaced rather than trusted to be unique.** An extension's id and a panel's id are
    /// both one segment, and `/` is in neither, so `charter/todos` and `ext/<id>/<panel>` can
    /// never collide — including with an extension that calls itself `charter`, which nothing
    /// forbids and which would otherwise be able to answer to charter's own panel's name.
    pub fn key(&self) -> String {
        match &self.from {
            By::Charter => format!("charter/{}", self.id),
            By::Extension(id) => format!("ext/{id}/{}", self.id),
        }
    }

    /// The order charter draws them in: by `order`, then charter's own before a stranger's,
    /// then by id. Total and stable, so the region does not reshuffle between launches.
    pub fn sort(panels: &mut [Self]) {
        panels.sort_by(|a, b| {
            a.order
                .cmp(&b.order)
                .then_with(|| by_rank(&a.from).cmp(&by_rank(&b.from)))
                .then_with(|| a.id.cmp(&b.id))
        });
    }
}

/// Charter's own first, then extensions by id.
fn by_rank(from: &By) -> (u8, &str) {
    match from {
        By::Charter => (0, ""),
        By::Extension(id) => (1, id.as_str()),
    }
}

/// One line for the consent prompt, for each panel an extension declares.
///
/// The operator reads this before he says yes, so it says what the panel is called and how much
/// of his region it wants, rather than the id he has never seen.
pub fn declares(panel: &Panel) -> String {
    let rows: usize = panel
        .blocks
        .iter()
        .map(|block| match block {
            Block::List { rows, .. } => rows.len(),
            Block::Note { .. } | Block::Chart(_) => 0,
        })
        .sum();
    format!(
        "a panel, “{}” — {rows} row{} charter draws in the window's side region",
        panel.title,
        if rows == 1 { "" } else { "s" }
    )
}

/// Read the panels a manifest's `contributes.panels` declares, or say why charter will not.
///
/// **Every refusal here is a state that would otherwise read as "nothing declared", which reads
/// as "safe"** — [`crate::extension::read_at`]'s rule, and the reason this returns an error for
/// the whole extension rather than dropping the panel it could not read. An extension whose
/// panel charter silently dropped would be approved for a contribution the operator saw and
/// would then not have it, which is the consent surface lying in the harmless direction and
/// then being trusted in the other one.
pub fn declared(value: &serde_json::Value, by: &str) -> Result<Vec<Panel>, String> {
    let list = value
        .as_array()
        .ok_or("has a 'contributes.panels' that is not an array")?;
    if list.len() > MOST_PANELS {
        return Err(format!(
            "declares {} panels, and charter draws at most {MOST_PANELS} from one extension",
            list.len()
        ));
    }
    let mut seen = BTreeSet::new();
    let mut panels = Vec::with_capacity(list.len());
    for (at, raw) in list.iter().enumerate() {
        let panel = one(raw, by).map_err(|why| format!("declares a panel at {at} that {why}"))?;
        if !seen.insert(panel.id.clone()) {
            return Err(format!(
                "declares two panels called {:?}, and a panel's id is how charter tells them \
                 apart",
                panel.id
            ));
        }
        panels.push(panel);
    }
    Ok(panels)
}

/// The keys a declared panel may carry. Anything else is refused: see property 1.
const PANEL_KEYS: [&str; 6] = ["id", "title", "order", "mark", "rows", "empty"];

/// The keys a declared row may carry.
const ROW_KEYS: [&str; 6] = ["key", "text", "note", "mark", "tone", "detail"];

/// The keys a declared empty state may carry.
const EMPTY_KEYS: [&str; 2] = ["headline", "body"];

fn one(raw: &serde_json::Value, by: &str) -> Result<Panel, String> {
    let object = raw.as_object().ok_or("is not an object")?;
    only(object.keys().map(String::as_str), &PANEL_KEYS, "a panel")?;

    let id = object
        .get("id")
        .and_then(serde_json::Value::as_str)
        .ok_or("has no id")?;
    // A panel id is a path segment for [`Panel::key`]'s reason and an extension id's charset
    // for the same reason that has one: it is a key, and a key that can be a path is a key that
    // means a file.
    if !crate::contain::segment_ok(id)
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        || !id.starts_with(|c: char| c.is_ascii_alphanumeric())
    {
        return Err(format!(
            "has the id {id:?}, and a panel's id is letters, digits, '-' and '_', starting with \
             a letter or a digit"
        ));
    }

    let title = words(object.get("title"), MOST_TEXT, "title")?.ok_or("has no title")?;

    let order = match object.get("order") {
        None => 0,
        Some(value) => i32::try_from(
            value
                .as_i64()
                .ok_or("has an 'order' that is not a whole number")?,
        )
        .map_err(|_| "has an 'order' charter cannot order by")?,
    };

    let mark = mark_of(object.get("mark"))?;

    let rows = rows_of(object.get("rows"))?;
    let empty = empty_of(object.get("empty"))?;

    Ok(Panel {
        id: id.to_owned(),
        title,
        order,
        mark,
        blocks: vec![Block::List { rows, empty }],
        from: By::Extension(by.to_owned()),
        about: None,
    })
}

/// The sentence charter says to an extension that tried to put one of its verbs on a row.
///
/// **It is a constant so that it is one sentence in one place**, said the same way about a row
/// and about an empty state's button, and changed only by changing a file with tests on it —
/// [`crate::extension::RUNS_AS_YOU`]'s reason, applied to the refusal rather than to the
/// consent.
pub const NO_VERB: &str = "names a charter action, and an extension may not put one on a row: \
     what an extension contributes to the window is data charter draws, whether its manifest \
     declared it or its program answered it, and a row that ran a charter verb on a click would \
     be charter acting on an extension's say-so with nothing in front of it.";

/// A list's rows, bounded, each with a key unique within the list.
///
/// One function for a declared panel and an answered list, so the two cannot drift: a row an
/// extension's program answers is held to exactly the rules a row its manifest declares is.
fn rows_of(value: Option<&serde_json::Value>) -> Result<Vec<Row>, String> {
    let listed = match value {
        None => return Ok(Vec::new()),
        Some(value) => value
            .as_array()
            .ok_or("has a 'rows' that is not an array")?,
    };
    if listed.len() > MOST_ROWS {
        return Err(format!(
            "declares {} rows, and charter holds at most {MOST_ROWS} per panel",
            listed.len()
        ));
    }
    let mut keys = BTreeSet::new();
    let mut rows = Vec::with_capacity(listed.len());
    for (at, raw) in listed.iter().enumerate() {
        let row = row_of(raw).map_err(|why| format!("has a row at {at} that {why}"))?;
        if !keys.insert(row.key.clone()) {
            return Err(format!(
                "has two rows called {:?}, and a row's key is what the window remembers it by",
                row.key
            ));
        }
        rows.push(row);
    }
    Ok(rows)
}

/// A list's empty state, or charter's default one.
fn empty_of(value: Option<&serde_json::Value>) -> Result<Empty, String> {
    let Some(value) = value else {
        return Ok(Empty {
            headline: "Nothing here".into(),
            body: None,
            offer: None,
        });
    };
    let object = value
        .as_object()
        .ok_or("has an 'empty' that is not an object")?;
    if object.contains_key("offer") {
        return Err(format!("has an empty state that {NO_VERB}"));
    }
    only(
        object.keys().map(String::as_str),
        &EMPTY_KEYS,
        "an empty state",
    )?;
    Ok(Empty {
        headline: words(object.get("headline"), MOST_TEXT, "empty.headline")?
            .unwrap_or_else(|| "Nothing here".into()),
        body: words(object.get("body"), MOST_DETAIL, "empty.body")?,
        // Never from an extension: an empty state's button is a verb, and a verb is charter's.
        // Refused rather than dropped — by name, above.
        offer: None,
    })
}

fn row_of(raw: &serde_json::Value) -> Result<Row, String> {
    let object = raw.as_object().ok_or("is not an object")?;
    if object.contains_key("runs") {
        return Err(NO_VERB.to_owned());
    }
    only(object.keys().map(String::as_str), &ROW_KEYS, "a row")?;
    let key = object
        .get("key")
        .and_then(serde_json::Value::as_str)
        .ok_or("has no key")?;
    if key.is_empty() || key.len() > MOST_TEXT || key.contains(undrawable) {
        return Err(format!(
            "has the key {key:?}, which charter will not hold it by"
        ));
    }
    Ok(Row {
        key: key.to_owned(),
        text: words(object.get("text"), MOST_TEXT, "text")?.ok_or("has no text")?,
        note: words(object.get("note"), MOST_TEXT, "note")?,
        mark: mark_of(object.get("mark"))?,
        tone: tone_of(object.get("tone"))?,
        // A declared card is the row's own longer words and nothing else: `Detail::Persona`
        // reads a file off the plane, which is a consumer charter chooses and not a value an
        // extension names.
        detail: words(object.get("detail"), MOST_DETAIL, "detail")?.map(Detail::Text),
        // Refused by name in `ROW_KEYS`, never silently dropped. See this module's header.
        runs: None,
    })
}

fn tone_of(value: Option<&serde_json::Value>) -> Result<Tone, String> {
    let Some(value) = value else {
        return Ok(Tone::Plain);
    };
    let word = value.as_str().ok_or("has a 'tone' that is not a word")?;
    Tone::parse(word).ok_or_else(|| {
        let every: Vec<&str> = Tone::every().iter().map(|it| it.as_str()).collect();
        format!(
            "has the tone {word:?}, and charter's tones are {} — a panel says what a row means, \
             never what colour it is",
            every.join(", ")
        )
    })
}

fn mark_of(value: Option<&serde_json::Value>) -> Result<Mark, String> {
    let Some(value) = value else {
        return Ok(Mark::Dot);
    };
    let word = value.as_str().ok_or("has a 'mark' that is not a word")?;
    Mark::parse(word).ok_or_else(|| {
        let every: Vec<&str> = Mark::every().iter().map(|it| it.as_str()).collect();
        format!(
            "has the mark {word:?}, and charter's marks are {} — an extension names one, it \
             does not bring one",
            every.join(", ")
        )
    })
}

/// Whether `c` is a character charter refuses to draw in anything an extension wrote: a control
/// character, a line or paragraph separator, or a **format** character (Unicode category `Cf`).
///
/// `Cf` is the one `char::is_control` does not cover and the one that matters most here. It
/// holds the bidirectional overrides and isolates (U+202A–202E, U+2066–2069), which draw nothing
/// and reverse what follows them — so a view titled `Stats\u{202E}` would draw the ` · <id>`
/// charter puts after it backwards, and the surface would no longer say which extension it came
/// from. It also holds the zero-width characters (U+200B–200F, U+2060, U+FEFF) that make two
/// different strings look identical. The set is [`crate::tui::tables::INVISIBLE`], which CPython
/// wrote for `charter/contain.py`, less its ordinary whitespace — a space or a tab in a row's
/// text is content, and draws as what it is.
pub(crate) fn undrawable(c: char) -> bool {
    c.is_control()
        || matches!(c, '\u{2028}' | '\u{2029}')
        || (crate::tui::in_table(c, &crate::tui::tables::INVISIBLE) && !c.is_whitespace())
}

/// A string charter will put on the screen: present, non-empty, bounded, and with nothing in it
/// that is not a character.
///
/// **Control and format characters are refused rather than stripped** ([`undrawable`]). A row's
/// text is drawn as a text node, so there is no injection to filter; what such a character does
/// is make two different strings look like one on screen — or turn the words around it
/// backwards — which is the deception half of ADR 0041's concern and the half no escaping helps
/// with.
fn words(
    value: Option<&serde_json::Value>,
    most: usize,
    what: &str,
) -> Result<Option<String>, String> {
    let Some(value) = value else { return Ok(None) };
    let text = value
        .as_str()
        .ok_or_else(|| format!("has a '{what}' that is not text"))?
        .trim();
    if text.is_empty() {
        return Ok(None);
    }
    if text.len() > most {
        return Err(format!(
            "has a '{what}' of {} bytes, and charter draws at most {most}",
            text.len()
        ));
    }
    if text.contains(undrawable) {
        return Err(format!(
            "has a '{what}' holding a control character or an invisible formatting one, which \
             charter will not draw"
        ));
    }
    Ok(Some(text.to_owned()))
}

/// Refuse a key charter did not publish, naming it and naming what was allowed.
///
/// This is property 1's enforcement and it is the reason a declared panel cannot quietly grow a
/// field: a manifest written against a later charter is refused by this one with the key in the
/// message, rather than approved for a contribution half of which does nothing.
fn only<'a>(
    keys: impl Iterator<Item = &'a str>,
    allowed: &[&str],
    what: &str,
) -> Result<(), String> {
    for key in keys {
        if !allowed.contains(&key) {
            return Err(format!(
                "carries {key:?}, which is not part of what {what} may say — charter's panel \
                 vocabulary is {}",
                allowed.join(", ")
            ));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------
// What an extension's program answers (ADR 0041 stage 2)
// ---------------------------------------------------------------------------------------

/// The most blocks one answer may hold. A view is one surface the operator opened; sixteen
/// charts and lists in it is a report nobody reads, and past that it is a program that is not
/// answering the question it was asked.
pub const MOST_BLOCKS: usize = 16;

/// The most points one chart may hold. A bar per persona or a column per week fits well under
/// it; a chart of a thousand bars is a list, and the list block is the one that searches.
pub const MOST_POINTS: usize = 64;

/// The keys each kind of answered block may carry. Anything else is refused by name — property
/// 1, as [`PANEL_KEYS`] is for a declared panel.
const LIST_KEYS: [&str; 3] = ["kind", "rows", "empty"];
const NOTE_KEYS: [&str; 3] = ["kind", "text", "tone"];
const CHART_KEYS: [&str; 5] = ["kind", "title", "shape", "unit", "points"];
const POINT_KEYS: [&str; 3] = ["label", "value", "note"];

/// The blocks an extension's program answered, or why charter will not draw them.
///
/// **The same vocabulary as a declared panel, parsed by the same functions, plus one word.**
/// A list's rows go through [`rows_of`] and so through [`row_of`], which refuses `runs` with
/// [`NO_VERB`]; a note is a note; and a chart is [`Block::Chart`], which only an answer may
/// carry. Nothing here is evaluated, nothing names a file or a place, and an unknown key refuses
/// the whole answer rather than being dropped — for the reason [`declared`] gives, which is
/// stronger here: a program that answers a key this charter does not know is a program written
/// against a charter that is not this one, and half an answer drawn as though it were whole is
/// the consent surface lying in the harmless direction.
pub fn answered(value: &serde_json::Value) -> Result<Vec<Block>, String> {
    let list = value
        .as_array()
        .ok_or("answered 'blocks' that are not an array")?;
    if list.len() > MOST_BLOCKS {
        return Err(format!(
            "answered {} blocks, and charter draws at most {MOST_BLOCKS} in one view",
            list.len()
        ));
    }
    list.iter()
        .enumerate()
        .map(|(at, raw)| {
            block_of(raw).map_err(|why| format!("answered a block at {at} that {why}"))
        })
        .collect()
}

fn block_of(raw: &serde_json::Value) -> Result<Block, String> {
    let object = raw.as_object().ok_or("is not an object")?;
    let kind = object
        .get("kind")
        .and_then(serde_json::Value::as_str)
        .ok_or("says no 'kind', so charter cannot say what it is")?;
    match kind {
        "list" => {
            only(object.keys().map(String::as_str), &LIST_KEYS, "a list")?;
            Ok(Block::List {
                rows: rows_of(object.get("rows"))?,
                empty: empty_of(object.get("empty"))?,
            })
        }
        "note" => {
            only(object.keys().map(String::as_str), &NOTE_KEYS, "a note")?;
            Ok(Block::Note {
                text: words(object.get("text"), MOST_DETAIL, "text")?.ok_or("has no text")?,
                tone: tone_of(object.get("tone"))?,
            })
        }
        "chart" => {
            only(object.keys().map(String::as_str), &CHART_KEYS, "a chart")?;
            Ok(Block::Chart(chart_of(object)?))
        }
        other => Err(format!(
            "is a {other:?}, and charter draws a list, a note or a chart — a block says what it \
             holds, and charter decides what that looks like"
        )),
    }
}

fn chart_of(object: &serde_json::Map<String, serde_json::Value>) -> Result<Chart, String> {
    let title = words(object.get("title"), MOST_TEXT, "title")?.ok_or("has no title")?;
    let shape = match object.get("shape") {
        None => Shape::Bars,
        Some(value) => {
            let word = value.as_str().ok_or("has a 'shape' that is not a word")?;
            Shape::parse(word).ok_or_else(|| {
                let every: Vec<&str> = Shape::every().iter().map(|it| it.as_str()).collect();
                format!(
                    "has the shape {word:?}, and charter's chart shapes are {}",
                    every.join(", ")
                )
            })?
        }
    };
    let unit = words(object.get("unit"), MOST_TEXT, "unit")?;
    let listed = object
        .get("points")
        .ok_or("has no points")?
        .as_array()
        .ok_or("has 'points' that are not an array")?;
    if listed.len() > MOST_POINTS {
        return Err(format!(
            "has {} points, and charter draws at most {MOST_POINTS} in one chart",
            listed.len()
        ));
    }
    let mut points = Vec::with_capacity(listed.len());
    for (at, raw) in listed.iter().enumerate() {
        let point = point_of(raw).map_err(|why| format!("has a point at {at} that {why}"))?;
        points.push(point);
    }
    Ok(Chart {
        title,
        shape,
        unit,
        points,
    })
}

fn point_of(raw: &serde_json::Value) -> Result<Point, String> {
    let object = raw.as_object().ok_or("is not an object")?;
    only(object.keys().map(String::as_str), &POINT_KEYS, "a point")?;
    let label = words(object.get("label"), MOST_TEXT, "label")?.ok_or("has no label")?;
    // `as_u64` and then `u32`, so a negative, a fraction, a string of digits and a number past
    // four billion are each refused rather than rounded, wrapped or read as zero.
    let value = object
        .get("value")
        .ok_or("has no value")?
        .as_u64()
        .ok_or("has a value that is not a whole number of at least zero")?;
    let value = u32::try_from(value)
        .map_err(|_| format!("has the value {value}, and charter draws counts below 2^32"))?;
    Ok(Point {
        label,
        value,
        note: words(object.get("note"), MOST_TEXT, "note")?,
    })
}

#[cfg(test)]
mod tests;
