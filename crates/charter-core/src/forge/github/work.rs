//! GitHub's work items as GitHub has them: issues and their types, sub-issues, dependencies,
//! milestones, and Projects v2 boards with their items (FW-2a, FI4).
//!
//! **GitHub's own types, crate-private methods.** FW-5 defines the neutral work model, and
//! [`super::read`] maps an issue's reads onto it behind the `WorkItems` area trait (FW-6a, ADR
//! 0070 §1). The writes are not on the seam yet. Every method takes the [`Caller`] and sends its
//! requests as [`Call`]s, so either transport carries them and the routing rules hold.
//!
//! **Every field is a literal string unless charter wrote it** (charter #323): a title, a body,
//! a label or an id is a [`Field::Text`]; only numbers charter holds are [`Field::Typed`].

use serde::Deserialize;
use serde_json::Value;

use super::GitHub;
use super::graphql::{self, add_project_item, project, project_items, set_project_field};
use crate::forge::backend::{Caller, ForgeRef};
use crate::forge::transport::{Call, Field, Method};
use crate::forge::{ForgeError, LIST_TIMEOUT, quote};

/// An issue: the fields charter reads.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Issue {
    /// The REST id, which sub-issue and dependency writes name.
    pub id: u64,
    /// The GraphQL node id, which a board names.
    pub node_id: String,
    pub number: u64,
    pub title: String,
    #[serde(default)]
    pub body: Option<String>,
    /// `open` or `closed`.
    pub state: String,
    pub html_url: String,
    #[serde(default)]
    pub labels: Vec<Label>,
    #[serde(default)]
    pub milestone: Option<Milestone>,
    #[serde(default, rename = "type")]
    pub issue_type: Option<IssueType>,
    /// Why it was closed: `completed`, `not_planned`, `duplicate`; `reopened` once reopened.
    #[serde(default)]
    pub state_reason: Option<String>,
    #[serde(default)]
    pub assignees: Vec<Login>,
    /// How many issues block it and it blocks. A host without issue dependencies answers none.
    #[serde(default)]
    pub issue_dependencies_summary: Option<DependencySummary>,
    /// Present when the number is a pull request's: GitHub answers one as an issue too.
    #[serde(default)]
    pub pull_request: Option<Value>,
}

/// An account, by its login.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Login {
    pub login: String,
}

/// An issue's dependency counts, open and closed alike.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct DependencySummary {
    #[serde(default)]
    pub total_blocked_by: u64,
    #[serde(default)]
    pub total_blocking: u64,
}

impl Issue {
    /// The issue's node id, as a board takes it.
    pub fn forge_ref(&self) -> ForgeRef {
        ForgeRef(self.node_id.clone())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Label {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Milestone {
    pub number: u64,
    /// The GraphQL node id, which the work model carries beside the title.
    #[serde(default)]
    pub node_id: Option<String>,
    pub title: String,
    pub state: String,
    #[serde(default)]
    pub due_on: Option<String>,
}

/// An organisation's issue type.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct IssueType {
    pub id: u64,
    #[serde(default)]
    pub node_id: Option<String>,
    pub name: String,
}

/// A new issue. `issue_type` is the type's name, which only an organisation's repo has.
#[derive(Debug, Clone, Default)]
pub struct NewIssue {
    pub title: String,
    pub body: Option<String>,
    pub labels: Vec<String>,
    pub milestone: Option<u64>,
    pub issue_type: Option<String>,
}

/// A change to an issue: only the fields set are sent. A label list replaces the issue's
/// labels; an empty one is not sent, since `gh api` has no field spelling for an empty list.
#[derive(Debug, Clone, Default)]
pub struct IssueEdit {
    pub title: Option<String>,
    pub body: Option<String>,
    /// `open` or `closed`.
    pub state: Option<String>,
    pub labels: Option<Vec<String>>,
    pub milestone: Option<u64>,
    pub issue_type: Option<String>,
}

/// A new milestone.
#[derive(Debug, Clone, Default)]
pub struct NewMilestone {
    pub title: String,
    pub description: Option<String>,
    pub due_on: Option<String>,
}

/// A Projects v2 board and the fields an item can be given.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Board {
    pub id: ForgeRef,
    pub title: String,
    pub url: String,
    pub fields: Vec<BoardField>,
}

/// One field of a board.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoardField {
    pub id: ForgeRef,
    pub name: String,
    /// A single-select field's options, `(id, name)`.
    pub options: Vec<(String, String)>,
    /// An iteration field's iterations, `(id, title, start date)`.
    pub iterations: Vec<(String, String, String)>,
}

