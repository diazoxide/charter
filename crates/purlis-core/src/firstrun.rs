//! The first run: what a new machine has, and a plane nobody was asked about (FR-4, #603).
//!
//! Decision G5 asks for a first run that reaches a working chat in five minutes, and W10 says
//! how: **never ask where the plane goes.** A machine with no project gets a local plane in
//! charter's own directory, with no remote. Sharing it with a team comes later. The one thing
//! the first run asks for is the repo to work on, and that repo becomes a
//! workspace named after it, so the first chat starts there and not at the plane root.
//!
//! Three pieces, each a seam of its own:
//!
//! - [`local_plane`] and [`ensure_local_plane`]: where the plane lives, and making it.
//! - [`take_in`]: a repo cloned into a workspace of its own name, and [`take_in_from`]: the
//!   same, with the project laid out from a project template first (FR-17). The project then
//!   tracks the forge the repo's `origin` is on, added when it did not ([`ForgeTaken`], #880).
//! - [`harnesses`]: which harnesses are installed and whether each has been signed in to.
//!
//! # Where the local plane lives
//!
//! In charter's directory in the config home, beside the machine store
//! (`$CHARTER_CONFIG_HOME`, else `$XDG_CONFIG_HOME`, else `~/.config`, then `charter/`), which
//! is the rule `machine.rs` keeps and ADR 0034's amendment records. The ticket says "app
//! data"; ADR 0069 §1 names the config home as the Machine tier's home rather than the OS
//! application-data directory. The plane itself is in the **Plane** tier: its files are in its
//! own git. With no remote, that tier's usual backup does not exist for it, which is FR-10's
//! to cover (`docs/plane-format.md`).
//!
//! # Nothing here signs anybody in, and no credential is read
//!
//! [`harnesses`] asks whether a sign-in is there. It never starts one and never runs the
//! harness. **A credential file is asked about by its size alone**, never opened: a harness's
//! `auth.json` or `.credentials.json` is a secret. An API key in the environment is asked
//! whether it is set, not what it says. The one file that is read is Claude Code's
//! `~/.claude.json`, which is its settings and not a credential (on macOS the credential is in
//! the keychain): charter parses it only to see whether an `oauthAccount` object is there. A
//! harness that is installed and not signed in still starts: its own login is in its own first
//! screen, which is where W10 puts it.

use std::path::{Path, PathBuf};

use crate::harness::Harness;

/// The local plane's directory name inside charter's directory in the config home.
pub const LOCAL_PLANE: &str = "local-plane";

/// Where this machine's local plane is, under `config_root` (see [`crate::machine::config_root`]).
pub fn local_plane(config_root: &Path) -> PathBuf {
    crate::machine::dir(config_root).join(LOCAL_PLANE)
}

/// Where the local plane's forge comes from when the plane is made (#839).
#[derive(Debug, Clone, Copy)]
pub enum ForgeFrom<'a> {
    /// The `origin` of the repo the first run opens.
    Repo(&'a Path),
    /// The operator's answer: a forge they picked, or the one whose sign-in they pressed.
    Named(crate::forge::Kind),
    /// The operator's answer to the question `repo`'s remote raised: the kind is theirs, and
    /// the host, with the owner when it reads as one, is carried over from the remote so it is
    /// not typed again (#881, [`crate::scaffold::fromremote::hosted_repo`]).
    Answered {
        kind: crate::forge::Kind,
        repo: &'a Path,
    },
}

/// Why [`ensure_local_plane`] made nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotMade {
    /// The repo's remote does not say which forge its project's repos are on: the operator is
    /// asked, and the answer comes back as [`ForgeFrom::Named`]. Says why the remote did not.
    AsksForForge(String),
    /// Anything else, in charter's words.
    Refused(String),
}

impl std::fmt::Display for NotMade {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NotMade::AsksForForge(why) => write!(
                f,
                "purlis cannot tell which forge this project's repos are on: {why}."
            ),
            NotMade::Refused(why) => f.write_str(why),
        }
    }
}

