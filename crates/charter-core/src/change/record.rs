//! The record itself: its closed key set, its validation, and its canonical bytes
//! (`charter/change.py` at `cli-final`; `docs/plane-format.md` §"a cross-repo change").
//!
//! Pure: nothing here touches a disk. [`Record::parse`] is the only way in and
//! [`Record::to_json`] the only way out, and both hold the same rules, so a record held in
//! memory cannot pick up a derived field and serialise it.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value};

use crate::contain;
use crate::shown;

/// The whole top-level key set, matched exactly: no seventh, none missing.
pub const KEYS: [&str; 6] = ["change", "why", "created", "by", "members", "excluded"];

/// One member's keys. There is no `state`, `landed`, `pr` or `ci`, and the closed set is what
/// makes them unrepresentable rather than merely unwritten.
pub const MEMBER_KEYS: [&str; 3] = ["repo", "branch", "needs"];

/// One exclusion's keys: a repo considered and deliberately left out, why, and when.
pub const EXCLUSION_KEYS: [&str; 3] = ["repo", "why", "at"];

/// How long a record's own text may be: `contain.PATH_DISPLAY_LIMIT`, the path budget and not
/// the row budget. A `why` a little over one row is a legitimate `why`; where rows are drawn,
/// [`shown::line`] clips it. What is refused here is a character with no glyph, or a value
/// longer than a committed value may be.
pub const TEXT_LIMIT: usize = 1024;

/// A change record, validated. Every field is intent; nothing git or the forge knows is here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub change: String,
    pub why: String,
    pub created: String,
    pub by: String,
    pub members: Vec<Member>,
    pub excluded: Vec<Exclusion>,
}

/// One repo's part of a change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    pub repo: String,
    /// This change's branch in that repo. Stored, never derived from a convention.
    pub branch: String,
    /// The members that must land first. Declared, because only a person knows it.
    pub needs: Vec<String>,
}

/// A repo considered and left out, with the reason somebody wrote down while they knew it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exclusion {
    pub repo: String,
    pub why: String,
    pub at: String,
}

/// A change record charter will not act on, with the reason. Raised rather than answered
/// with an empty record: `add` is read-modify-write, and a read that degraded to empty would
/// write back a record holding only the new member.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct RecordError(pub String);

fn refused<T>(message: String) -> Result<T, RecordError> {
    Err(RecordError(message))
}

/// Can `slug` name a change? `^[A-Za-z0-9][A-Za-z0-9._-]*$` (`instance.change_name_ok`). It
/// names a file in the plane, a branch in every member and a commit trailer, so it is refused
/// rather than rewritten.
pub fn name_ok(slug: &str) -> bool {
    let mut chars = slug.chars();
    chars.next().is_some_and(|c| c.is_ascii_alphanumeric())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// The branch `charter change add` offers. Stored in the record once chosen.
pub fn default_branch(slug: &str) -> String {
    format!("change/{slug}")
}

/// A value from the record, as a refusal names it: a string escaped to one readable line,
/// anything else as Python's `repr` writes it.
fn readable(value: &Value) -> String {
    match value {
        Value::String(s) => shown::short(s),
        other => shown::short(&crate::pyrepr::repr_json(other)),
    }
}

/// Why this branch name must not be handed to git, or `None`.
///
/// `git check-ref-format` accepts `refs/heads/-b`, so ref grammar does not answer this: the
/// value comes out of a committed file and reaches git as argv, where a leading dash is a flag.
pub fn branch_refusal(branch: &str) -> Option<String> {
    if branch.is_empty() {
        return Some(format!(
            "{} is not a branch name (a non-empty string)",
            shown::BLANK
        ));
    }
    if branch.starts_with('-') {
        return Some(format!(
            "branch {} begins with '-', so it reaches git as a FLAG rather than as a branch. \
             `git check-ref-format` accepts `refs/heads/-b`, so ref grammar does not answer this.",
            shown::short(branch)
        ));
    }
    if shown::one_line(branch, TEXT_LIMIT) != branch {
        return Some(format!(
            "branch {} is not one plain line — it carries a character with no glyph, or is \
             longer than a committed value may be.",
            shown::short(branch)
        ));
    }
    None
}

fn branch_value_refusal(value: &Value) -> Option<String> {
    match value.as_str() {
        Some(branch) => branch_refusal(branch),
        None => Some(format!(
            "{} is not a branch name (a non-empty string)",
            readable(value)
        )),
    }
}

/// The closed key set, both directions, refused by name. Unknown first: a record with `need`
/// where `needs` was meant has both, and the typo is the one to name.
fn closed<'a>(
    value: &'a Value,
    keys: &[&str],
    where_: &str,
) -> Result<&'a Map<String, Value>, RecordError> {
    let Some(obj) = value.as_object() else {
        return refused(format!("{where_}: not an object"));
    };
    let mut unknown: Vec<String> = obj
        .keys()
        .filter(|k| !keys.contains(&k.as_str()))
        .map(|k| shown::short(k))
        .collect();
    unknown.sort();
    if !unknown.is_empty() {
        return refused(format!(
            "{where_}: unknown key {}. The key set is closed — {} — and a key charter does not \
             read reads as nothing at all, so it is named rather than ignored.",
            unknown.join(", "),
            keys.join(", ")
        ));
    }
    let missing: Vec<&str> = keys
        .iter()
        .copied()
        .filter(|k| !obj.contains_key(*k))
        .collect();
    if !missing.is_empty() {
        return refused(format!("{where_}: missing key {}", missing.join(", ")));
    }
    Ok(obj)
}