/// One item on a board: what it is, and the value of each field it has.
#[derive(Debug, Clone, PartialEq)]
pub struct BoardItem {
    pub id: ForgeRef,
    pub content: ItemContent,
    /// `(field name, value)` for each field the item has a value in.
    pub values: Vec<(String, ItemValue)>,
}

/// What a board item is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemContent {
    /// An issue or a pull request: its node id, repo (`owner/name`), number and title.
    Issue {
        id: ForgeRef,
        repo: String,
        number: u64,
        title: String,
    },
    PullRequest {
        id: ForgeRef,
        repo: String,
        number: u64,
        title: String,
    },
    /// A draft, which lives on the board only.
    Draft { id: ForgeRef, title: String },
    /// Something charter does not read, or nothing it may see.
    Other,
}

/// A field's value on one item.
#[derive(Debug, Clone, PartialEq)]
pub enum ItemValue {
    Text(String),
    Number(f64),
    Date(String),
    /// A single-select option: its id and name.
    Option {
        id: String,
        name: String,
    },
    /// An iteration: its id and title.
    Iteration {
        id: String,
        title: String,
    },
}

/// A value to set on a board item's field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldValue {
    Text(String),
    /// An integer: `gh api -F` reads integers, and charter sends what both transports send
    /// alike.
    Number(i64),
    /// An ISO 8601 date.
    Date(String),
    /// A single-select option's id.
    Option(String),
    /// An iteration's id.
    Iteration(String),
}

/// A listing longer than this many pages is refused rather than read without end.
const MOST_PAGES: usize = 1000;

impl GitHub {
    fn issues_of(repo: &str) -> Result<String, ForgeError> {
        match repo.split_once('/') {
            Some((owner, name)) if !owner.is_empty() && !name.is_empty() && !name.contains('/') => {
                Ok(format!("repos/{}/{}/issues", quote(owner), quote(name)))
            }
            _ => Err(ForgeError::of(
                crate::forge::Failure::NotFound,
                format!("{repo} is not a GitHub repository path"),
            )),
        }
    }

    /// `call`'s answer read as `T`.
    fn typed<T: serde::de::DeserializeOwned>(
        &self,
        caller: &Caller,
        call: &Call,
        doing: &str,
    ) -> Result<T, ForgeError> {
        let answer = self.0.ask(caller, call, doing)?;
        serde_json::from_value(answer).map_err(|e| {
            ForgeError::new(format!(
                "{doing}: the answer is not what charter reads: {e}"
            ))
        })
    }

    /// Every page of a REST listing at `base`, a hundred at a time, until a short page.
    fn pages<T: serde::de::DeserializeOwned>(
        &self,
        caller: &Caller,
        base: &str,
        doing: &str,
    ) -> Result<Vec<T>, ForgeError> {
        let joint = if base.contains('?') { '&' } else { '?' };
        let mut out = Vec::new();
        for page in 1..=MOST_PAGES {
            let call = Call::get(
                format!("{base}{joint}per_page=100&page={page}"),
                LIST_TIMEOUT,
            );
            let batch: Vec<T> = self.typed(caller, &call, doing)?;
            let short = batch.len() < 100;
            out.extend(batch);
            if short {
                return Ok(out);
            }
        }
        Err(ForgeError::new(format!(
            "{doing}: ran past {MOST_PAGES} pages"
        )))
    }

    /// One GraphQL document from `queries/` and its variables, its `data` read as `T`.
    pub(super) fn graphql<T: serde::de::DeserializeOwned>(
        &self,
        caller: &Caller,
        query: &str,
        variables: Vec<Field>,
        doing: &str,
    ) -> Result<T, ForgeError> {
        let answer = self.0.ask(
            caller,
            &Call::graphql(query, variables, LIST_TIMEOUT),
            doing,
        )?;
        serde_json::from_value(answer["data"].clone()).map_err(|e| {
            ForgeError::new(format!(
                "{doing}: the answer is not what charter reads: {e}"
            ))
        })
    }

    /// Issue `number` of `repo`.
    pub(crate) fn issue(
        &self,
        caller: &Caller,
        repo: &str,
        number: u64,
    ) -> Result<Issue, ForgeError> {
        let issues = Self::issues_of(repo)?;
        self.typed(
            caller,
            &Call::get(format!("{issues}/{number}"), LIST_TIMEOUT),
            &format!("reading issue #{number} of {repo}"),
        )
    }

