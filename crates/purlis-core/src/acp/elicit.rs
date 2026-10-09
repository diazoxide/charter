//! ACP elicitation (`elicitation/create`, ADR 0080 §2): the agent asks the person for values
//! through a form, and only the person answers it, in purlis's window.
//!
//! - **Form mode only.** purlis offers `elicitation.form` and never `elicitation.url`: a URL
//!   mode request sends the person to a page the agent chose, which is not a value handed
//!   through a dialog the app owns.
//! - **Every elicitation elicits a secret.** Nothing the agent says about a field can be
//!   trusted to say it is not one, so each is raised with `elicits_secret` and answered from
//!   the window alone ([`crate::harness::asks`]).
//! - **What the agent wrote is shown as plain, bounded text** ([`Form::read`]): every character
//!   a window draws as nothing or draws elsewhere is dropped, and each piece is cut to a bound.
//!   A request too large, with too many fields or choices, or with a field of a type purlis
//!   cannot draw, is never raised: the agent hears `cancel`, as an oversized permission request
//!   hears `cancelled`.
//! - **The values go to the agent and nowhere else.** [`Elicited`] never prints them, they are
//!   never in an event, the ask registry or an answer's record, and a refusal names the field,
//!   never what was typed in it ([`Form::check`]).

use std::collections::BTreeMap;

use agent_client_protocol::schema::v1::{
    CreateElicitationRequest, ElicitationContentValue, ElicitationMode, ElicitationPropertySchema,
    ElicitationScope, EnumOption, MultiSelectItems, StringFormat,
};

use crate::harness::hooked::drawn_otherwise;
use crate::harness::model::{
    Action, Ask, Channel, Choice, ChoiceKind, ChoiceScope, Deadline, MayAnswer, Risk, Summary,
};

/// The most fields one form may ask for. A form with more is never raised.
pub const MOST_FIELDS: usize = 32;

/// The most choices one field may offer. A field with more is never raised.
pub const MOST_CHOICES: usize = 64;

/// The most bytes of the agent's message shown with a form; a longer one ends in `…`.
pub const MOST_MESSAGE_BYTES: usize = 4 * 1024;

/// The most bytes of a field's or a choice's label, and of a form's title; a longer one ends
/// in `…`.
pub const MOST_LABEL_BYTES: usize = 256;

/// The most bytes of a field's help text; a longer one ends in `…`.
pub const MOST_HELP_BYTES: usize = 1024;

/// The most bytes of a field's name or a choice's value, which go back to the agent as they
/// came and so are never cut: a form with a longer one is never raised.
pub const MOST_NAME_BYTES: usize = 256;

/// The most bytes the person may give one text field.
pub const MOST_VALUE_BYTES: usize = 64 * 1024;

/// The option ids an elicitation's ask offers, as the ask registry closes it on: the person
/// submits the form, declines it, or dismisses it.
pub const ACCEPT: &str = "accept";
pub const DECLINE: &str = "decline";
pub const CANCEL: &str = "cancel";

/// A form the agent asked the person to fill in, as the window draws it: every piece of text
/// plain and bounded.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Form {
    /// What the agent says it needs, on as many lines as it wrote.
    pub message: String,
    /// The form's own title, where it gave one.
    pub title: Option<String>,
    /// The fields, in the agent's order of names.
    pub fields: Vec<Field>,
}

/// One field of a [`Form`].
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Field {
    /// The field's name, as the agent gave it: the key its value goes back under. Shown only
    /// through [`Field::label`].
    pub name: String,
    /// What the field is called: its title, or else its name, plain.
    pub label: String,
    /// What the agent says about it, plain; empty where it said nothing.
    pub help: String,
    /// Whether the form cannot be submitted without it.
    pub required: bool,
    pub kind: Kind,
}

/// What a [`Field`] takes.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum Kind {
    /// Text, at least `min` and at most `max` characters; `format` is the agent's hint
    /// (`email`, `uri`, `date`, `date-time`), which the agent checks.
    Text {
        min: Option<u32>,
        max: Option<u32>,
        format: Option<String>,
    },
    /// One of `choices`.
    One { choices: Vec<Pick> },
    /// Any number of `choices`, at least `min` and at most `max` of them.
    Many {
        choices: Vec<Pick>,
        min: Option<u64>,
        max: Option<u64>,
    },
    /// A number between `min` and `max`.
    Number { min: Option<f64>, max: Option<f64> },
    /// A whole number between `min` and `max`.
    Integer { min: Option<i64>, max: Option<i64> },
    /// Yes or no.
    Boolean,
}

