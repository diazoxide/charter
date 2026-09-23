//! The plane's FIRST clone: the repository `init` was pointed at, cloned into the workspace
//! the plane starts with.
//!
//! A port of `charter/commands.py:_clone_first_workspace`, widened by exactly one argument.
//! Python clones the repo the plane was scaffolded INTO — the plane root is the repo, which
//! is the only shape it has. [`into_first_workspace`] takes the source repository as a
//! parameter instead, so one set of steps serves both shapes charter-app has:
//!
//! - **`--clone-this-repo`**, where the source IS the plane root. Byte for byte Python's, and
//!   the recorded `init-*` scenarios hold it there (ADR 0046).
//! - **`--adopt <repo>`**, where the source is a repository somewhere else and the plane was
//!   made beside it. That is ADR 0035's default — *"`charter init` on an existing repo adopts
//!   that repo as the plane's first clone and makes the plane beside it"* — and it is the
//!   half charter-app#175 filed as missing.
//!
//! # Why the clone comes off the disk and not off the forge
//!
//! Python's reasoning is this module's too, and it is the reason `repocmd::clone` is NOT what
//! adopt calls. That command resolves an inventory record, builds an HTTPS URL from it and
//! goes to the network holding a forge credential. This runs during setup, before `doctor`
//! has checked that the forge CLI is even authed, and it must not be the step that stalls on
//! a passphrase or fails on a token nobody has minted yet. A local clone also carries commits
//! that were never pushed, which for the one person standing in their own project is the work
//! they were in the middle of.
//!
//! The clone's `origin` is then repointed at the SOURCE's own origin, rewritten to HTTPS by
//! that forge's rule (`planegit::origin_https`) because golden rule 0 is that git talks over
//! a token and never SSH. Left pointing at the source directory it would look right and fail
//! at the first push: git refuses a push to a non-bare repository's checked-out branch.
//!
//! # What it does to the source
//!
//! Nothing. It is read, and only read — `rev-parse`, `remote get-url`, and being the argument
//! to a `git clone`. The source is a working tree somebody is in the middle of using, and
//! under `--adopt` it is a repository the operator pointed at from a file dialog, where a
//! write they did not type is the whole subject of ADR 0035.

use std::path::Path;

use super::{Run, first_clone_name, planefile};
use crate::worktree::git;

/// Clone `source` into the plane's first workspace. Returns the exit status, and says
/// everything it did through `run`. Python's `_clone_first_workspace`.
///
/// **Additive, exactly like the rest of `init`.** A destination that already exists is left
/// as it is and reported, so a re-run never clones over work already sitting there.
pub(super) fn into_first_workspace(
    run: &mut Run,
    root: &Path,
    source: &Path,
    now: chrono::DateTime<chrono::Utc>,
) -> u8 {
    let name = first_clone_name(source);
    let ws = match planefile::load(root) {
        planefile::Read::Config(cfg) => planefile::default_workspace(&cfg),
        _ => "default".to_owned(),
    };
    let author = crate::wscmd::ensure::author();
    let ws_dir = match crate::wscmd::ensure::ensure(root, &ws, now, &author) {
        Ok(_) => root.join("workspaces").join(&ws),
        Err(why) => {
            run.err(why);
            return 1;
        }
    };
    // The exact path created is gated as itself, not as its parent: `name` comes off an
    // `origin` URL, which is a string somebody else chose. `repocmd::clone` gates its
    // destination the same way and for the same reason, so a first clone is held to the rule
    // every later clone is.
    let dest = match crate::repocmd::clone::destination(root, &ws, &ws_dir, &name) {
        Ok(dest) => dest,
        Err(why) => {
            run.err(format!("{name}: not cloned — {why}."));
            return 1;
        }
    };
    if dest.exists() {
        run.info(format!(
            "{name}: already cloned in '{ws}' — left exactly as it is."
        ));
        return 0;
    }

    run.info(format!("Cloning {name} into workspace '{ws}' …"));
    let src_arg = source.display().to_string();
    let dest_arg = dest.display().to_string();
    // `--` before the two paths: a source directory whose name begins with a dash is a path,
    // never an option. Python passes neither path after a separator; this is the one place
    // the port is stricter, and it cannot change what git does to any path that is not one.
    let cloned = git::run(
        root,
        &["clone", "--quiet", "--", &src_arg, &dest_arg],
        git::NETWORK,
    );
    let failed = match &cloned {
        Ok(done) if done.ok() => None,
        Ok(done) => Some(done.err.trim().to_owned()),
        Err(why) => Some(why.to_string()),
    };
    if let Some(said) = failed {
        run.err(format!(
            "could not clone {} into {} — nothing else was changed.\n{said}",
            source.display(),
            dest.display()
        ));
        return 1;
    }

    match crate::planegit::origin_https(source).or_else(|| {
        git::run(source, &["remote", "get-url", "origin"], git::READ)
            .ok()
            .filter(git::Run::ok)
            .map(|r| r.line().to_owned())
            .filter(|url| !url.is_empty())
    }) {
        Some(upstream) => {
            let _ = git::run(
                &dest,
                &["remote", "set-url", "origin", &upstream],
                git::READ,
            );
        }
        // Python's sentence says "the plane root", which is true of the only shape it has.
        // Under `--adopt` the plane root is not where the clone came from, and a warning that
        // names the wrong directory is one nobody can act on.
        None => {
            let left_at = if source == root {
                "the plane root".to_owned()
            } else {
                source.display().to_string()
            };
            run.warn(format!(
                "{name} has no origin of its own yet, so this clone's origin is {left_at} — \
                 give it a real remote before you push."
            ));
        }
    }
    // Golden rule 0, the same call `charter clone` makes per clone: one credential per forge,
    // over HTTPS, no signing. `gitpolicy` is ported in full (M2.5, #64) — the note in
    // `scaffold`'s header that said it was not is what charter-app#175 was filed on.
    crate::gitpolicy::apply(&dest, root);

    let rel = dest
        .strip_prefix(root)
        .unwrap_or(&dest)
        .display()
        .to_string();
    run.ok(format!("{name} → {rel}"));
    hint_docs(run, &dest, &name);
    run.info(format!(
        "  work there: cd {rel}   (being in that directory IS this workspace)"
    ));
    0
}

/// `  ↳ <name> ships its own <file>` for the first of the three a clone has. Python's
/// `_hint_repo_docs`, which `repocmd::clone` has its own copy of because it prints through a
/// `Sink` and this prints through a [`Run`].
fn hint_docs(run: &mut Run, dest: &Path, name: &str) {
    for file in ["CLAUDE.md", "AGENTS.md", "README.md"] {
        if dest.join(file).exists() {
            run.info(format!(
                "  ↳ {name} ships its own {file} — read it before working there."
            ));
            return;
        }
    }
}