    /// Every issue of `repo` in `state` (`open`, `closed` or `all`). GitHub lists pull
    /// requests as issues too; they are left out.
    pub(crate) fn issues(
        &self,
        caller: &Caller,
        repo: &str,
        state: &str,
    ) -> Result<Vec<Issue>, ForgeError> {
        let issues = Self::issues_of(repo)?;
        let doing = format!("listing the issues of {repo}");
        let all: Vec<Value> =
            self.pages(caller, &format!("{issues}?state={}", quote(state)), &doing)?;
        all.into_iter()
            .filter(|i| i.get("pull_request").is_none())
            .map(|i| {
                serde_json::from_value(i).map_err(|e| {
                    ForgeError::new(format!("{doing}: an issue charter cannot read: {e}"))
                })
            })
            .collect()
    }

    pub(crate) fn create_issue(
        &self,
        caller: &Caller,
        repo: &str,
        new: &NewIssue,
    ) -> Result<Issue, ForgeError> {
        let issues = Self::issues_of(repo)?;
        let mut fields = vec![Field::text("title", &new.title)];
        if let Some(body) = &new.body {
            fields.push(Field::text("body", body));
        }
        fields.extend(new.labels.iter().map(|l| Field::text("labels[]", l)));
        if let Some(milestone) = new.milestone {
            fields.push(Field::typed("milestone", &milestone.to_string()));
        }
        if let Some(kind) = &new.issue_type {
            fields.push(Field::text("type", kind));
        }
        self.typed(
            caller,
            &Call::write(Method::Post, issues, fields),
            &format!("opening an issue in {repo}"),
        )
    }

    pub(crate) fn update_issue(
        &self,
        caller: &Caller,
        repo: &str,
        number: u64,
        edit: &IssueEdit,
    ) -> Result<Issue, ForgeError> {
        let issues = Self::issues_of(repo)?;
        let mut fields = Vec::new();
        for (name, value) in [
            ("title", &edit.title),
            ("body", &edit.body),
            ("state", &edit.state),
            ("type", &edit.issue_type),
        ] {
            if let Some(value) = value {
                fields.push(Field::text(name, value));
            }
        }
        if let Some(labels) = &edit.labels {
            fields.extend(labels.iter().map(|l| Field::text("labels[]", l)));
        }
        if let Some(milestone) = edit.milestone {
            fields.push(Field::typed("milestone", &milestone.to_string()));
        }
        self.typed(
            caller,
            &Call::write(Method::Patch, format!("{issues}/{number}"), fields),
            &format!("changing issue #{number} of {repo}"),
        )
    }

    /// The sub-issues of issue `parent`.
    pub(crate) fn sub_issues(
        &self,
        caller: &Caller,
        repo: &str,
        parent: u64,
    ) -> Result<Vec<Issue>, ForgeError> {
        let issues = Self::issues_of(repo)?;
        self.pages(
            caller,
            &format!("{issues}/{parent}/sub_issues"),
            &format!("listing the sub-issues of #{parent} of {repo}"),
        )
    }

    /// Make the issue whose REST id is `child_id` a sub-issue of `parent`.
    pub(crate) fn add_sub_issue(
        &self,
        caller: &Caller,
        repo: &str,
        parent: u64,
        child_id: u64,
    ) -> Result<(), ForgeError> {
        let issues = Self::issues_of(repo)?;
        self.0
            .ask(
                caller,
                &Call::write(
                    Method::Post,
                    format!("{issues}/{parent}/sub_issues"),
                    vec![Field::typed("sub_issue_id", &child_id.to_string())],
                ),
                &format!("adding a sub-issue to #{parent} of {repo}"),
            )
            .map(|_| ())
    }

    pub(crate) fn remove_sub_issue(
        &self,
        caller: &Caller,
        repo: &str,
        parent: u64,
        child_id: u64,
    ) -> Result<(), ForgeError> {
        let issues = Self::issues_of(repo)?;
        self.0
            .ask(
                caller,
                &Call::write(
                    Method::Delete,
                    format!("{issues}/{parent}/sub_issue"),
                    vec![Field::typed("sub_issue_id", &child_id.to_string())],
                ),
                &format!("removing a sub-issue from #{parent} of {repo}"),
            )
            .map(|_| ())
    }

    /// The issues that block issue `number`.
    pub(crate) fn blocked_by(
        &self,
        caller: &Caller,
        repo: &str,
        number: u64,
    ) -> Result<Vec<Issue>, ForgeError> {
        let issues = Self::issues_of(repo)?;
        self.pages(
            caller,
            &format!("{issues}/{number}/dependencies/blocked_by"),
            &format!("listing what blocks #{number} of {repo}"),
        )
    }

