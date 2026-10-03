//! The harness capability card (HP-19, DECISIONS W10, ADR 0073 §6): what one harness can do in
//! charter, said where the operator meets it — the picker, a chat's header, its own view tab,
//! and a control that is off because the harness lacks something.
//!
//! **Read off the real sources, never a list kept by hand.** A card is built from the harness's
//! declaration ([`crate::harness_declaration`], FD-14): its `[levels]`, its `[capabilities]` and
//! its `[terminal] ready_to_type`. What a declaration cannot say, because it is charter code
//! rather than a fact (ADR 0073 §3), comes from the adapter charter ships for a built-in
//! ([`crate::harness::HarnessAdapter`]): whether charter can sandbox it. A harness a project
//! declares has no adapter, so its card says so rather than guessing.
//!
//! **Neutral, like the declarations** (charter is harness-agnostic). Nothing here names a
//! harness: Claude Code, Codex and opencode differ only in what their declarations and adapters
//! answer, and a fourth harness gets a card by being declared.
//!
//! **In the first hour's words** (ADR 0072 §3, ADR 0073 §6). The card is labelled *What
//! <product> can do here* ([`Card::label`]); each line says what the operator will or will not
//! see, in plain words and the budget's nouns — never "capability", "harness" or "level", which
//! stay in this code. A control that is off says one such line ([`Card::lacks`]) followed by the
//! card's label.
//!
//! **Silence never reads as yes**, as in the declarations: a capability a declaration leaves
//! unanswered is drawn as *not declared*, and [`Card::lacks`] treats it as missing.

use std::path::Path;

use crate::harness::Harness;
use crate::harness_declaration::{self, Answer, Declaration, Origin, ReadyToType};
use crate::panel::{Block, Detail, Empty, Fact, Mark, Row, Tone};

/// The line a control that types a prompt into a chat once it starts asks about — a curation
/// chat, the first task: the declaration's `[terminal] ready_to_type`.
pub const READY_TO_TYPE: &str = "ready_to_type";

/// The line a sandboxed project asks of every chat it starts (ADR 0067).
pub const SANDBOX: &str = "sandbox";

/// Level 2: the harness's own hooks, armed by an adapter charter ships (ADR 0073 §1).
pub const HOOKS: &str = "hooks";

/// Level 3: an ACP agent the declaration names (ADR 0073 §1, HP-2).
pub const ACP: &str = "acp";

/// One line of a card: something charter does with a harness, and whether this one can.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ability {
    /// A declaration's capability key ([`harness_declaration::CAPABILITIES`]), or one of
    /// [`HOOKS`], [`ACP`], [`READY_TO_TYPE`] and [`SANDBOX`].
    pub id: &'static str,
    /// What it does, as the card's row says it: *Tells charter when it is waiting for you*.
    pub label: &'static str,
    /// What the operator sees without it, after the harness's name: *does not tell charter when
    /// it is waiting, so its chats will not show needs you*.
    pub without: &'static str,
    pub answer: Answer,
}

/// The words of one line, written once for every harness.
struct Words {
    id: &'static str,
    label: &'static str,
    without: &'static str,
}

