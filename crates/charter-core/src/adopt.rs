//! `charter update`, minus the install: what the versions this plane skipped brought, and what
//! it has not taken up.
//!
//! `charter/commands_update.py` converges three things that all get called "updating charter",
//! and they have three different blast radii: the **CLI**, one machine-global install shared by
//! every plane on the machine; the **harness artifact**, per project; and the **pin** in
//! `charter.toml`, shared with every teammate once it is pushed. After all three it runs one
//! more phase, `_handoff`, which starts the newly installed binary and asks it `charter news
//! --since <baseline>`.
//!
//! **Only that last phase is ported here, and the reason is not scope.** Every one of the other
//! three moves a *Python package*: `uv tool install charter-cp==X`, a version PyPI publishes, a
//! pin naming that version. None of them describes a Rust binary shipped inside a signed app,
//! which moves as the app moves — through Tauri's updater, in M4 — and cannot be moved by this
//! command at all. The PR for this milestone puts the question of what `charter update` should
//! MEAN for such a binary beside M2.12's, which is the same question asked of `charter
//! version`; until it is answered, this command does the half it can do and says plainly which
//! half that is.
//!
//! **A charter that tells an operator to adopt something it cannot install is worse than one
//! that says which half it does.** So the refusal is the first line of the output, before the
//! news, rather than a footnote under it.
//!
//! **It reads the baseline and never writes one.** `_stamp_baseline` records the version a
//! plane updated FROM, before anything moves, so an interrupted update still knows where it
//! started. Nothing moves here, so there is nothing to stamp — and stamping anyway would
//! overwrite the mark a real update left and shorten the range the NEXT one reports.

use std::path::{Path, PathBuf};

use crate::news::{self, Dispatch, Report};
use crate::scaffold::Say;

/// Where the version a plane last updated FROM is recorded.
///
/// Per developer and gitignored — which version this laptop came from is not a fact about the
/// plane, and ADR 0011 keeps the committed record to what git cannot know.
///
/// **`<root>/.charter`, and charter also honours `$CHARTER_HOME`.** Every reader of the state
/// directory in this crate hard-codes `.charter` (`profiletrust`, `wiring`, `hookwire`), so
/// this one does too rather than becoming the single place in charter-app that reads a
/// different directory from its neighbours. On a plane whose operator has moved their state
/// home, charter writes the baseline somewhere this does not look and the range comes back
/// empty — reported in the PR as a plane-wide question rather than answered differently here.
pub fn baseline_file(root: &Path) -> PathBuf {
    root.join(".charter").join("cache").join("update-baseline")
}

/// The version this plane last updated FROM, or `None`.
///
/// Gated on the FILE, not on the directory above it: a link at `cache/update-baseline` pointing
/// out of the plane makes charter read somebody else's file and report a range from it, and a
/// gate on `.charter` would not see that. (`contain::readable`'s data roots do not cover
/// `.charter` at all — see `profiletrust::record_launched`, which spells the same rule out for
/// the same directory.)
pub fn read_baseline(root: &Path) -> Option<String> {
    let path = baseline_file(root);
    if !crate::contain::within_plane(root, &path) {
        return None;
    }
    let text = std::fs::read_to_string(&path).ok()?;
    let trimmed = crate::memstore::py_strip(&text);
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

/// What `charter update` says about the half it does not do.
///
/// One sentence, and it names the mechanism rather than apologising: an operator who reads
/// "charter update did nothing" and an operator who reads this take different next steps.
const NOT_THE_INSTALLER: &str = "charter update does not install anything here. This charter is a binary inside the app, \
     and the app is what moves it — not a package manager this command can call.";

/// And what it DOES do, said in the same breath, so the refusal is not the whole message.
const THE_HALF_IT_DOES: &str = "  the other half is this command's: what the versions this plane skipped brought, and \
     what it has not taken up.";

/// `--to <version>`: a published Python package version, which is the one thing there is
/// nothing here to install.
const NO_TARGET: &str = "--to names a published version to install, and this command installs nothing — the \
     version was ignored. What this charter ships is the news it was built with: charter news \
     --for <version>.";

/// `--bump`: the pin is a Python package pin.
const NO_BUMP: &str = "--bump moves this plane's `[charter] version` pin, which names a PUBLISHED charter-cp \
     release. This charter is not installed from one, so the pin was left alone.";

/// What `charter update` was asked for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UpdateArgs {
    /// `--to <version>`, as typed.
    pub to: String,
    /// `--bump`.
    pub bump: bool,
}

