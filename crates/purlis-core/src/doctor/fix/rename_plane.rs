//! **`rename-plane`** (RN-7, V93g): the committed files of a project, renamed to purlis's names
//! in ONE commit the operator asked for and can review.
//!
//! Never automatic: it is a fix applied only by its id ([`super::FixId::by_name_only`]), because
//! it changes what every teammate pulls. What the commit holds, and nothing else:
//!
//! - `charter.toml` → `purlis.toml`, with `requires = [{ feature = "purlis-names", … }]` (and
//!   `schema = 2`, which a project that lists `requires` declares, V37a). A build without the
//!   feature opens the project read-only and says which build has it ([`crate::compat`]). A
//!   build older than the purlis manifest reads no `charter.toml` here and does not see a
//!   project at all.
//! - `.charter-scan-allow.toml` → `.purlis-scan-allow.toml`.
//! - The managed blocks' markers, rewritten in place: the merge rules in `.gitattributes`, the
//!   live workspaces in `.gitignore`, the personas block in `README.md`, and the marker on each
//!   agent `persona sync-agents` generated. A committed `workspace.json` that charter stamped
//!   has its `charter_generated` key renamed where it stands; the digest does not cover the key,
//!   so the file stays charter's.
//! - `.claude/settings.json`: `CHARTER_HARNESS`, and the `charter …` ask and deny rules each
//!   given a `purlis …` twin, through [`settings::renamed_to_purlis`]; `opencode.json`'s ask and
//!   deny globs the same. The `charter` rules stay, because `charter` still runs the command line
//!   for the rename's window. Hook and status-line commands keep `charter` for the whole window
//!   (D-RN7-11): the alias resolves on every build, and a hook whose program is not on `PATH`
//!   fails open.
//! - Personas' `charter:<skill>` references, in their definitions and in the agents generated
//!   from them, become `purlis:<skill>`.
//!
//! Only files git tracks are touched: an ignored `workspace.json` is this machine's, and is
//! renamed the next time charter writes it.
//!
//! # Refused, with nothing written
//!
//! From inside a chat (D-RN7-12): it rewrites committed files for everyone, so it is the
//! operator's own act, from the app or a terminal. On a project this charter may not write
//! ([`super::refusal`], as every fix), outside a git repository whose top is the project, with tracked changes not committed (the commit must
//! hold the rename and nothing else), and with a file under both names (which one holds the
//! settings is the operator's call).
//!
//! # All or nothing
//!
//! Every change is made in the working tree, staged by its path, and committed — through
//! charter's own git runner, which runs no hooks, as every commit charter makes. When a step
//! fails before the commit is made — a write, the stage, the commit itself — every path it
//! touched is put back from `HEAD`, the files it created are removed, and the fix answers why.
//! Once the commit is made nothing is undone.

use std::path::Path;

use crate::scaffold::settings;

use super::Fixed;

/// The commit's subject line.
pub const SUBJECT: &str = "Rename this project's files to purlis names (rename-plane)";

/// How long the fix waits for a save running in this process to let go of the project.
const CLAIM_WAIT: std::time::Duration = std::time::Duration::from_secs(10);

/// What a chat asking for the fix is told (D-RN7-12).
pub const FROM_A_CHAT: &str = "rename-plane rewrites this project's committed files for everyone \
     who works in it, so it is not run from inside a chat; nothing was written. Run `purlis \
     doctor --fix rename-plane` from a terminal, or apply the fix from the app's Doctor";

/// Apply `rename-plane` to the project at `root`, as this process's environment says it runs.
/// The caller has already refused a project this charter may not write.
pub fn apply(root: &Path) -> Fixed {
    apply_in(root, &|name| crate::envvar::var(name))
}

