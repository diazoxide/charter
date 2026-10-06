//! The leak guard reads a command substitution wherever a shell runs one: inside double quotes,
//! a `${ … }` default, backticks, an expanding heredoc body, and nested, with the same verdict
//! it gives the same read written unquoted, and it refuses what it cannot read whole.
//!
//! A differential against the real shells. Each shape is run by `bash` (and `zsh`, where one is
//! installed) in a fixture project whose vault file holds a marker, with a stand-in command line
//! on `PATH` that prints the marker when asked to reveal. A shape whose output carries the marker
//! really reads a secret, and the guard must refuse it. A shape whose output does not is a
//! mention, and the guard must let it through, so the reading stays as narrow as the shell's.
//!
//! The Python oracle never read these forms (they were among its documented limits), so the
//! shells are the oracle here.

use std::path::{Path, PathBuf};
use std::process::Command;

use purlis_core::{guardcaps, leakguard, shellseg};

/// What the fixture's vault file holds, and what the stand-in prints on `--reveal`.
const MARKER: &str = "MARKER-1412-SECRET";

/// What a stand-in prints when a command a host rule gates really runs.
const RAN: &str = "RAN-GATED-COMMAND";

/// The fixture project: a vault file holding [`MARKER`], an ordinary `sub/`, and a `bin/` with a
/// stand-in command line under both of its names.
struct Fixture {
    _tmp: tempfile::TempDir,
    root: PathBuf,
    vault: String,
    bin: PathBuf,
}

fn fixture() -> Fixture {
    let tmp = tempfile::tempdir().expect("a temporary directory");
    let root = tmp.path().canonicalize().expect("the root resolves");
    let vaults = root.join(".purlis").join("vaults");
    std::fs::create_dir_all(&vaults).expect("mkdir");
    std::fs::create_dir_all(root.join("sub")).expect("mkdir");
    let vault = vaults.join("f.json");
    std::fs::write(&vault, format!("{MARKER}\n")).expect("write the vault");
    let bin = root.join("bin");
    std::fs::create_dir_all(&bin).expect("mkdir");
    // Spelt in two halves, so a search of the project finds the marker only in the vault.
    let (a, b) = MARKER.split_at(6);
    // …and a consent-gated report, or a ruled `terraform apply`, prints [`RAN`] when it runs.
    let (c, d) = RAN.split_at(4);
    let script = format!(
        "#!/bin/sh\nfor w in \"$@\"; do [ \"$w\" = --reveal ] && printf '%s%s\\n' '{a}' '{b}'; \
         [ \"$1\" = report ] && [ \"$w\" = --yes ] && printf '%s%s\\n' '{c}' '{d}'; \
         [ \"$1\" = handoff ] && printf '%s%s\\n' '{c}' '{d}'; \
         done\nexit 0\n"
    );
    let ruled = format!("#!/bin/sh\n[ \"$1\" = apply ] && printf '%s%s\\n' '{c}' '{d}'\nexit 0\n");
    for (name, script) in [
        ("charter", &script),
        ("purlis", &script),
        ("terraform", &ruled),
    ] {
        let p = bin.join(name);
        std::fs::write(&p, script).expect("write the stand-in");
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }
    Fixture {
        vault: vault.to_string_lossy().into_owned(),
        _tmp: tmp,
        root,
        bin,
    }
}

/// `inner` wrapped `depth` times in `echo "$( … )"`.
fn nested(inner: &str, depth: usize) -> String {
    (0..depth).fold(inner.to_string(), |s, _| format!("echo \"$({s})\""))
}

/// A shell the corpus knows, by name and major version: the ones whose readings of the grammar
/// corners differ.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Shell {
    /// macOS's `/bin/bash`.
    Bash3,
    /// Linux's `bash`.
    Bash5,
    Zsh,
}

use Shell::{Bash3, Bash5, Zsh};

/// What the shells do with a shape.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Shells {
    /// Every shell prints the marker.
    Read,
    /// Only these shells print it (bash 3.2, bash 5 and zsh differ on a few grammar corners).
    /// One shell somewhere reads it, so the guard refuses it on every machine. A listed shell
    /// installed here must print the marker and a known one not listed must not; a shell the
    /// corpus does not know is not asked (D-T56-2).
    SomeRead(&'static [Shell]),
    /// No shell prints it: a mention, which the guard lets through.
    Mention,
}