// A number's bounds come from JSON, which has no NaN, and are refused unless finite
// ([`Form::read`]), so equality is reflexive.
impl Eq for Kind {}

/// One choice of a [`Kind::One`] or [`Kind::Many`] field.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Pick {
    /// What goes back to the agent, as it gave it.
    pub value: String,
    /// What the window shows: its title, or else its value, plain.
    pub label: String,
}

/// The person's answer to a form.
#[derive(Clone, PartialEq)]
pub enum Elicited {
    /// Submitted, with a value for each field they filled in, keyed by [`Field::name`].
    Accept(BTreeMap<String, Given>),
    /// Declined: they will not give these values.
    Decline,
    /// Dismissed without a choice.
    Cancel,
}

impl std::fmt::Debug for Elicited {
    /// Never the values: they may be secrets.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Accept(values) => f
                .debug_tuple("Accept")
                .field(&values.keys().collect::<Vec<_>>())
                .finish(),
            Self::Decline => f.write_str("Decline"),
            Self::Cancel => f.write_str("Cancel"),
        }
    }
}

impl Elicited {
    /// The id of the option it chooses on the elicitation's ask.
    pub fn option(&self) -> &'static str {
        match self {
            Self::Accept(_) => ACCEPT,
            Self::Decline => DECLINE,
            Self::Cancel => CANCEL,
        }
    }
}

/// One value the person gave, as the window sends it.
#[derive(Clone, PartialEq, serde::Deserialize)]
#[serde(untagged)]
pub enum Given {
    Boolean(bool),
    Integer(i64),
    Number(f64),
    Text(String),
    Many(Vec<String>),
}

impl std::fmt::Debug for Given {
    /// Never the value: it may be a secret.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Given(..)")
    }
}

/// Why a request was not raised as an ask.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Unfit {
    /// Not a form: a URL mode request, or a mode ACP may add. purlis offers only forms.
    NotAForm,
    /// Scoped to another session, or to a request rather than this chat's session.
    NotOurs,
    /// Past a bound, or a field purlis cannot draw: the agent hears `cancel`.
    TooMuch,
}

impl Form {
    /// The form `request` asks for, if it is a form for `session` that fits every bound.
    pub fn read(request: &CreateElicitationRequest, session: &str) -> Result<Self, Unfit> {
        let ElicitationMode::Form(form) = &request.mode else {
            return Err(Unfit::NotAForm);
        };
        match &form.scope {
            ElicitationScope::Session(scope) if *scope.session_id.0 == *session => {}
            _ => return Err(Unfit::NotOurs),
        }
        let schema = &form.requested_schema;
        if schema.properties.len() > MOST_FIELDS {
            return Err(Unfit::TooMuch);
        }
        let required = schema.required.clone().unwrap_or_default();
        let fields = schema
            .properties
            .iter()
            .map(|(name, property)| field(name, property, required.contains(name)))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            message: plain(&request.message, MOST_MESSAGE_BYTES, true),
            title: schema
                .title
                .as_deref()
                .map(|title| plain(title, MOST_LABEL_BYTES, false))
                .filter(|title| !title.is_empty()),
            fields,
        })
    }

    /// The ask an elicitation of this form is raised as: window-only, with no deadline, its
    /// message as the summary, and the three answers ACP gives the person.
    pub fn ask(&self, session: &str) -> Ask {
        let option = |id: &str, label: &str, kind| Choice {
            id: id.to_owned(),
            label: label.to_owned(),
            kind,
            scope: ChoiceScope::Once,
        };
        Ask {
            action: Action::Elicit {
                fields: self
                    .fields
                    .iter()
                    .map(|field| field.label.clone())
                    .collect(),
            },
            options: vec![
                option(ACCEPT, "Submit", ChoiceKind::Allow),
                option(DECLINE, "Decline", ChoiceKind::Reject),
                option(CANCEL, "Cancel", ChoiceKind::Cancel),
            ],
            may_answer: MayAnswer::Operator,
            deadline: Deadline::None,
            risk: Risk::SecretUse,
            summary: Summary::of(&self.message),
            elicits_secret: true,
            // No request id of the agent's: each elicitation is its own ask, and never
            // supersedes a permission request for the same tool call.
            channel: Channel::Acp {
                session: session.to_owned(),
                tool_call: String::new(),
            },
        }
    }

    /// `values` as the agent is sent them, if they fit this form: no field it does not ask
    /// for, every required one there, each of its type and within its bounds. A refusal names
    /// the field and never the value.
    pub fn check(
        &self,
        values: &BTreeMap<String, Given>,
    ) -> Result<BTreeMap<String, ElicitationContentValue>, String> {
        // Not even the name is said back: it came from the client, not the form.
        if values
            .keys()
            .any(|name| !self.fields.iter().any(|field| field.name == *name))
        {
            return Err("it names a field this form does not ask for".to_owned());
        }
        let mut content = BTreeMap::new();
        for field in &self.fields {
            match values.get(&field.name) {
                None if field.required => {
                    return Err(format!("{} is required", field.label));
                }
                None => {}
                Some(given) => {
                    let value =
                        fits(field, given).map_err(|why| format!("{} {why}", field.label))?;
                    content.insert(field.name.clone(), value);
                }
            }
        }
        Ok(content)
    }
}

