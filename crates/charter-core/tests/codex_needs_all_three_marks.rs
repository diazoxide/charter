//! Codex: three marks in `$CODEX_HOME/config.toml`, and it needs all three.
//!
//! The plugin declares the hooks, the policy line is the only thing that can tell a Codex
//! shell which harness it is, and **a hook Codex has not trusted is inert** — so a plugin
//! nobody approved is installed and does nothing, which reads exactly like wired to anything
//! that stops at the plugin table.
//!
//! The trust rule is the measured one and it is deliberately weak: `trusted_hash` cannot be
//! recomputed from the plugin's own `hooks.json`, and Codex writes an entry per hook LAZILY,
//! as each first fires. Measured on this machine 2026-09-18, a genuinely wired home held 12
//! of the plugin's 18 keys — so "an entry for every hook key" would call it unwired. What
//! charter can honestly say is that charter's GUARD hook was approved there at least once.
//!
//! The shapes here are the real ones, read off a wired `~/.codex` on 2026-09-18:
//! `plugins."charter@charter" = { enabled = true }`,
//! `shell_environment_policy.set = { CHARTER_HARNESS = "codex" }`, and trust keys spelled
//! `charter@charter:hooks/hooks.json:pre_tool_use:0:0`. In that home the installed 0.42.0
//! plugin placed the guard at exactly one key, and that key was trusted.

use std::fs;
use std::path::{Path, PathBuf};

use charter_core::profiles::{self, Profile};
use charter_core::wiring::{self, State};

/// A plane with a Codex profile pointing at a `CODEX_HOME` this test builds.
struct Home {
    dir: tempfile::TempDir,
}

