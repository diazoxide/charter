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
    let script = format!(
        "#!/bin/sh\nfor w in \"$@\"; do [ \"$w\" = --reveal ] && printf '%s%s\\n' '{a}' '{b}'; \
         done\nexit 0\n"
    );
    for name in ["charter", "purlis"] {
        let p = bin.join(name);
        std::fs::write(&p, &script).expect("write the stand-in");
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
    text.contains(MARKER)
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