/// `given` as the agent is sent it, if it fits `field`; else why not, after the field's label.
fn fits(field: &Field, given: &Given) -> Result<ElicitationContentValue, String> {
    let picked = |choices: &[Pick], value: &str| choices.iter().any(|pick| pick.value == value);
    match (&field.kind, given) {
        (Kind::Text { min, max, .. }, Given::Text(text)) => {
            let chars = u32::try_from(text.chars().count()).unwrap_or(u32::MAX);
            if text.len() > MOST_VALUE_BYTES {
                return Err(format!("is longer than {MOST_VALUE_BYTES} bytes"));
            }
            if min.is_some_and(|min| chars < min) || max.is_some_and(|max| chars > max) {
                return Err("is not of a length it takes".to_owned());
            }
            Ok(ElicitationContentValue::String(text.clone()))
        }
        (Kind::One { choices }, Given::Text(text)) if picked(choices, text) => {
            Ok(ElicitationContentValue::String(text.clone()))
        }
        (Kind::One { .. }, _) => Err("must be one of its choices".to_owned()),
        (Kind::Many { choices, min, max }, Given::Many(picks)) => {
            if !picks.iter().all(|value| picked(choices, value)) {
                return Err("takes only its own choices".to_owned());
            }
            let count = u64::try_from(picks.len()).unwrap_or(u64::MAX);
            if min.is_some_and(|min| count < min) || max.is_some_and(|max| count > max) {
                return Err("needs another number of choices".to_owned());
            }
            Ok(ElicitationContentValue::StringArray(picks.clone()))
        }
        (Kind::Number { .. }, Given::Integer(number)) => {
            // A whole number the person typed in a number field: as JSON would read it.
            let number: f64 = number.to_string().parse().unwrap_or(f64::NAN);
            fits(field, &Given::Number(number))
        }
        (Kind::Number { min, max }, Given::Number(number)) => {
            let number = *number;
            if !number.is_finite()
                || min.is_some_and(|min| number < min)
                || max.is_some_and(|max| number > max)
            {
                return Err("is out of its range".to_owned());
            }
            Ok(ElicitationContentValue::Number(number))
        }
        (Kind::Integer { min, max }, Given::Integer(number)) => {
            if min.is_some_and(|min| *number < min) || max.is_some_and(|max| *number > max) {
                return Err("is out of its range".to_owned());
            }
            Ok(ElicitationContentValue::Integer(*number))
        }
        (Kind::Boolean, Given::Boolean(yes)) => Ok(ElicitationContentValue::Boolean(*yes)),
        _ => Err("is not of the type it takes".to_owned()),
    }
}