use Shells::{Mention, Read, SomeRead};

/// Every shape, with `{V}` for the vault file, `{R}` for a reveal and `{ROOT}` for the project.
fn shapes() -> Vec<(String, Shells)> {
    let mut out: Vec<(String, Shells)> = [
        // Inside double quotes.
        ("echo \"$(cat {V})\"", Read),
        ("x=\"$(cat {V})\"; echo \"$x\"", Read),
        ("echo \"$({R})\"", Read),
        ("echo \"`{R}`\"", Read),
        ("echo \"`cat {V}`\"", Read),
        ("echo \"before $(cat {V}) after\"", Read),
        ("printf '%s\\n' \"$(cat < {V})\"", Read),
        ("echo \"$(env cat {V})\"", Read),
        ("echo \"$(echo \"a)b\"; cat {V})\"", Read),
        ("echo \"$( (cat {V}) )\"", Read),
        // Inside a parameter expansion's default, quoted and not, and a `'` that is a plain
        // character there.
        ("echo \"${unset_1412:-$(cat {V})}\"", Read),
        ("echo ${unset_1412:-$(cat {V})}", Read),
        ("echo \"${unset_1412:-'$(cat {V})'}\"", Read),
        ("echo \"${unset_1412:-'`cat {V}`'}\"", Read),
        (
            "echo \"${unset_1412-'}$(cat {V})${unset_1412-'}\"",
            SomeRead(&[Zsh]),
        ),
        // ANSI-C quoting, whose escaped quote does not end it.
        ("echo $'\\'' \"$(cat {V})\" $'\\''", Read),
        ("echo $'\\'' \"$({R})\" $'\\''", Read),
        // A comment inside a substitution, holding a `)`.
        ("echo \"$(echo hi # )\ncat {V}\n)\"", Read),
        ("echo \"$(: # )\n{R})\"", Read),
        // A heredoc body inside a substitution, holding a `)`.
        (
            "echo \"$(cat <<'EOF'\n)\nEOF\ncat {V})\"",
            SomeRead(&[Bash5, Zsh]),
        ),
        (
            "echo \"$(cat <<EOF\n)\nEOF\ncat {V})\"",
            SomeRead(&[Bash5, Zsh]),
        ),
        (
            "echo \"$(cat <<'EOF'\n)\nEOF\n{R})\"",
            SomeRead(&[Bash5, Zsh]),
        ),
        // A `case` pattern's `)`.
        (
            "echo \"$(case x in x) cat {V};; esac)\"",
            SomeRead(&[Bash5, Zsh]),
        ),
        (
            "echo $(case x in x) cat {V};; esac)",
            SomeRead(&[Bash5, Zsh]),
        ),
        // …and a pattern written with its optional leading paren, in any branch.
        ("case x in (x) cat {V};; esac", Read),
        ("case x in (x) {R};; esac", Read),
        ("case x in (y) :;; (x) cat {V};; esac", Read),
        ("echo \"$(case x in (x) cat {V};; esac)\"", Read),
        (
            "case x in (y) :;& (x) cat {V};; esac",
            SomeRead(&[Bash5, Zsh]),
        ),
        // Backticks around a quoted substitution, and the other way about.
        ("echo `echo \"$(cat {V})\"`", Read),
        ("echo \"$(echo `cat {V}`)\"", Read),
        ("echo `echo \\`cat {V}\\``", Read),
        // A relocation before the quoted read.
        ("cd {ROOT}/.purlis && echo \"$(cat vaults/f.json)\"", Read),
        // A walk into the state folder, from inside quotes.
        ("echo \"$(grep -rn MARKER .)\"", Read),
        // Text the lexer cannot read whole, inside a substitution: read line by line.
        (
            "echo \"$(cd .purlis; echo $'it\\'s'; cat vaults/f.json)\"",
            Read,
        ),
        ("echo \"$(echo $'it\\'s'; grep -rn MARKER .)\"", Read),
        (
            "echo \"$(cd .purlis; cat <<EOF\nit's\nEOF\ncat vaults/f.json)\"",
            SomeRead(&[Bash5, Zsh]),
        ),
        (
            "echo \"$(echo `echo '`; grep -rn MARKER .)\"",
            SomeRead(&[Bash3, Bash5]),
        ),
        // Past the directories read: refused as too big to check.
        (
            "cd /usr; cd /bin; cd /usr; cd /bin; cd /usr; cd /bin; cd /usr; cd /bin; cd /usr; \
             cd {ROOT}/.purlis; echo \"$(cat vaults/f.json)\"",
            Read,
        ),
        (
            "cd /usr; cd /bin; cd /usr/bin; cd /usr/lib; cd /bin; cd /usr/share; cd /sbin; \
             cd /usr/sbin; cd /usr/libexec; cd {ROOT}; echo \"$(grep -rn MARKER .)\"",
            Read,
        ),
        // A heredoc body whose delimiter is unquoted expands its substitutions.
        ("cat <<EOF\n$(cat {V})\nEOF", Read),
        ("cat <<EOF\n\"$(cat {V})\"\nEOF", Read),
        ("cat <<EOF\nit's\n$(cat {V})\nit's\nEOF", Read),
        ("cat <<EOF\n'$(cat {V})'\nEOF", Read),
        ("cat <<E\\\nOF\n$(cat {V})\nEOF", Read),
        ("cat <<\\\nEOF\n$(cat {V})\nEOF", Read),
        // A line continuation inside what only becomes a substitution, a `case` or a heredoc
        // once the shell takes the backslash-newline out.
        ("echo \"$\\\n(cat {V})\"", SomeRead(&[Bash3, Bash5])),
        ("echo $\\\n(cat {V})", Read),
        ("echo \"$\\\n({R})\"", SomeRead(&[Bash3, Bash5])),
        ("echo \"`ca\\\nt {V}`\"", Read),
        (
            "echo \"$(ca\\\nse x in x) cat {V};; esac)\"",
            SomeRead(&[Bash5, Zsh]),
        ),
        ("cat <\\\n<EOF\n$(cat {V})\nEOF", SomeRead(&[Bash3, Bash5])),
        ("echo \"$\\\n{unset_1412:-$(cat {V})}\"", Read),
        // A relocation into the vault folder and back out again, spelt so its directories
        // differ only before they are normalised.
        (
            "cd {ROOT}/.purlis/vaults; cd ..; cd ..; cd .purlis/vaults; cd ..; cd ..; \
             cd .purlis/vaults; cd ..; cd ..; cd .purlis/vaults; cd ..; cd ..; \
             cd .purlis/vaults; echo \"$(cat f.json)\"",
            Read,
        ),
        // A function the command defines and calls runs its body where it is called.
        ("f(){ cat {V}; }; f", Read),
        ("f() {\n  cat {V}\n}\nf", Read),
        ("function f { cat {V}; }; f", Read),
        ("function f() { cat {V}; }; f", Read),
        ("f() ( cat {V} ); f", Read),
        ("f(){ {R}; }; f", Read),
        ("f(){ g(){ cat {V}; }; g; }; f", Read),
        ("if true; then f(){ cat {V}; }; fi; f", Read),
        ("f(){ cat vaults/f.json; }; cd .purlis; f", Read),
        ("f(){ cat vaults/f.json; }; echo \"$(cd .purlis; f)\"", Read),
        ("echo \"$(f(){ cat {V}; }; f)\"", Read),
        ("echo \"$(f(){ cd .purlis; }; f; cat vaults/f.json)\"", Read),
        ("f() cat {V}; f", SomeRead(&[Zsh])),
        ("f(){ echo {V}; }; f", Mention),
        // A reader's heredoc body is data, whatever function or `case` it spells.
        ("cat > s.sh <<EOF\nf(){ cat {V}; }\nf\nEOF", Mention),
        (
            "cat > s.sh <<EOF\ncase x in (x) cat {V};; esac\nEOF",
            Mention,
        ),
        // Mentions: nothing runs, so nothing is read. A backslash-newline the shell keeps,
        // inside single quotes, a comment or a quoted heredoc body, opens nothing.
        ("echo '$\\\n(cat {V})'", Mention),
        ("cat <<'EOF'\n$\\\n(cat {V})\nEOF", Mention),
        ("echo hi # \\\necho \"$(echo {V})\"", Mention),
        ("cat <<'EOF'\n$(cat {V})\nEOF", Mention),
        ("echo '$(cat {V})'", Mention),
        ("echo \"\\$(cat {V})\"", Mention),
        ("echo \"$(echo {V})\"", Mention),
        ("echo \"$(cd {ROOT}/.purlis/vaults && ls)\"", Mention),
        ("echo \"the flag is --reveal\"", Mention),
        ("echo '\"$(cat {V})\"'", Mention),
        ("echo \"$(echo hi # $(cat {V})\n)\"", Mention),
        (
            "echo \"$(cat <<EOF\nDon't run purlis secret get a --reveal; never cat \
             .purlis/vaults/x.json\nEOF\n)\"",
            Mention,
        ),
        // The same two directories again and again are two directories, not twelve.
        (
            "cd sub; cd ..; cd sub; cd ..; cd sub; cd ..; cd sub; cd ..; cd sub; cd ..; cd sub; \
             cd ..; echo \"$(echo hi)\"",
            Mention,
        ),
    ]
    .into_iter()
    .map(|(s, shells)| (s.to_string(), shells))
    .collect();
    // A long function and many substitutions after it: its body is read once from each
    // directory, not once per substitution, so the command is read whole and let through.
    let body = "echo a-line-of-the-function; ".repeat(80);
    let subs = "echo \"$(echo hi)\"; ".repeat(60);
    out.push((format!("f(){{ {body}}}; {subs}f"), Mention));
    // Nested, read at every depth up to the cap.
    for depth in 1..=guardcaps::MAX_NESTING {
        out.push((nested("cat {V}", depth), Read));
        out.push((nested("{R}", depth), Read));
        out.push((nested("echo {V}", depth), Mention));
    }
    out
}