/// A non-empty single plain line, or a refusal naming where it was found.
fn one_line(value: &Value, where_: &str) -> Result<String, RecordError> {
    let text = match value.as_str() {
        Some(s) if !s.trim().is_empty() => s,
        _ => {
            return refused(format!(
                "{where_}: expected a non-empty string, got {}",
                readable(value)
            ));
        }
    };
    if shown::one_line(text, TEXT_LIMIT) != text {
        return refused(format!(
            "{where_}: {} is not one plain line. This is repeated back on a report row and \
             written into a pull request body, where a newline forges a second row.",
            readable(value)
        ));
    }
    Ok(text.to_string())
}

/// A repo name: `contain::segment_ok` and deliberately not the workspace name rule, because
/// the name comes from a forge and `.github` is a real repository.
fn repo_name(value: &Value, where_: &str) -> Result<String, RecordError> {
    match value.as_str() {
        Some(name) if contain::segment_ok(name) => Ok(name.to_string()),
        _ => refused(format!(
            "{where_}: {} cannot name one entry inside the workspace",
            readable(value)
        )),
    }
}

fn list<'a>(value: &'a Value, what: &str, where_: &str) -> Result<&'a Vec<Value>, RecordError> {
    value
        .as_array()
        .ok_or_else(|| RecordError(format!("{where_}: '{what}' is not a list")))
}

impl Record {
    /// A change with no members and no exclusions yet.
    pub fn new(slug: &str, why: &str, by: &str, created: &str) -> Record {
        Record {
            change: slug.to_string(),
            why: why.to_string(),
            created: created.to_string(),
            by: by.to_string(),
            members: Vec::new(),
            excluded: Vec::new(),
        }
    }

    /// The record `text` holds for `slug`, validated, or what is wrong with it.
    ///
    /// The slug is asked about first, so every sentence after it can name it plainly: from
    /// there on it is `[A-Za-z0-9][A-Za-z0-9._-]*`, which cannot forge a row.
    pub fn parse(text: &str, slug: &str) -> Result<Record, RecordError> {
        if !name_ok(slug) {
            return refused(format!("{} is not a change name", shown::short(slug)));
        }
        let value: Value = match serde_json::from_str(text) {
            Ok(value) => value,
            Err(e) => return refused(format!("change '{slug}': the record is not JSON ({e})")),
        };
        let at = format!("change '{slug}'");
        if !value.is_object() {
            return refused(format!("{at}: the record is not an object"));
        }
        let obj = closed(&value, &KEYS, &at)?;
        if obj["change"].as_str() != Some(slug) {
            return refused(format!(
                "{at}: the record calls itself {}. The filename and the name are one identity — \
                 the name is what a merge commit's trailer carries, so a record that disagrees \
                 with its own file has two of them.",
                readable(&obj["change"])
            ));
        }
        let why = one_line(&obj["why"], &format!("{at}: why"))?;
        let created = one_line(&obj["created"], &format!("{at}: created"))?;
        let by = one_line(&obj["by"], &format!("{at}: by"))?;

        let mut members = Vec::new();
        for m in list(&obj["members"], "members", &at)? {
            if !m.is_object() {
                return refused(format!("{at}: a member is not an object"));
            }
            let m = closed(m, &MEMBER_KEYS, &format!("{at}: member"))?;
            let repo = repo_name(&m["repo"], &format!("{at}: member"))?;
            if let Some(complaint) = branch_value_refusal(&m["branch"]) {
                return refused(format!(
                    "{at}: member '{}': {complaint}",
                    shown::short(&repo)
                ));
            }
            let needs_at = format!("{at}: member '{}'", shown::short(&repo));
            let needs = list(&m["needs"], "needs", &needs_at)?
                .iter()
                .map(|n| repo_name(n, &format!("{needs_at}: needs")))
                .collect::<Result<Vec<_>, _>>()?;
            members.push(Member {
                repo,
                branch: m["branch"].as_str().unwrap_or_default().to_string(),
                needs,
            });
        }

        let mut excluded = Vec::new();
        for e in list(&obj["excluded"], "excluded", &at)? {
            if !e.is_object() {
                return refused(format!("{at}: an exclusion is not an object"));
            }
            let e = closed(e, &EXCLUSION_KEYS, &format!("{at}: exclusion"))?;
            let repo = repo_name(&e["repo"], &format!("{at}: exclusion"))?;
            let here = format!("{at}: exclusion '{}'", shown::short(&repo));
            excluded.push(Exclusion {
                why: one_line(&e["why"], &format!("{here}: why"))?,
                at: one_line(&e["at"], &format!("{here}: at"))?,
                repo,
            });
        }

        let record = Record {
            change: slug.to_string(),
            why,
            created,
            by,
            members,
            excluded,
        };
        record.across_fields()?;
        Ok(record)
    }