/// The local plane under `config_root`, made first when it is not there yet.
///
/// A plane already there is left exactly as it is, and nothing is asked. The plane is
/// scaffolded the way the app's New project dialog scaffolds one, with no repo adopted and no
/// remote: `git remote` is empty until the operator shares it. Its `[[forge]]` comes from
/// `forge`, by `charter init`'s rule: a repo's `origin`, else the operator's answer.
pub fn ensure_local_plane(config_root: &Path, forge: ForgeFrom<'_>) -> Result<PathBuf, NotMade> {
    let at = local_plane(config_root);
    let root = at.canonicalize().ok();
    if let Some(root) = root.filter(|root| crate::names::has_manifest(root)) {
        return Ok(root);
    }
    // Asked before anything is made, so a question leaves no empty directory behind.
    let (kind, owner, host) = match forge {
        ForgeFrom::Named(kind) => (kind, String::new(), None),
        ForgeFrom::Answered { kind, repo } => {
            match crate::scaffold::fromremote::hosted_repo(repo, kind) {
                Some(hosted) => (kind, hosted.owner, Some(hosted.host)),
                None => (kind, String::new(), None),
            }
        }
        ForgeFrom::Repo(repo) => match crate::scaffold::fromremote::forge_of_repo(repo) {
            Ok(found) => (found.kind, found.owner, None),
            Err(why) => return Err(NotMade::AsksForForge(why)),
        },
    };
    std::fs::create_dir_all(&at).map_err(|why| {
        NotMade::Refused(format!("purlis could not make {} ({why}).", at.display()))
    })?;
    let root = at
        .canonicalize()
        .map_err(|why| NotMade::Refused(format!("purlis cannot read {} ({why}).", at.display())))?;
    let outcome = crate::scaffold::init(
        &crate::plane::Place {
            root: root.clone(),
            is_plane: false,
        },
        &crate::scaffold::InitArgs {
            owner,
            host,
            ..crate::scaffold::InitArgs::for_the_app(false, None, Some(kind))
        },
    );
    if outcome.code != 0 {
        return Err(NotMade::Refused(crate::scaffold::Say::in_full(
            &outcome.said,
        )));
    }
    Ok(root)
}

/// What [`take_in`] made: the workspace and the clone the first chat starts in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TakenIn {
    /// The workspace, named after the repo.
    pub workspace: String,
    /// The repo's clone inside it, which is where a chat about it works.
    pub clone: PathBuf,
    /// The project template laid out, by id, when one was.
    pub template: Option<String>,
    /// What taking the repo in did to the project's forges (#880).
    pub forge: ForgeTaken,
}

/// What [`take_in_from`] did about the forge the repo's `origin` is on (#880).
///
/// **Added, not asked** (D-1094-1): taking the repo in is the operator's act, and a project
/// that does not track the forge its own workspace's repo is on leaves the repo picker and
/// `discover` blind to it. Asking would spend W10's interrupt budget on a question with one
/// sensible answer; the block shows in Settings › Project › Forges, where it can be removed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ForgeTaken {
    /// The remote names no forge purlis can tell (none, or a self-managed host): nothing was
    /// added, and nothing is asked.
    Unnamed,
    /// The project already tracks that forge: a block of that kind is on its host.
    Tracked,
    /// A `[[forge]]` block for it was added, with the owner the remote names.
    Added(crate::scaffold::fromremote::FromRemote),
    /// It was not added, and why. The repo is taken in all the same.
    NotAdded(String),
}

/// Which project template the first run lays out (FR-17).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Choice {
    /// The one that fits the repo ([`crate::template::detect`]), or none when none does: what
    /// the first run picks until the operator picks something else.
    Fits,
    /// No template: the project as `charter init` makes it. The window calls it *None*.
    NoTemplate,
    /// This one, by id, whatever the repo looks like.
    Named(String),
}

/// Clones `repo` into a workspace of the plane at `root` named after it.
///
/// The name is the one `init --adopt` gives the clone: the tail of the repo's `origin`,
/// else its directory's name, made into a workspace name. The repo is read and never
/// written to. Taking in a repo that is already there answers with what is there, so
/// opening the same repo twice is the same workspace both times.
pub fn take_in(root: &Path, repo: &Path) -> Result<TakenIn, String> {
    take_in_from(root, repo, &Choice::NoTemplate)
}

