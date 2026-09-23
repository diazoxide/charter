//! The right-hand panels for the focused workspace: its repos, their branches, CI, its
//! todos and the plane's personas (spec decision 1). Read-only, as M1 says panels are.
//!
//! # Two answers, and that is the design
//!
//! `of` is a directory listing and a handful of small files. It comes back at once, so the
//! todos and the persona row are on screen the moment a workspace is focused.
//!
//! `repo_states` runs `git status` once per clone, which is bounded at five seconds each and
//! is the only slow thing a panel does. It is its own command so that nothing else waits for
//! it, and the window draws "reading…" in the meantime.
//!
//! **Nothing here crosses a network, at all.** The CI cell is read out of
//! `.charter/cache/glstate.json`, which some other process writes; `charter_core::cistate`
//! says why in full. A panel that fetched would put a forge token in the process that draws
//! the window and hold that window for as long as `gh` takes.

use std::path::Path;

use charter_core::cistate::{self, Reading};
use charter_core::panel;
use charter_core::repos::{self, Head};
use charter_core::workspaces::Plane;

// ---------------------------------------------------------------------------------------
// The contribution contract, on the wire
// ---------------------------------------------------------------------------------------

/// What opens when a row is opened, as the window receives it.
///
/// A mirror of [`panel::Detail`] rather than the thing itself, for the reason
/// [`crate::extensions::ExtensionAsk`] is one: `charter-core` never depends on the app, and the
/// app's wire types are what generate `app/src/bindings.ts`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(crate) enum PanelDetail {
    /// The row's own words, in full. The only kind a contributed panel may use.
    Text { text: String },
    /// What this plane says this persona is. **A name and not an answer**: the window asks
    /// `persona_details` when the card opens, because a definition is a file an operator edits
    /// while charter is running, and because folding it in here would read every persona's
    /// definition on every workspace focus for something nobody has asked to see.
    Persona { persona: String },
}

/// One row of a panel's list.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub(crate) struct PanelRow {
    pub key: String,
    pub text: String,
    /// A short trailing note — a date, a word like `default`.
    pub note: Option<String>,
    /// A word out of [`panel::Mark`]'s closed set. The window maps it to a glyph it already
    /// ships; a word it does not know draws the plain one rather than nothing, because a
    /// missing icon is cosmetic and a missing row is not.
    pub mark: String,
    /// A word out of [`panel::Tone`]'s closed set.
    pub tone: String,
    pub detail: Option<PanelDetail>,
    /// The catalogue row this runs when pressed (`app/src/actions.ts`), or nothing.
    ///
    /// **Never set from a manifest** — `charter_core::panel`'s header has the whole of why, and
    /// `panel::NO_VERB` is the sentence an extension that tried gets. What is here comes from
    /// charter's own contributions, below, and the window looks the id up in the catalogue: a
    /// row cannot invent a verb even here.
    pub runs: Option<String>,
}

/// What a list says when it has no rows.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub(crate) struct PanelEmpty {
    pub headline: String,
    pub body: Option<String>,
    /// The catalogue row the empty state offers as a way out. charter's own, for `runs`'s
    /// reason.
    pub offer: Option<String>,
}

/// One part of a panel's body.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(crate) enum PanelBlock {
    List {
        rows: Vec<PanelRow>,
        empty: PanelEmpty,
    },
    Note {
        text: String,
        tone: String,
    },
    /// Magnitudes charter draws — only ever in an answer from an extension's program
    /// (`panel::answered`), never declared. See `charter_core::panel`'s header for why.
    Chart {
        title: String,
        /// `bars` or `columns` (`panel::Shape`).
        shape: String,
        unit: Option<String>,
        points: Vec<PanelPoint>,
    },
}

/// One magnitude in a chart.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub(crate) struct PanelPoint {
    pub label: String,
    /// A whole count. `u32` so it is a `number` in TypeScript and not a `bigint`.
    pub value: u32,
    pub note: Option<String>,
}

/// A panel, as the window receives it: a key, a title, an ordering and a body.
///
/// **This is the whole of what a panel is**, and the window's renderer takes nothing else. That
/// is the test of the contract: charter's own todos and personas arrive in this shape, an
/// approved extension's declared panel arrives in this shape, and `app/src/Panels.tsx` cannot
/// tell them apart except by [`Self::from`] — which it draws, because ADR 0041 item 5 says what
/// is in force is shown after approval and not only at it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub(crate) struct PanelView {
    /// `charter/<id>` or `ext/<extension>/<id>` — see `panel::Panel::key`.
    pub key: String,
    pub title: String,
    pub order: i32,
    pub mark: String,
    pub blocks: Vec<PanelBlock>,
    /// The extension that contributed it, or `null` for charter's own.
    pub from: Option<String>,
    /// What it is about (`panel::Subject`), when it is about a subject charter publishes. The
    /// window offers the views about the same subject on this panel's heading — which is
    /// charter's choice of where, made once, rather than an extension's.
    pub about: Option<String>,
}