    /// Whether this record, built in memory, is one [`Self::parse`] would accept: asked before
    /// every write. It round-trips through the parser, so a record written is held to exactly
    /// the rules a record read is, field rules included, and there is one rule set and not two.
    pub fn validate(&self) -> Result<(), RecordError> {
        Record::parse(&self.to_json(), &self.change).map(|_| ())
    }

    /// The rules no single field can answer: a repo twice, a repo both in and out, and an
    /// ordering that cannot be true.
    fn across_fields(&self) -> Result<(), RecordError> {
        let slug = &self.change;
        if !name_ok(slug) {
            return refused(format!("{} is not a change name", shown::short(slug)));
        }
        let at = format!("change '{slug}'");
        let mut seen = BTreeSet::new();
        for m in &self.members {
            repo_name(&Value::String(m.repo.clone()), &format!("{at}: member"))?;
            if !seen.insert(m.repo.as_str()) {
                return refused(format!(
                    "{at}: '{}' is a member twice",
                    shown::short(&m.repo)
                ));
            }
            if let Some(complaint) = branch_refusal(&m.branch) {
                return refused(format!(
                    "{at}: member '{}': {complaint}",
                    shown::short(&m.repo)
                ));
            }
        }
        for e in &self.excluded {
            if seen.contains(e.repo.as_str()) {
                return refused(format!(
                    "{at}: '{}' is both a member and excluded",
                    shown::short(&e.repo)
                ));
            }
        }
        if let Some(complaint) = self.order_refusal() {
            return refused(format!("{at}: {complaint}"));
        }
        Ok(())
    }

    /// Why this record's `needs` cannot be true, or `None`: a member that blocks itself, a
    /// `needs` naming a repo that is not a member, or a cycle — named whole, every member in it.
    fn order_refusal(&self) -> Option<String> {
        let graph: BTreeMap<&str, &[String]> = self
            .members
            .iter()
            .map(|m| (m.repo.as_str(), m.needs.as_slice()))
            .collect();
        for m in &self.members {
            let repo = shown::short(&m.repo);
            for n in &m.needs {
                if n == &m.repo {
                    return Some(format!(
                        "member '{repo}' declares itself as its own blocker — it would wait for \
                         its own landing"
                    ));
                }
                if !graph.contains_key(n.as_str()) {
                    return Some(format!(
                        "member '{repo}' needs '{}', which is not a member of this change. Add \
                         it first (charter change add), or drop the need — a blocker nothing \
                         will land is a member that never becomes ready.",
                        shown::short(n)
                    ));
                }
            }
        }
        let cycle = self.cycle()?;
        Some(format!(
            "ordering cycle: {} — no member can go first, so this is a record that cannot be \
             true",
            cycle
                .iter()
                .map(|r| format!("'{}'", shown::short(r)))
                .collect::<Vec<_>>()
                .join(" → ")
        ))
    }