/// [`take_in`], with the project template `choice` names laid into the project first and its
/// starter given to the workspace the repo is taken into.
///
/// **The template is in the project whole, or not at all.** It is resolved, and every harness
/// file it writes a rule into is asked whether it can take one, before the repo is copied, so
/// a template charter does not ship, or a file it cannot extend, stops the open with nothing
/// copied and nothing written. Then the repo is copied, and the template is laid out: personas,
/// then the ask rules, then the workspace's starter last. A failure at any of
/// those takes back everything the template wrote, harness files and `workspace.md` byte for
/// byte ([`crate::template::apply`]), and says why; the workspace is left as a repo opened with
/// no template. Laying a template out is additive, so a second repo opened into the same
/// project adds what the first one's template did not.
pub fn take_in_from(root: &Path, repo: &Path, choice: &Choice) -> Result<TakenIn, String> {
    let template = match choice {
        Choice::NoTemplate => None,
        Choice::Fits => crate::template::detect(repo),
        Choice::Named(id) => Some(crate::template::named(id).ok_or_else(|| {
            format!(
                "purlis has no project template called '{}', so nothing was copied. Pick one \
                 of: {}.",
                crate::shown::one_line(id, 64),
                crate::template::all()
                    .iter()
                    .map(|one| one.id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })?),
    };
    if let Some(template) = template {
        crate::template::check(root, template)?;
    }
    let (workspace, clone) = crate::scaffold::adopt_as_workspace(root, repo, chrono::Utc::now())?;
    if let Some(template) = template {
        crate::template::apply_checked(root, template, Some(&workspace))?;
    }
    // Last, so an open that is refused adds no forge.
    let forge = match crate::scaffold::fromremote::forge_of_repo(repo) {
        Ok(found) => track_forge(root, &found),
        Err(_) => ForgeTaken::Unnamed,
    };
    Ok(TakenIn {
        workspace,
        clone,
        template: template.map(|one| one.id.clone()),
        forge,
    })
}

/// Makes the project at `root` track `found`'s forge: a new `[[forge]]` block through the
/// Settings write seam ([`crate::settings::forges::add`]), unless a block already puts a forge
/// of that kind on its host (D-1094-2: a forge is its kind and host; the owner says only what
/// `discover` lists, and a second owner on a tracked forge is the operator's to add).
///
/// A project that declares no block at all is left as it is: `discover` reads it as the
/// default forge, and a first block would take that one away.
pub fn track_forge(root: &Path, found: &crate::scaffold::fromremote::FromRemote) -> ForgeTaken {
    use crate::settings::{self, Which, forges};
    let read = match settings::read(root, Which::Shared) {
        Ok(read) => read,
        Err(why) => return ForgeTaken::NotAdded(why),
    };
    let manifest = crate::names::manifest_name(root);
    let Ok(cfg) = read.text.parse::<toml::Table>() else {
        return ForgeTaken::NotAdded(format!("{manifest} is not valid TOML"));
    };
    if forges::listed(&read.text).is_empty() {
        return ForgeTaken::NotAdded(format!(
            "{manifest} declares no [[forge]] block, and a first one would change which forge \
             `purlis discover` reads"
        ));
    }
    let wanted = crate::forge::Forge::default_of(found.kind);
    let tracked = crate::forge::to_query(&cfg)
        .map(|blocks| blocks.iter().any(|(forge, _, _)| *forge == wanted));
    match tracked {
        Ok(true) => return ForgeTaken::Tracked,
        Ok(false) => {}
        Err(why) => return ForgeTaken::NotAdded(why),
    }
    let entry = forges::Entry {
        kind: found.kind.word().to_owned(),
        owner: found.owner.clone(),
        host: String::new(),
        exclude: Vec::new(),
    };
    let base = read.exists.then_some(read.text.as_str());
    match forges::add(root, base, &entry) {
        Ok(_) => ForgeTaken::Added(found.clone()),
        Err(refusal) => ForgeTaken::NotAdded(said(refusal.named_at(root))),
    }
}

/// Every reason in `refusal`, in one line.
fn said(refusal: crate::settings::collection::Refusal) -> String {
    refusal
        .file
        .into_iter()
        .chain(refusal.fields.into_iter().map(|one| one.why))
        .chain(refusal.referrers.into_iter().map(|one| one.what))
        .collect::<Vec<_>>()
        .join("; ")
}

/// One harness as the first run found it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HarnessFound {
    pub harness: Harness,
    /// Where its program is, or `None` when it is not installed where charter looks.
    pub program: Option<PathBuf>,
    /// Whether a sign-in was found: a credential file the harness writes at login, or an API
    /// key in the environment it reads. Never a value, only that one is there.
    pub signed_in: bool,
}

/// What [`harnesses`] looks at, handed in so a test can say it rather than set it.
pub struct Looking<'a> {
    /// The environment, asked by name.
    pub env: &'a dyn Fn(&str) -> Option<String>,
    /// The operator's home directory.
    pub home: Option<&'a Path>,
    /// Where programs are searched for (see [`crate::programs::search_dirs`]).
    pub dirs: &'a [PathBuf],
}

/// Every harness charter starts, in [`Harness::ALL`]'s order, with whether it is installed
/// and whether it has been signed in to.
pub fn harnesses(look: &Looking<'_>) -> Vec<HarnessFound> {
    Harness::ALL
        .iter()
        .map(|&harness| HarnessFound {
            harness,
            program: crate::programs::find(harness.name(), look.dirs),
            signed_in: signed_in(harness, look),
        })
        .collect()
}