impl From<&panel::Panel> for PanelView {
    fn from(it: &panel::Panel) -> Self {
        Self {
            key: it.key(),
            title: it.title.clone(),
            order: it.order,
            mark: it.mark.as_str().to_owned(),
            blocks: it.blocks.iter().map(PanelBlock::from).collect(),
            from: match &it.from {
                panel::By::Charter => None,
                panel::By::Extension(id) => Some(id.clone()),
            },
            about: it.about.map(|about| about.as_str().to_owned()),
        }
    }
}

impl From<&panel::Block> for PanelBlock {
    fn from(it: &panel::Block) -> Self {
        match it {
            panel::Block::List { rows, empty } => Self::List {
                rows: rows.iter().map(PanelRow::from).collect(),
                empty: PanelEmpty {
                    headline: empty.headline.clone(),
                    body: empty.body.clone(),
                    offer: empty.offer.clone(),
                },
            },
            panel::Block::Note { text, tone } => Self::Note {
                text: text.clone(),
                tone: tone.as_str().to_owned(),
            },
            panel::Block::Chart(chart) => Self::Chart {
                title: chart.title.clone(),
                shape: chart.shape.as_str().to_owned(),
                unit: chart.unit.clone(),
                points: chart
                    .points
                    .iter()
                    .map(|point| PanelPoint {
                        label: point.label.clone(),
                        value: point.value,
                        note: point.note.clone(),
                    })
                    .collect(),
            },
        }
    }
}

impl From<&panel::Row> for PanelRow {
    fn from(it: &panel::Row) -> Self {
        Self {
            key: it.key.clone(),
            text: it.text.clone(),
            note: it.note.clone(),
            mark: it.mark.as_str().to_owned(),
            tone: it.tone.as_str().to_owned(),
            detail: it.detail.as_ref().map(|detail| match detail {
                panel::Detail::Text(text) => PanelDetail::Text { text: text.clone() },
                panel::Detail::Persona(name) => PanelDetail::Persona {
                    persona: name.clone(),
                },
            }),
            runs: it.runs.clone(),
        }
    }
}

/// One open todo. There is no state field: a closed todo is a deleted file (ADR 0004).
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub(crate) struct PanelTodo {
    /// The file stem, which is what a todo is closed by.
    slug: String,
    title: String,
    /// The date the todo was written, as the file records it.
    stamp: String,
}

/// Everything the panels can draw without running git.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub(crate) struct Panels {
    /// The workspace this is about, so a late answer can be matched to the ask and an answer
    /// for a workspace that is no longer focused can be thrown away.
    workspace: String,
    /// The clones on disk, by name, in the order the directory lists them.
    repos: Vec<String>,
    /// Repos `workspace.json` names that are not cloned here. Membership, not presence.
    absent: Vec<String>,
    /// What charter would not look at, by name and reason. Shown, never dropped: a row that
    /// is missing is otherwise merely missing.
    refused: Vec<(String, String)>,
    todos: Vec<PanelTodo>,
    /// Why the todos could not be read, where they could not. A store that is a link out of
    /// the plane is refused, and "no todos" would be the wrong thing to draw for it.
    todos_refused: Option<String>,
    /// The plane's personas, and the one a chat started here would adopt.
    personas: Vec<String>,
    persona: Option<String>,
    /// **The same facts again, as contributions** — charter's own two panels, in the shape a
    /// stranger's extension contributes one in (`charter_core::panel`).
    ///
    /// # Why the fields above survived, which is a decision and not an oversight
    ///
    /// `todos`, `personas` and `persona` are not panel bodies. They are facts about the
    /// workspace that three other surfaces read: the status line counts the todos, the
    /// catalogue builds a `persona.show:<name>` row per persona, and a test pins the default.
    /// Deleting them would have moved those three onto a shape designed for drawing, which is
    /// the opposite of the separation this change is for. **What moved is the drawing**:
    /// `Panels.tsx` reads `contributed` and nothing else, so the panels on screen do come
    /// through the seam.
    ///
    /// # And why they are in THIS call, when the brief said a contributed panel cannot be
    ///
    /// The thing a contributed panel cannot be is a *field*. `Panels` names `todos` and
    /// `personas`; there is no field for a panel nobody has written yet, and there is no
    /// honest way to add one. A *list* has room for every contributor. The round trip was never
    /// the problem, and splitting it would cost something real: focusing a workspace has 100 ms
    /// and this command is the one that has to answer inside it.
    ///
    /// **A contributed panel needs no round trip of its own.** An extension's panel is
    /// declared, so its rows came off the disk at survey time, and charter's own are produced
    /// from the plane read this command already does. What an extension answers LIVE is a
    /// *view* (`crate::views`), asked when the operator opens it and never on this path: a
    /// program started on every workspace focus would spend the 100 ms on a fork.
    contributed: Vec<PanelView>,
}