/// `charter update`: say what this command cannot do, then do the adoption half.
///
/// `root` is the plane, or `None` outside one — the same distinction `news` draws, and for the
/// same reason: "has this plane adopted it?" has no subject outside a control plane.
pub fn update_report(root: Option<&Path>, args: &UpdateArgs, d: &dyn Dispatch) -> Report {
    let mut report = Report::new_public();
    report.said.push(Say::Warn(NOT_THE_INSTALLER.to_owned()));
    report.said.push(Say::Info(THE_HALF_IT_DOES.to_owned()));
    if !args.to.is_empty() {
        report.said.push(Say::Warn(NO_TARGET.to_owned()));
    }
    if args.bump {
        report.said.push(Say::Warn(NO_BUMP.to_owned()));
    }

    let baseline = root.and_then(read_baseline);
    let has_plane = root.is_some();
    match baseline {
        // `_handoff`'s news phase, exactly: the range from where this plane last was, up to
        // what this build ships.
        Some(since) => {
            let news = news::range_report(&since, "", d, has_plane);
            report.absorb(news);
        }
        None => {
            report.said.push(Say::Info(format!(
                "no update baseline recorded on this plane, so there is no range to report — \
                 charter stamps one when it moves a plane, and nothing has moved this one \
                 through {}.",
                baseline_file(root.unwrap_or(Path::new("."))).display()
            )));
            report.said.push(Say::Info(
                "  what this plane has not adopted:  charter news --pending".to_owned(),
            ));
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::news::CommandTree;

    struct NoCommands;
    impl Dispatch for NoCommands {
        fn tree(&self) -> &CommandTree {
            static TREE: std::sync::OnceLock<CommandTree> = std::sync::OnceLock::new();
            TREE.get_or_init(|| CommandTree::with("charter", vec![]))
        }
        fn run(&self, _tokens: &[String]) -> Option<i32> {
            unreachable!("nothing is probeable against an empty tree")
        }
    }

    fn said(report: &Report) -> String {
        report
            .said
            .iter()
            .map(|line| match line {
                Say::Info(t) | Say::Ok(t) | Say::Warn(t) | Say::Err(t) => t.clone(),
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn it_says_which_half_it_does_before_it_does_it() {
        let dir = tempfile::tempdir().unwrap();
        let report = update_report(Some(dir.path()), &UpdateArgs::default(), &NoCommands);
        assert!(matches!(report.said.first(), Some(Say::Warn(_))));
        assert!(said(&report).contains("does not install anything here"));
        assert_eq!(report.code, 0);
    }

    #[test]
    fn a_baseline_becomes_the_range_and_its_absence_is_said() {
        let dir = tempfile::tempdir().unwrap();
        let bare = update_report(Some(dir.path()), &UpdateArgs::default(), &NoCommands);
        assert!(said(&bare).contains("no update baseline recorded"));
        assert!(bare.out.is_empty());

        let file = baseline_file(dir.path());
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, "0.62.0\n").unwrap();
        let ranged = update_report(Some(dir.path()), &UpdateArgs::default(), &NoCommands);
        assert_eq!(read_baseline(dir.path()).as_deref(), Some("0.62.0"));
        // 0.62.1 is the newest entry the corpus ships, so the range is not empty.
        assert!(ranged.out.contains("0.62.1"), "{}", ranged.out);
    }

    #[cfg(unix)]
    #[test]
    fn a_baseline_linked_out_of_the_plane_is_not_read() {
        // The gate is on the FILE. A link at the exact path charter opens redirects the read,
        // and a gate one level up — on `.charter`, or on `cache/` — sees a directory that is
        // exactly where it should be. Six review rounds in this repository were about that one
        // level.
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let theirs = outside.path().join("someone-elses-baseline");
        std::fs::write(&theirs, "0.44.0\n").unwrap();
        let file = baseline_file(dir.path());
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&theirs, &file).unwrap();
        assert_eq!(
            read_baseline(dir.path()),
            None,
            "a link out of the plane was read"
        );
        // And the report does not quietly report a range from it either.
        let said = said(&update_report(
            Some(dir.path()),
            &UpdateArgs::default(),
            &NoCommands,
        ));
        assert!(said.contains("no update baseline recorded"), "{said}");
    }

    #[test]
    fn the_two_flags_that_move_a_python_package_are_refused_by_name() {
        let dir = tempfile::tempdir().unwrap();
        let args = UpdateArgs {
            to: "0.63.0".to_owned(),
            bump: true,
        };
        let text = said(&update_report(Some(dir.path()), &args, &NoCommands));
        assert!(text.contains("--to names a published version"));
        assert!(text.contains("--bump moves this plane's `[charter] version` pin"));
    }
}
