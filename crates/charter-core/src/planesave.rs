//! How far a save goes, for the plane and for each workspace repo (charter-app#292, ADR 0051).
//!
//! **One function answers it, [`Settings::from_text`]**, and every reader asks it.
//! `charter.toml`'s `[plane]` and `[repos.<name>]` are **Shared**; `charter.local.toml`'s are
//! **Local** and override Shared **key by key**, as ADR 0048's overlay does for extensions. Each
//! answer says which file decided it, so every surface can name that file.

use std::time::Duration;

pub use crate::settings::Source;

/// How far a save goes. Each mode includes the steps of the one before it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Charter never commits.
    Off,
    /// Local commits only.
    Commit,
    /// Commit, then push to the target branch.
    Push,
    /// Commit, push to a save branch, and keep one PR open into the target branch.
    Pr,
    /// As [`Mode::Pr`], and the PR is set to auto-merge.
    PrMerge,
}

/// One answer, and the file that decided it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved<T> {
    pub value: T,
    pub source: Source,
}

/// How the plane is saved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plane {
    /// `None` when neither file says: the Saving view asks once.
    pub mode: Resolved<Option<Mode>>,
    /// Whether the mode came from `[memory] share`, the deprecated alias, which `charter
    /// doctor` names.
    pub from_share: bool,
    /// The target branch. `None`: the remote's default branch.
    pub branch: Resolved<Option<String>>,
    /// The rolling branch the PR modes push to. `None`: `charter/save/<host>`.
    pub save_branch: Resolved<Option<String>>,
    /// Whether save commits are signed.
    pub sign: Resolved<bool>,
    /// Whether charter saves by itself.
    pub autosave: Resolved<bool>,
    /// The quiet period after the last change before an auto-save.
    pub autosave_after: Resolved<Duration>,
}

/// How one workspace repo is saved: `[repos.<name>]`, keyed by the repo's name in
/// `inventory/repos.json`, so one table governs every workspace's clone of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repo {
    /// `pr` when neither file says.
    pub mode: Resolved<Mode>,
    /// The branch a PR goes into. `None`: the repo's default branch.
    pub branch: Resolved<Option<String>>,
    pub sign: Resolved<bool>,
    /// Off when neither file says.
    pub autosave: Resolved<bool>,
    pub autosave_after: Resolved<Duration>,
}

/// What a project's two files say about saving.
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub plane: Plane,
    files: Files,
}

impl Settings {
    /// The two files' text, as they would be read: `None` for a file that is not there.
    pub fn from_text(shared: Option<&str>, local: Option<&str>) -> Self {
        let files = Files {
            shared: shared.map(top_of).unwrap_or_default(),
            local: local.map(top_of).unwrap_or_default(),
        };
        let declared = files.pick(&["plane"], "mode", |v| v.as_str().and_then(Mode::parse));
        let from_share = declared
            .is_none()
            .then(|| share_alias(&files.shared))
            .flatten();
        let mode = match (declared, from_share) {
            (Some(r), _) => Resolved {
                value: Some(r.value),
                source: r.source,
            },
            (None, Some(mode)) => Resolved {
                value: Some(mode),
                source: Source::Shared,
            },
            (None, None) => Resolved {
                value: None,
                source: Source::Default,
            },
        };
        Self {
            plane: Plane {
                mode,
                from_share: from_share.is_some(),
                branch: files.branch(&["plane"], "branch"),
                save_branch: files.branch(&["plane"], "save_branch"),
                sign: files.or(&["plane"], "sign", toml::Value::as_bool, false),
                autosave: files.or(&["plane"], "autosave", toml::Value::as_bool, true),
                autosave_after: files.or(&["plane"], "autosave_after", quiet_period, QUIET),
            },
            files,
        }
    }

    /// The plane at `root`'s two files, each taken through [`crate::settings::layer_text`]: a
    /// file that cannot be read says nothing, and nor does a `charter.local.toml` git would
    /// commit, because an ignored file must not change plane policy with no trace in git.
    pub fn read(root: &std::path::Path) -> Self {
        use crate::settings::{Which, layer_text};
        Self::from_text(
            layer_text(root, Which::Shared).text(),
            layer_text(root, Which::Local).text(),
        )
    }

