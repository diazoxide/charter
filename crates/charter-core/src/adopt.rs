//! `charter update`, minus the install, and `charter version` — what charter this is.
//!
//! `charter/commands_update.py` converges three things that all get called "updating charter",
//! and they have three different blast radii: the **CLI**, one machine-global install shared by
//! every plane on the machine; the **harness artifact**, per project; and the **pin** in
//! `charter.toml`, shared with every teammate once it is pushed. Every one of them moves a
//! *Python package*: `uv tool install charter-cp==X`, a version PyPI publishes, a pin naming
//! that version. None of them describes a Rust binary shipped inside a signed app, which moves
//! as the app moves — through Tauri's updater (ADR 0042) — and cannot be moved by this command
//! at all. So `update` says which half it cannot do, which channel the app updates from, and
//! where to read what a version brought: `charter news`, the app's own CHANGELOG.md (#352).
//!
//! **That question — what a version means for a binary that is not a Python package — is
//! answered here too, and it is why `charter version` shares this file.** ADR 0045 settles it,
//! amending ADR 0030: this charter's version is the app's, a plane's pin names a version of the
//! app, and a pin on the Python charter's line is named as that rather than called drift. The
//! two commands live together because they turn on one fact — [`THE_APP_MOVES_IT`] — and a
//! fact stated twice is a fact that drifts.
//!
//! **A charter that tells an operator to adopt something it cannot install is worse than one
//! that says which half it does.** So the refusal is the first line of the output.
//!
//! **No update baseline.** The Python charter stamped the version a plane updated FROM and
//! reported the news range from it. Nothing in this app writes one, and `charter news` no
//! longer reports a range, so nothing here reads one either (#352).

use std::path::Path;

use crate::news::Report;
use crate::scaffold::Say;

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

/// Where to read what a version brought, said in the same breath, so the refusal is not the
/// whole message.
const WHAT_IT_BROUGHT: &str = "  what each version of the app brought:  charter news";

/// `--to <version>`: a version to install, which is the one thing this command cannot do.
const NO_TARGET: &str = "--to names a published version to install, and this command installs nothing — the \
     version was ignored. The app installs its own updates, from the channel `charter update \
     --channel` picks.";

/// `--bump`: moving the pin to what was installed, when nothing was.
const NO_BUMP: &str = "--bump moves this plane's `[charter] version` pin to the version it installs, and this \
     command installs nothing, so the pin was left alone. What this charter is, and what the \
     plane pins: charter version";

/// What `charter update` was asked for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UpdateArgs {
    /// `--to <version>`, as typed.
    pub to: String,
    /// `--bump`.
    pub bump: bool,
}