/// What one persona says about itself, for the row a reader clicked.
///
/// **The vault is a NAME and nothing else.** charter refuses a secret by kind and never
/// echoes one, and a panel is the last place that rule should get a special case: this struct
/// carries the word `vault: <name>` puts in the definition, so the window can say which vault
/// a chat as this persona would open. Nothing here ever reads the vault.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub(crate) struct PersonaDetails {
    name: String,
    /// `role:`, inherited-inclusive.
    role: Option<String>,
    /// `delegate-when:` — the work that should come to this persona, which is what makes it
    /// findable and what a router reads.
    delegate_when: Option<String>,
    /// `tools:`, the union down the `extends:` chain.
    tools: Vec<String>,
    /// The vault's name, where the definition declares one.
    vault: Option<String>,
    /// Whether the definition declares `vault: none` — that it holds no credentials at all.
    ///
    /// **Separate from `vault` being absent, and the window must keep them apart.** charter's
    /// own `vault_of` falls back to a vault tagged with this persona in the registry, and
    /// nothing in Rust reads that registry yet — so "no `vault:` line" means charter-app has
    /// not looked, not that there is nothing. Drawing the two the same way would have the
    /// window claim a persona holds no credentials on the strength of a file nobody read.
    declares_no_vault: bool,
    /// The `extends:` chain, child first. One name long for a persona that extends nothing.
    lineage: Vec<String>,
    /// The definition file, relative to the plane.
    file: String,
}

/// What a persona's definition says, or charter's own sentence saying why it will not answer.
pub(crate) fn persona(root: &Path, name: &str) -> Result<PersonaDetails, String> {
    let shown = charter_core::personas::details(root, name)?;
    let (vault, declares_no_vault) = match shown.vault {
        charter_core::personas::Vault::Named(name) => (Some(name), false),
        charter_core::personas::Vault::DeclaredNone => (None, true),
        charter_core::personas::Vault::Undeclared => (None, false),
    };
    Ok(PersonaDetails {
        name: shown.name,
        role: shown.role,
        delegate_when: shown.delegate_when,
        tools: shown.tools,
        vault,
        declares_no_vault,
        lineage: shown.lineage,
        file: shown.file,
    })
}

/// One clone's git state, and what the forge cache last recorded for its branch.
#[derive(Debug, Clone, Default, serde::Serialize, specta::Type)]
pub(crate) struct RepoState {
    name: String,
    /// The branch the checkout is on, where it is on one.
    branch: Option<String>,
    /// Whether that branch holds no commit yet.
    unborn: bool,
    /// The commit HEAD sits on when it is on no branch.
    detached: Option<String>,
    upstream: Option<String>,
    ahead: u32,
    behind: u32,
    /// Changed files git is tracking.
    tracked: u32,
    /// Files git is not tracking.
    untracked: u32,
    /// Why charter could not read the tree. Every count above is zero when this is set, and
    /// it means charter does not know — not that the tree is clean.
    unreadable: Option<String>,
    /// What the forge cache last recorded, one of the seven states charter knows.
    ci: Option<String>,
    change: Option<u32>,
    sigil: Option<String>,
    /// How long ago the cache entry was written, which is how old this answer is.
    fetched_seconds_ago: Option<u32>,
    /// Why there is nothing from the forge to show. Absent when something was fetched, even
    /// when what was fetched named no pipeline.
    not_fetched: Option<String>,
}

/// Every clone of one workspace, as the panels draw them.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub(crate) struct RepoStates {
    workspace: String,
    repos: Vec<RepoState>,
    /// Why the forge cache was not read at all, where it was not. Reported once for the
    /// listing rather than repeated on every row.
    cache_refused: Option<String>,
}

/// The panels that need no git, for one workspace of one project.
///
/// **The project is handed in, never resolved here.** This used to walk up from the process's
/// working directory — the singleton resolver ADR 0034 was written to remove — which was
/// already wrong the moment #121 let a window open a project the launch had not, and is
/// plainly wrong now that a window holds several: the panels would have drawn the workspace of
/// whichever project the process happened to start in, under the heading of the one on screen.
pub(crate) fn of(root: &Path, workspace: &str) -> Result<Panels, String> {
    let plane = Plane::open(root);
    let found = repos::clones(root, workspace).map_err(|why| why.to_string())?;
    let here: Vec<String> = found.repos.iter().map(|repo| repo.name.clone()).collect();
    let absent = repos::declared(&plane, workspace)
        .into_iter()
        .filter(|name| !here.contains(name))
        .collect();
    let ws = plane.workspace(workspace).map_err(|why| why.to_string())?;
    let read = ws.todos();
    let (todos, todos_refused): (Vec<PanelTodo>, Option<String>) = match &read {
        Ok(open) => (
            open.iter()
                .map(|todo| PanelTodo {
                    slug: todo.slug.clone(),
                    title: todo.title.clone(),
                    stamp: todo.stamp.clone(),
                })
                .collect(),
            None,
        ),
        Err(why) => (Vec::new(), Some(why.to_string())),
    };
    let personas = plane.personas().map_err(|why| why.to_string())?;
    let persona = plane.default_persona();
    let contributed = charters_own(
        root,
        read.as_deref().unwrap_or(&[]),
        todos_refused.as_deref(),
        &personas,
        persona.as_deref(),
    );
    Ok(Panels {
        workspace: workspace.to_string(),
        repos: here,
        absent,
        refused: found.refused,
        todos,
        todos_refused,
        personas,
        persona,
        contributed,
    })
}

