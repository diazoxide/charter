//! `charter update`, minus the install, and `charter version` — what charter this is.
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
//! command at all. So this command does the half it can do and says plainly which half that is.
//!
//! **That question — what a version means for a binary that is not a Python package — is
//! answered here too, and it is why `charter version` shares this file.** M2.12 settled it
//! (ADR 0030): the binary reports the charter release its news corpus comes up to, the build
//! carrying it, and the pin, and says so in the same sentence as `update` when the plane pins
//! something the corpus does not reach. The two commands live together because they turn on one
//! fact — [`THE_APP_MOVES_IT`] — and a fact stated twice is a fact that drifts.
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

/// The fact `charter update` and `charter version` both turn on, written once.
///
/// Two commands saying this in two sentences is two sentences to keep in step, and the day
/// they disagree an operator gets two different accounts of the same artifact. M2.12 is what
/// made it a constant rather than a paragraph in one command's output.
pub const THE_APP_MOVES_IT: &str = "This charter is a binary inside the app, and the app is what moves it — not a package \
     manager this command can call.";

/// What `charter update` says about the half it does not do.
///
/// One sentence, and it names the mechanism rather than apologising: an operator who reads
/// "charter update did nothing" and an operator who reads this take different next steps.
fn not_the_installer() -> String {
    format!("charter update does not install anything here. {THE_APP_MOVES_IT}")
}

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
    report.said.push(Say::Warn(not_the_installer()));
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

// ------------------------------------------------------------------------------------------
// `charter version` (M2.12)
// ------------------------------------------------------------------------------------------

/// What Python prints in the `locked` row when a plane pins nothing — its words, because this
/// row means the same thing in both implementations.
const NO_PIN: &str = "— (this control plane pins no version)";

/// `instance.locked_version`: `[charter] version` as the manifest holds it, or `None`.
///
/// The pin is reported **as written**, a value that is not a version included: refusing here
/// would fold "pinned something malformed" into "pinned nothing", and a plane that opted into
/// conformance would then behave exactly like one that never did.
///
/// **One divergence, and it is Python's crash.** `(cfg.get("charter") or {}).get("version")`
/// raises `AttributeError` when `charter` is a scalar, so `charter version` tracebacks on a
/// manifest `doctor`'s `version lock` row reports calmly. Answering `None` is this binary
/// declining to reproduce a traceback; nothing downstream can tell the two apart, because the
/// only caller is a row that then prints the no-pin line.
pub fn locked_version(root: &Path) -> Option<String> {
    let text = std::fs::read_to_string(root.join(crate::plane::MANIFEST)).ok()?;
    let doc: toml::Table = text.parse().ok()?;
    let pinned = doc.get("charter")?.as_table()?.get("version")?.as_str()?;
    let pinned = crate::memstore::py_strip(pinned);
    (!pinned.is_empty()).then(|| pinned.to_owned())
}