/// Every line a card has, in the order it draws them: how charter follows the chat, then each
/// capability a declaration answers, then what charter does with a chat once it is open. A
/// test holds this to [`harness_declaration::CAPABILITIES`], so a capability charter starts
/// reading cannot be left off the card.
const LINES: [Words; 10] = [
    Words {
        id: HOOKS,
        label: "Tells charter whether it is working, waiting or done",
        without: "does not report to charter through hooks of its own, so its chats show only \
                  that they are open",
    },
    Words {
        id: ACP,
        label: "Works without a terminal, through its ACP agent",
        without: "has no ACP agent charter can talk to, so charter can hand it work only in a \
                  terminal",
    },
    Words {
        id: "reports_its_process",
        label: "Tells charter which process it reports from",
        without: "does not tell charter which process it reports from, so a chat keeps the \
                  first conversation it reports and a later one is not followed",
    },
    Words {
        id: "reports_its_start_before_the_first_prompt",
        label: "Tells charter it has started before your first prompt",
        without: "says nothing until your first prompt, so a new chat looks idle until then",
    },
    Words {
        id: "keeps_conversations_by_directory",
        label: "Keeps its conversations by folder",
        without: "finds a conversation by its id from any folder, not by the folder it was \
                  started in",
    },
    Words {
        id: "reports_waiting",
        label: "Tells charter when it is waiting for you",
        without: "does not tell charter when it is waiting, so its chats will not show needs you",
    },
    Words {
        id: "resumes_by_id",
        label: "Picks a conversation back up by its id",
        without: "cannot pick a conversation back up by its id, so a chat opened again starts a \
                  new conversation",
    },
    Words {
        id: "per_chat_plugins",
        label: "Takes plugins for one chat alone",
        without: "cannot take plugins for one chat alone, so the plugins chosen for this \
                  project are not handed to it",
    },
    Words {
        id: READY_TO_TYPE,
        label: "Can have a prompt typed in for you when it starts",
        without: "cannot have a prompt typed in for you, because charter cannot tell when it has \
                  finished starting",
    },
    Words {
        id: SANDBOX,
        label: "Opens in a project that sandboxes its chats",
        without: "cannot be sandboxed by charter yet, so a project that sandboxes its chats \
                  opens none on it",
    },
];

/// One harness's card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Card {
    /// The word a profile's `kind` names.
    pub name: String,
    /// What the operator calls it: the product's own name.
    pub title: String,
    /// The program a chat of it starts.
    pub program: String,
    /// Shipped with charter, or declared by this project.
    pub origin: Origin,
    /// The versions its facts were measured on, where the declaration says.
    pub tested: Option<String>,
    /// Every line, in [`LINES`]'s order.
    pub abilities: Vec<Ability>,
    /// What a chat of it cannot tell charter, said on the chat (`Harness::unreported`): a
    /// built-in's own sentence, or none.
    pub unreported: Option<&'static str>,
}

impl Card {
    /// What the card is labelled, everywhere it is drawn: *What Codex can do here*.
    pub fn label(&self) -> String {
        format!("What {} can do here", self.title)
    }

    /// The lines this harness does not have, yes being the only answer that holds.
    pub fn lacking(&self) -> impl Iterator<Item = &Ability> {
        self.abilities
            .iter()
            .filter(|ability| !ability.answer.holds())
    }

    /// The one line a missing ability is said in: *Codex does not tell charter when it is
    /// waiting, so its chats will not show needs you.*
    fn line(&self, ability: &Ability) -> String {
        match ability.answer {
            Answer::Unknown => format!(
                "{} {}, as far as charter knows: its declaration does not say.",
                self.title, ability.without
            ),
            _ => format!("{} {}.", self.title, ability.without),
        }
    }

    /// The card's lines for what this harness lacks, in its order: what the picker and a chat's
    /// header say.
    pub fn lines(&self) -> Vec<String> {
        self.lacking().map(|ability| self.line(ability)).collect()
    }

    /// What a control that needs `ability` says while it is off for this harness: the card's
    /// line for it, followed by the card's label (ADR 0072 §3) — or `None` where the harness
    /// has it, or where no line is called `ability`.
    pub fn lacks(&self, ability: &str) -> Option<String> {
        let found = self.lacking().find(|a| a.id == ability)?;
        Some(format!("{} See {}.", self.line(found), self.label()))
    }