/// charter's own two panels, built through the seam a stranger's extension contributes through.
///
/// **This is the proof, and it is the whole reason the contract is worth having.** These two
/// were hardcoded React fed by named fields; they are now `panel::Panel` values that go down the
/// same wire, into the same renderer, drawn by the same list primitive as a declared panel.
/// Everything the window knows about a todo row it learned from the vocabulary.
///
/// **Where they differ from a stranger's, said out loud rather than smoothed over.** Two things,
/// and both are the same thing: charter is code that is already in the process.
///
/// 1. **Their rows carry `runs`** — a persona row runs `persona.show:<name>`, which is a
///    catalogue row the palette and a context menu already run (charter-app#174). A declared or
///    answered row may not (`panel::NO_VERB`): a row that ran a charter verb on an extension's
///    say-so would be charter acting with nothing in front of it.
/// 2. **Their cards may name a consumer that reads the plane** — `Detail::Persona` costs a
///    `persona_details` call. A declared card is `Detail::Text`, which reads nothing.
///
/// Neither is a privilege of being charter, and neither is permanent. The grant that lifts the
/// first is *this extension may offer this catalogue row*, consented per extension per row —
/// and nobody has asked for it yet, which under ADR 0041 is the reason it does not exist.
fn charters_own(
    root: &Path,
    todos: &[charter_core::workspaces::Entry],
    todos_refused: Option<&str>,
    personas: &[String],
    default: Option<&str>,
) -> Vec<PanelView> {
    let mut panels = Vec::new();

    // **The todos, at 10.** `order` is a hint charter sorts by and these two are deliberately
    // ten apart, so a contributed panel has somewhere to land between them without either of
    // charter's moving.
    let mut blocks = Vec::new();
    if let Some(why) = todos_refused {
        // Drawn and never swallowed: a store charter would not read is not a workspace with
        // nothing to do, and "Nothing to do" is what an empty list would say about it.
        blocks.push(panel::Block::Note {
            text: why.to_owned(),
            tone: panel::Tone::Trouble,
        });
    }
    blocks.push(panel::Block::List {
        rows: todos
            .iter()
            .map(|todo| panel::Row {
                key: todo.slug.clone(),
                text: todo.title.clone(),
                note: (!todo.stamp.is_empty()).then(|| todo.stamp.clone()),
                mark: panel::Mark::Todo,
                tone: panel::Tone::Plain,
                // **The operator's own request** — *"same as persona description"*: a row is one
                // line and the card carries the whole of it. The body when the todo has one,
                // and the untruncated title when it does not, because the complaint the card
                // answers is a title that broke across three lines.
                detail: Some(panel::Detail::Text(if todo.body.trim().is_empty() {
                    todo.title.clone()
                } else {
                    todo.body.clone()
                })),
                runs: None,
            })
            .collect(),
        empty: panel::Empty {
            headline: "Nothing to do".into(),
            body: Some("Todos are files in this workspace's store (charter ADR 0004).".into()),
            offer: None,
        },
    });
    panels.push(panel::Panel {
        id: "todos".into(),
        title: "Todos".into(),
        order: 10,
        mark: panel::Mark::Todo,
        blocks,
        from: panel::By::Charter,
        about: None,
    });

    panels.push(panel::Panel {
        blocks: vec![panel::Block::List {
            rows: personas
                .iter()
                .map(|name| panel::Row {
                    key: name.clone(),
                    text: name.clone(),
                    // **The word stays a word, and now it carries a count.** The region's
                    // scenario spec asks the panel whether it says `default`, and a screen
                    // reader gets the sentence a sighted reader does; the star the tone draws
                    // is decoration on top of it.
                    //
                    // The count is `personas::memory_count`, which is a `read_dir` and no file
                    // opens — once per persona on the path that has 100 ms to draw. Reading
                    // them is `persona_memories`, when a reader asks.
                    note: Some(note_for(root, name, Some(name.as_str()) == default)),
                    mark: panel::Mark::Persona,
                    tone: if Some(name.as_str()) == default {
                        panel::Tone::Default
                    } else {
                        panel::Tone::Plain
                    },
                    detail: Some(panel::Detail::Persona(name.clone())),
                    runs: Some(format!("persona.show:{name}")),
                })
                .collect(),
            empty: panel::Empty {
                headline: "No personas on this plane".into(),
                body: Some("`charter persona create` is how one arrives.".into()),
                offer: None,
            },
        }],
        id: "personas".into(),
        title: "Personas".into(),
        order: 20,
        mark: panel::Mark::Persona,
        from: panel::By::Charter,
        // **What makes the statistics button appear on this heading**, and the only thing:
        // an approved extension's view about personas is offered where charter's panel about
        // personas is. charter chose the place; the extension chose nothing but its subject.
        about: Some(panel::Subject::Personas),
    });

    panel::Panel::sort(&mut panels);
    panels.iter().map(PanelView::from).collect()
}