/// [`apply`], reading the environment through `env`: refused when it carries a chat's session
/// (the product's own variable, under either name).
pub fn apply_in(root: &Path, env: &dyn Fn(&str) -> Option<String>) -> Fixed {
    if env(crate::active::SESSION_ID_ENV).is_some_and(|id| !id.is_empty()) {
        return Fixed::Refused(FROM_A_CHAT.to_owned());
    }
    let Some(_claim) = crate::planegit::Claim::within(root, CLAIM_WAIT) else {
        return Fixed::Refused(
            "a save is running in this project; nothing was written. Run the fix again once it \
             has finished"
                .to_owned(),
        );
    };
    let _held = crate::rewrite::Lock::on(root);
    if let Err(why) = ready(root) {
        return Fixed::Refused(why);
    }
    let tracked = match tracked(root) {
        Ok(tracked) => tracked,
        Err(why) => return Fixed::Refused(why),
    };
    let mut work = Work::default();
    if let Err(why) = rename(root, &tracked, &mut work) {
        let restored = work.undo(root);
        return Fixed::Refused(format!("{why}; {restored}"));
    }
    if work.touched.is_empty() {
        return Fixed::Ran {
            said: vec![
                "✓ this project's committed files already use purlis's names — nothing to do."
                    .to_owned(),
            ],
            complete: true,
        };
    }
    let before = git(root, &["rev-parse", "HEAD"]).unwrap_or_default();
    match commit(root, &work) {
        Ok(sha) => {
            let mut said = work.said;
            match sha {
                Some(sha) => said.push(format!("✓ committed {sha}: {SUBJECT}")),
                None => said.push(format!(
                    "✓ committed: {SUBJECT} (git could not name the commit; `git log -1` shows it)"
                )),
            }
            said.push(format!(
                "  A build without the `{}` feature now opens this project read-only; teammates \
                 update the app to work in it.",
                crate::names::PURLIS_NAMES_FEATURE
            ));
            // The rules are in force in every workspace layer at once, as `guard ask` carries
            // them (#449). Layers are this machine's and not in the commit.
            for line in crate::guardcmd::mirror(root).lines() {
                said.push(format!("  {line}"));
            }
            Fixed::Ran {
                said,
                complete: true,
            }
        }
        Err(why) => {
            // A commit git reports failing may still have been made (a deadline that passed
            // after it wrote): a moved HEAD is a commit, and a commit is never undone.
            let now = git(root, &["rev-parse", "HEAD"]).unwrap_or_default();
            if !before.is_empty() && now != before {
                return Fixed::Ran {
                    said: vec![format!(
                        "✗ {why}, but HEAD moved: the rename is committed; check it with `git \
                         log -1`"
                    )],
                    complete: false,
                };
            }
            let restored = work.undo(root);
            Fixed::Refused(format!("{why}; {restored}"))
        }
    }
}

/// What the fix changed so far: each path it touched (relative to the project, as git spells
/// it), the ones it created, and the lines it says.
#[derive(Default)]
struct Work {
    touched: Vec<String>,
    created: Vec<String>,
    said: Vec<String>,
}

impl Work {
    fn touch(&mut self, path: &str) {
        if !self.touched.iter().any(|p| p == path) {
            self.touched.push(path.to_owned());
        }
    }

    /// Put every touched path back as `HEAD` has it, and remove the paths the fix created.
    /// Answers what it did, for the refusal.
    fn undo(&self, root: &Path) -> String {
        if self.touched.is_empty() {
            return "nothing was written".to_owned();
        }
        let mut args = vec!["reset", "-q", "--"];
        args.extend(self.touched.iter().map(String::as_str));
        let reset = git(root, &args);
        let in_head: Vec<&str> = self
            .touched
            .iter()
            .filter(|p| !self.created.contains(p))
            .map(String::as_str)
            .collect();
        let mut restored = reset.is_ok();
        if !in_head.is_empty() {
            let mut args = vec!["checkout", "-q", "HEAD", "--"];
            args.extend(in_head);
            restored &= git(root, &args).is_ok();
        }
        for path in &self.created {
            let at = root.join(path);
            if at.exists() {
                restored &= std::fs::remove_file(at).is_ok();
            }
        }
        if restored {
            "every file it changed was put back as it was".to_owned()
        } else {
            format!(
                "not every file could be put back — `git status` shows what is left; `git \
                 checkout HEAD -- {}` restores them",
                self.touched.join(" ")
            )
        }
    }
}