    /// How the repo called `name` is saved.
    pub fn repo(&self, name: &str) -> Repo {
        let path = ["repos", name];
        let files = &self.files;
        Repo {
            mode: files.or(
                &path,
                "mode",
                |v| v.as_str().and_then(Mode::parse),
                Mode::Pr,
            ),
            branch: files.branch(&path, "branch"),
            sign: files.or(&path, "sign", toml::Value::as_bool, false),
            autosave: files.or(&path, "autosave", toml::Value::as_bool, false),
            autosave_after: files.or(&path, "autosave_after", quiet_period, QUIET),
        }
    }
}

/// How long auto-save waits after the last change when neither file says.
pub const QUIET: Duration = Duration::from_secs(30);

/// Each file's top table.
#[derive(Debug, Clone, PartialEq)]
struct Files {
    shared: toml::Table,
    local: toml::Table,
}

impl Files {
    /// [`Files::pick`], or `default` from neither file.
    fn or<T>(
        &self,
        path: &[&str],
        key: &str,
        read: impl Fn(&toml::Value) -> Option<T>,
        default: T,
    ) -> Resolved<T> {
        self.pick(path, key, read).unwrap_or(Resolved {
            value: default,
            source: Source::Default,
        })
    }

    /// A branch name at `key`, or `None` from neither file.
    fn branch(&self, path: &[&str], key: &str) -> Resolved<Option<String>> {
        self.pick(path, key, |v| {
            v.as_str().filter(|b| branch_ok(b)).map(str::to_owned)
        })
        .map(|r| Resolved {
            value: Some(r.value),
            source: r.source,
        })
        .unwrap_or(Resolved {
            value: None,
            source: Source::Default,
        })
    }

    /// `key`'s value from the highest file that holds one `read` accepts: Local, then Shared.
    /// A value `read` refuses is passed over, never guessed at.
    fn pick<T>(
        &self,
        path: &[&str],
        key: &str,
        read: impl Fn(&toml::Value) -> Option<T>,
    ) -> Option<Resolved<T>> {
        [(&self.local, Source::Local), (&self.shared, Source::Shared)]
            .into_iter()
            .find_map(|(top, source)| {
                let value = read(table_at(top, path)?.get(key)?)?;
                Some(Resolved { value, source })
            })
    }
}

impl Mode {
    /// The word the files write for this mode.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Commit => "commit",
            Self::Push => "push",
            Self::Pr => "pr",
            Self::PrMerge => "pr-merge",
        }
    }

    /// Whether a save in this mode opens a pull request.
    pub fn opens_a_pr(self) -> bool {
        matches!(self, Self::Pr | Self::PrMerge)
    }

    /// The mode `word` names, as the files write it; `None` for any other word.
    pub fn parse(word: &str) -> Option<Self> {
        Some(match word {
            "off" => Self::Off,
            "commit" => Self::Commit,
            "push" => Self::Push,
            "pr" => Self::Pr,
            "pr-merge" => Self::PrMerge,
            _ => return None,
        })
    }
}

/// The top table of `text`, or an empty one when the text is not TOML.
fn top_of(text: &str) -> toml::Table {
    text.parse().unwrap_or_default()
}

/// The table at `path` under `top`, when every step of it is a table.
fn table_at<'a>(top: &'a toml::Table, path: &[&str]) -> Option<&'a toml::Table> {
    path.iter()
        .try_fold(top, |table, step| table.get(*step)?.as_table())
}

/// A quiet period as the files write it: a whole number of seconds or minutes above zero,
/// `"30s"` or `"2m"`.
fn quiet_period(value: &toml::Value) -> Option<Duration> {
    let text = value.as_str()?;
    let (digits, per) = match (text.strip_suffix('s'), text.strip_suffix('m')) {
        (Some(digits), _) => (digits, 1),
        (_, Some(digits)) => (digits, 60),
        _ => return None,
    };
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let n: u64 = digits.parse().ok().filter(|n| *n > 0)?;
    Some(Duration::from_secs(n.checked_mul(per)?))
}

/// Whether `name` could be a branch: git's `check-ref-format --branch` rules, checked without
/// asking git.
pub fn branch_ok(name: &str) -> bool {
    !name.is_empty()
        && name != "@"
        && !name.starts_with(['-', '/', '.'])
        && !name.ends_with(['/', '.'])
        && !name.split('/').any(|part| part.ends_with(".lock"))
        && !name.contains("..")
        && !name.contains("//")
        && !name.contains("@{")
        && !name.contains("/.")
        && !name
            .chars()
            .any(|c| c.is_control() || c.is_whitespace() || "~^:?*[\\".contains(c))
}