fn spell(shape: &str, f: &Fixture) -> String {
    shape
        .replace("{V}", &f.vault)
        .replace("{R}", "charter secret get a --reveal")
        .replace("{ROOT}", &f.root.to_string_lossy())
}

/// Whether `shell` prints the marker when it runs `cmd` in the fixture.
fn shell_reads(shell: &Path, cmd: &str, f: &Fixture) -> bool {
    shell_prints(shell, cmd, f, MARKER)
}

/// Whether `shell` prints `marker` when it runs `cmd` in the fixture.
fn shell_prints(shell: &Path, cmd: &str, f: &Fixture, marker: &str) -> bool {
    let mut command = Command::new(shell);
    command
        .arg("-c")
        .arg(cmd)
        .current_dir(&f.root)
        .env_clear()
        .env("PATH", format!("{}:/usr/bin:/bin", f.bin.display()))
        .env("HOME", &f.root)
        // Where a shell writes a heredoc's body: bash asks the first, zsh the second.
        .env("TMPDIR", std::env::temp_dir())
        .env("TMPPREFIX", std::env::temp_dir().join("zsh"));
    let out = purlis_core::forklock::output(&mut command).expect("the shell runs");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    text.contains(marker)
}

fn shells() -> Vec<PathBuf> {
    ["/bin/bash", "/bin/zsh", "/usr/bin/zsh"]
        .iter()
        .map(PathBuf::from)
        .filter(|p| p.is_file())
        .collect()
}