    /// Mark issue `number` as blocked by the issue whose REST id is `blocker_id`.
    pub(crate) fn add_blocked_by(
        &self,
        caller: &Caller,
        repo: &str,
        number: u64,
        blocker_id: u64,
    ) -> Result<(), ForgeError> {
        let issues = Self::issues_of(repo)?;
        self.0
            .ask(
                caller,
                &Call::write(
                    Method::Post,
                    format!("{issues}/{number}/dependencies/blocked_by"),
                    vec![Field::typed("issue_id", &blocker_id.to_string())],
                ),
                &format!("marking #{number} of {repo} blocked"),
            )
            .map(|_| ())
    }

    /// The issues that issue `number` blocks.
    pub(crate) fn blocking(
        &self,
        caller: &Caller,
        repo: &str,
        number: u64,
    ) -> Result<Vec<Issue>, ForgeError> {
        let issues = Self::issues_of(repo)?;
        self.pages(
            caller,
            &format!("{issues}/{number}/dependencies/blocking"),
            &format!("listing what #{number} of {repo} blocks"),
        )
    }

    pub(crate) fn remove_blocked_by(
        &self,
        caller: &Caller,
        repo: &str,
        number: u64,
        blocker_id: u64,
    ) -> Result<(), ForgeError> {
        let issues = Self::issues_of(repo)?;
        self.0
            .ask(
                caller,
                &Call::write(
                    Method::Delete,
                    format!("{issues}/{number}/dependencies/blocked_by/{blocker_id}"),
                    Vec::new(),
                ),
                &format!("unblocking #{number} of {repo}"),
            )
            .map(|_| ())
    }

    /// The issue types organisation `org` has.
    pub(crate) fn issue_types(
        &self,
        caller: &Caller,
        org: &str,
    ) -> Result<Vec<IssueType>, ForgeError> {
        self.typed(
            caller,
            &Call::get(format!("orgs/{}/issue-types", quote(org)), LIST_TIMEOUT),
            &format!("reading the issue types of {org}"),
        )
    }

    /// Every milestone of `repo`, open and closed.
    pub(crate) fn milestones(
        &self,
        caller: &Caller,
        repo: &str,
    ) -> Result<Vec<Milestone>, ForgeError> {
        let issues = Self::issues_of(repo)?;
        let repo_path = issues.trim_end_matches("/issues");
        self.pages(
            caller,
            &format!("{repo_path}/milestones?state=all"),
            &format!("listing the milestones of {repo}"),
        )
    }

    pub(crate) fn create_milestone(
        &self,
        caller: &Caller,
        repo: &str,
        new: &NewMilestone,
    ) -> Result<Milestone, ForgeError> {
        let issues = Self::issues_of(repo)?;
        let repo_path = issues.trim_end_matches("/issues");
        let mut fields = vec![Field::text("title", &new.title)];
        if let Some(description) = &new.description {
            fields.push(Field::text("description", description));
        }
        if let Some(due) = &new.due_on {
            fields.push(Field::text("due_on", due));
        }
        self.typed(
            caller,
            &Call::write(Method::Post, format!("{repo_path}/milestones"), fields),
            &format!("making a milestone in {repo}"),
        )
    }

    /// Board `number` of `owner` (an organisation or a user), with its fields.
    pub(crate) fn board(
        &self,
        caller: &Caller,
        owner: &str,
        number: u64,
    ) -> Result<Board, ForgeError> {
        let doing = format!("reading board {number} of {owner}");
        let data: project::ResponseData = self.graphql(
            caller,
            graphql::project::QUERY,
            vec![
                Field::text("owner", owner),
                Field::typed("number", &number.to_string()),
            ],
            &doing,
        )?;
        use project::ProjectRepositoryOwner as On;
        let board = data.repository_owner.and_then(|o| match o {
            On::Organization(org) => org.project_v2,
            On::User(user) => user.project_v2,
        });
        board.map(board_of).ok_or_else(|| {
            ForgeError::of(
                crate::forge::Failure::NotFound,
                format!("{doing}: there is no such board"),
            )
        })
    }