/// `charter update`: say what this command cannot do, and where to read what a version brought.
pub fn update_report(args: &UpdateArgs) -> Report {
    let mut report = Report::new_public();
    report.said.push(Say::Warn(not_the_installer()));
    report.said.push(Say::Info(WHAT_IT_BROUGHT.to_owned()));
    if !args.to.is_empty() {
        report.said.push(Say::Warn(NO_TARGET.to_owned()));
    }
    if args.bump {
        report.said.push(Say::Warn(NO_BUMP.to_owned()));
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

/// The last release of the Python charter — `charter-cp` on PyPI (ADR 0045).
///
/// The boundary a pin is read against: a pin at or below it names the Python line, not this
/// app. A constant, because a boundary that moved without anybody deciding it would change what
/// every plane's pin means.
pub const PYTHON_LINE_LAST: &str = "0.62.1";

/// This charter's version: the app's, which every crate in the workspace shares (ADR 0045).
///
/// The one number `charter version` prints for "which charter is this", and the one a plane's
/// `[charter] version` pin is compared with.
pub fn app_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// What a plane's `[charter] version` pin says against this charter — ADR 0045, and the one
/// comparison every surface asks (`charter version`, the status line's alert row, the window's
/// pin dialog). ADR 0030's rule, kept: no surface compares a pin its own way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PinVerdict {
    /// The plane pins nothing.
    Unpinned,
    /// The pin is this charter's version.
    Met(String),
    /// The pin names a release of the Python charter (`charter-cp`) — an older charter line,
    /// not a version of this app. Not drift: there is nothing here to compare it with.
    PythonLine(String),
    /// The pin names a version of this app that this charter is not, or is not a version.
    Drift(String),
}

impl PinVerdict {
    /// `charter version`'s exit status for this verdict: 1 on drift, 0 otherwise.
    pub fn code(&self) -> u8 {
        u8::from(matches!(self, Self::Drift(_)))
    }
}

/// The verdict for `pinned` — a pin as [`locked_version`] reads it.
///
/// **How a Python pin is told from an app pin.** Every pin a plane carries today was written by
/// the Python charter (`charter version bump`); this charter writes none. The Python line ended
/// at [`PYTHON_LINE_LAST`], so a pin that is a version, is not this app's own, and is no newer
/// than that release names the Python line. Anything newer, or anything that is not a version at
/// all, is a pin on this app's line that this charter does not meet.
///
/// **The cost, stated rather than hidden** (ADR 0045): while the app's own version is at or
/// below 0.62.1 the two lines share numbers, so a pin written by hand for this app below that
/// release reads as the Python line and is never called drift. The ambiguity ends when the app's
/// version passes 0.62.1, or when this charter starts writing pins of its own.
pub fn pin_verdict(pinned: Option<&str>) -> PinVerdict {
    let Some(pin) = pinned else {
        return PinVerdict::Unpinned;
    };
    let pin = pin.to_owned();
    if pin == app_version() {
        return PinVerdict::Met(pin);
    }
    let key = crate::version::version_key(&pin);
    if key.is_version() && key <= crate::version::version_key(PYTHON_LINE_LAST) {
        return PinVerdict::PythonLine(pin);
    }
    PinVerdict::Drift(pin)
}

/// `charter version`: which charter this is, what the plane asks for, and whether they agree.
///
/// **ADR 0045, amending ADR 0030.** This charter's version is the app's — [`app_version`] — and
/// a plane's `[charter] version` means a version of the app. ADR 0030 printed the newest release
/// the Python news corpus names as "the charter this brought", with the app's own number second
/// as the build; that corpus was never this app's, and has since been removed (#352).
///
/// * `charter` — [`app_version`].
/// * `pinned` — `[charter] version`, verbatim.
///
/// **The verdict does not reuse the Python charter's sentences** (ADR 0030's reason stands):
/// *in sync with the lock* would claim a parity with a release this app is not. The exit status
/// is still the one scripts read: 0 with no pin, 0 when the pin is met, 0 for a pin on the
/// Python charter's line — it is not drift — and 1 on drift.
pub fn version_report(root: Option<&Path>) -> Report {
    let mut report = Report::new_public();
    let version = app_version();
    let pinned = root.and_then(locked_version);

    report.out.push_str(&format!("  charter    {version}\n"));
    report.out.push_str(&format!(
        "  pinned     {}\n\n",
        pinned.as_deref().unwrap_or(NO_PIN)
    ));

    let verdict = pin_verdict(pinned.as_deref());
    report.code = verdict.code();
    match verdict {
        PinVerdict::Unpinned => {
            report.said.push(Say::Info(
                "this control plane pins no charter version, so there is nothing to conform \
                 to."
                .to_owned(),
            ));
            report.said.push(Say::Info(format!("  {THE_APP_MOVES_IT}")));
        }
        PinVerdict::Met(pin) => {
            report.said.push(Say::Ok(format!(
                "this charter is {pin}, which is what this control plane pins."
            )));
        }
        PinVerdict::PythonLine(pin) => {
            report.said.push(Say::Info(format!(
                "this control plane pins {pin}, a release of the Python charter (charter-cp): an \
                 older charter line, not a version of this app, so it is not drift and there is \
                 nothing to compare."
            )));
            report.said.push(Say::Info(format!(
                "  to hold this plane to this app instead:  set `[charter] version = \
                 \"{version}\"` in charter.toml, or remove the pin."
            )));
        }
        PinVerdict::Drift(pin) => {
            report.said.push(Say::Warn(format!(
                "drift: this control plane pins {pin}, and this charter is {version}."
            )));
            report.said.push(Say::Info(format!("  {THE_APP_MOVES_IT}")));
            report.said.push(Say::Info(format!(
                "  conform the plane instead:  set `[charter] version = \"{version}\"` in \
                 charter.toml, or update the app to the version the plane pins."
            )));
        }
    }
    report
}

/// `charter version sync` and `charter version bump`: what neither of them can do here.
///
/// **They are answered rather than left to clap**, for M2.21's reason: a usage error is what a
/// script meets for a verb a plane's scripts may already call, and it says nothing about why.
/// Both install a charter — `sync` the one the plane pins, `bump` a newer one whose version it
/// then writes into `charter.toml` — and this binary cannot install itself: the app moves it.
///
/// Exit 1, because the operator asked for something that did not happen.
pub fn version_move_refusal(verb: &str) -> Report {
    let mut report = Report::new_public();
    report.said.push(Say::Err(format!(
        "charter version {verb} moves a pin by installing the charter it names, and this \
         charter cannot install itself."
    )));
    report.said.push(Say::Info(format!("  {THE_APP_MOVES_IT}")));
    report.said.push(Say::Info(
        "  what this charter is, and what this plane pins:  charter version".to_owned(),
    ));
    report.code = 1;
    report
}

// ------------------------------------------------------------------------------------------
// `charter update --channel` (ADR 0042)
// ------------------------------------------------------------------------------------------

/// Which stream the app takes its next version from, said beside [`THE_APP_MOVES_IT`].
///
/// `update` is the command whose subject is "moving charter", and its first line already says
/// the app is what does the moving. The channel is which stream it moves along, so this is the
/// natural place to say it — and until M6.4's status bar exists, the only place an operator can
/// change it at all.
fn channel_line(channel: crate::updates::Channel) -> String {
    let other = match channel {
        crate::updates::Channel::Stable => crate::updates::Channel::Dev,
        crate::updates::Channel::Dev => crate::updates::Channel::Stable,
    };
    format!(
        "  the app on this machine updates from the {} channel.  to move it:  charter update \
         --channel {}",
        channel.name(),
        other.name()
    )
}

/// `charter update` without `--channel`: the report as it was, plus the channel line.
///
/// `config_root` is `None` on a machine with no config home, which reads as the default — the
/// same answer the app gives, from the same store.
pub fn update_report_with_channel(config_root: Option<&Path>, args: &UpdateArgs) -> Report {
    let mut report = update_report(args);
    let channel = config_root
        .map(|at| crate::machine::read(at).store.channel)
        .unwrap_or_default();
    // Second, right under the refusal that names the app as the mover — not at the bottom,
    // where it would read as a footnote to a different subject.
    report.said.insert(1, Say::Info(channel_line(channel)));
    report
}

/// `charter update --channel <word>`: put this machine on a channel, or refuse the word.
///
/// Only the channel moves. The pointer to `charter news` is not said, because an operator
/// switching streams is not asking what a version brought, and a command that said two things
/// at once would bury the one sentence they typed it for.
///
/// **A word charter does not know exits 1 and changes nothing.** The exact reader
/// ([`crate::updates::Channel::named`]) is the rule; `Dev` or `nightly` is a typo, and guessing
/// what it meant would be guessing toward the less safe channel.
pub fn set_channel_report(config_root: Option<&Path>, word: &str) -> Report {
    let mut report = Report::new_public();
    let Some(channel) = crate::updates::Channel::named(word) else {
        report.said.push(Say::Err(format!(
            "{word:?} is not an update channel. There are two: stable and dev."
        )));
        report.code = 1;
        return report;
    };
    let Some(config_root) = config_root else {
        report.said.push(Say::Err(
            "this machine has no config home, so charter has nowhere to keep the channel."
                .to_owned(),
        ));
        report.code = 1;
        return report;
    };
    match crate::machine::update(config_root, |store| store.channel = channel) {
        Ok(_) => {
            report.said.push(Say::Ok(format!(
                "the app on this machine now updates from the {} channel.",
                channel.name()
            )));
            report.said.push(Say::Info(format!(
                "  it reads {} at its next check; nothing is installed until you say so in the \
                 app.",
                channel.endpoint()
            )));
        }
        Err(why) => {
            report.said.push(Say::Err(format!(
                "the channel could not be recorded: {why}"
            )));
            report.code = 1;
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_charters_version_is_the_workspaces_one_as_numbers() {
        // The one number `charter version` prints and a pin is compared with: the app's, which
        // every crate in the workspace shares — never a placeholder, and always dotted numbers.
        assert_eq!(app_version(), env!("CARGO_PKG_VERSION"));
        let parts: Vec<&str> = app_version().split('.').collect();
        assert_eq!(parts.len(), 3, "{}", app_version());
        assert!(
            parts
                .iter()
                .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit())),
            "{}",
            app_version()
        );
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
    fn it_says_which_half_it_does_and_where_to_read_what_a_version_brought() {
        let report = update_report(&UpdateArgs::default());
        assert!(matches!(report.said.first(), Some(Say::Warn(_))));
        assert!(said(&report).contains("does not install anything here"));
        assert!(said(&report).contains("charter news"), "{}", said(&report));
        assert!(report.out.is_empty(), "{}", report.out);
        assert_eq!(report.code, 0);
    }

    #[test]
    fn a_python_update_baseline_is_not_read_and_not_mentioned() {
        // #352: the range view is retired, so an update baseline the Python charter left on a
        // plane is history like its corpus, and no sentence here names one.
        let report = update_report(&UpdateArgs::default());
        assert!(!said(&report).contains("baseline"), "{}", said(&report));
        assert!(!said(&report).contains("--pending"), "{}", said(&report));
    }

    #[test]
    fn the_two_flags_that_install_a_charter_are_refused_by_name() {
        let args = UpdateArgs {
            to: "0.63.0".to_owned(),
            bump: true,
        };
        let text = said(&update_report(&args));
        assert!(text.contains("--to names a published version"));
        assert!(text.contains("--bump moves this plane's `[charter] version` pin"));
    }

    // `charter version` (M2.12, ADR 0030 as amended by ADR 0045)

    /// A plane whose manifest is `manifest`.
    fn pinned(manifest: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(crate::plane::MANIFEST), manifest).unwrap();
        dir
    }

    #[test]
    fn the_rows_name_the_apps_version_and_the_pin_and_nothing_from_the_news_corpus() {
        // ADR 0045: one number for "which charter is this", and it is the app's. The frozen
        // corpus reaches 0.62.1, and printing that here is the answer this replaced.
        let dir = pinned("");
        let report = version_report(Some(dir.path()));
        assert_eq!(
            report.out,
            format!("  charter    {}\n  pinned     {NO_PIN}\n\n", app_version())
        );
        assert!(
            !report.out.contains(PYTHON_LINE_LAST),
            "the corpus's version leaked into the rows: {}",
            report.out
        );
        assert_eq!(report.code, 0, "pinning nothing is not drift");
    }

    #[test]
    fn the_python_lines_last_release_is_not_the_apps_version() {
        assert!(
            crate::version::version_key(app_version())
                != crate::version::version_key(PYTHON_LINE_LAST),
            "the app's version and the Python line's last are the same number; the verdict \
             for that pin would be `Met` and the tests below would say less than they claim"
        );
    }

    #[test]
    fn the_apps_own_version_is_met_and_says_so_without_claiming_a_python_parity() {
        let met = pinned(&format!("[charter]\nversion = \"{}\"\n", app_version()));
        let report = version_report(Some(met.path()));
        assert_eq!(report.code, 0, "{}", said(&report));
        assert!(
            said(&report).contains(&format!("this charter is {}, which is what", app_version())),
            "{}",
            said(&report)
        );
        assert!(!said(&report).contains("in sync with the lock"));
    }

    #[test]
    fn a_pin_on_the_python_line_is_named_as_that_and_is_not_drift() {
        // The plane every Python charter left behind: pinned to a charter-cp release. Opening
        // it with the app must not turn it red, and must not tell the operator to run Python.
        for pin in [PYTHON_LINE_LAST, "0.44.0", "0.2.0"] {
            if pin == app_version() {
                continue;
            }
            let dir = pinned(&format!("[charter]\nversion = \"{pin}\"\n"));
            let report = version_report(Some(dir.path()));
            let text = said(&report);
            assert_eq!(report.code, 0, "{pin}: {text}");
            assert!(
                text.contains(&format!(
                    "this control plane pins {pin}, a release of the Python charter"
                )),
                "{text}"
            );
            assert!(text.contains("not drift"), "{text}");
            assert!(!text.contains("drift:"), "{text}");
            assert!(
                text.contains(&format!("[charter] version = \"{}\"", app_version())),
                "the way to hold the plane to this app is named: {text}"
            );
            assert!(
                !text.contains("uv tool") && !text.contains("run the charter-cp"),
                "{text}"
            );
            assert!(report.out.contains(&format!("  pinned     {pin}\n")));
        }
    }

    #[test]
    fn a_pin_past_the_python_line_that_this_app_is_not_is_drift_and_exits_one() {
        for pin in ["0.62.2", "0.63.0", "1.0.0", "not-a-version"] {
            let dir = pinned(&format!("[charter]\nversion = \"{pin}\"\n"));
            let report = version_report(Some(dir.path()));
            let text = said(&report);
            assert_eq!(report.code, 1, "{pin}: drift is exit 1");
            assert!(
                text.contains(&format!(
                    "drift: this control plane pins {pin}, and this charter is {}.",
                    app_version()
                )),
                "{text}"
            );
            assert!(!text.contains("charter-cp"), "{text}");
        }
    }

    #[test]
    fn every_surface_reads_one_verdict() {
        assert_eq!(pin_verdict(None), PinVerdict::Unpinned);
        assert_eq!(
            pin_verdict(Some(app_version())),
            PinVerdict::Met(app_version().to_owned())
        );
        assert_eq!(
            pin_verdict(Some("0.50.0")),
            PinVerdict::PythonLine("0.50.0".to_owned())
        );
        assert_eq!(
            pin_verdict(Some("9.0.0")),
            PinVerdict::Drift("9.0.0".to_owned())
        );
        assert_eq!(PinVerdict::Unpinned.code(), 0);
        assert_eq!(PinVerdict::PythonLine("0.50.0".to_owned()).code(), 0);
        assert_eq!(PinVerdict::Drift("9.0.0".to_owned()).code(), 1);
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
                text.contains(&format!("charter version {verb} moves a pin")),
                "{text}"
            );
            assert!(text.contains(THE_APP_MOVES_IT), "{text}");
            assert!(!text.contains("charter-cp"), "{text}");
        }
    }

    #[test]
    fn one_sentence_says_what_moves_this_charter_and_both_commands_use_it() {
        // The constant exists so `update` and `version` cannot drift into two accounts of one
        // artifact. This is what holds that: both outputs carry the same bytes.
        let dir = tempfile::tempdir().unwrap();
        let update = said(&update_report(&UpdateArgs::default()));
        let version = said(&version_report(Some(dir.path())));
        assert!(update.contains(THE_APP_MOVES_IT), "{update}");
        assert!(version.contains(THE_APP_MOVES_IT), "{version}");
    }

    // `charter update --channel` (ADR 0042)

    #[test]
    fn update_says_which_channel_the_app_takes_charter_from_right_under_the_refusal() {
        let machine = tempfile::tempdir().unwrap();
        let report = update_report_with_channel(Some(machine.path()), &UpdateArgs::default());
        let Some(Say::Info(line)) = report.said.get(1) else {
            panic!("{:?}", report.said);
        };
        assert!(line.contains("from the stable channel"), "{line}");
        assert!(line.contains("charter update --channel dev"), "{line}");
    }

    #[test]
    fn a_channel_set_from_the_terminal_is_the_one_the_app_reads() {
        let machine = tempfile::tempdir().unwrap();
        let report = set_channel_report(Some(machine.path()), "dev");
        assert_eq!(report.code, 0, "{}", said(&report));
        assert_eq!(
            crate::machine::read(machine.path()).store.channel,
            crate::updates::Channel::Dev
        );
        let back = update_report_with_channel(Some(machine.path()), &UpdateArgs::default());
        assert!(
            said(&back).contains("from the dev channel"),
            "{}",
            said(&back)
        );
    }

    #[test]
    fn a_word_that_is_not_a_channel_exits_one_and_moves_nothing() {
        let machine = tempfile::tempdir().unwrap();
        set_channel_report(Some(machine.path()), "dev");
        for word in ["Dev", "STABLE", "nightly", "", "dev "] {
            let report = set_channel_report(Some(machine.path()), word);
            assert_eq!(report.code, 1, "{word:?} was accepted");
            assert_eq!(
                crate::machine::read(machine.path()).store.channel,
                crate::updates::Channel::Dev,
                "{word:?} moved the channel"
            );
        }
    }
}