/// What a persona row's note says: whether the plane defaults to it, and how much it remembers.
///
/// **Both in one string, because `note` is one string and a second field would be a second
/// vocabulary word invented for one row.** `panel::Row::note` is *a short trailing note*; a
/// contributed panel that wants a count puts it here too, so nothing charter's own row does is
/// out of reach of a declared one.
///
/// `0 memories` is said rather than left off: a persona with none reads as one nobody has
/// taught anything, and a row that silently omits the count reads as one charter did not look
/// at. They are different facts.
fn note_for(root: &Path, persona: &str, is_default: bool) -> String {
    let held = charter_core::personas::memory_count(root, persona);
    let memories = format!("{held} {}", if held == 1 { "memory" } else { "memories" });
    if is_default {
        format!("default · {memories}")
    } else {
        memories
    }
}

/// One persona's memories, as rows of the same vocabulary a panel is drawn from.
///
/// **It answers in `PanelRow`s, and that is the point rather than a convenience.** The window's
/// list primitive shortens a row, opens its card, bounds the count, offers more and grows a
/// search once there is more than a page of them — and it does all of that for these without
/// knowing what a memory is, because they arrive as rows. A panel body and a row's detail
/// surface are the same vocabulary drawn by the same code, which is the test of whether the
/// contract was worth defining.
///
/// **Its own command, asked when a persona's card is opened.** `workspace_panels` carries the
/// *count*, which is a `read_dir`; this reads every memory file, and folding it in would read
/// every persona's whole store on every workspace focus for something nobody has asked to see —
/// `persona_details`' reason, at a larger size.
#[tauri::command]
#[specta::specta]
pub(crate) async fn persona_memories(
    planes: tauri::State<'_, crate::planes::Planes>,
    plane: crate::planes::PlaneId,
    persona: String,
) -> Result<Vec<PanelRow>, String> {
    let root = planes.held(&plane)?.root().to_path_buf();
    // On a blocking thread: a store is one file read per memory, and a plane's oldest persona
    // can hold hundreds. The card draws "reading…" while it comes.
    tauri::async_runtime::spawn_blocking(move || memories_of(&root, &persona))
        .await
        .map_err(|err| format!("reading that persona's memories did not finish: {err}"))?
}

/// [`persona_memories`] against a path, so it is a test's to drive.
fn memories_of(root: &Path, persona: &str) -> Result<Vec<PanelRow>, String> {
    Ok(charter_core::personas::memories(root, persona)?
        .iter()
        .map(|memory| {
            PanelRow::from(&panel::Row {
                key: memory.slug.clone(),
                text: memory.title.clone(),
                note: (!memory.stamp.is_empty()).then(|| memory.stamp.clone()),
                mark: panel::Mark::Note,
                tone: panel::Tone::Plain,
                // The whole of it, which is what the operator asked for: *"user will be able to
                // read all memories from ui"*. A memory's body is the durable fact; the title
                // is the sentence it is filed under.
                detail: Some(panel::Detail::Text(if memory.body.trim().is_empty() {
                    memory.title.clone()
                } else {
                    memory.body.clone()
                })),
                runs: None,
            })
        })
        .collect())
}

/// The panel that needs git. Call it off the thread that draws, for one project's workspace.
pub(crate) fn repo_states(root: &Path, workspace: &str) -> Result<RepoStates, String> {
    let binary = crate::charter_binary();
    states_of(root, workspace, binary.as_deref())
}

/// [`repo_states`] with the `charter` binary said out loud rather than discovered.
///
/// Split so the refresh trigger below is a test's to drive: `charter_binary` looks beside the
/// running executable, and a test that may not touch the environment cannot point it anywhere
/// (`std::env::set_var` is `unsafe`, and this workspace forbids that).
fn states_of(root: &Path, workspace: &str, binary: Option<&Path>) -> Result<RepoStates, String> {
    let found = repos::clones(root, workspace).map_err(|why| why.to_string())?;
    // Read once for the whole listing, so every row ages against the same instant and a
    // refusal is reported once rather than on every row.
    let (cache, cache_refused) = match cistate::read(root) {
        Ok(cache) => (Some(cache), None),
        Err(why) => (None, Some(why.to_string())),
    };
    // Read FIRST, then decide whether to refresh: this listing draws what the cache holds now,
    // and the refresh is for the next one. Python's render path does the two in this order for
    // the same reason (`glstate.read_for`, then `glstate.maybe_spawn`).
    refresh_if_it_is_due(root, workspace, binary);
    Ok(RepoStates {
        workspace: workspace.to_string(),
        repos: found
            .repos
            .iter()
            .map(|repo| one(repo, cache.as_ref()))
            .collect(),
        cache_refused,
    })
}