/// `charter version`: which charter this is, what the plane asks for, and whether they agree.
///
/// **This is a decision, not a port — ADR 0030, and it is the question `adopt`'s own module
/// note parks beside `charter update`'s.** Python's three rows are three facts about a PYTHON
/// PACKAGE: the `charter-cp` wheel installed on this machine, the release the plane pins, and
/// the newest release on PyPI. Two of the three have no subject here. This binary is not
/// installed from an index, so there is no *installed* wheel to name and no *latest* to
/// compare against — `charter update` already says why, in the same sentence this does
/// ([`THE_APP_MOVES_IT`]).
///
/// So the command answers the question the rows were for, with the numbers this artifact
/// actually has:
///
/// * `charter` — [`crate::news::shipped_version`], the release this build's news corpus comes
///   up to. That function's own note is why it stands in for `charter.__version__`: an entry
///   travels with the code that implements it, so the newest entry a binary ships is the
///   newest thing that binary brought. **It is the only number here on the same scale as the
///   pin**, which is what makes a comparison possible at all.
/// * `build` — `charter-app`'s own version, the artifact carrying it. Two numbers rather than
///   one because they move independently and an operator debugging a plane needs both: the
///   first says which charter this behaves like, the second says which build to re-download.
/// * `pinned` — `[charter] version`, verbatim.
///
/// **The verdict does not reuse charter's sentences, and that is deliberate.** Python says *in
/// sync with the lock* when the numbers match. A Rust charter saying that would claim parity
/// it does not have: M2 is still porting commands, so a binary whose corpus reaches 0.62.1 is
/// not everything 0.62.1 does. ADR 0013's rule — the absence of information is not evidence of
/// health — applies to charter's own claims about itself, so the line states the fact it can
/// substantiate (*this charter brought X, which is what the plane pins*) and no more.
///
/// **The exit status IS charter's**, and that is the part scripts read: 0 with no pin, 0 when
/// the pin is met, 1 on drift. A wrapper that branches on `charter version` behaves the same
/// against either implementation even though neither sentence matches — which is what the
/// differential scenarios compare, since the words legitimately differ.
pub fn version_report(root: Option<&Path>) -> Report {
    let mut report = Report::new_public();
    let brought = crate::news::shipped_version();
    let pinned = root.and_then(locked_version);

    report.out.push_str(&format!("  charter    {brought}\n"));
    report
        .out
        .push_str(&format!("  build      charter-app {}\n", build_version()));
    report.out.push_str(&format!(
        "  pinned     {}\n\n",
        pinned.as_deref().unwrap_or(NO_PIN)
    ));

    match pinned {
        None => {
            report.said.push(Say::Info(
                "this control plane pins no charter version, so there is nothing to conform \
                 to."
                .to_owned(),
            ));
            report.said.push(Say::Info(format!("  {THE_APP_MOVES_IT}")));
        }
        Some(pin) if pin == brought => {
            report.said.push(Say::Ok(format!(
                "this charter brought {pin}, which is what this control plane pins."
            )));
        }
        Some(pin) => {
            report.said.push(Say::Warn(format!(
                "drift: this control plane pins {pin}, and this charter brought {brought}."
            )));
            report.said.push(Say::Info(format!("  {THE_APP_MOVES_IT}")));
            report.said.push(Say::Info(
                "  conform the plane instead:  move `[charter] version` to the release this \
                 charter brought, or run the charter-cp the plane names."
                    .to_owned(),
            ));
            report.code = 1;
        }
    }
    report
}