    /// Every item on board `number` of `owner`, with each field's value (FI4).
    pub(crate) fn board_items(
        &self,
        caller: &Caller,
        owner: &str,
        number: u64,
    ) -> Result<Vec<BoardItem>, ForgeError> {
        let doing = format!("reading the items of board {number} of {owner}");
        let mut out = Vec::new();
        let mut after: Option<String> = None;
        for _ in 0..MOST_PAGES {
            let mut variables = vec![
                Field::text("owner", owner),
                Field::typed("number", &number.to_string()),
            ];
            if let Some(cursor) = &after {
                variables.push(Field::text("after", cursor));
            }
            let data: project_items::ResponseData =
                self.graphql(caller, graphql::project_items::QUERY, variables, &doing)?;
            use project_items::ProjectItemsRepositoryOwner as On;
            let items = data
                .repository_owner
                .and_then(|o| match o {
                    On::Organization(org) => org.project_v2,
                    On::User(user) => user.project_v2,
                })
                .map(|board| board.items)
                .ok_or_else(|| {
                    ForgeError::of(
                        crate::forge::Failure::NotFound,
                        format!("{doing}: there is no such board"),
                    )
                })?;
            out.extend(
                items
                    .nodes
                    .unwrap_or_default()
                    .into_iter()
                    .flatten()
                    .map(item_of),
            );
            match (items.page_info.has_next_page, items.page_info.end_cursor) {
                (true, Some(cursor)) => after = Some(cursor),
                _ => return Ok(out),
            }
        }
        Err(ForgeError::new(format!(
            "{doing}: ran past {MOST_PAGES} pages"
        )))
    }

    /// Put the issue or pull request `content` on board `board`; the item's id.
    pub(crate) fn add_to_board(
        &self,
        caller: &Caller,
        board: &ForgeRef,
        content: &ForgeRef,
    ) -> Result<ForgeRef, ForgeError> {
        let doing = "putting an item on a board";
        let data: add_project_item::ResponseData = self.graphql(
            caller,
            graphql::add_project_item::QUERY,
            vec![
                Field::text("project", &board.0),
                Field::text("content", &content.0),
            ],
            doing,
        )?;
        data.add_project_v2_item_by_id
            .and_then(|p| p.item)
            .map(|item| ForgeRef(item.id))
            .ok_or_else(|| ForgeError::new(format!("{doing}: the board named no item")))
    }

    /// Set `field` of `item` on `board` to `value`.
    pub(crate) fn set_board_field(
        &self,
        caller: &Caller,
        board: &ForgeRef,
        item: &ForgeRef,
        field: &ForgeRef,
        value: &FieldValue,
    ) -> Result<(), ForgeError> {
        let value = match value {
            FieldValue::Text(t) => Field::text("value[text]", t),
            FieldValue::Number(n) => Field::typed("value[number]", &n.to_string()),
            FieldValue::Date(d) => Field::text("value[date]", d),
            FieldValue::Option(o) => Field::text("value[singleSelectOptionId]", o),
            FieldValue::Iteration(i) => Field::text("value[iterationId]", i),
        };
        let _: set_project_field::ResponseData = self.graphql(
            caller,
            graphql::set_project_field::QUERY,
            vec![
                Field::text("project", &board.0),
                Field::text("item", &item.0),
                Field::text("field", &field.0),
                value,
            ],
            "setting a board field",
        )?;
        Ok(())
    }
}

fn board_of(board: project::Board) -> Board {
    use project::BoardFieldsNodes as F;
    let fields = board
        .fields
        .nodes
        .unwrap_or_default()
        .into_iter()
        .flatten()
        .map(|node| match node {
            F::ProjectV2Field(f) => BoardField {
                id: ForgeRef(f.id),
                name: f.name,
                options: Vec::new(),
                iterations: Vec::new(),
            },
            F::ProjectV2SingleSelectField(f) => BoardField {
                id: ForgeRef(f.id),
                name: f.name,
                options: f.options.into_iter().map(|o| (o.id, o.name)).collect(),
                iterations: Vec::new(),
            },
            F::ProjectV2IterationField(f) => BoardField {
                id: ForgeRef(f.id),
                name: f.name,
                options: Vec::new(),
                iterations: f
                    .configuration
                    .iterations
                    .into_iter()
                    .map(|i| (i.id, i.title, i.start_date))
                    .collect(),
            },
        })
        .collect();
    Board {
        id: ForgeRef(board.id),
        title: board.title,
        url: board.url,
        fields,
    }
}

fn field_name(field: project_items::FieldName) -> String {
    use project_items::FieldName as N;
    match field {
        N::ProjectV2Field(f) => f.name,
        N::ProjectV2SingleSelectField(f) => f.name,
        N::ProjectV2IterationField(f) => f.name,
    }
}