/// Why the fix may not run here, or `Ok`.
fn ready(root: &Path) -> Result<(), String> {
    let top = git(root, &["rev-parse", "--show-toplevel"]).map_err(|_| {
        "this project is not a git repository, so there is no commit to make the rename in; \
         nothing was written"
            .to_owned()
    })?;
    let same = |a: &Path, b: &Path| match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    };
    if !same(Path::new(top.trim()), root) {
        return Err(format!(
            "this project is inside the git repository at {}, not at its top, so a commit there \
             would carry more than this project; nothing was written",
            top.trim()
        ));
    }
    let status = git(root, &["status", "--porcelain=v1", "-uno"])
        .map_err(|why| format!("git could not say whether this project has changes ({why})"))?;
    let changed: Vec<&str> = status.lines().filter(|l| !l.trim().is_empty()).collect();
    if !changed.is_empty() {
        let shown: Vec<String> = changed
            .iter()
            .take(5)
            .map(|l| l.get(3..).unwrap_or(l).to_owned())
            .collect();
        let more = changed.len().saturating_sub(shown.len());
        let more = if more > 0 {
            format!(" and {more} more")
        } else {
            String::new()
        };
        return Err(format!(
            "this project has changes that are not committed ({}{more}), and the rename must be a \
             commit of its own; nothing was written. Save or commit them first, then run the fix \
             again",
            shown.join(", ")
        ));
    }
    for name in [crate::names::PLANE_MANIFEST, crate::names::SCAN_ALLOW] {
        let at = name.file_in(root);
        if !at.leftovers.is_empty() {
            return Err(format!(
                "this project has both {} and {}, and which of them holds what you meant is \
                 yours to say; nothing was written. Keep one — move what you need from {} into \
                 {} and remove {} — then run the fix again",
                name.write,
                name.newest_old(),
                name.newest_old(),
                name.write,
                name.newest_old()
            ));
        }
    }
    Ok(())
}

/// Every path git tracks in the project.
fn tracked(root: &Path) -> Result<Vec<String>, String> {
    let out = git(root, &["ls-files", "-z"])
        .map_err(|why| format!("git could not list this project's files ({why})"))?;
    Ok(out
        .split('\0')
        .filter(|p| !p.is_empty())
        .map(str::to_owned)
        .collect())
}

/// Every change, made in the working tree and recorded in `work`.
fn rename(root: &Path, tracked: &[String], work: &mut Work) -> Result<(), String> {
    let has = |path: &str| tracked.iter().any(|p| p == path);
    manifest(root, &has, work)?;

    let old_allow = crate::names::SCAN_ALLOW.newest_old();
    let new_allow = crate::names::SCAN_ALLOW.write;
    if has(old_allow) {
        git(root, &["mv", "--", old_allow, new_allow])
            .map_err(|why| format!("could not rename {old_allow} ({why})"))?;
        work.touch(old_allow);
        work.touch(new_allow);
        work.created.push(new_allow.to_owned());
        work.said
            .push(format!("✓ renamed {old_allow} to {new_allow}"));
    }

    use crate::names::{
        LIVE_BEGIN, LIVE_END, MERGE_RULES_BEGIN, MERGE_RULES_END, PERSONAS_BEGIN,
        SYNC_AGENTS_MARKER,
    };
    if has(".gitattributes") {
        markers(
            root,
            ".gitattributes",
            &[MERGE_RULES_BEGIN, MERGE_RULES_END],
            work,
        )?;
    }
    if has(".gitignore") {
        markers(root, ".gitignore", &[LIVE_BEGIN, LIVE_END], work)?;
    }
    if has("README.md") {
        markers(root, "README.md", &[PERSONAS_BEGIN], work)?;
    }
    for path in tracked {
        if is_workspace_manifest(path) {
            workspace_key(root, path, work)?;
        } else if is_agent(path) {
            let text = read(root, path)?;
            if crate::personaverbs::agents::carries_marker(&text) {
                let renamed = skill_refs(&swap(&text, &[SYNC_AGENTS_MARKER]));
                write_if_changed(root, path, &text, &renamed, "the agent marker", work)?;
            }
        } else if is_persona(path) {
            let text = read(root, path)?;
            write_if_changed(
                root,
                path,
                &text,
                &skill_refs(&text),
                "skill references",
                work,
            )?;
        }
    }

    if has(settings::SETTINGS)
        && let Some(text) = settings::renamed_to_purlis(root)?
    {
        write(root, settings::SETTINGS, &text)?;
        work.touch(settings::SETTINGS);
        work.said.push(format!(
            "✓ {}: the harness variable and rules under purlis's name",
            settings::SETTINGS
        ));
    }
    if has(settings::OPENCODE) {
        let twins = settings::opencode_rules_to_twin(root)?;
        // Touched before the first write, so a write that fails part-way is put back too.
        if !twins.is_empty() {
            work.touch(settings::OPENCODE);
        }
        let mut added = 0;
        for (glob, decision) in twins {
            match settings::ensure_opencode_rule(root, &glob, &decision, false) {
                settings::Wrote::Created => added += 1,
                settings::Wrote::Present | settings::Wrote::Denied => {}
                settings::Wrote::Malformed(what) => {
                    return Err(format!("{what} is not a file purlis can read"));
                }
                settings::Wrote::Blocked(dir) => {
                    return Err(format!("{} is not a directory", dir.display()));
                }
                settings::Wrote::Failed(path, e) => {
                    return Err(format!("could not write {} ({e})", path.display()));
                }
            }
        }
        if added == 0 {
            // Every twin was there already: nothing was written, so nothing to commit.
            work.touched.retain(|p| p != settings::OPENCODE);
        } else {
            work.said.push(format!(
                "✓ {}: {added} rule(s) under purlis's name, beside charter's",
                settings::OPENCODE
            ));
        }
    }
    Ok(())
}