/// This build's own version — `charter-app`'s, which every crate in the workspace shares.
fn build_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// `charter version sync` and `charter version bump`: what neither of them can do here.
///
/// **They are answered rather than left to clap**, for M2.21's reason: a usage error is what a
/// script meets for a verb the tool being replaced has, and it says nothing about why. Both
/// move a PUBLISHED `charter-cp` release — `sync` installs one over this machine's binary,
/// `bump` writes one into `charter.toml` after installing and verifying it — and this binary
/// is neither installed from an index nor able to verify a wheel it cannot run.
///
/// Exit 1, because the operator asked for something that did not happen.
pub fn version_move_refusal(verb: &str) -> Report {
    let mut report = Report::new_public();
    report.said.push(Say::Err(format!(
        "charter version {verb} moves a published charter-cp release, and this charter is not \
         one."
    )));
    report.said.push(Say::Info(format!("  {THE_APP_MOVES_IT}")));
    report.said.push(Say::Info(
        "  what this charter is, and what this plane pins:  charter version".to_owned(),
    ));
    report.code = 1;
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

    // `charter version` (M2.12, ADR 0030)

    /// A plane whose manifest is `manifest`.
    fn pinned(manifest: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(crate::plane::MANIFEST), manifest).unwrap();
        dir
    }

    #[test]
    fn the_rows_name_the_charter_this_build_brought_and_the_build_carrying_it() {
        // Two numbers, not one, and they are different numbers: the corpus reaches a charter
        // release and the artifact has a version of its own. A row that printed only the
        // second would answer the pin with a number that cannot be compared to it; a row that
        // printed only the first would hide which build to re-download.
        let dir = pinned("");
        let report = version_report(Some(dir.path()));
        let brought = crate::news::shipped_version();
        assert!(!brought.is_empty(), "the corpus names no released version");
        assert_ne!(
            brought,
            build_version(),
            "this test is vacuous if the two numbers are the same"
        );
        assert_eq!(
            report.out,
            format!(
                "  charter    {brought}\n  build      charter-app {}\n  pinned     {NO_PIN}\n\n",
                build_version()
            )
        );
        assert_eq!(report.code, 0, "pinning nothing is not drift");
    }

    #[test]
    fn a_pin_the_corpus_reaches_is_met_and_a_pin_it_does_not_is_drift() {
        let brought = crate::news::shipped_version();
        let met = pinned(&format!("[charter]\nversion = \"{brought}\"\n"));
        let report = version_report(Some(met.path()));
        assert_eq!(report.code, 0, "{}", said(&report));
        assert!(
            said(&report).contains(&format!("this charter brought {brought}, which is what")),
            "{}",
            said(&report)
        );
        // And NOT charter's own verdict: "in sync with the lock" claims a parity a partial
        // port does not have (ADR 0013, applied to charter's claims about itself).
        assert!(!said(&report).contains("in sync with the lock"));

        let adrift = pinned("[charter]\nversion = \"0.44.0\"\n");
        let report = version_report(Some(adrift.path()));
        assert_eq!(report.code, 1, "drift is exit 1, as it is in charter");
        assert!(said(&report).contains("drift: this control plane pins 0.44.0"));
        assert!(
            report.out.contains("  pinned     0.44.0\n"),
            "the row prints the pin as written: {}",
            report.out
        );
    }

    #[test]
    fn a_pin_is_reported_as_written_and_a_manifest_that_pins_nothing_reads_as_no_pin() {
        // `instance.locked_version`: as written, malformed included, because folding a bad
        // pin into "no pin" makes a plane that opted into conformance behave like one that
        // never did.
        let junk = pinned("[charter]\nversion = \"  not-a-version  \"\n");
        assert_eq!(
            locked_version(junk.path()).as_deref(),
            Some("not-a-version"),
            "the value is stripped, not judged"
        );
        for manifest in [
            "",
            "schema = 1\n",
            "[charter]\n",
            "[charter]\nversion = \"\"\n",
            "[charter]\nversion = \"   \"\n",
            // `isinstance(v, str)` in Python: a number is not a pin.
            "[charter]\nversion = 62\n",
            // `charter` is not a table: Python raises here, and this answers no-pin rather
            // than reproducing a traceback.
            "charter = \"nope\"\n",
            "this is not toml at all [[[\n",
        ] {
            let dir = pinned(manifest);
            assert_eq!(
                locked_version(dir.path()),
                None,
                "{manifest:?} read as a pin"
            );
        }
        assert_eq!(locked_version(Path::new("/nonexistent/plane")), None);
    }

    #[test]
    fn outside_a_plane_there_is_no_pin_to_report_and_nothing_is_read() {
        let report = version_report(None);
        assert!(report.out.contains(NO_PIN), "{}", report.out);
        assert_eq!(report.code, 0);
    }

    #[test]
    fn moving_a_published_release_is_refused_by_name_and_exits_one() {
        for verb in ["sync", "bump"] {
            let report = version_move_refusal(verb);
            assert_eq!(report.code, 1, "a refusal is not a success");
            assert!(report.out.is_empty(), "nothing goes to stdout");
            let text = said(&report);
            assert!(
                text.contains(&format!("charter version {verb} moves a published")),
                "{text}"
            );
            assert!(text.contains(THE_APP_MOVES_IT), "{text}");
        }
    }

    #[test]
    fn one_sentence_says_what_moves_this_charter_and_both_commands_use_it() {
        // The constant exists so `update` and `version` cannot drift into two accounts of one
        // artifact. This is what holds that: both outputs carry the same bytes.
        let dir = tempfile::tempdir().unwrap();
        let update = said(&update_report(
            Some(dir.path()),
            &UpdateArgs::default(),
            &NoCommands,
        ));
        let version = said(&version_report(Some(dir.path())));
        assert!(update.contains(THE_APP_MOVES_IT), "{update}");
        assert!(version.contains(THE_APP_MOVES_IT), "{version}");
    }
}