fn item_of(item: project_items::ItemsItemsNodes) -> BoardItem {
    use project_items::ItemsItemsNodesContent as C;
    use project_items::ItemsItemsNodesFieldValuesNodes as V;
    let content = match item.content {
        Some(C::Issue(i)) => ItemContent::Issue {
            id: ForgeRef(i.id),
            repo: i.repository.name_with_owner,
            number: u64::try_from(i.number).unwrap_or_default(),
            title: i.title,
        },
        Some(C::PullRequest(p)) => ItemContent::PullRequest {
            id: ForgeRef(p.id),
            repo: p.repository.name_with_owner,
            number: u64::try_from(p.number).unwrap_or_default(),
            title: p.title,
        },
        Some(C::DraftIssue(d)) => ItemContent::Draft {
            id: ForgeRef(d.id),
            title: d.title,
        },
        None => ItemContent::Other,
    };
    let values = item
        .field_values
        .nodes
        .unwrap_or_default()
        .into_iter()
        .flatten()
        .filter_map(|node| match node {
            V::ProjectV2ItemFieldTextValue(v) => {
                Some((field_name(v.field), ItemValue::Text(v.text?)))
            }
            V::ProjectV2ItemFieldNumberValue(v) => {
                Some((field_name(v.field), ItemValue::Number(v.number?)))
            }
            V::ProjectV2ItemFieldDateValue(v) => {
                Some((field_name(v.field), ItemValue::Date(v.date?)))
            }
            V::ProjectV2ItemFieldSingleSelectValue(v) => Some((
                field_name(v.field),
                ItemValue::Option {
                    id: v.option_id?,
                    name: v.name?,
                },
            )),
            V::ProjectV2ItemFieldIterationValue(v) => Some((
                field_name(v.field),
                ItemValue::Iteration {
                    id: v.iteration_id,
                    title: v.title,
                },
            )),
            _ => None,
        })
        .collect();
    BoardItem {
        id: ForgeRef(item.id),
        content,
        values,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::json;

    use super::*;
    use crate::forge::backend::{Asker, Fixed};
    use crate::forge::recorded::Recorded;
    use crate::forge::{Forge, Kind};

    /// GitHub over these recorded exchanges, and the recording to check afterwards.
    fn over(exchanges: Value) -> (GitHub, Arc<Recorded>) {
        let text = json!({"source": "GitHub REST API version 2022-11-28 and GraphQL schema",
                          "exchanges": exchanges});
        let recorded = Arc::new(Recorded::parse(&text.to_string()).unwrap());
        let github = GitHub(Asker {
            forge: Forge::default_of(Kind::GitHub),
            transports: Arc::new(Fixed(recorded.clone())),
        });
        (github, recorded)
    }

    fn spent(recorded: &Recorded) {
        assert_eq!(recorded.unspent(), Vec::new(), "recorded and never asked");
    }

    fn rest(method: Option<&str>, path: &str, fields: Value, out: Value) -> Value {
        json!({"call": {"endpoint": {"rest": {"method": method, "path": path}}, "fields": fields},
               "reply": {"code": 0, "out": out.to_string()}})
    }

    fn gql(query: &str, variables: Value, out: Value) -> Value {
        let mut fields = vec![json!({"text": ["query", query]})];
        fields.extend(variables.as_array().unwrap().iter().cloned());
        json!({"call": {"endpoint": "graphql", "fields": fields},
               "reply": {"code": 0, "out": out.to_string()}})
    }

    fn issue(number: u64, title: &str) -> Value {
        json!({"id": 1000 + number, "node_id": format!("I_{number}"), "number": number,
               "title": title, "body": null, "state": "open",
               "html_url": format!("https://github.com/o/r/issues/{number}"),
               "labels": [{"name": "bug"}], "milestone": null,
               "type": {"id": 7, "node_id": "IT_7", "name": "Bug"}})
    }

    fn me() -> Caller {
        Caller::command()
    }

    #[test]
    fn issues_are_listed_without_pull_requests_and_written_with_their_type_as_literals() {
        let mut pr = issue(2, "a PR");
        pr["pull_request"] = json!({"url": "x"});
        let (github, recorded) = over(json!([
            rest(
                None,
                "repos/o/r/issues?state=open&per_page=100&page=1",
                json!([]),
                json!([issue(1, "one"), pr])
            ),
            rest(
                Some("POST"),
                "repos/o/r/issues",
                json!([{"text": ["title", "@not-a-file"]}, {"text": ["labels[]", "bug"]},
                        {"typed": ["milestone", "3"]}, {"text": ["type", "Bug"]}]),
                issue(3, "@not-a-file")
            ),
            rest(
                Some("PATCH"),
                "repos/o/r/issues/3",
                json!([{"text": ["state", "closed"]}]),
                issue(3, "@not-a-file")
            ),
        ]));
        let open = github.issues(&me(), "o/r", "open").unwrap();
        assert_eq!(open.len(), 1);
        assert_eq!(open[0].issue_type.as_ref().unwrap().name, "Bug");
        let made = github
            .create_issue(
                &me(),
                "o/r",
                &NewIssue {
                    title: "@not-a-file".into(),
                    labels: vec!["bug".into()],
                    milestone: Some(3),
                    issue_type: Some("Bug".into()),
                    ..NewIssue::default()
                },
            )
            .unwrap();
        assert_eq!((made.number, made.id), (3, 1003));
        github
            .update_issue(
                &me(),
                "o/r",
                3,
                &IssueEdit {
                    state: Some("closed".into()),
                    ..IssueEdit::default()
                },
            )
            .unwrap();
        spent(&recorded);
    }

    #[test]
    fn sub_issues_and_dependencies_are_read_and_written_by_rest_id() {
        let (github, recorded) = over(json!([
            rest(
                None,
                "repos/o/r/issues/1/sub_issues?per_page=100&page=1",
                json!([]),
                json!([issue(2, "child")])
            ),
            rest(
                Some("POST"),
                "repos/o/r/issues/1/sub_issues",
                json!([{"typed": ["sub_issue_id", "1003"]}]),
                issue(1, "parent")
            ),
            rest(
                Some("DELETE"),
                "repos/o/r/issues/1/sub_issue",
                json!([{"typed": ["sub_issue_id", "1003"]}]),
                issue(1, "parent")
            ),
            rest(
                None,
                "repos/o/r/issues/1/dependencies/blocked_by?per_page=100&page=1",
                json!([]),
                json!([issue(4, "blocker")])
            ),
            rest(
                Some("POST"),
                "repos/o/r/issues/1/dependencies/blocked_by",
                json!([{"typed": ["issue_id", "1004"]}]),
                issue(1, "parent")
            ),
            rest(
                Some("DELETE"),
                "repos/o/r/issues/1/dependencies/blocked_by/1004",
                json!([]),
                issue(1, "parent")
            ),
        ]));
        assert_eq!(github.sub_issues(&me(), "o/r", 1).unwrap()[0].number, 2);
        github.add_sub_issue(&me(), "o/r", 1, 1003).unwrap();
        github.remove_sub_issue(&me(), "o/r", 1, 1003).unwrap();
        assert_eq!(github.blocked_by(&me(), "o/r", 1).unwrap()[0].number, 4);
        github.add_blocked_by(&me(), "o/r", 1, 1004).unwrap();
        github.remove_blocked_by(&me(), "o/r", 1, 1004).unwrap();
        spent(&recorded);
    }

    #[test]
    fn issue_types_and_milestones_are_read_and_a_milestone_is_made() {
        let (github, recorded) = over(json!([
            rest(
                None,
                "orgs/o/issue-types",
                json!([]),
                json!([{"id": 7, "node_id": "IT_7", "name": "Bug"}])
            ),
            rest(
                None,
                "repos/o/r/milestones?state=all&per_page=100&page=1",
                json!([]),
                json!([{"number": 1, "title": "M1", "state": "open"}])
            ),
            rest(
                Some("POST"),
                "repos/o/r/milestones",
                json!([{"text": ["title", "M2"]}]),
                json!({"number": 2, "title": "M2", "state": "open"})
            ),
        ]));
        assert_eq!(github.issue_types(&me(), "o").unwrap()[0].name, "Bug");
        assert_eq!(github.milestones(&me(), "o/r").unwrap()[0].title, "M1");
        let made = github
            .create_milestone(
                &me(),
                "o/r",
                &NewMilestone {
                    title: "M2".into(),
                    ..NewMilestone::default()
                },
            )
            .unwrap();
        assert_eq!(made.number, 2);
        spent(&recorded);
    }

    #[test]
    fn a_board_is_read_and_an_issue_is_put_on_it_with_a_field_set() {
        let (github, recorded) = over(json!([
            gql(
                graphql::project::QUERY,
                json!([{"text": ["owner", "o"]}, {"typed": ["number", "5"]}]),
                json!({"data": {"repositoryOwner": {"__typename": "Organization", "projectV2": {
                "id": "PVT_5", "title": "Roadmap", "url": "https://github.com/orgs/o/projects/5",
                "fields": {"nodes": [
                    {"__typename": "ProjectV2Field", "id": "F_title", "name": "Title"},
                    {"__typename": "ProjectV2SingleSelectField", "id": "F_status",
                     "name": "Status", "options": [{"id": "opt_todo", "name": "Todo"}]},
                    {"__typename": "ProjectV2IterationField", "id": "F_iter", "name": "Sprint",
                     "configuration": {"iterations": [
                         {"id": "it_1", "title": "Sprint 1", "startDate": "2026-10-01"}]}}
                ]}}}}})
            ),
            gql(
                graphql::add_project_item::QUERY,
                json!([{"text": ["project", "PVT_5"]}, {"text": ["content", "I_1"]}]),
                json!({"data": {"addProjectV2ItemById": {"item": {"id": "PVTI_1"}}}})
            ),
            gql(
                graphql::set_project_field::QUERY,
                json!([{"text": ["project", "PVT_5"]}, {"text": ["item", "PVTI_1"]},
                       {"text": ["field", "F_status"]},
                       {"text": ["value[singleSelectOptionId]", "opt_todo"]}]),
                json!({"data": {"updateProjectV2ItemFieldValue": {"projectV2Item": {"id": "PVTI_1"}}}})
            ),
        ]));
        let board = github.board(&me(), "o", 5).unwrap();
        assert_eq!(board.title, "Roadmap");
        let status = board.fields.iter().find(|f| f.name == "Status").unwrap();
        assert_eq!(
            status.options,
            [("opt_todo".to_string(), "Todo".to_string())]
        );
        let sprint = board.fields.iter().find(|f| f.name == "Sprint").unwrap();
        assert_eq!(sprint.iterations[0].1, "Sprint 1");
        let item = github
            .add_to_board(&me(), &board.id, &ForgeRef("I_1".into()))
            .unwrap();
        github
            .set_board_field(
                &me(),
                &board.id,
                &item,
                &status.id,
                &FieldValue::Option("opt_todo".into()),
            )
            .unwrap();
        spent(&recorded);
    }

    #[test]
    fn a_boards_items_are_read_page_by_page_with_each_fields_value() {
        let page = |nodes: Value, next: Option<&str>| {
            json!({"data": {"repositoryOwner": {"__typename": "User", "projectV2": {"items": {
                "pageInfo": {"hasNextPage": next.is_some(), "endCursor": next},
                "nodes": nodes}}}}})
        };
        let (github, recorded) = over(json!([
            gql(
                graphql::project_items::QUERY,
                json!([{"text": ["owner", "me"]}, {"typed": ["number", "2"]}]),
                page(
                    json!([{"id": "PVTI_1",
                    "content": {"__typename": "Issue", "id": "I_1", "number": 1, "title": "one",
                                "repository": {"nameWithOwner": "me/r"}},
                    "fieldValues": {"nodes": [
                        {"__typename": "ProjectV2ItemFieldSingleSelectValue", "name": "Todo",
                         "optionId": "opt_todo",
                         "field": {"__typename": "ProjectV2SingleSelectField", "name": "Status"}},
                        {"__typename": "ProjectV2ItemFieldRepositoryValue"}
                    ]}}]),
                    Some("c1")
                )
            ),
            gql(
                graphql::project_items::QUERY,
                json!([{"text": ["owner", "me"]}, {"typed": ["number", "2"]},
                       {"text": ["after", "c1"]}]),
                page(
                    json!([{"id": "PVTI_2",
                    "content": {"__typename": "DraftIssue", "id": "DI_2", "title": "idea"},
                    "fieldValues": {"nodes": [
                        {"__typename": "ProjectV2ItemFieldNumberValue", "number": 3.0,
                         "field": {"__typename": "ProjectV2Field", "name": "Points"}}
                    ]}}]),
                    None
                )
            ),
        ]));
        let items = github.board_items(&me(), "me", 2).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(
            items[0].content,
            ItemContent::Issue {
                id: ForgeRef("I_1".into()),
                repo: "me/r".into(),
                number: 1,
                title: "one".into()
            }
        );
        assert_eq!(
            items[0].values,
            [(
                "Status".to_string(),
                ItemValue::Option {
                    id: "opt_todo".into(),
                    name: "Todo".into()
                }
            )]
        );
        assert_eq!(
            items[1].values,
            [("Points".to_string(), ItemValue::Number(3.0))]
        );
        spent(&recorded);
    }
}