    /// One cycle in the `needs` graph as a path returning to its start, in member order.
    fn cycle(&self) -> Option<Vec<String>> {
        #[derive(Clone, Copy, PartialEq)]
        enum Mark {
            OnPath,
            Done,
        }
        fn walk<'a>(
            node: &'a str,
            graph: &BTreeMap<&'a str, &'a [String]>,
            marks: &mut BTreeMap<&'a str, Mark>,
            path: &mut Vec<&'a str>,
        ) -> Option<Vec<String>> {
            marks.insert(node, Mark::OnPath);
            path.push(node);
            for next in graph.get(node).copied().unwrap_or_default() {
                match marks.get(next.as_str()) {
                    Some(Mark::OnPath) => {
                        let start = path.iter().position(|p| *p == next.as_str())?;
                        let mut found: Vec<String> =
                            path[start..].iter().map(|s| s.to_string()).collect();
                        found.push(next.clone());
                        return Some(found);
                    }
                    Some(Mark::Done) => {}
                    None => {
                        if let Some(found) = walk(next.as_str(), graph, marks, path) {
                            return Some(found);
                        }
                    }
                }
            }
            path.pop();
            marks.insert(node, Mark::Done);
            None
        }
        let graph: BTreeMap<&str, &[String]> = self
            .members
            .iter()
            .map(|m| (m.repo.as_str(), m.needs.as_slice()))
            .collect();
        let mut marks = BTreeMap::new();
        for m in &self.members {
            if !marks.contains_key(m.repo.as_str())
                && let Some(found) = walk(&m.repo, &graph, &mut marks, &mut Vec::new())
            {
                return Some(found);
            }
        }
        None
    }

    /// The record's bytes: keys in [`KEYS`] order, two-space indent, one trailing newline,
    /// non-ASCII escaped — `json.dumps(ordered, indent=2) + "\n"`. Canonical, so a record read
    /// and written back is byte-identical.
    pub fn to_json(&self) -> String {
        let strings =
            |items: &[String]| Value::Array(items.iter().cloned().map(Value::String).collect());
        let mut top = Map::new();
        top.insert("change".into(), self.change.clone().into());
        top.insert("why".into(), self.why.clone().into());
        top.insert("created".into(), self.created.clone().into());
        top.insert("by".into(), self.by.clone().into());
        let members = self
            .members
            .iter()
            .map(|m| {
                let mut row = Map::new();
                row.insert("repo".into(), m.repo.clone().into());
                row.insert("branch".into(), m.branch.clone().into());
                row.insert("needs".into(), strings(&m.needs));
                Value::Object(row)
            })
            .collect();
        top.insert("members".into(), Value::Array(members));
        let excluded = self
            .excluded
            .iter()
            .map(|e| {
                let mut row = Map::new();
                row.insert("repo".into(), e.repo.clone().into());
                row.insert("why".into(), e.why.clone().into());
                row.insert("at".into(), e.at.clone().into());
                Value::Object(row)
            })
            .collect();
        top.insert("excluded".into(), Value::Array(excluded));
        crate::pyjson::dumps_indent2(&Value::Object(top))
    }

    /// The member row for `repo`.
    pub fn member(&self, repo: &str) -> Option<&Member> {
        self.members.iter().find(|m| m.repo == repo)
    }

    /// The exclusion row for `repo`.
    pub fn exclusion(&self, repo: &str) -> Option<&Exclusion> {
        self.excluded.iter().find(|e| e.repo == repo)
    }

    /// `{member: its blockers that have NOT landed}`: derived from the declaration and what
    /// the caller read as landed, and stored nowhere. Computed for every member, a landed one
    /// included, so a member that went in ahead of its blocker is visible.
    pub fn blocked(&self, landed: &BTreeSet<String>) -> BTreeMap<String, Vec<String>> {
        self.members
            .iter()
            .filter_map(|m| {
                let pending: Vec<String> = m
                    .needs
                    .iter()
                    .filter(|n| !landed.contains(*n))
                    .cloned()
                    .collect();
                (!pending.is_empty()).then(|| (m.repo.clone(), pending))
            })
            .collect()
    }

    /// The members that declare `repo` as a blocker: what `drop` has to name.
    pub fn dependents(&self, repo: &str) -> Vec<String> {
        self.members
            .iter()
            .filter(|m| m.needs.iter().any(|n| n == repo))
            .map(|m| m.repo.clone())
            .collect()
    }
}