/// The manifest, renamed and marked as requiring purlis's names.
fn manifest(root: &Path, has: &dyn Fn(&str) -> bool, work: &mut Work) -> Result<(), String> {
    let old = crate::names::PLANE_MANIFEST.newest_old();
    let new = crate::names::PLANE_MANIFEST.write;
    let at = crate::names::manifest_name(root);
    if !has(at) {
        return Err(format!(
            "git does not track {at}, so a commit cannot carry its rename; commit it first"
        ));
    }
    let text = read(root, at)?;
    let marked = with_feature(&text).map_err(|why| format!("{at}: {why}"))?;
    if at == old {
        git(root, &["mv", "--", old, new])
            .map_err(|why| format!("could not rename {old} ({why})"))?;
        work.touch(old);
        work.touch(new);
        work.created.push(new.to_owned());
        work.said.push(format!("✓ renamed {old} to {new}"));
    }
    if let Some(marked) = marked {
        write(root, new, &marked)?;
        work.touch(new);
        work.said.push(format!(
            "✓ {new} requires `{}`",
            crate::names::PURLIS_NAMES_FEATURE
        ));
    }
    Ok(())
}

/// `text` with `requires` holding the purlis-names feature and `schema` at least 2, in the
/// file's own layout, or `None` when it already says both.
fn with_feature(text: &str) -> Result<Option<String>, String> {
    let mut doc: toml_edit::DocumentMut = text
        .parse()
        .map_err(|e: toml_edit::TomlError| format!("it is not valid TOML ({})", e.message()))?;
    let mut changed = false;
    let schema = doc.get("schema").and_then(toml_edit::Item::as_integer);
    if schema.is_none_or(|s| s < crate::compat::SCHEMA) {
        if doc.get("schema").is_some() && schema.is_none() {
            return Err("its `schema` is not a number".to_owned());
        }
        doc["schema"] = toml_edit::value(crate::compat::SCHEMA);
        changed = true;
    }
    let feature = crate::names::PURLIS_NAMES_FEATURE;
    let mut entry = toml_edit::InlineTable::new();
    entry.insert("feature", feature.into());
    entry.insert("since", env!("CARGO_PKG_VERSION").into());
    match doc.get_mut("requires") {
        None => {
            let mut list = toml_edit::Array::new();
            list.push(entry);
            doc["requires"] = toml_edit::value(list);
            changed = true;
        }
        Some(item) => {
            let Some(list) = item.as_array_mut() else {
                return Err("its `requires` is not a list".to_owned());
            };
            let there = list.iter().any(|e| {
                e.as_inline_table()
                    .and_then(|t| t.get("feature"))
                    .and_then(toml_edit::Value::as_str)
                    == Some(feature)
            });
            if !there {
                list.push(entry);
                changed = true;
            }
        }
    }
    Ok(changed.then(|| doc.to_string()))
}