/// Whether `shared`, `charter.toml`'s text, holds a `[memory] share` that says anything:
/// `commit` or `push`. `local` is what `charter init` always wrote, and says nothing.
pub fn share_is_set(shared: &str) -> bool {
    share_alias(&top_of(shared)).is_some()
}

/// The mode `charter.toml`'s `[memory] share` stands for: `commit` and `push` carry over, and
/// `local` — which `charter init` always wrote — says nothing, so the plane is asked once.
fn share_alias(shared: &toml::Table) -> Option<Mode> {
    match table_at(shared, &["memory"])?.get("share")?.as_str()? {
        "commit" => Some(Mode::Commit),
        "push" => Some(Mode::Push),
        _ => None,
    }
}

/// The keys `[plane]` holds that this module reads.
const PLANE_KEYS: [&str; 6] = [
    "mode",
    "branch",
    "save_branch",
    "sign",
    "autosave",
    "autosave_after",
];

/// The keys `[repos.<name>]` holds.
const REPO_KEYS: [&str; 5] = ["mode", "branch", "sign", "autosave", "autosave_after"];

/// Everything in `text`'s `[plane]` and `[repos]` that charter would not read, as `file` holds
/// it, one sentence each and in the file's order. `local` is whether `file` is
/// `charter.local.toml`. Empty when the text is not TOML at all: that is the file's own
/// reader's refusal, not this one's.
pub fn refusals(text: &str, local: bool, file: &str) -> Vec<String> {
    let Ok(top) = text.parse::<toml::Table>() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    match top.get("plane") {
        None => {}
        Some(toml::Value::Table(plane)) => {
            for (key, value) in plane {
                if key == "worktrees" {
                    if local {
                        out.push(format!(
                            "plane.worktrees in {file} is not read — it belongs in charter.toml, \
                             and $CHARTER_WORKTREES sets it for this machine alone"
                        ));
                    }
                } else if PLANE_KEYS.contains(&key.as_str()) {
                    out.extend(value_refusal("plane", key, value, file));
                } else {
                    out.push(format!(
                        "plane.{} in {file} is not read — [plane] holds mode, branch, \
                         save_branch, sign, autosave, autosave_after and worktrees",
                        toml_key(key)
                    ));
                }
            }
        }
        Some(_) => out.push(format!("plane in {file} is not a table")),
    }
    match top.get("repos") {
        None => {}
        Some(toml::Value::Table(repos)) => {
            for (name, repo) in repos {
                let at = format!("repos.{}", toml_key(name));
                let Some(repo) = repo.as_table() else {
                    out.push(format!("{at} in {file} is not a table"));
                    continue;
                };
                for (key, value) in repo {
                    if REPO_KEYS.contains(&key.as_str()) {
                        out.extend(value_refusal(&at, key, value, file));
                    } else {
                        out.push(format!(
                            "{at}.{} in {file} is not read — [repos.<name>] holds mode, branch, \
                             sign, autosave and autosave_after",
                            toml_key(key)
                        ));
                    }
                }
            }
        }
        Some(_) => out.push(format!("repos in {file} is not a table")),
    }
    out
}

/// Why `value` at `at.key` is not one charter reads, or `None` when it is.
fn value_refusal(at: &str, key: &str, value: &toml::Value, file: &str) -> Option<String> {
    let ok = match key {
        "mode" => value.as_str().and_then(Mode::parse).is_some(),
        "branch" | "save_branch" => value.as_str().is_some_and(branch_ok),
        "sign" | "autosave" => value.is_bool(),
        _ => quiet_period(value).is_some(),
    };
    if ok {
        return None;
    }
    Some(match key {
        "mode" => {
            format!("{at}.{key} in {file} is not a mode — one of off, commit, push, pr, pr-merge")
        }
        "branch" | "save_branch" => {
            format!("{at}.{key} in {file} is not a branch name git would accept")
        }
        "sign" | "autosave" => format!("{at}.{key} in {file} is not true or false"),
        _ => format!(
            "{at}.{key} in {file} is not a quiet period — a whole number of seconds or minutes, \
             like \"30s\" or \"2m\""
        ),
    })
}

/// A key as TOML would write it: bare when it can be, quoted when it cannot.
fn toml_key(key: &str) -> String {
    toml_edit::Key::new(key).display_repr().into_owned()
}

#[cfg(test)]
mod tests;