impl Home {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("charter.toml"), "").unwrap();
        fs::create_dir_all(dir.path().join("codex")).unwrap();
        let home = Self { dir };
        fs::write(
            home.root().join(profiles::LOCAL_FILE),
            format!(
                "[harness.work]\nkind = \"codex\"\ncommand = [\"codex\"]\n\
                 env = {{ CODEX_HOME = {:?} }}\n",
                home.codex().display().to_string()
            ),
        )
        .unwrap();
        home
    }

    fn root(&self) -> &Path {
        self.dir.path()
    }

    fn codex(&self) -> PathBuf {
        self.root().join("codex")
    }

    fn config(&self, toml: &str) -> &Self {
        fs::write(self.codex().join("config.toml"), toml).unwrap();
        self
    }

    /// An installed copy of the plugin at `version`, whose `hooks.json` places the guard at
    /// the group and hook given.
    fn plugin(&self, version: &str, guard_at: Option<(usize, usize)>) -> &Self {
        let dir = self
            .codex()
            .join("plugins/cache/charter/charter")
            .join(version)
            .join("hooks");
        fs::create_dir_all(&dir).unwrap();
        let hooks = match guard_at {
            Some((group, hook)) => {
                let mut groups = vec![String::new(); group + 1];
                for (g, slot) in groups.iter_mut().enumerate() {
                    let mut entries =
                        vec![r#"{"command":"charter hook posttooluse"}"#.to_owned(); hook + 1];
                    if g == group {
                        entries[hook] = r#"{"command":"charter hook pretooluse"}"#.to_owned();
                    }
                    *slot = format!(r#"{{"hooks":[{}]}}"#, entries.join(","));
                }
                format!(r#"{{"hooks":{{"PreToolUse":[{}]}}}}"#, groups.join(","))
            }
            None => {
                r#"{"hooks":{"PreToolUse":[{"hooks":[{"command":"charter hook posttooluse"}]}]}}"#
                    .to_owned()
            }
        };
        fs::write(dir.join("hooks.json"), hooks).unwrap();
        self
    }

    fn profile(&self) -> Profile {
        let p = profiles::current(self.root()).get("work").unwrap().clone();
        charter_core::profiletrust::record_launched(
            self.root(),
            &p.name,
            &charter_core::profiletrust::fingerprint(&p),
        )
        .unwrap();
        p
    }

    fn detect(&self) -> wiring::Wiring {
        wiring::detect(&self.profile(), self.root(), self.root())
    }
}

/// A config carrying all three marks, with the guard trusted at `pre_tool_use:0:0`.
const ALL_THREE: &str = r#"
[plugins."charter@charter"]
enabled = true

[shell_environment_policy.set]
CHARTER_HARNESS = "codex"

[hooks.state."charter@charter:hooks/hooks.json:pre_tool_use:0:0"]
trusted_hash = "abc123"
"#;

#[test]
fn a_codex_home_with_all_three_marks_is_wired() {
    let home = Home::new();
    home.config(ALL_THREE).plugin("0.42.0", Some((0, 0)));

    let w = home.detect();

    assert_eq!(w.state, State::Wired, "{w:?}");
    assert!(w.detail.contains("1 trusted guard hook(s)"), "{w:?}");
}

#[test]
fn a_codex_home_with_no_config_at_all_is_unwired_and_says_every_missing_mark() {
    let home = Home::new();

    let w = home.detect();

    assert_eq!(w.state, State::Unwired, "{w:?}");
    assert!(
        w.detail
            .contains("charter@charter is not an enabled plugin"),
        "{w:?}"
    );
    assert!(
        w.detail
            .contains(r#"shell_environment_policy.set has no CHARTER_HARNESS = "codex""#),
        "{w:?}"
    );
    assert!(w.detail.contains("so there is no guard to trust"), "{w:?}");
}

#[test]
fn a_plugin_nobody_approved_is_installed_and_does_nothing_so_it_is_not_wired() {
    // The case that reads exactly like wired to anything that stops at the plugin table.
    let home = Home::new();
    home.config(
        r#"
[plugins."charter@charter"]
enabled = true

[shell_environment_policy.set]
CHARTER_HARNESS = "codex"
"#,
    )
    .plugin("0.42.0", Some((0, 0)));

    let w = home.detect();

    assert_eq!(w.state, State::Unwired, "{w:?}");
    assert!(
        w.detail.contains("no guard hook of charter's is trusted"),
        "{w:?}"
    );
}

#[test]
fn charters_own_half_being_written_leaves_codexs_own_steps_as_the_fix() {
    // Naming `charter harness install` here would name the command that has already done
    // everything it can — a fix that changes nothing.
    let home = Home::new();
    home.config("[shell_environment_policy.set]\nCHARTER_HARNESS = \"codex\"\n")
        .plugin("0.42.0", Some((0, 0)));

    let w = home.detect();

    assert_eq!(w.state, State::Unwired);
    assert!(w.fix.contains("codex plugin marketplace add"), "{w:?}");
    assert!(
        w.fix
            .contains("start codex once and approve charter's hooks when it asks"),
        "{w:?}"
    );
    assert!(!w.fix.contains("charter harness install"), "{w:?}");
}

#[test]
fn a_policy_table_charter_did_not_write_is_left_alone_and_the_fix_is_the_line_to_add() {
    // charter does not edit TOML it did not write, and an install would answer `present`
    // for ANY `[shell_environment_policy]` table — another fix that changes nothing.
    let home = Home::new();
    home.config("[shell_environment_policy]\ninherit = \"all\"\n");

    let w = home.detect();

    assert_eq!(w.state, State::Unwired);
    assert!(
        w.fix
            .contains("charter does not edit TOML it did not write"),
        "{w:?}"
    );
    assert!(
        w.fix.contains(r#"set = { CHARTER_HARNESS = "codex" }"#),
        "{w:?}"
    );
}

#[test]
fn a_shape_codex_never_writes_is_unknown_and_names_the_key() {
    // `plugins."charter@charter" = true` parses. Read without the check it is a crash in a
    // launch; read with it, it is an unknown that says which key — and an unknown refuses,
    // because charter cannot say what Codex makes of it.
    let home = Home::new();
    home.config("[plugins]\n\"charter@charter\" = true\n");

    let w = home.detect();

    assert_eq!(w.state, State::Unknown, "{w:?}");
    assert!(
        w.detail
            .contains(r#"holds plugins."charter@charter" as something other than a table"#),
        "{w:?}"
    );
}

#[test]
fn a_trust_entry_that_is_not_a_table_is_unknown_rather_than_untrusted() {
    let home = Home::new();
    home.config(
        r#"
[plugins."charter@charter"]
enabled = true
[shell_environment_policy.set]
CHARTER_HARNESS = "codex"
[hooks.state]
"charter@charter:hooks/hooks.json:pre_tool_use:0:0" = "yes"
"#,
    )
    .plugin("0.42.0", Some((0, 0)));

    assert_eq!(home.detect().state, State::Unknown);
}

#[test]
fn two_cached_copies_that_disagree_about_where_the_guard_is_trust_neither() {
    // Codex can keep an older copy beside the current one, and charter cannot tell which of
    // them it numbered the ledger by — so the rule is the one that fails closed.
    let home = Home::new();
    home.config(ALL_THREE)
        .plugin("0.42.0", Some((0, 0)))
        .plugin("0.62.0", Some((3, 0)));

    let w = home.detect();

    // The intersection is empty, so charter has no key every copy agrees is the guard —
    // which reads as "nothing to trust" rather than "trusted but disagreed". Python says it
    // the same way, and this expectation was corrected against that rather than guessed.
    assert_eq!(w.state, State::Unwired, "{w:?}");
    assert!(w.detail.contains("so there is no guard to trust"), "{w:?}");
}

#[test]
fn a_copy_that_places_no_guard_at_all_means_there_is_nothing_to_trust() {
    let home = Home::new();
    home.config(ALL_THREE).plugin("0.42.0", None);

    let w = home.detect();

    assert_eq!(w.state, State::Unwired);
    assert!(w.detail.contains("so there is no guard to trust"), "{w:?}");
}

#[test]
fn codex_is_never_wired_by_a_launch_because_trust_is_a_persons_and_needs_a_session() {
    let home = Home::new();
    home.config(ALL_THREE).plugin("0.42.0", None);
    let p = home.profile();
    let w = home.detect();

    assert!(
        !wiring::would_install(&p, &w),
        "a launch would have tried to wire Codex, whose last step no command can take"
    );
    let answer = wiring::wired_or_refusal(&p, home.root(), home.root());
    assert!(!answer.may_start(), "{answer:?}");
    assert_eq!(
        answer.wired, "",
        "nothing was installed, so nothing is claimed"
    );
}

/// Against a REAL wired `~/.codex`, which the fixtures above cannot speak for.
///
/// Ignored by default: it needs a Codex home somebody has wired and approved in a session.
/// Run with `CHARTER_REAL_CODEX_HOME=~/.codex cargo test -p charter-core -- --ignored`.
#[test]
#[ignore = "needs a real, wired and approved CODEX_HOME"]
fn a_real_wired_codex_home_reads_as_wired() {
    let Ok(real) = std::env::var("CHARTER_REAL_CODEX_HOME") else {
        panic!("set CHARTER_REAL_CODEX_HOME");
    };
    let home = Home::new();
    fs::write(
        home.root().join(profiles::LOCAL_FILE),
        format!("[harness.work]\nkind = \"codex\"\ncommand = [\"codex\"]\nenv = {{ CODEX_HOME = {real:?} }}\n"),
    )
    .unwrap();

    let w = wiring::detect(&home.profile(), home.root(), home.root());

    assert_eq!(w.state, State::Wired, "{w:?}");
}