/// Kick off a background forge refresh for this workspace, if the policy says one is due —
/// charter-app#69.
///
/// **This is the trigger, and it is a user action rather than a timer.** Focusing a workspace
/// is what runs this panel, and no daemon runs anywhere in charter-app: an app nobody touches
/// makes no forge call, ever. `charter_core::glstate` holds the decision — the refresh window,
/// the cooldown, the stuck window, and the lock that names the refresh in flight — so that two
/// panels in quick succession are one refresh and a wedged one is not replaced every two
/// minutes, each replacement holding the forge credential.
///
/// It **spawns**, never waits: the spec's budget for a workspace switch is 100 ms, and the
/// slow thing in this command is already the one `git status` per clone above.
///
/// Silent where a refusal is routine — cooling down, already running, nothing stale — and out
/// loud where it is not, because a CI column that never fills over a `charter` binary that
/// went missing would otherwise look exactly like one nobody has refreshed.
fn refresh_if_it_is_due(root: &Path, workspace: &str, binary: Option<&Path>) {
    use charter_core::glstate::Refreshing;

    let Some(binary) = binary else {
        // Already said once, at startup, by the launch that could not find it. Saying it again
        // on every workspace focus would be the same sentence fifty times an hour.
        return;
    };
    // The same list the refresher itself walks, and the same one the panel draws.
    let Ok(targets) = charter_core::glrefresh::trees(root, workspace) else {
        return;
    };
    match charter_core::glstate::maybe_spawn(root, workspace, &targets.trees, binary) {
        // Cooling down, one already in flight, nothing stale, or the operator's own brake:
        // every one of these is the policy working, and none of them is news.
        Refreshing::Started { .. } | Refreshing::Declined(_) => {}
        Refreshing::NotStarted { why } => {
            eprintln!("charter: a forge refresh for '{workspace}' would not start ({why})");
        }
    }
}