/// The one harness that is both installed and signed in, when exactly one is: the first chat
/// starts on it without asking which (FR-4, W10's interrupt budget). `None` when there is a
/// choice to make, or nothing ready to start.
pub fn only_ready(found: &[HarnessFound]) -> Option<Harness> {
    let mut ready = found
        .iter()
        .filter(|one| one.program.is_some() && one.signed_in);
    match (ready.next(), ready.next()) {
        (Some(one), None) => Some(one.harness),
        _ => None,
    }
}

/// [`harnesses`] for this process: its environment, its home and its search path.
pub fn harnesses_here() -> Vec<HarnessFound> {
    let env = crate::envvar::var;
    let home = crate::profiles::home();
    let dirs = crate::programs::search_dirs();
    harnesses(&Looking {
        env: &env,
        home: home.as_deref(),
        dirs: &dirs,
    })
}

/// Whether `harness` has a sign-in on this machine, by the places each one documents keeping
/// it. Reads for presence only.
fn signed_in(harness: Harness, look: &Looking<'_>) -> bool {
    let set = |name: &str| (look.env)(name).is_some_and(|value| !value.trim().is_empty());
    let dir = |var: &str, fallback: &str| -> Option<PathBuf> {
        (look.env)(var)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .or_else(|| look.home.map(|home| home.join(fallback)))
    };
    match harness {
        // An API key or a long-lived token in the environment, the credential file Claude
        // Code writes where there is no keychain, or — on macOS, where the credential is in
        // the keychain — the account its config records once somebody has logged in.
        Harness::ClaudeCode => {
            set("ANTHROPIC_API_KEY")
                || set("CLAUDE_CODE_OAUTH_TOKEN")
                || dir("CLAUDE_CONFIG_DIR", ".claude")
                    .is_some_and(|d| nonempty_file(&d.join(".credentials.json")))
                || look
                    .home
                    .is_some_and(|home| names_an_account(&home.join(".claude.json")))
        }
        Harness::Codex => {
            set("OPENAI_API_KEY")
                || dir("CODEX_HOME", ".codex").is_some_and(|d| nonempty_file(&d.join("auth.json")))
        }
        // Its `auth.json` is `{}` until a provider is added, and a provider's entry is longer
        // than any spelling of an empty object, so the size says which without opening it.
        Harness::Opencode => dir("XDG_DATA_HOME", ".local/share")
            .is_some_and(|d| longer_than(&d.join("opencode").join("auth.json"), EMPTY_OBJECT)),
    }
}

/// The longest an empty JSON object is likely to be written: `{}`, with whitespace and a
/// newline around it. A provider's entry is well past it.
const EMPTY_OBJECT: u64 = 8;

fn nonempty_file(path: &Path) -> bool {
    longer_than(path, 0)
}

/// A file at `path` longer than `bytes`, asked of its metadata: nothing in it is read.
fn longer_than(path: &Path, bytes: u64) -> bool {
    std::fs::metadata(path).is_ok_and(|meta| meta.is_file() && meta.len() > bytes)
}

/// Claude Code's `~/.claude.json` — its settings, not a credential — records `oauthAccount`
/// once somebody has logged in. The one file here that is parsed; see the module note.
fn names_an_account(path: &Path) -> bool {
    read_object(path).is_some_and(|object| {
        object
            .get("oauthAccount")
            .is_some_and(|account| account.is_object())
    })
}