/// Each whole line of `rel` that is one of `names`' old spellings, as that name's purlis one.
fn markers(
    root: &Path,
    rel: &str,
    names: &[crate::names::Name],
    work: &mut Work,
) -> Result<(), String> {
    let text = read(root, rel)?;
    let renamed: String = text
        .split_inclusive('\n')
        .map(|line| {
            let body = line.trim_end_matches(['\n', '\r']);
            let end = &line[body.len()..];
            names
                .iter()
                .find(|name| name.reads.contains(&body))
                .map_or_else(|| line.to_owned(), |name| format!("{}{end}", name.write))
        })
        .collect();
    // The personas marker is found anywhere in the README, as the roster finds it.
    let renamed = if rel == "README.md" {
        swap(&renamed, names)
    } else {
        renamed
    };
    write_if_changed(root, rel, &text, &renamed, "markers", work)
}

/// `text` with every old spelling of each of `names` replaced by its purlis one.
fn swap(text: &str, names: &[crate::names::Name]) -> String {
    names.iter().fold(text.to_owned(), |text, name| {
        name.reads
            .iter()
            .fold(text, |text, old| text.replace(old, name.write))
    })
}

/// `text` with each `charter:<skill>` reference as `purlis:<skill>`: the skill namespace
/// followed by a skill's name (`[a-z0-9][a-z0-9-]*`), not preceded by a word character, so
/// `mycharter:x` and `charter: a sentence` are left alone.
fn skill_refs(text: &str) -> String {
    static REF: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        let old = regex::escape(crate::names::SKILL_NAMESPACE.newest_old());
        regex::Regex::new(&format!(r"(^|[^A-Za-z0-9_.-]){old}([a-z0-9][a-z0-9-]*)"))
            .expect("the pattern compiles")
    });
    let namespace = crate::names::SKILL_NAMESPACE.write;
    REF.replace_all(text, |c: &regex::Captures<'_>| {
        format!("{}{namespace}{}", &c[1], &c[2])
    })
    .into_owned()
}

/// A tracked `workspaces/<name>/workspace.json` whose digest charter wrote under the old key,
/// with the key renamed where it stands. A manifest a hand edited is the operator's and is left
/// alone ([`crate::manifest::Ownership`]).
fn workspace_key(root: &Path, rel: &str, work: &mut Work) -> Result<(), String> {
    use crate::manifest::{Ownership, ownership};
    let text = read(root, rel)?;
    if ownership(Some(&text)) != Ownership::Charter {
        return Ok(());
    }
    let old = format!("\"{}\"", crate::names::GENERATED_KEY.newest_old());
    let new = format!("\"{}\"", crate::names::GENERATED_KEY.write);
    if text.matches(&old).count() != 1 || text.contains(&new) {
        return Ok(());
    }
    let renamed = text.replacen(&old, &new, 1);
    // Renamed as the key it was, and nothing else: the digest still matches the body.
    if ownership(Some(&renamed)) != Ownership::Charter {
        return Ok(());
    }
    write_if_changed(root, rel, &text, &renamed, "its digest key", work)
}

fn is_workspace_manifest(path: &str) -> bool {
    let parts: Vec<&str> = path.split('/').collect();
    matches!(parts.as_slice(), ["workspaces", _, "workspace.json"])
}

fn is_agent(path: &str) -> bool {
    let parts: Vec<&str> = path.split('/').collect();
    matches!(parts.as_slice(), [".claude", "agents", file] if file.ends_with(".md"))
}

/// A persona's definition: `personas/<name>/persona.md`, or the flat `personas/<name>.md`.
fn is_persona(path: &str) -> bool {
    let parts: Vec<&str> = path.split('/').collect();
    match parts.as_slice() {
        ["personas", _, "persona.md"] => true,
        ["personas", file] => file.ends_with(".md") && *file != "README.md",
        _ => false,
    }
}

fn read(root: &Path, rel: &str) -> Result<String, String> {
    std::fs::read_to_string(root.join(rel)).map_err(|e| format!("could not read {rel} ({e})"))
}

fn write(root: &Path, rel: &str, text: &str) -> Result<(), String> {
    crate::rewrite::replace(
        root,
        &root.join(rel),
        text.as_bytes(),
        crate::rewrite::Mode::Kept,
    )
    .map_err(|e| format!("could not write {rel} ({e})"))
}

fn write_if_changed(
    root: &Path,
    rel: &str,
    was: &str,
    now: &str,
    what: &str,
    work: &mut Work,
) -> Result<(), String> {
    if was == now {
        return Ok(());
    }
    write(root, rel, now)?;
    work.touch(rel);
    work.said
        .push(format!("✓ {rel}: {what} under purlis's name"));
    Ok(())
}