    /// The card as its view tab draws it, in the panel vocabulary: its label, where it comes
    /// from, a row per line with yes, no or not declared and the declaration's own reason, and
    /// what its chats cannot tell charter.
    pub fn blocks(&self) -> Vec<Block> {
        let mut facts = vec![
            Fact {
                label: "Program".to_owned(),
                value: self.program.clone(),
            },
            Fact {
                label: "Declared".to_owned(),
                value: match self.origin {
                    Origin::BuiltIn => "shipped with charter".to_owned(),
                    Origin::Project => format!(
                        "by this project, in {}/{}.toml",
                        harness_declaration::DIR,
                        self.name
                    ),
                },
            },
        ];
        if let Some(tested) = &self.tested {
            facts.push(Fact {
                label: "Measured on".to_owned(),
                value: tested.clone(),
            });
        }
        let rows = self
            .abilities
            .iter()
            .map(|ability| Row {
                key: ability.id.to_owned(),
                text: ability.label.to_owned(),
                note: Some(
                    match ability.answer {
                        Answer::Yes => "yes",
                        Answer::No(_) => "no",
                        Answer::Unknown => "not declared",
                    }
                    .to_owned(),
                ),
                mark: if ability.answer.holds() {
                    Mark::Dot
                } else {
                    Mark::Note
                },
                tone: Tone::Plain,
                detail: match &ability.answer {
                    Answer::Yes => None,
                    Answer::No(why) => {
                        Some(Detail::Text(format!("{} Why: {why}.", self.line(ability))))
                    }
                    Answer::Unknown => Some(Detail::Text(self.line(ability))),
                },
                runs: None,
                actions: Vec::new(),
            })
            .collect();
        let mut out = vec![
            Block::Note {
                text: self.label(),
                tone: Tone::Plain,
            },
            Block::Facts(facts),
            Block::List {
                rows,
                empty: Empty {
                    headline: "Nothing to say".to_owned(),
                    body: None,
                    offer: None,
                },
            },
        ];
        if let Some(unreported) = self.unreported {
            out.push(Block::Note {
                text: unreported.to_owned(),
                tone: Tone::Plain,
            });
        }
        out
    }
}

/// The built-in harness a declaration is, where it is one: the key to its adapter.
fn built_in(declaration: &Declaration) -> Option<Harness> {
    (declaration.origin == Origin::BuiltIn)
        .then(|| Harness::of_kind(&declaration.name))
        .flatten()
}

/// What `declaration` (and, for a built-in, its adapter `harness`) answers for line `id`.
fn answer(declaration: &Declaration, harness: Option<Harness>, id: &str) -> Answer {
    let no = |why: &str| Answer::No(why.to_owned());
    match id {
        HOOKS if declaration.has_adapter() => Answer::Yes,
        HOOKS => no(
            "charter ships no adapter to arm its hooks, and a project's declaration cannot carry one",
        ),
        ACP => match declaration.levels.acp {
            Some(_) => Answer::Yes,
            None => no("its declaration names no ACP agent"),
        },
        READY_TO_TYPE => match declaration.terminal.ready_to_type {
            ReadyToType::OnStart | ReadyToType::RawAndQuiet => Answer::Yes,
            ReadyToType::Never => no("its declaration says ready_to_type = \"never\""),
        },
        SANDBOX => match harness {
            Some(harness) if crate::sandbox::compiler(harness).is_some() => Answer::Yes,
            Some(_) => no("charter has no sandbox compiler for it yet"),
            None => no("charter sandboxes only a harness it ships an adapter for"),
        },
        capability => declaration.capability(capability),
    }
}

/// The card of one declared harness.
pub fn of(declaration: &Declaration) -> Card {
    let harness = built_in(declaration);
    Card {
        name: declaration.name.clone(),
        title: declaration.title.clone(),
        program: declaration.program.clone(),
        origin: declaration.origin,
        tested: declaration.tested.clone(),
        abilities: LINES
            .iter()
            .map(|words| Ability {
                id: words.id,
                label: words.label,
                without: words.without,
                answer: answer(declaration, harness, words.id),
            })
            .collect(),
        unreported: harness.and_then(Harness::unreported),
    }
}

/// A card for every harness the project at `root` has, the built-ins first: what
/// [`harness_declaration::read`] reads, so a declaration it refuses has no card.
pub fn read(root: &Path) -> Vec<Card> {
    harness_declaration::read(root)
        .declared
        .iter()
        .map(of)
        .collect()
}

/// The card of the harness a profile's `kind` names, in the project at `root`.
pub fn named(root: &Path, kind: &str) -> Option<Card> {
    harness_declaration::read(root).get(kind).map(of)
}

/// The card of a built-in harness, which needs no project to read: charter ships a declaration
/// for each one ([`harness_declaration::builtins`]), and a test holds them together.
pub fn built_in_card(harness: Harness) -> Card {
    let declaration = harness_declaration::builtin(harness.name())
        .unwrap_or_else(|| panic!("charter ships no declaration for {}", harness.name()));
    of(declaration)
}

#[cfg(test)]
mod tests;