fn read_object(path: &Path) -> Option<serde_json::Map<String, serde_json::Value>> {
    // Only ever `~/.claude.json`; every credential file is asked about by its size.
    let text = std::fs::read_to_string(path).ok()?;
    match serde_json::from_str(&text).ok()? {
        serde_json::Value::Object(object) => Some(object),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_env(_: &str) -> Option<String> {
        None
    }

    fn a_program(dir: &Path, name: &str) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::create_dir_all(dir).expect("a bin directory");
        let path = dir.join(name);
        std::fs::write(&path, "#!/bin/sh\n").expect("a program");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
            .expect("made runnable");
    }

    fn found(home: &Path, env: &dyn Fn(&str) -> Option<String>) -> Vec<HarnessFound> {
        let dirs = vec![home.join("bin")];
        harnesses(&Looking {
            env,
            home: Some(home),
            dirs: &dirs,
        })
    }

    fn of(found: &[HarnessFound], harness: Harness) -> HarnessFound {
        found
            .iter()
            .find(|one| one.harness == harness)
            .cloned()
            .expect("every harness is listed")
    }

    #[test]
    fn a_machine_with_nothing_installed_lists_every_harness_as_missing_and_signed_out() {
        let home = tempfile::tempdir().expect("a home");

        let found = found(home.path(), &no_env);

        assert_eq!(found.len(), Harness::ALL.len());
        assert!(
            found
                .iter()
                .all(|one| one.program.is_none() && !one.signed_in)
        );
    }

    #[test]
    fn an_installed_harness_is_found_where_charter_searches() {
        let home = tempfile::tempdir().expect("a home");
        a_program(&home.path().join("bin"), "codex");

        let found = found(home.path(), &no_env);

        assert_eq!(
            of(&found, Harness::Codex).program,
            Some(home.path().join("bin/codex"))
        );
        assert_eq!(of(&found, Harness::ClaudeCode).program, None);
    }

    #[test]
    fn claude_code_is_signed_in_when_its_config_names_an_account() {
        let home = tempfile::tempdir().expect("a home");
        std::fs::write(
            home.path().join(".claude.json"),
            r#"{"numStartups": 3, "oauthAccount": {"emailAddress": "a@b.invalid"}}"#,
        )
        .expect("claude's config");

        assert!(of(&found(home.path(), &no_env), Harness::ClaudeCode).signed_in);
    }

    #[test]
    fn claude_code_with_a_config_and_no_account_is_not_signed_in() {
        let home = tempfile::tempdir().expect("a home");
        std::fs::write(home.path().join(".claude.json"), r#"{"numStartups": 1}"#)
            .expect("claude's config");

        assert!(!of(&found(home.path(), &no_env), Harness::ClaudeCode).signed_in);
    }

    #[test]
    fn claude_code_is_signed_in_by_its_credential_file_or_an_api_key() {
        let home = tempfile::tempdir().expect("a home");
        std::fs::create_dir_all(home.path().join(".claude")).expect("claude's directory");
        std::fs::write(home.path().join(".claude/.credentials.json"), "{\"x\":1}")
            .expect("the credential file");
        assert!(of(&found(home.path(), &no_env), Harness::ClaudeCode).signed_in);

        let bare = tempfile::tempdir().expect("another home");
        let keyed = |name: &str| (name == "ANTHROPIC_API_KEY").then(|| "sk-test".to_owned());
        assert!(of(&found(bare.path(), &keyed), Harness::ClaudeCode).signed_in);
    }

    #[test]
    fn codex_is_signed_in_by_its_auth_file_under_codex_home() {
        let home = tempfile::tempdir().expect("a home");
        let elsewhere = home.path().join("codex-home");
        std::fs::create_dir_all(&elsewhere).expect("codex home");
        std::fs::write(elsewhere.join("auth.json"), "{\"tokens\":{}}").expect("auth");
        let env = {
            let at = elsewhere.display().to_string();
            move |name: &str| (name == "CODEX_HOME").then(|| at.clone())
        };

        assert!(of(&found(home.path(), &env), Harness::Codex).signed_in);
        assert!(!of(&found(home.path(), &no_env), Harness::Codex).signed_in);
    }

    #[test]
    fn opencode_is_signed_in_only_once_a_provider_is_in_its_auth_file() {
        let home = tempfile::tempdir().expect("a home");
        let data = home.path().join(".local/share/opencode");
        std::fs::create_dir_all(&data).expect("opencode's data");
        std::fs::write(data.join("auth.json"), "{}").expect("empty auth");
        assert!(!of(&found(home.path(), &no_env), Harness::Opencode).signed_in);

        std::fs::write(data.join("auth.json"), r#"{"anthropic":{"type":"oauth"}}"#)
            .expect("a provider");
        assert!(of(&found(home.path(), &no_env), Harness::Opencode).signed_in);
    }

    const GITHUB: ForgeFrom<'static> = ForgeFrom::Named(crate::forge::Kind::GitHub);

    /// The `[[forge]]` block's `kind` and `owner` in the plane at `root`.
    fn forge_of(root: &Path) -> (String, String) {
        let toml: toml::Table = std::fs::read_to_string(root.join(crate::plane::MANIFEST))
            .expect("charter.toml")
            .parse()
            .expect("charter.toml is TOML");
        let block = &toml["forge"][0];
        let text = |key: &str| {
            block
                .get(key)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_owned()
        };
        (text("kind"), text("owner"))
    }

    /// #839: the first run's project tracks the forge the opened repo's `origin` is on, with
    /// its owner, by `charter init`'s rule.
    #[test]
    fn the_local_plane_takes_its_forge_and_owner_from_the_repos_origin() {
        let dir = tempfile::tempdir().expect("a directory");
        let repo = a_repo(&dir.path().join("svc"));
        assert!(
            crate::testgit::run(
                &repo,
                &["remote", "add", "origin", "git@gitlab.com:group/svc.git"]
            )
            .ok()
        );

        let root = ensure_local_plane(&dir.path().join("cfg"), ForgeFrom::Repo(&repo))
            .expect("the local plane");

        assert_eq!(forge_of(&root), ("gitlab".to_owned(), "group".to_owned()));
    }

    /// #839's "else ask": a repo whose remote does not say makes nothing, and the answer the
    /// operator gives is the plane's forge.
    #[test]
    fn a_repo_whose_remote_does_not_say_asks_and_the_answer_is_the_forge() {
        let dir = tempfile::tempdir().expect("a directory");
        let repo = a_repo(&dir.path().join("svc"));
        let config = dir.path().join("cfg");

        let asked = ensure_local_plane(&config, ForgeFrom::Repo(&repo));

        assert!(
            matches!(&asked, Err(NotMade::AsksForForge(why)) if why.ends_with("has no `origin` remote")),
            "{asked:?}"
        );
        assert!(!local_plane(&config).exists(), "asking made a directory");

        let root = ensure_local_plane(&config, ForgeFrom::Named(crate::forge::Kind::GitLab))
            .expect("the local plane");
        assert_eq!(forge_of(&root).0, "gitlab");

        let again = ensure_local_plane(&config, ForgeFrom::Repo(&repo));
        assert_eq!(again, Ok(root), "a plane that is there asks nothing");
    }

    /// #881: the answer to a self-managed remote's question keeps the remote's host, and the
    /// owner its path names, in the project's forge block.
    #[test]
    fn an_answered_self_managed_remote_keeps_its_host_and_owner() {
        let dir = tempfile::tempdir().expect("a directory");
        let repo = a_repo(&dir.path().join("svc"));
        assert!(
            crate::testgit::run(
                &repo,
                &[
                    "remote",
                    "add",
                    "origin",
                    "https://git.example.com/platform/svc.git"
                ]
            )
            .ok()
        );
        let config = dir.path().join("cfg");
        assert!(matches!(
            ensure_local_plane(&config, ForgeFrom::Repo(&repo)),
            Err(NotMade::AsksForForge(_))
        ));

        let root = ensure_local_plane(
            &config,
            ForgeFrom::Answered {
                kind: crate::forge::Kind::GitLab,
                repo: &repo,
            },
        )
        .expect("the local plane");

        let cfg = crate::forge::load_config(&root).expect("charter.toml");
        let forges = crate::forge::to_query(&cfg).expect("the forges read");
        assert_eq!(forges.len(), 1, "{forges:?}");
        assert_eq!(forges[0].0.host, "git.example.com");
        assert_eq!(forges[0].1, "platform");
    }

    /// #880, end to end: a GitLab repo taken into a project made for GitHub adds the GitLab
    /// forge once, and a second GitLab repo adds nothing.
    #[test]
    fn a_repo_on_a_second_forge_is_tracked_once_it_is_taken_in() {
        let dir = tempfile::tempdir().expect("a directory");
        let root = ensure_local_plane(&dir.path().join("cfg"), GITHUB).expect("the local plane");
        let on = |name: &str, url: &str| {
            let repo = a_repo(&dir.path().join(name));
            assert!(crate::testgit::run(&repo, &["remote", "add", "origin", url]).ok());
            repo
        };

        let first = take_in(&root, &on("api", "git@gitlab.com:platform/api.git")).expect("in");
        let second = take_in(&root, &on("web", "https://gitlab.com/platform/web.git")).expect("in");
        let plain = take_in(&root, &a_repo(&dir.path().join("notes"))).expect("in");

        assert!(
            matches!(first.forge, ForgeTaken::Added(_)),
            "{:?}",
            first.forge
        );
        assert_eq!(second.forge, ForgeTaken::Tracked);
        assert_eq!(plain.forge, ForgeTaken::Unnamed);
        assert_eq!(
            hosts(&root),
            [
                (crate::forge::Kind::GitHub, String::new()),
                (crate::forge::Kind::GitLab, "platform".to_owned())
            ]
        );
    }

    /// Each forge `discover` and the repo picker would query in the project at `root`, with
    /// its owner.
    fn hosts(root: &Path) -> Vec<(crate::forge::Kind, String)> {
        let cfg = crate::forge::load_config(root).expect("charter.toml");
        crate::forge::to_query(&cfg)
            .expect("the forges read")
            .into_iter()
            .map(|(forge, owner, _)| (forge.kind, owner))
            .collect()
    }

    /// A scratch project with `shared` as its `charter.toml`.
    fn project(shared: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("a directory");
        std::fs::write(dir.path().join("charter.toml"), shared).expect("charter.toml");
        crate::testgit::run(dir.path(), &["init", "-q"]);
        dir
    }

    fn remote(kind: crate::forge::Kind, owner: &str) -> crate::scaffold::fromremote::FromRemote {
        crate::scaffold::fromremote::FromRemote {
            kind,
            owner: owner.to_owned(),
        }
    }

    const ON_GITHUB: &str = "schema = 1\n\n[[forge]]\nkind = \"github\"\nowner = \"acme\"\n";

    #[test]
    fn a_forge_the_project_does_not_track_is_added_with_the_remotes_owner() {
        use crate::forge::Kind;
        let dir = project(ON_GITHUB);

        let taken = track_forge(dir.path(), &remote(Kind::GitLab, "platform/web"));

        assert_eq!(
            taken,
            ForgeTaken::Added(remote(Kind::GitLab, "platform/web"))
        );
        assert_eq!(
            hosts(dir.path()),
            [
                (Kind::GitHub, "acme".to_owned()),
                (Kind::GitLab, "platform/web".to_owned())
            ]
        );
        assert_eq!(
            track_forge(dir.path(), &remote(Kind::GitLab, "platform/web")),
            ForgeTaken::Tracked
        );
    }

    /// D-1094-2: a forge is its kind and host. Another owner on a tracked forge adds nothing.
    #[test]
    fn another_owner_on_a_tracked_forge_adds_nothing() {
        use crate::forge::Kind;
        let dir = project(ON_GITHUB);

        assert_eq!(
            track_forge(dir.path(), &remote(Kind::GitHub, "someone-else")),
            ForgeTaken::Tracked
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join("charter.toml")).expect("read"),
            ON_GITHUB
        );
    }

    /// A self-managed block is not the kind's own host: the default host is added beside it.
    #[test]
    fn a_self_managed_block_does_not_track_the_kinds_own_host() {
        use crate::forge::Kind;
        let dir =
            project("schema = 1\n\n[[forge]]\nkind = \"gitlab\"\nhost = \"git.example.com\"\n");

        assert_eq!(
            track_forge(dir.path(), &remote(Kind::GitLab, "group")),
            ForgeTaken::Added(remote(Kind::GitLab, "group"))
        );
    }

    /// A project with no block reads as the default forge; a first block would take it away.
    #[test]
    fn a_project_that_declares_no_forge_is_left_as_it_is() {
        use crate::forge::Kind;
        let dir = project("schema = 1\n");

        let taken = track_forge(dir.path(), &remote(Kind::GitHub, "acme"));

        assert!(
            matches!(&taken, ForgeTaken::NotAdded(why) if why.contains("declares no [[forge]] block")),
            "{taken:?}"
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join("charter.toml")).expect("read"),
            "schema = 1\n"
        );
    }

    #[test]
    fn the_local_plane_is_in_charters_directory_in_the_config_home() {
        assert_eq!(
            local_plane(Path::new("/cfg")),
            PathBuf::from("/cfg/charter/local-plane")
        );
    }

    #[test]
    fn the_local_plane_is_made_once_with_no_remote_and_left_alone_after() {
        let config = tempfile::tempdir().expect("a config home");

        let root = ensure_local_plane(config.path(), GITHUB).expect("the local plane is made");

        assert!(root.join(crate::plane::MANIFEST).is_file());
        assert!(root.starts_with(config.path().canonicalize().expect("resolved")));
        let remotes = crate::forklock::output(
            std::process::Command::new("git")
                .arg("-C")
                .arg(&root)
                .arg("remote"),
        )
        .expect("git runs");
        assert!(
            String::from_utf8_lossy(&remotes.stdout).trim().is_empty(),
            "the local plane has a remote"
        );

        std::fs::write(root.join("mine.txt"), "kept").expect("a file of the operator's");
        let again = ensure_local_plane(config.path(), GITHUB).expect("a second call answers");
        assert_eq!(again, root);
        assert!(root.join("mine.txt").is_file());
    }

    /// A repo with one commit, made through `testgit` so it never asks the developer's
    /// signer (charter-app#191).
    fn a_repo(at: &Path) -> PathBuf {
        std::fs::create_dir_all(at).expect("the repo's directory");
        for argv in [
            vec!["init", "-q", "-b", "main", "."],
            vec!["config", "user.email", "t@e.invalid"],
            vec!["config", "user.name", "t"],
            vec!["commit", "-q", "--allow-empty", "-m", "first"],
        ] {
            assert!(crate::testgit::run(at, &argv).ok(), "git {argv:?}");
        }
        at.to_path_buf()
    }

    #[test]
    fn a_repo_is_taken_in_as_a_workspace_named_after_it_and_not_written_into() {
        let dir = tempfile::tempdir().expect("a directory");
        let root = ensure_local_plane(&dir.path().join("cfg"), GITHUB).expect("the local plane");
        let repo = a_repo(&dir.path().join("svc"));
        let before = std::fs::read_dir(&repo).expect("the repo").count();

        let taken = take_in(&root, &repo).expect("a repo is taken in");

        assert_eq!(taken.workspace, "svc");
        assert_eq!(taken.clone, root.join("workspaces/svc/svc"));
        assert!(taken.clone.join(".git").exists(), "nothing was cloned");
        assert_eq!(
            std::fs::read_dir(&repo).expect("the repo").count(),
            before,
            "something was written into the repo"
        );
    }

    #[test]
    fn taking_in_the_same_repo_twice_answers_with_the_same_workspace() {
        let dir = tempfile::tempdir().expect("a directory");
        let root = ensure_local_plane(&dir.path().join("cfg"), GITHUB).expect("the local plane");
        let repo = a_repo(&dir.path().join("svc"));

        let first = take_in(&root, &repo).expect("taken in");
        let second = take_in(&root, &repo).expect("taken in again");

        assert_eq!(first, second);
    }

    #[test]
    fn a_directory_that_is_not_a_repo_is_refused_and_no_workspace_is_made() {
        let dir = tempfile::tempdir().expect("a directory");
        let root = ensure_local_plane(&dir.path().join("cfg"), GITHUB).expect("the local plane");
        let papers = dir.path().join("papers");
        std::fs::create_dir_all(&papers).expect("not a repo");

        let refused = take_in(&root, &papers).expect_err("only a repo is taken in");

        assert!(
            refused.contains("is not the top level of a git repo"),
            "{refused}"
        );
        assert!(!root.join("workspaces/papers").exists());
    }

    #[test]
    fn a_different_repo_with_the_same_name_gets_a_workspace_of_its_own() {
        // Two repos called `svc` from different places: the second must never be handed the
        // first one's clone, which would start the chat in the wrong repo.
        let dir = tempfile::tempdir().expect("a directory");
        let root = ensure_local_plane(&dir.path().join("cfg"), GITHUB).expect("the local plane");
        let one = a_repo(&dir.path().join("a/svc"));
        let other = a_repo(&dir.path().join("b/svc"));

        let first = take_in(&root, &one).expect("the first svc");
        let second = take_in(&root, &other).expect("the second svc");
        let again = take_in(&root, &other).expect("the second svc, opened again");

        assert_eq!(first.workspace, "svc");
        assert_eq!(second.workspace, "svc-2");
        assert_eq!(second.clone, root.join("workspaces/svc-2/svc-2"));
        assert_eq!(
            again, second,
            "reopening the second svc is its own workspace again"
        );
    }

    #[test]
    fn a_clone_already_in_the_projects_workspaces_is_that_workspace_and_is_not_copied_again() {
        let dir = tempfile::tempdir().expect("a directory");
        let root = ensure_local_plane(&dir.path().join("cfg"), GITHUB).expect("the local plane");
        let taken = take_in(&root, &a_repo(&dir.path().join("svc"))).expect("taken in");

        let picked = take_in(&root, &taken.clone).expect("its clone, picked");

        assert_eq!(picked, taken);
        assert!(
            !root.join("workspaces/svc-2").exists(),
            "the clone was copied again"
        );
    }

    #[test]
    fn a_path_that_cannot_be_read_is_refused_rather_than_taken_as_typed() {
        let dir = tempfile::tempdir().expect("a directory");
        let root = ensure_local_plane(&dir.path().join("cfg"), GITHUB).expect("the local plane");

        let refused = take_in(&root, &dir.path().join("not-there")).expect_err("refused");

        assert!(refused.contains("cannot read"), "{refused}");
    }

    fn ready(harness: Harness, installed: bool, signed_in: bool) -> HarnessFound {
        HarnessFound {
            harness,
            program: installed.then(|| PathBuf::from("/bin/x")),
            signed_in,
        }
    }

    #[test]
    fn the_first_chat_skips_the_picker_only_when_exactly_one_harness_is_ready() {
        let one = [
            ready(Harness::ClaudeCode, true, true),
            ready(Harness::Codex, true, false),
            ready(Harness::Opencode, false, true),
        ];
        assert_eq!(only_ready(&one), Some(Harness::ClaudeCode));

        let two = [
            ready(Harness::ClaudeCode, true, true),
            ready(Harness::Codex, true, true),
        ];
        assert_eq!(only_ready(&two), None);
        assert_eq!(only_ready(&[ready(Harness::Codex, true, false)]), None);
    }
}