/// Which [`Shell`] `shell` is, or `None` for one the corpus does not know (bash 4, say).
fn kind(shell: &Path) -> Option<Shell> {
    let mut command = Command::new(shell);
    command
        .arg("-c")
        .arg("echo \"${BASH_VERSINFO[0]}:${ZSH_VERSION}\"")
        .env_clear();
    let out = purlis_core::forklock::output(&mut command).ok()?;
    let said = String::from_utf8_lossy(&out.stdout);
    match said.trim().split_once(':')? {
        ("3", "") => Some(Bash3),
        ("5", "") => Some(Bash5),
        ("", zsh) if !zsh.is_empty() => Some(Zsh),
        _ => None,
    }
}

#[test]
fn every_shape_a_shell_reads_a_secret_with_is_refused_and_no_other() {
    purlis_core::unsteered!();
    let f = fixture();
    let state = f.root.join(".purlis");
    let cwd = f.root.to_string_lossy().into_owned();
    let installed = shells();
    let kinds: Vec<Option<Shell>> = installed.iter().map(|shell| kind(shell)).collect();
    let mut wrong = Vec::new();
    for (shape, expected) in shapes() {
        let cmd = spell(&shape, &f);
        let read: Vec<bool> = installed
            .iter()
            .map(|shell| shell_reads(shell, &cmd, &f))
            .collect();
        let agrees = match expected {
            Read => read.iter().all(|&r| r),
            SomeRead(readers) => kinds
                .iter()
                .zip(&read)
                .all(|(kind, &r)| kind.is_none_or(|kind| readers.contains(&kind) == r)),
            Mention => !read.iter().any(|&r| r),
        };
        if !installed.is_empty() && !agrees {
            wrong.push(format!(
                "the shells ({installed:?}) ({kinds:?}) printed the marker {read:?} for {shape:?}; the \
                 corpus says {expected:?}"
            ));
        }
        let refused = leakguard::leak_reason(&cmd, &cwd, &state);
        if refused.is_some() != (expected != Mention) {
            wrong.push(format!(
                "the guard {} {shape:?}",
                if expected == Mention {
                    "refused"
                } else {
                    "let through"
                },
            ));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn a_substitution_nested_past_the_cap_is_refused_as_too_big_to_check() {
    purlis_core::unsteered!();
    let f = fixture();
    let state = f.root.join(".purlis");
    let cwd = f.root.to_string_lossy().into_owned();
    let past = guardcaps::MAX_NESTING + 1;
    for depth in [past, 33, 100, 400] {
        for inner in [
            "cat {V}",
            "{R}",
            "grep -rn MARKER .",
            "cd .purlis; cat vaults/f.json",
            "echo {V}",
        ] {
            let cmd = spell(&nested(inner, depth), &f);
            let (refused, too_deep) =
                shellseg::too_deep_within(|| leakguard::leak_reason(&cmd, &cwd, &state));
            assert!(
                refused.is_some() && too_deep,
                "depth {depth}: {inner:?} was not refused as too big to check ({refused:?})"
            );
        }
    }
}

/// The whole verdict agrees with the caps every other guard is held to: past the nesting cap
/// the call is refused as too big to check, in both modes, and up to it a read is refused as a
/// read.
#[test]
fn the_verdict_past_the_cap_is_too_big_to_check_in_both_modes() {
    purlis_core::unsteered!();
    use purlis_core::handoffguard::Caller;
    use purlis_core::toolgate::{self, Call};
    let f = fixture();
    let state = f.root.join(".purlis");
    let cwd = f.root.to_string_lossy().into_owned();
    for mode in ["default", "bypassPermissions"] {
        let verdict = |cmd: &str| {
            toolgate::verdict(
                &Call {
                    command: cmd,
                    cwd: &cwd,
                    state_dir: &state,
                    caller: Caller {
                        agent_id: None,
                        harness: Some("claude-code"),
                        permission_mode: Some(mode),
                    },
                },
                None,
            )
            .map(|v| v.reason)
        };
        for depth in [guardcaps::MAX_NESTING + 1, 33] {
            for inner in ["grep -rn MARKER .", "cd .purlis; cat vaults/f.json", "{R}"] {
                let cmd = spell(&nested(inner, depth), &f);
                assert_eq!(
                    verdict(&cmd).as_deref(),
                    Some(guardcaps::REASON),
                    "{mode}, depth {depth}: {inner:?}"
                );
            }
        }
        let at_the_cap = spell(&nested("cat {V}", guardcaps::MAX_NESTING), &f);
        let reason = verdict(&at_the_cap).expect("a read at the cap is refused");
        assert_ne!(reason, guardcaps::REASON, "{mode}: read whole at the cap");
    }
}

/// Every shape that runs `{G}`, a command a host rule gates, from inside a substitution or a
/// string the rule never reads, and every shape that only mentions it.
fn gated_shapes() -> Vec<(String, Shells)> {
    let mut out: Vec<(String, Shells)> = [
        ("echo \"$({G})\"", Read),
        ("echo \"`{G}`\"", Read),
        ("echo \"${unset_1417:-$({G})}\"", Read),
        ("echo \"${unset_1417:-'$({G})'}\"", Read),
        ("echo $'\\'' \"$({G})\" $'\\''", Read),
        ("echo \"$(echo hi # )\n{G}\n)\"", Read),
        (
            "echo \"$(cat <<'EOF'\n)\nEOF\n{G})\"",
            SomeRead(&[Bash5, Zsh]),
        ),
        (
            "echo \"$(case x in x) {G};; esac)\"",
            SomeRead(&[Bash5, Zsh]),
        ),
        ("echo `echo \"$({G})\"`", Read),
        ("echo \"$(echo `{G}`)\"", Read),
        ("echo \"$\\\n({G})\"", SomeRead(&[Bash3, Bash5])),
        // An expanding heredoc body runs its substitutions, whichever program reads the body.
        ("cat <<EOF\n$({G})\nEOF", Read),
        ("sort <<EOF\n\"$({G})\"\nEOF", Read),
        ("cat <<EOF\nit's\n`{G}`\nEOF", Read),
        // A `case` branch and a function body run their commands, and the rule reads neither.
        ("case x in x) {G};; esac", Read),
        ("case x in y) :;; x) {G};; esac", Read),
        ("case x in (x) {G};; esac", Read),
        ("case x in (y) :;; (x) {G};; esac", Read),
        ("echo \"$(case x in (x) {G};; esac)\"", Read),
        // A line continuation in the delimiter is taken out first: the body expands.
        ("cat <<E\\\nOF\n$({G})\nEOF", Read),
        ("cat <<\\\nEOF\n$({G})\nEOF", Read),
        // A reader's heredoc runs only the substitutions in its body.
        ("tee s.sh <<EOF\nf() { x; }\n$({G})\nEOF", Read),
        ("f(){ {G}; }; f", Read),
        ("function f { {G}; }; f", Read),
        ("echo \"$(f(){ {G}; }; f)\"", Read),
        // Mentions.
        ("echo \"$(echo {G})\"", Mention),
        ("f(){ echo {G}; }; f", Mention),
        // A reader's heredoc body is data, whatever function or `case` it spells.
        ("cat > s.sh <<EOF\nf() { {G}; }\nEOF", Mention),
        ("cat > s.sh <<EOF\n/usr/local/bin/{G}\nEOF", Mention),
        ("cat > s.sh <<EOF\nf() { {G}; }\nf\nEOF", Mention),
        ("cat > s.sh <<EOF\ncase $1 in a) {G};; esac\nEOF", Mention),
        (
            "cat > s.sh <<EOF\ncase x in (y) :;; (x) {G};; esac\nEOF",
            Mention,
        ),
        ("echo '$({G})'", Mention),
        ("echo \"\\$({G})\"", Mention),
        ("cat <<'EOF'\n$({G})\nEOF", Mention),
        ("echo \"$(echo hi # $({G})\n)\"", Mention),
        ("echo \"`echo {G}`\"", Mention),
    ]
    .into_iter()
    .map(|(s, shells)| (s.to_string(), shells))
    .collect();
    for depth in 1..=guardcaps::MAX_NESTING {
        out.push((nested("{G}", depth), Read));
        out.push((nested("echo {G}", depth), Mention));
    }
    out
}

/// Runs every [`gated_shapes`] shape `shapes` keeps for each gated command in `gated` through
/// the shells and through `judge`, and answers every disagreement: a shape a shell runs the command with must be
/// refused, and a mention let through.
fn disagreements(
    gated: &[&str],
    judge: &dyn Fn(&str) -> Option<String>,
    shapes: impl Fn(&str) -> bool,
) -> Vec<String> {
    let f = fixture();
    let installed = shells();
    let kinds: Vec<Option<Shell>> = installed.iter().map(|shell| kind(shell)).collect();
    let mut wrong = Vec::new();
    for g in gated {
        for (shape, expected) in gated_shapes().into_iter().filter(|(s, _)| shapes(s)) {
            let cmd = spell(&shape.replace("{G}", g), &f);
            let ran: Vec<bool> = installed
                .iter()
                .map(|shell| shell_prints(shell, &cmd, &f, RAN))
                .collect();
            let agrees = match expected {
                Read => ran.iter().all(|&r| r),
                SomeRead(readers) => kinds
                    .iter()
                    .zip(&ran)
                    .all(|(kind, &r)| kind.is_none_or(|kind| readers.contains(&kind) == r)),
                Mention => !ran.iter().any(|&r| r),
            };
            if !installed.is_empty() && !agrees {
                wrong.push(format!(
                    "the shells ({installed:?}) ran it {ran:?} for {cmd:?}; the corpus says \
                     {expected:?}"
                ));
            }
            let (refused, _) = shellseg::too_deep_within(|| judge(&cmd));
            if refused.is_some() != (expected != Mention) {
                wrong.push(format!(
                    "the backstop {} {cmd:?}",
                    if expected == Mention {
                        "refused"
                    } else {
                        "let through"
                    },
                ));
            }
        }
    }
    wrong
}

#[test]
fn every_shape_a_shell_runs_an_operators_ruled_command_with_is_refused_and_no_other() {
    purlis_core::unsteered!();
    let rules = purlis_core::rulespelling::Rules::of_claude_settings(
        ".claude/settings.json",
        r#"{"permissions": {"ask": ["Bash(terraform apply *)"]}}"#,
    );
    let wrong = disagreements(&["terraform apply x"], &|cmd| rules.refusal(cmd), |_| true);
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn every_shape_a_shell_runs_a_consent_gated_command_with_is_refused_at_every_depth() {
    purlis_core::unsteered!();
    // A project with no consent rule under the new name, so neither spelling is the host's to
    // ask about once it sits where the rule does not read.
    let project = tempfile::tempdir().expect("a project");
    let judge =
        |cmd: &str| purlis_core::consentspelling::refusal(cmd, project.path(), &[project.path()]);
    let mut wrong = disagreements(
        &["purlis report bug --yes x", "charter report bug --yes x"],
        &judge,
        |_| true,
    );
    // A handoff in a substitution, which the handoff guard does not read there.
    wrong.extend(disagreements(&["charter handoff beta"], &judge, |shape| {
        shape.contains("$(") || shape.contains('`')
    }));
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn a_gated_command_nested_past_the_cap_is_refused_as_too_big_to_check() {
    purlis_core::unsteered!();
    let project = tempfile::tempdir().expect("a project");
    let rules = purlis_core::rulespelling::Rules::of_claude_settings(
        ".claude/settings.json",
        r#"{"permissions": {"ask": ["Bash(terraform apply *)"]}}"#,
    );
    for depth in [guardcaps::MAX_NESTING + 1, 33] {
        let consent = nested("purlis report bug --yes x", depth);
        let (refused, too_deep) = shellseg::too_deep_within(|| {
            purlis_core::consentspelling::refusal(&consent, project.path(), &[project.path()])
        });
        assert!(
            refused.is_some() && too_deep,
            "consent, depth {depth}: {refused:?}"
        );
        let ruled = nested("terraform apply x", depth);
        let (refused, too_deep) = shellseg::too_deep_within(|| rules.refusal(&ruled));
        assert!(
            refused.is_some() && too_deep,
            "rule, depth {depth}: {refused:?}"
        );
    }
}

#[test]
fn a_refusal_says_where_the_command_sits_and_to_run_it_as_its_own_command() {
    purlis_core::unsteered!();
    let project = tempfile::tempdir().expect("a project");
    let rules = purlis_core::rulespelling::Rules::of_claude_settings(
        ".claude/settings.json",
        r#"{"permissions": {"ask": ["Bash(terraform apply *)"]}}"#,
    );
    for (shape, place) in [
        ("x=$({G})", "inside a command substitution"),
        (
            "echo \"$(echo \"$({G})\")\"",
            "inside a command substitution",
        ),
        ("case x in y) :;; x) {G};; esac", "inside a `case` branch"),
        ("f(){ {G}; }; f", "inside a function body"),
    ] {
        for (g, judged) in [
            (
                "terraform apply x",
                rules.refusal(&shape.replace("{G}", "terraform apply x")),
            ),
            (
                "purlis report bug --yes x",
                purlis_core::consentspelling::refusal(
                    &shape.replace("{G}", "purlis report bug --yes x"),
                    project.path(),
                    &[project.path()],
                ),
            ),
        ] {
            let why = judged.unwrap_or_else(|| panic!("{shape} with {g} was let through"));
            assert!(why.contains(place), "{shape} with {g}: {why}");
            assert!(
                why.contains("as a command of its own"),
                "{shape} with {g}: {why}"
            );
            assert!(!why.contains("another case"), "{shape} with {g}: {why}");
        }
    }
}