/// Stage exactly the touched paths and commit them; charter's git runner runs no hooks.
/// Answers the short sha, or `None` when the commit was made and git could not name it. `Err`
/// only when no commit was made.
fn commit(root: &Path, work: &Work) -> Result<Option<String>, String> {
    // A path `git mv` took away is staged already, and is no pathspec `add` can match.
    let mut args = vec!["add", "--"];
    args.extend(
        work.touched
            .iter()
            .filter(|p| root.join(p).exists())
            .map(String::as_str),
    );
    git(root, &args).map_err(|why| format!("could not stage the rename ({why})"))?;
    let body = format!(
        "{SUBJECT}\n\nMade by `purlis doctor --fix rename-plane`. The project now requires the \
         `{}` feature, so a build without it opens it read-only.\n",
        crate::names::PURLIS_NAMES_FEATURE
    );
    git(root, &["commit", "-q", "-m", &body])
        .map_err(|why| format!("git did not make the commit ({why})"))?;
    Ok(git(root, &["rev-parse", "--short", "HEAD"])
        .ok()
        .map(|sha| sha.trim().to_owned()))
}

/// `git <args>` in `root`: its stdout, or what it said when it failed.
fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    let run = crate::worktree::git::run(root, args, crate::worktree::git::NETWORK)
        .map_err(|e| e.to_string())?;
    if run.ok() {
        Ok(run.out)
    } else if run.code.is_none() {
        Err("git did not answer in time".to_owned())
    } else {
        let said = run.err.trim();
        Err(if said.is_empty() {
            format!("git exited {}", run.code.unwrap_or(-1))
        } else {
            crate::shown::short(said)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_skill_reference_is_renamed_and_prose_is_not() {
        assert_eq!(
            skill_refs("Use `charter:handoff`, then charter:secrets.\n- charter:persona"),
            "Use `purlis:handoff`, then purlis:secrets.\n- purlis:persona"
        );
        for kept in [
            "charter: a sentence",
            "mycharter:x",
            "Charter:handoff",
            "a.charter:x",
        ] {
            assert_eq!(skill_refs(kept), kept);
        }
    }

    #[test]
    fn the_manifest_gains_the_feature_and_schema_in_its_own_layout() {
        let marked = with_feature("# mine\nname = \"x\"\n\n[[forge]]\nkind = \"github\"\n")
            .unwrap()
            .unwrap();
        let doc: toml::Table = marked.parse().unwrap();
        assert_eq!(doc["schema"].as_integer(), Some(2));
        assert_eq!(
            doc["requires"][0]["feature"].as_str(),
            Some(crate::names::PURLIS_NAMES_FEATURE)
        );
        assert!(marked.starts_with("# mine\nname = \"x\"\n"), "{marked}");
        assert!(
            marked.contains("[[forge]]\nkind = \"github\"\n"),
            "{marked}"
        );
        assert_eq!(
            with_feature(&marked).unwrap(),
            None,
            "a second run adds nothing"
        );

        let other = "schema = 2\nrequires = [{ feature = \"x\" }]\n";
        let both = with_feature(other).unwrap().unwrap();
        let doc: toml::Table = both.parse().unwrap();
        assert_eq!(doc["requires"].as_array().unwrap().len(), 2, "{both}");
    }

    #[test]
    fn only_a_whole_marker_line_is_renamed() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let old = crate::names::LIVE_BEGIN.reads[0];
        std::fs::write(
            root.join(".gitignore"),
            format!("x\n{old}\n# {old}\n{}\r\n", crate::names::LIVE_END.reads[0]),
        )
        .unwrap();
        let mut work = Work::default();
        markers(
            root,
            ".gitignore",
            &[crate::names::LIVE_BEGIN, crate::names::LIVE_END],
            &mut work,
        )
        .unwrap();
        assert_eq!(
            std::fs::read_to_string(root.join(".gitignore")).unwrap(),
            format!(
                "x\n{}\n# {old}\n{}\r\n",
                crate::names::LIVE_BEGIN.write,
                crate::names::LIVE_END.write
            )
        );
        assert_eq!(work.touched, [".gitignore"]);
    }
}