/// One row: what git said, and then what the cache says about the branch git named.
fn one(repo: &repos::Repo, cache: Option<&cistate::Cache>) -> RepoState {
    let mut row = RepoState {
        name: repo.name.clone(),
        ..RepoState::default()
    };
    let state = match repos::state_of(&repo.path) {
        Ok(state) => state,
        Err(why) => {
            row.unreadable = Some(why.to_string());
            // The cache is keyed by branch, and charter does not know which branch this is.
            row.not_fetched = Some(
                "charter could not read the checkout, so it cannot say which branch to ask \
                 about"
                    .into(),
            );
            return row;
        }
    };
    let branch = match &state.head {
        Head::Branch(name) => {
            row.branch = Some(name.clone());
            Some(name.clone())
        }
        Head::Unborn(name) => {
            row.branch = Some(name.clone());
            row.unborn = true;
            Some(name.clone())
        }
        Head::Detached(at) => {
            row.detached = Some(at.clone());
            None
        }
    };
    row.upstream = state.upstream;
    row.ahead = state.ahead;
    row.behind = state.behind;
    row.tracked = state.tracked;
    row.untracked = state.untracked;

    let Some(branch) = branch else {
        row.not_fetched =
            Some("this checkout is on no branch, and forge state is recorded per branch".into());
        return row;
    };
    // A cache charter refused is reported once for the listing; leaving the row's own reason
    // empty is what keeps the window from saying the same sentence on every line.
    let Some(cache) = cache else {
        return row;
    };
    match cache.about(&repo.path, &branch) {
        Reading::Fetched {
            state,
            change,
            sigil,
            seconds_ago,
        } => {
            row.ci = state;
            row.change = u32::try_from(change.unwrap_or(0)).ok().filter(|n| *n > 0);
            row.sigil = sigil.map(String::from);
            row.fetched_seconds_ago = Some(u32::try_from(seconds_ago).unwrap_or(u32::MAX));
        }
        Reading::NotFetched(why) => row.not_fetched = Some(why),
    }
    row
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// A plane with one workspace holding one clone, and nothing fetched for it.
    fn plane_with_a_clone() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("a plane");
        let root = std::fs::canonicalize(dir.path()).expect("a resolved plane");
        std::fs::write(root.join("charter.toml"), "").expect("a manifest");
        std::fs::create_dir_all(root.join("workspaces/alpha/svc/.git")).expect("a clone");
        std::fs::write(
            root.join("workspaces/alpha/svc/.git/HEAD"),
            "ref: refs/heads/main\n",
        )
        .expect("a HEAD");
        (dir, root)
    }

    /// A stand-in `charter` that records how it was called and then ends at once.
    fn stand_in(at: &Path) -> PathBuf {
        let binary = at.join("charter-stand-in");
        std::fs::write(
            &binary,
            "#!/bin/sh\nprintf '%s %s\\n' \"$1\" \"$3\" >> \"$(dirname \"$0\")/ran\"\n",
        )
        .expect("the stand-in is written");
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o755))
            .expect("it is runnable");
        binary
    }

    /// Everything the stand-in has recorded so far, once it has recorded anything.
    fn ran(at: &Path) -> String {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            if let Ok(text) = std::fs::read_to_string(at.join("ran"))
                && !text.is_empty()
            {
                // Give a second line the chance to arrive, so "exactly one" is not merely
                // "the first one got there first".
                std::thread::sleep(std::time::Duration::from_millis(200));
                return std::fs::read_to_string(at.join("ran")).unwrap_or(text);
            }
            assert!(
                std::time::Instant::now() < deadline,
                "the refresh never ran"
            );
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }

    /// A plane with one workspace holding one todo, and two personas — one of them the
    /// plane's default, one of them with a memory.
    fn plane_with_a_todo_and_two_personas() -> (tempfile::TempDir, PathBuf) {
        let (dir, root) = plane_with_a_clone();
        std::fs::write(
            root.join("charter.toml"),
            "[persona]\ndefault = \"steward\"\n",
        )
        .expect("a manifest naming a default");
        std::fs::create_dir_all(root.join("workspaces/alpha/todos")).expect("a todo store");
        std::fs::write(
            root.join("workspaces/alpha/todos/20260302-091400-review.md"),
            "# Review the rollout plan\n\n_2026-03-02 09:14 · todo_\n\nEvery step of it.\n",
        )
        .expect("a todo");
        for who in ["devops", "steward"] {
            std::fs::create_dir_all(root.join("personas").join(who).join("memory"))
                .expect("a persona");
            std::fs::write(
                root.join("personas").join(who).join("persona.md"),
                format!("---\nrole: {who}\n---\n\n# {who}\n"),
            )
            .expect("a definition");
        }
        std::fs::write(
            root.join("personas/steward/memory/charter-defects-go-upstream.md"),
            "# Charter defects go upstream\n\n_2026-09-20 10:00 · durable_\n\nFile the issue.\n",
        )
        .expect("a memory");
        (dir, root)
    }

    #[test]
    fn charters_own_panels_come_through_the_same_seam_an_extension_would_use() {
        // **The claim the whole change rests on.** Todos and personas used to be named fields
        // that hardcoded React read. They are `charter_core::panel` values now, produced here
        // and drawn by the loop in `Panels.tsx` that draws a stranger's declared panel — so
        // what the window knows about a todo it learned from the vocabulary.
        let (_plane, root) = plane_with_a_todo_and_two_personas();

        let drawn = of(&root, "alpha").expect("the panels draw");

        let keys: Vec<&str> = drawn
            .contributed
            .iter()
            .map(|panel| panel.key.as_str())
            .collect();
        assert_eq!(keys, ["charter/todos", "charter/personas"]);
        assert!(
            drawn.contributed.iter().all(|panel| panel.from.is_none()),
            "charter's own panels named an extension as their contributor"
        );
    }

    #[test]
    fn a_todo_row_carries_its_whole_body_for_the_card_the_row_opens() {
        // The operator's own request — *"on clicking we should show full body"* — and the
        // reason the row itself is only the title: a row is one line and the card is the rest.
        let (_plane, root) = plane_with_a_todo_and_two_personas();

        let drawn = of(&root, "alpha").expect("the panels draw");

        let rows = list_of(&drawn.contributed[0]);
        assert_eq!(rows[0].text, "Review the rollout plan");
        // The stamp as the file records it, which is the store's own reading and not this
        // panel's — a row that reformatted a date would be a second answer to what a todo says.
        assert_eq!(rows[0].note.as_deref(), Some("2026-03-02 09:14"));
        assert_eq!(
            rows[0].detail,
            Some(PanelDetail::Text {
                text: "Every step of it.".into()
            })
        );
    }

    #[test]
    fn a_persona_row_says_how_much_it_remembers_without_reading_a_memory() {
        // The count is a `read_dir` on the path that has 100 ms to draw; the memories
        // themselves are `persona_memories`, when a reader asks. `0 memories` is said rather
        // than left off — a row that omits the count reads as one charter did not look at,
        // which is a different fact from a persona with none.
        let (_plane, root) = plane_with_a_todo_and_two_personas();

        let drawn = of(&root, "alpha").expect("the panels draw");

        let rows = list_of(&drawn.contributed[1]);
        let notes: Vec<Option<&str>> = rows.iter().map(|row| row.note.as_deref()).collect();
        assert_eq!(notes, [Some("0 memories"), Some("default · 1 memory")]);
        assert_eq!(rows[1].tone, "default");
        assert_eq!(rows[1].runs.as_deref(), Some("persona.show:steward"));
    }

    #[test]
    fn a_contributed_row_could_never_carry_the_verb_a_persona_row_does() {
        // The asymmetry, pinned where it is created rather than only where it is refused.
        // `charter_core::panel::declared` refuses `runs` from a manifest; this is the other
        // half — charter's own producer is the only thing that sets it, and it sets it to a
        // catalogue id the window looks up rather than to anything it made up.
        let (_plane, root) = plane_with_a_todo_and_two_personas();

        let drawn = of(&root, "alpha").expect("the panels draw");

        for row in list_of(&drawn.contributed[0]) {
            assert_eq!(row.runs, None, "a todo row grew a verb nobody asked for");
        }
        for row in list_of(&drawn.contributed[1]) {
            assert!(
                row.runs
                    .as_deref()
                    .is_some_and(|id| id.starts_with("persona.show:")),
                "a persona row's verb is not the catalogue row that opens its card"
            );
        }
    }

    #[test]
    fn a_memory_arrives_as_a_row_of_the_same_vocabulary_a_panel_is_drawn_from() {
        // **The second consumer of the list primitive**, and what makes it a primitive rather
        // than a panel with a general-sounding name: the window's search, bound and card work
        // on these without anything in it knowing what a memory is.
        let (_plane, root) = plane_with_a_todo_and_two_personas();

        let rows = memories_of(&root, "steward").expect("the memories read");

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].text, "Charter defects go upstream");
        assert_eq!(rows[0].mark, "note");
        assert_eq!(rows[0].runs, None);
        assert_eq!(
            rows[0].detail,
            Some(PanelDetail::Text {
                text: "File the issue.".into()
            })
        );
    }

    #[test]
    fn a_todo_store_charter_will_not_read_is_a_sentence_and_never_an_empty_list() {
        // "Nothing to do" is what an empty list would claim about a store charter refused, and
        // that claim is false in the direction that matters.
        let (_plane, root) = plane_with_a_clone();
        std::fs::create_dir_all(root.join("workspaces/alpha")).expect("the workspace");
        let outside = root.parent().expect("a parent").join("elsewhere");
        std::fs::create_dir_all(&outside).expect("somewhere outside");
        std::os::unix::fs::symlink(&outside, root.join("workspaces/alpha/todos"))
            .expect("a link out of the plane");

        let drawn = of(&root, "alpha").expect("the panels draw");

        let told = drawn.contributed[0]
            .blocks
            .iter()
            .find_map(|block| match block {
                PanelBlock::Note { text, tone } => Some((text.clone(), tone.clone())),
                PanelBlock::List { .. } | PanelBlock::Chart { .. } => None,
            })
            .expect("the refusal is drawn");
        assert_eq!(told.1, "trouble");
        assert!(!told.0.is_empty());
    }

    /// The rows of a panel's one list block.
    fn list_of(panel: &PanelView) -> &[PanelRow] {
        panel
            .blocks
            .iter()
            .find_map(|block| match block {
                PanelBlock::List { rows, .. } => Some(rows.as_slice()),
                PanelBlock::Note { .. } | PanelBlock::Chart { .. } => None,
            })
            .expect("a list block")
    }

    #[test]
    fn focusing_a_workspace_is_what_kicks_a_forge_refresh() {
        // **The wiring, and the whole of charter-app#69's visible half.** Until this, nothing
        // anywhere called the refresher: the CI column showed whatever the last
        // `charter gl-refresh` typed by hand had left. `glstate` holds the policy, and this is
        // the only thing that asks it — a user action, never a timer.
        let (_plane, root) = plane_with_a_clone();
        let beside = tempfile::tempdir().expect("somewhere for the stand-in");
        let binary = stand_in(beside.path());

        let drawn = states_of(&root, "alpha", Some(&binary)).expect("the panel draws");

        assert_eq!(drawn.workspace, "alpha");
        assert_eq!(
            ran(beside.path()).trim(),
            "gl-refresh alpha",
            "the refresh was not asked for the workspace that was focused"
        );
    }

    #[test]
    fn focusing_it_again_does_not_start_a_second_refresh() {
        // The cooldown reaching all the way out to the trigger: an operator clicking between
        // two workspaces, or a panel asked twice, is one forge process and not two.
        let (_plane, root) = plane_with_a_clone();
        let beside = tempfile::tempdir().expect("somewhere for the stand-in");
        let binary = stand_in(beside.path());

        states_of(&root, "alpha", Some(&binary)).expect("the panel draws");
        let once = ran(beside.path());
        states_of(&root, "alpha", Some(&binary)).expect("the panel draws again");
        std::thread::sleep(std::time::Duration::from_millis(300));

        assert_eq!(once.lines().count(), 1, "the first focus ran {once:?}");
        assert_eq!(
            std::fs::read_to_string(beside.path().join("ran")).unwrap_or_default(),
            once,
            "the second focus started another refresh"
        );
    }

    #[test]
    fn a_panel_drawn_with_no_charter_beside_the_app_still_draws() {
        // Without a binary there is nothing to spawn, and a panel is not the place to say so:
        // the launch already said it once, and repeating it per focus is fifty lines an hour.
        let (_plane, root) = plane_with_a_clone();

        let drawn = states_of(&root, "alpha", None).expect("the panel draws");

        assert_eq!(drawn.repos.len(), 1);
    }
}