/// One property of the agent's schema as a [`Field`], or [`Unfit::TooMuch`] for one purlis
/// cannot draw or that is past a bound.
fn field(name: &str, property: &ElicitationPropertySchema, required: bool) -> Result<Field, Unfit> {
    if name.len() > MOST_NAME_BYTES {
        return Err(Unfit::TooMuch);
    }
    let (title, description, kind) = match property {
        ElicitationPropertySchema::String(text) => {
            let kind = match (&text.enum_values, &text.one_of) {
                (_, Some(options)) => Kind::One {
                    choices: titled(options)?,
                },
                (Some(values), None) => Kind::One {
                    choices: untitled(values)?,
                },
                (None, None) => Kind::Text {
                    min: text.min_length,
                    max: text.max_length,
                    format: text.format.as_ref().map(format_word),
                },
            };
            (&text.title, &text.description, kind)
        }
        ElicitationPropertySchema::Array(many) => {
            let choices = match &many.items {
                MultiSelectItems::String(items) => untitled(&items.values)?,
                MultiSelectItems::Titled(items) => titled(&items.options)?,
                // Items of a shape ACP may add, which purlis cannot check.
                _ => return Err(Unfit::TooMuch),
            };
            (
                &many.title,
                &many.description,
                Kind::Many {
                    choices,
                    min: many.min_items,
                    max: many.max_items,
                },
            )
        }
        ElicitationPropertySchema::Number(number) => {
            if [number.minimum, number.maximum]
                .into_iter()
                .flatten()
                .any(|bound| !bound.is_finite())
            {
                return Err(Unfit::TooMuch);
            }
            (
                &number.title,
                &number.description,
                Kind::Number {
                    min: number.minimum,
                    max: number.maximum,
                },
            )
        }
        ElicitationPropertySchema::Integer(number) => (
            &number.title,
            &number.description,
            Kind::Integer {
                min: number.minimum,
                max: number.maximum,
            },
        ),
        ElicitationPropertySchema::Boolean(yes) => (&yes.title, &yes.description, Kind::Boolean),
        // A type ACP may add, which purlis does not know how to draw or check.
        _ => return Err(Unfit::TooMuch),
    };
    let label = title
        .as_deref()
        .map(|title| plain(title, MOST_LABEL_BYTES, false))
        .filter(|label| !label.is_empty())
        .unwrap_or_else(|| plain(name, MOST_LABEL_BYTES, false));
    Ok(Field {
        name: name.to_owned(),
        label,
        help: description
            .as_deref()
            .map(|help| plain(help, MOST_HELP_BYTES, true))
            .unwrap_or_default(),
        required,
        kind,
    })
}

fn titled(options: &[EnumOption]) -> Result<Vec<Pick>, Unfit> {
    choices(
        options
            .iter()
            .map(|option| (&option.value, Some(&option.title))),
    )
}

fn untitled(values: &[String]) -> Result<Vec<Pick>, Unfit> {
    choices(values.iter().map(|value| (value, None)))
}

fn choices<'a>(
    options: impl ExactSizeIterator<Item = (&'a String, Option<&'a String>)>,
) -> Result<Vec<Pick>, Unfit> {
    if options.len() > MOST_CHOICES {
        return Err(Unfit::TooMuch);
    }
    options
        .map(|(value, title)| {
            if value.len() > MOST_NAME_BYTES {
                return Err(Unfit::TooMuch);
            }
            let label = title
                .map(|title| plain(title, MOST_LABEL_BYTES, false))
                .filter(|label| !label.is_empty())
                .unwrap_or_else(|| plain(value, MOST_LABEL_BYTES, false));
            Ok(Pick {
                value: value.clone(),
                label,
            })
        })
        .collect()
}

fn format_word(format: &StringFormat) -> String {
    serde_json::to_value(format)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default()
}

/// `text` as plain text: without any character a window draws as nothing or draws elsewhere
/// (a control, a format character, a bidirectional override), its line breaks kept only where
/// `lines` (and otherwise read as spaces), trimmed, and cut to `most` bytes ending in `…`. The
/// Unicode line and paragraph separators are line breaks too, so neither starts a line in a
/// one-line label.
pub fn plain(text: &str, most: usize, lines: bool) -> String {
    let shown: String = text
        .chars()
        .filter_map(|c| match c {
            '\n' | '\u{2028}' | '\u{2029}' if lines => Some('\n'),
            '\n' | '\t' | '\u{2028}' | '\u{2029}' => Some(' '),
            c if drawn_otherwise(c) => None,
            c => Some(c),
        })
        .collect();
    super::cut(shown.trim().to_owned(), most)
}

#[cfg(test)]
mod tests;
