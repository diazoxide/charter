//! The policy compiled for opencode: charter's own wrap around the whole harness (ADR 0067 §2),
//! because opencode has no sandbox of its own. Every expected answer is written out.

use std::path::{Path, PathBuf};

use super::*;

/// The sandbox on, without the `toolchains` preset: that preset gives a chat the project's own
/// package caches (#1337), made under the machine's data home when the chat starts, which these
/// tests' machines do not have. What the caches add to a wrap is `sandbox::tests`' to show.
const ON: &str = "[sandbox]\nmode = \"on\"\negress = [\"model-providers\", \"forge\"]\n";

fn plane_saying(toml: &str) -> tempfile::TempDir {
    let plane = tempfile::tempdir().expect("a plane");
    std::fs::write(plane.path().join("charter.toml"), toml).expect("charter.toml");
    plane
}

fn machine(os: Os) -> Machine {
    Machine {
        env: crate::secrets::Env::of(&[]),
        home: Some(PathBuf::from("/Users/op")),
        os,
    }
}

fn wrapped_in(plane: &tempfile::TempDir) -> Applied {
    for_start(
        Harness::Opencode,
        plane.path(),
        &machine(Os::MacOs),
        &|_| true,
    )
    .expect("starts")
    .expect("sandboxed")
}

fn words(line: &str) -> Vec<String> {
    line.split(' ').map(str::to_owned).collect()
}

/// The line `applied` gives an opencode chat in `cwd`, reporting to `socket`, confined by
/// `confinement`.
fn line_in(
    applied: &Applied,
    cwd: &Path,
    socket: Option<&Path>,
    confinement: &Confinement,
) -> Result<Line, String> {
    applied.line(
        Words {
            program: "opencode".to_owned(),
            command: Vec::new(),
            armed: Vec::new(),
            charters: words("--session ses_1"),
        },
        &At {
            cwd: Some(cwd),
            hook_socket: socket,
            confinement: Some(confinement),
            no_opt_out: false,
        },
    )
}

/// The profile handed to `sandbox-exec` on `line`.
fn profile_of(line: &Line) -> &str {
    assert_eq!(line.program, backend::SANDBOX_EXEC);
    assert_eq!(line.args[0], "-p");
    &line.args[1]
}

#[test]
fn an_opencode_chat_in_a_sandboxed_plane_starts_wrapped_on_macos() {
    let plane = plane_saying(ON);
    let applied = wrapped_in(&plane);
    assert_eq!(applied.harness(), Harness::Opencode);
    assert!(
        matches!(applied.form(), Form::Opencode(_)),
        "{:?}",
        applied.form()
    );
}

#[test]
fn on_linux_an_opencode_chat_is_refused_until_charter_can_wrap_it_there() {
    let plane = plane_saying(ON);
    let refused = for_start(
        Harness::Opencode,
        plane.path(),
        &machine(Os::Linux),
        &|_| true,
    )
    .expect_err("refused");
    assert_eq!(
        refused,
        NotStarted::Uncompilable(Uncompilable {
            harness: Harness::Opencode,
            unheld: Unheld::Wrap(Os::Linux),
        })
    );
    // Which harness it names instead is this machine's to say: on Linux, Claude Code.
    assert!(
        refused.to_string().starts_with(
            "this project runs every chat sandboxed, and purlis runs opencode inside a sandbox of \
             its own, which it can apply on macOS but not yet on Linux, so it was not started. \
             Start this chat on a Claude Code"
        ),
        "{refused}"
    );
}

#[test]
fn a_keyring_vault_does_not_refuse_a_wrapped_opencode_chat_because_the_wrap_holds_the_store() {
    // Measured on macOS: under the profile, `security find-generic-password` could not reach
    // an item it read outside it.
    let plane = plane_saying(ON);
    std::fs::write(
        plane.path().join("vaults.json"),
        r#"{"vaults": {"dev": {"provider": "keyring"}}}"#,
    )
    .expect("the registry");
    let applied = wrapped_in(&plane);
    let confinement = applied.confine().expect("confined").expect("a wrap");
    let line = line_in(&applied, plane.path(), None, &confinement).expect("starts");
    let profile = profile_of(&line);
    assert!(
        profile.contains("(deny file-read* file-write* (subpath \"/Users/op/Library/Keychains\"))"),
        "{profile}"
    );
    assert!(!profile.contains("Security"), "{profile}");
    assert!(!profile.contains("securityd"), "{profile}");
}

#[test]
fn the_wrapped_line_runs_the_whole_harness_under_sandbox_exec() {
    let plane = plane_saying(ON);
    let applied = wrapped_in(&plane);
    let confinement = applied.confine().expect("confined").expect("a wrap");
    let line = applied
        .line(
            Words {
                program: "ocw".to_owned(),
                command: words("work"),
                armed: words("--port 0"),
                charters: words("--session ses_1"),
            },
            &At {
                cwd: Some(plane.path()),
                hook_socket: None,
                confinement: Some(&confinement),
                no_opt_out: false,
            },
        )
        .expect("starts");
    assert_eq!(line.program, "/usr/bin/sandbox-exec");
    assert_eq!(line.args[0], "-p");
    assert_eq!(line.args[2..], words("ocw work --port 0 --session ses_1"));
}

#[test]
fn a_wrapped_chat_reaches_the_network_through_charters_proxy_and_has_its_own_temp_directory() {
    let plane = plane_saying(ON);
    let applied = wrapped_in(&plane);
    let confinement = applied.confine().expect("confined").expect("a wrap");
    let line = line_in(&applied, plane.path(), None, &confinement).expect("starts");
    let proxy = format!("http://127.0.0.1:{}", confinement.proxy_port());
    let tmp = confinement.tmp().display().to_string();
    let env: std::collections::BTreeMap<&str, &str> = line
        .env
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect();
    for key in [
        "HTTPS_PROXY",
        "HTTP_PROXY",
        "ALL_PROXY",
        "https_proxy",
        "http_proxy",
        "all_proxy",
    ] {
        assert_eq!(env.get(key), Some(&proxy.as_str()), "{key}");
    }
    assert_eq!(env.get("NO_PROXY"), Some(&""));
    assert_eq!(env.get("no_proxy"), Some(&""));
    assert_eq!(env.get("TMPDIR"), Some(&tmp.as_str()));
    // What other tools read in place of TMPDIR, each in the chat's own temp directory (#1120).
    assert_eq!(env.get("TMP"), Some(&tmp.as_str()));
    assert_eq!(env.get("TEMP"), Some(&tmp.as_str()));
    let within = |key: &str| {
        env.get(key)
            .is_some_and(|value| Path::new(value).parent() == Some(confinement.tmp()))
    };
    assert!(within("xcrun_db"), "{env:?}");
    assert!(within("CLANG_MODULE_CACHE_PATH"), "{env:?}");
    assert!(confinement.tmp().is_dir());
    let profile = profile_of(&line);
    assert!(
        profile.contains(&format!(
            "(allow network-outbound (remote ip \"localhost:{}\"))",
            confinement.proxy_port()
        )),
        "{profile}"
    );
}

#[test]
fn the_profile_denies_by_default_and_reaches_only_the_proxy_and_the_hook_socket() {
    let plane = plane_saying(ON);
    let applied = wrapped_in(&plane);
    let confinement = applied.confine().expect("confined").expect("a wrap");
    let socket = plane.path().join(".charter/app/hooks.sock");
    let line = line_in(&applied, plane.path(), Some(&socket), &confinement).expect("starts");
    let profile = profile_of(&line);
    assert!(
        profile.starts_with("(version 1)\n(deny default)\n"),
        "{profile}"
    );
    assert!(!profile.contains("(allow default)"), "{profile}");
    let outbound: Vec<&str> = profile
        .lines()
        .filter(|line| line.contains("network-outbound") || line.contains("network-bind"))
        .collect();
    assert_eq!(
        outbound,
        [
            format!(
                "(allow network-outbound (remote ip \"localhost:{}\"))",
                confinement.proxy_port()
            ),
            format!(
                "(allow network-outbound (remote ip \"localhost:{}\"))",
                confinement.socks_port()
            ),
            format!(
                "(allow network-outbound (remote unix-socket (path-literal \"{}\")))",
                real(&socket).display()
            ),
        ]
    );
    assert!(!profile.contains("network-inbound"), "{profile}");
}

/// `path` as the kernel names it, which is what a profile must say: on macOS `/tmp` and
/// `/var` are links into `/private`.
fn real(path: &Path) -> PathBuf {
    let (mut existing, mut rest) = (path.to_path_buf(), Vec::new());
    while !existing.exists() {
        rest.push(existing.file_name().expect("a name").to_owned());
        existing.pop();
    }
    let mut out = existing.canonicalize().expect("canonical");
    out.extend(rest.into_iter().rev());
    out
}

/// The rules of the profile's one `file-write*` allow, each as written there.
fn write_allows(profile: &str) -> Vec<String> {
    let start = profile.find("(allow file-write*\n").expect("a write allow");
    let block = &profile[start..=profile[start..].find("))\n").expect("its end") + start];
    block
        .lines()
        .skip(1)
        .map(|line| line.trim().to_owned())
        .collect()
}

#[test]
fn the_profile_writes_the_chat_directory_its_temp_and_only_what_an_opencode_turn_writes() {
    // Measured on a fresh data directory (ruling V73a): a turn writes its database and its log;
    // storage is where older versions keep sessions. No snapshots, and no directory of
    // opencode's own is made or moved: charter makes them first.
    let plane = plane_saying(ON);
    let applied = wrapped_in(&plane);
    let confinement = applied.confine().expect("confined").expect("a wrap");
    let line = line_in(&applied, plane.path(), None, &confinement).expect("starts");
    let data = "/Users/op/.local/share/opencode";
    assert_eq!(
        write_allows(profile_of(&line)),
        [
            format!("(subpath \"{}\")", real(plane.path()).display()),
            format!("(subpath \"{}\")", real(confinement.tmp()).display()),
            format!(
                "(regex \"^{}/opencode\\\\.db(-wal|-shm|-journal)?$\")",
                data.replace('.', "\\\\.")
            ),
            format!("(regex \"^{}/log/.+\")", data.replace('.', "\\\\.")),
            format!("(regex \"^{}/storage/.+\")", data.replace('.', "\\\\.")),
            "(literal \"/Users/op/.config/opencode/.gitignore\")".to_owned(),
            "(literal \"/dev/null\")".to_owned(),
            "(literal \"/dev/tty\")".to_owned(),
            "(literal \"/dev/ptmx\")".to_owned(),
            "(regex #\"^/dev/ttys[0-9]+$\")".to_owned(),
        ]
    );
}

#[test]
fn nothing_a_later_opencode_loads_is_written() {
    // Ruling V73a: its credentials, which can name a remote config that starts servers, and a
    // snapshot repository's config, hooks and directory, which git reads when opencode runs it.
    let plane = plane_saying(ON);
    let applied = wrapped_in(&plane);
    let confinement = applied.confine().expect("confined").expect("a wrap");
    let line = line_in(&applied, plane.path(), None, &confinement).expect("starts");
    let profile = profile_of(&line);
    let data = "/Users/op/.local/share/opencode";
    for rule in [
        format!("(deny file-write* (literal \"{data}/auth.json\"))"),
        format!("(deny file-write* (literal \"{data}/mcp-auth.json\"))"),
        format!("(deny file-write* (subpath \"{data}/snapshot\"))"),
        format!("(deny file-write-create (require-all (vnode-type SYMLINK) (subpath \"{data}\")))"),
        format!("(deny file-write-create (require-all (vnode-type DIRECTORY) (subpath \"{data}\")))"),
        "(deny file-write-create (require-all (vnode-type SYMLINK) (literal \"/Users/op/.config/opencode/.gitignore\")))".to_owned(),
    ] {
        let at = profile.find(&rule).unwrap_or_else(|| panic!("{rule} missing:\n{profile}"));
        assert!(at > profile.find("(allow file-write*").expect("an allow"), "{rule}");
    }
    assert!(!profile.contains("/Users/op/.cache/opencode"), "{profile}");
}

#[test]
fn every_later_code_name_is_denied_at_any_depth_of_the_chat_directory() {
    let plane = plane_saying(ON);
    let applied = wrapped_in(&plane);
    let confinement = applied.confine().expect("confined").expect("a wrap");
    let line = line_in(&applied, plane.path(), None, &confinement).expect("starts");
    let profile = profile_of(&line);
    let root = real(plane.path())
        .display()
        .to_string()
        .replace('.', "\\\\.");
    assert!(
        profile.contains(&format!(
            "(deny file-write* (regex \"^{root}/(.*/)?\\\\.git/config(/.*)?$\"))"
        )),
        "{profile}"
    );
    // The `.git` itself, so a directory holding a config is never moved into its place.
    assert!(
        profile.contains(&format!(
            "(deny file-write* (regex \"^{root}/(.*/)?\\\\.git$\"))"
        )),
        "{profile}"
    );
    assert_eq!(
        profile
            .matches(&format!("(deny file-write* (regex \"^{root}/(.*/)?"))
            .count(),
        PLANTED.len()
    );
}

#[test]
fn opencodes_own_directories_follow_the_xdg_variables_where_they_are_absolute() {
    let plane = plane_saying(ON);
    let machine = Machine {
        env: crate::secrets::Env::of(&[
            ("XDG_DATA_HOME", "/data"),
            ("XDG_STATE_HOME", "/state"),
            ("XDG_CONFIG_HOME", "/config"),
            // Not absolute, so not a base directory.
            ("XDG_DATA_HOME", "relative"),
        ]),
        ..machine(Os::MacOs)
    };
    let applied = for_start(Harness::Opencode, plane.path(), &machine, &|_| true)
        .expect("starts")
        .expect("sandboxed");
    let confinement = applied.confine().expect("confined").expect("a wrap");
    let line = line_in(&applied, plane.path(), None, &confinement).expect("starts");
    let profile = profile_of(&line);
    assert!(
        profile.contains("(regex \"^/Users/op/\\\\.local/share/opencode/log/.+\")"),
        "{profile}"
    );
    assert!(
        profile.contains("(literal \"/config/opencode/.gitignore\")"),
        "{profile}"
    );
    // Its own state directory is never written: the chat is given one in its temp directory.
    assert!(!profile.contains("/state/opencode"), "{profile}");
    assert!(
        line.env.contains(&(
            "XDG_STATE_HOME".to_owned(),
            confinement.tmp().join("state").display().to_string()
        )),
        "{:?}",
        line.env
    );
}

#[test]
fn a_chat_whose_directory_holds_opencodes_own_files_is_not_wrapped() {
    // Its directory's grant would cover what a later opencode loads.
    let plane = plane_saying(ON);
    let home = plane.path().join("home");
    for dir in [".config", ".local/share/opencode/log"] {
        std::fs::create_dir_all(home.join(dir)).expect("a directory");
    }
    let machine = Machine {
        env: crate::secrets::Env::of(&[]),
        home: Some(home.clone()),
        os: Os::MacOs,
    };
    let applied = for_start(Harness::Opencode, plane.path(), &machine, &|_| true)
        .expect("starts")
        .expect("sandboxed");
    let confinement = applied.confine().expect("confined").expect("a wrap");
    for cwd in [
        home.clone(),
        home.join(".config"),
        home.join(".local/share/opencode/log"),
    ] {
        assert_eq!(
            line_in(&applied, &cwd, None, &confinement),
            Err(
                "this project runs every chat sandboxed, and purlis cannot wrap an opencode chat \
                 whose directory holds opencode's own files, or is inside them, so nothing was \
                 started."
                    .to_owned()
            ),
            "{}",
            cwd.display()
        );
    }
}

#[test]
fn the_profile_denies_every_class_after_what_it_lets_the_chat_write() {
    // Seatbelt takes the last rule that matches, so a denial inside the chat's own directory
    // must come after the allow that covers it.
    let plane = plane_saying(ON);
    let applied = wrapped_in(&plane);
    let confinement = applied.confine().expect("confined").expect("a wrap");
    let line = line_in(&applied, plane.path(), None, &confinement).expect("starts");
    let profile = profile_of(&line);
    let allow = profile.find("(allow file-write*").expect("a write allow");
    let Form::Opencode(wrap) = applied.form() else {
        panic!("compiled for opencode");
    };
    assert!(!wrap.denied.is_empty());
    for denial in &wrap.denied {
        let path = real(&denial.path).display().to_string();
        let rule = match denial.access {
            Access::ReadWrite => format!("(deny file-read* file-write* (subpath \"{path}\"))"),
            Access::Write => format!("(deny file-write* (subpath \"{path}\"))"),
        };
        let at = profile
            .find(&rule)
            .unwrap_or_else(|| panic!("{rule} missing:\n{profile}"));
        assert!(at > allow, "{rule} comes before the write allow");
    }
    // The integrity class inside the plane, written for.
    assert!(
        profile.contains(&format!(
            "(deny file-write* (subpath \"{}\"))",
            real(&plane.path().join(".charter/app")).display()
        )),
        "{profile}"
    );
}

#[test]
fn a_path_is_quoted_so_it_cannot_end_its_string_and_one_with_a_control_character_is_refused() {
    let plane = plane_saying(ON);
    let applied = wrapped_in(&plane);
    let confinement = applied.confine().expect("confined").expect("a wrap");
    let odd = plane.path().join("a\"b\\c");
    std::fs::create_dir(&odd).expect("an odd directory");
    let line = line_in(&applied, &odd, None, &confinement).expect("starts");
    let profile = profile_of(&line);
    let quoted = real(&odd)
        .display()
        .to_string()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    assert!(
        profile.contains(&format!("(subpath \"{quoted}\")")),
        "{profile}"
    );

    let newline = plane.path().join("a\nb");
    std::fs::create_dir(&newline).expect("a directory with a newline");
    assert_eq!(
        line_in(&applied, &newline, None, &confinement),
        Err(
            "this project runs every chat sandboxed, and purlis cannot write a sandbox profile \
             for a path holding a control character, so nothing was started."
                .to_owned()
        )
    );
}

#[test]
fn a_wrapped_chat_with_no_directory_or_no_confinement_is_refused() {
    let plane = plane_saying(ON);
    let applied = wrapped_in(&plane);
    let confinement = applied.confine().expect("confined").expect("a wrap");
    let words = || Words {
        program: "opencode".to_owned(),
        command: Vec::new(),
        armed: Vec::new(),
        charters: Vec::new(),
    };
    assert_eq!(
        applied.line(
            words(),
            &At {
                cwd: None,
                hook_socket: None,
                confinement: Some(&confinement),
                no_opt_out: false,
            }
        ),
        Err(FolderRefusal::Missing.said(&policy::Locks::none()))
    );
    assert_eq!(
        applied.line(
            words(),
            &At {
                cwd: Some(plane.path()),
                hook_socket: None,
                confinement: None,
                no_opt_out: false,
            }
        ),
        Err(
            "this project runs every chat sandboxed, and purlis's egress proxy was not started \
             for this opencode chat, so nothing was started."
                .to_owned()
        )
    );
}

#[test]
fn a_harness_with_its_own_sandbox_is_not_confined_by_charter() {
    let plane = plane_saying(ON);
    let applied =
        compiled_anyway(Harness::ClaudeCode, plane.path(), &machine(Os::MacOs)).expect("compiles");
    assert!(applied.confine().expect("nothing to start").is_none());
}

/// The profile, applied for real: what it denies is denied, what it allows is allowed.
#[cfg(target_os = "macos")]
mod live {
    use super::*;
    use std::process::Command;

    fn run(line: &Line, script: &str) -> std::process::Output {
        crate::forklock::output(
            Command::new(&line.program)
                .args(&line.args[..2])
                .args(["/bin/sh", "-c", script])
                .envs(line.env.iter().map(|(k, v)| (k.as_str(), v.as_str()))),
        )
        .expect("sandbox-exec runs")
    }

    #[test]
    fn the_wrap_holds_paths_and_the_network() {
        let plane = plane_saying(ON);
        let cwd = plane.path().join("work");
        std::fs::create_dir(&cwd).expect("a workspace");
        // A plain-file vault, which the vaults class denies to every chat.
        std::fs::write(plane.path().join("vaults.json"), r#"{"vaults": {}}"#).expect("registry");
        let vaults = plane.path().join(".charter/vaults");
        std::fs::create_dir_all(&vaults).expect("vaults");
        std::fs::write(vaults.join("secret"), "s3cret").expect("a secret");
        let elsewhere = tempfile::tempdir().expect("elsewhere");
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a listener");
        let port = listener.local_addr().expect("an address").port();

        // A home of its own, so nothing here can touch the operator's opencode.
        let home = tempfile::tempdir().expect("a home");
        let machine = Machine {
            env: crate::secrets::Env::of(&[]),
            home: Some(home.path().to_path_buf()),
            os: Os::MacOs,
        };
        let applied = for_start(Harness::Opencode, plane.path(), &machine, &|_| true)
            .expect("starts")
            .expect("sandboxed");
        let confinement = applied.confine().expect("confined").expect("a wrap");
        let line = line_in(&applied, &cwd, None, &confinement).expect("starts");

        let wrote = run(&line, &format!("echo ok > '{}/mine'", cwd.display()));
        assert!(wrote.status.success(), "{wrote:?}");
        let tmp = run(&line, "echo ok > \"$TMPDIR/scratch\"");
        assert!(tmp.status.success(), "{tmp:?}");

        let read = run(&line, &format!("cat '{}/secret'", vaults.display()));
        assert!(!read.status.success(), "read a vault: {read:?}");
        let outside = run(
            &line,
            &format!("echo x > '{}/x'", elsewhere.path().display()),
        );
        assert!(!outside.status.success(), "wrote outside: {outside:?}");
        let integrity = run(
            &line,
            &format!("mkdir -p '{}/.charter/app/x'", plane.path().display()),
        );
        assert!(
            !integrity.status.success(),
            "wrote integrity: {integrity:?}"
        );

        // `nc` is on every macOS: a direct connection to a port that is not the proxy's.
        let direct = run(&line, &format!("/usr/bin/nc -z -w 2 127.0.0.1 {port}"));
        assert!(
            !direct.status.success(),
            "connected past the proxy: {direct:?}"
        );
        let proxy = run(
            &line,
            &format!("/usr/bin/nc -z -w 2 127.0.0.1 {}", confinement.proxy_port()),
        );
        assert!(proxy.status.success(), "the proxy: {proxy:?}");
        // Its own SOCKS5 port, and never another chat's proxy (#1664).
        let socks = run(
            &line,
            &format!("/usr/bin/nc -z -w 2 127.0.0.1 {}", confinement.socks_port()),
        );
        assert!(socks.status.success(), "its SOCKS port: {socks:?}");
        let another = applied.confine().expect("confined").expect("a wrap");
        for port in another.proxy_ports() {
            let theirs = run(&line, &format!("/usr/bin/nc -z -w 2 127.0.0.1 {port}"));
            assert!(
                !theirs.status.success(),
                "reached another chat's proxy: {theirs:?}"
            );
        }
    }
    /// A home reached through a link (a dotfiles `~/.config`, a config root under `/var` or
    /// `/tmp`): every denial is still held, because Seatbelt matches the path as the kernel
    /// names it, and the profile writes each denial so. The human scopes' credentials stand for
    /// every class (FD-27).
    #[test]
    fn a_denial_under_a_linked_home_is_still_held() {
        let plane = plane_saying(ON);
        let cwd = plane.path().join("work");
        std::fs::create_dir(&cwd).expect("a workspace");
        let homes = tempfile::tempdir().expect("homes");
        let real_home = homes.path().join("real");
        let credentials = real_home.join(".config/charter/charterd");
        std::fs::create_dir_all(&credentials).expect("the credentials' directory");
        std::fs::write(credentials.join("local-ui"), "c0ffee").expect("a credential");
        let linked_home = homes.path().join("linked");
        std::os::unix::fs::symlink(&real_home, &linked_home).expect("a linked home");
        let machine = Machine {
            env: crate::secrets::Env::of(&[]),
            home: Some(linked_home.clone()),
            os: Os::MacOs,
        };
        let applied = for_start(Harness::Opencode, plane.path(), &machine, &|_| true)
            .expect("starts")
            .expect("sandboxed");
        let confinement = applied.confine().expect("confined").expect("a wrap");
        let line = line_in(&applied, &cwd, None, &confinement).expect("starts");
        // A control: the wrap reads what no class denies.
        let control = run(
            &line,
            &format!("cat '{}'", plane.path().join("charter.toml").display()),
        );
        assert!(control.status.success(), "{control:?}");

        for path in [
            linked_home.join(".config/charter/charterd/local-ui"),
            credentials.join("local-ui"),
        ] {
            let read = run(&line, &format!("cat '{}'", path.display()));
            assert!(
                !read.status.success(),
                "read {} under the wrap: {read:?}",
                path.display()
            );
        }
    }

    /// The dispatch records hold briefs and reports (#1452, D-1452-11): under the profile a
    /// chat can neither read one, list the store, nor write or remove one, under either name
    /// of the state folder. The record is a real one, opened as the app opens it.
    #[test]
    fn a_dispatch_record_is_neither_read_listed_nor_written_under_the_wrap() {
        use crate::dispatchrecord::{Asker, ChatRef, Mode, Opening, Place, Worker};

        let plane = plane_saying(ON);
        let cwd = plane.path().join("work");
        std::fs::create_dir(&cwd).expect("a workspace");
        let chat = |chat: u32| ChatRef {
            chat,
            id: None,
            name: format!("chat {chat}"),
            persona: Some("devops".to_owned()),
        };
        let record = crate::dispatchrecord::open(
            plane.path(),
            Opening {
                mode: Mode::Handoff,
                asker: Asker {
                    chat: chat(1),
                    ..Asker::default()
                },
                persona: Some("devops".to_owned()),
                worker: Worker {
                    chat: chat(2),
                    ..Worker::default()
                },
                task: None,
                place: Place::default(),
                brief: "a brief for devops alone".to_owned(),
                report_owed: false,
            },
            chrono::Utc::now(),
        )
        .expect("the app opens a record");
        let store = crate::dispatchrecord::dir(plane.path());
        let file = store.join(format!("{}.json", record.id));
        assert!(file.is_file(), "{}", file.display());
        // And the store under the state folder's other name, as a project mid-rename has it.
        let other = [".charter", ".purlis"]
            .iter()
            .map(|state| plane.path().join(state).join("app/dispatches"))
            .find(|dir| *dir != store)
            .expect("the other spelling");
        std::fs::create_dir_all(&other).expect("the other store");
        std::fs::write(other.join("planted.json"), "{}").expect("a file there");

        let home = tempfile::tempdir().expect("a home");
        let machine = Machine {
            env: crate::secrets::Env::of(&[]),
            home: Some(home.path().to_path_buf()),
            os: Os::MacOs,
        };
        let applied = for_start(Harness::Opencode, plane.path(), &machine, &|_| true)
            .expect("starts")
            .expect("sandboxed");
        let confinement = applied.confine().expect("confined").expect("a wrap");
        let line = line_in(&applied, &cwd, None, &confinement).expect("starts");
        // A control: the wrap reads what no class denies.
        let control = run(
            &line,
            &format!("cat '{}'", plane.path().join("charter.toml").display()),
        );
        assert!(control.status.success(), "{control:?}");

        for script in [
            format!("cat '{}'", file.display()),
            format!("ls '{}'", store.display()),
            format!("echo x >> '{}'", file.display()),
            format!("rm '{}'", file.display()),
            format!("echo '{{}}' > '{}/forged.json'", store.display()),
            format!("cat '{}/planted.json'", other.display()),
            format!("ls '{}'", other.display()),
        ] {
            let ran = run(&line, &script);
            assert!(!ran.status.success(), "`{script}` under the wrap: {ran:?}");
            assert!(
                !String::from_utf8_lossy(&ran.stdout).contains("a brief for devops alone"),
                "`{script}` read the brief: {ran:?}"
            );
        }
        // Still there, as the app wrote it.
        assert_eq!(
            crate::dispatchrecord::read(plane.path(), &record.id).map(|read| read.brief),
            Some("a brief for devops alone".to_owned())
        );
        assert!(!store.join("forged.json").exists());
    }

    /// What waits to be told to a chat on its next turn (D-T59-19) is NOT denied under the
    /// wrap: the wrap is around the whole harness, so the hook that delivers a report runs
    /// inside it, and it reads the report's file and then removes it. A report left for this
    /// chat by the app is taken under the wrap, as the hook takes it.
    #[test]
    fn a_report_left_for_a_wrapped_chat_can_still_be_taken_by_its_hook_under_the_wrap() {
        use crate::active::Place;
        use crate::handback::{self, For, Handback};

        let plane = plane_saying(ON);
        let cwd = plane.path().join("work");
        std::fs::create_dir(&cwd).expect("a workspace");
        let report = Handback {
            from: "check the queue".to_owned(),
            from_workspace: Place::Workspace("alpha".to_owned()),
            to: "steward 3".to_owned(),
            to_workspace: Place::Workspace("alpha".to_owned()),
            summary: "the queue is clear".to_owned(),
            task: None,
            answered: None,
            stopped: None,
        };
        handback::leave(plane.path(), For::Chat(3), &report).expect("the app leaves a report");
        let waiting = handback::dir(plane.path()).join("chat-3");
        let file = std::fs::read_dir(&waiting)
            .expect("the chat's reports")
            .flatten()
            .map(|entry| entry.path())
            .next()
            .expect("the report's file");

        let home = tempfile::tempdir().expect("a home");
        let machine = Machine {
            env: crate::secrets::Env::of(&[]),
            home: Some(home.path().to_path_buf()),
            os: Os::MacOs,
        };
        let applied = for_start(Harness::Opencode, plane.path(), &machine, &|_| true)
            .expect("starts")
            .expect("sandboxed");
        let confinement = applied.confine().expect("confined").expect("a wrap");
        let line = line_in(&applied, &cwd, None, &confinement).expect("starts");

        // What `handback::take` does, under the wrap: read the file, then remove it.
        let read = run(&line, &format!("cat '{}'", file.display()));
        assert!(read.status.success(), "{read:?}");
        assert!(
            String::from_utf8_lossy(&read.stdout).contains("the queue is clear"),
            "{read:?}"
        );
        let removed = run(&line, &format!("rm '{}'", file.display()));
        assert!(removed.status.success(), "{removed:?}");
        assert!(!file.exists());
    }

    /// A file denial does not stop a connect to a unix socket, which Seatbelt judges as
    /// network: the wrap refuses it because it allows no unix socket but the hook socket. So a
    /// socket in the credentials' directory, and one anywhere else, is refused, and the hook
    /// socket is reached (FD-27, ADR 0068 §5).
    #[test]
    fn the_wrap_connects_to_no_unix_socket_but_the_hook_socket() {
        let plane = plane_saying(ON);
        let cwd = plane.path().join("work");
        std::fs::create_dir(&cwd).expect("a workspace");
        let home = tempfile::tempdir().expect("a home");
        let credentials = home.path().join(".config/charter/charterd");
        std::fs::create_dir_all(&credentials).expect("the credentials' directory");
        let machine = Machine {
            env: crate::secrets::Env::of(&[]),
            home: Some(home.path().to_path_buf()),
            os: Os::MacOs,
        };
        let sockets = tempfile::tempdir().expect("sockets");
        let host = credentials.join("charterd.sock");
        let other = sockets.path().join("other.sock");
        let hook = sockets.path().join("hooks.sock");
        let _listening: Vec<std::os::unix::net::UnixListener> = [&host, &other, &hook]
            .into_iter()
            .map(|path| std::os::unix::net::UnixListener::bind(path).expect("a socket"))
            .collect();
        let applied = for_start(Harness::Opencode, plane.path(), &machine, &|_| true)
            .expect("starts")
            .expect("sandboxed");
        let confinement = applied.confine().expect("confined").expect("a wrap");
        let line = line_in(&applied, &cwd, Some(&hook), &confinement).expect("starts");
        let connects = |socket: &Path| {
            // `perl`, on every macOS: macOS's `nc -U` reports failure even on a connect that
            // succeeded.
            let probe = "IO::Socket::UNIX->new(Peer => $ARGV[0]) or exit 1";
            run(
                &line,
                &format!(
                    "/usr/bin/perl -MIO::Socket::UNIX -e '{probe}' '{}'",
                    socket.display()
                ),
            )
            .status
            .success()
        };
        assert!(connects(&hook), "the hook socket");
        assert!(!connects(&host), "charterd.sock was reached");
        assert!(!connects(&other), "another unix socket was reached");
    }

    /// `script` under the wrap, in `dir`, and whether it succeeded.
    fn ran_in(line: &Line, dir: &Path, script: &str) -> bool {
        run(line, &format!("cd '{}' && {script}", dir.display()))
            .status
            .success()
    }

    #[test]
    fn the_wrap_keeps_a_chat_from_writing_what_a_later_program_loads() {
        let plane = plane_saying(ON);
        let cwd = plane.path().join("work");
        let nested = cwd.join("clone");
        std::fs::create_dir_all(nested.join(".git/hooks")).expect("a nested clone");
        std::fs::write(nested.join(".git/config"), "[core]\n").expect("its config");
        let home = tempfile::tempdir().expect("a home");
        let data = home.path().join(".local/share/opencode");
        let snapshot = data.join("snapshot/proj/abc");
        std::fs::create_dir_all(snapshot.join("objects")).expect("a snapshot");
        std::fs::write(snapshot.join("config"), "[core]\n").expect("its config");
        let machine = Machine {
            env: crate::secrets::Env::of(&[]),
            home: Some(home.path().to_path_buf()),
            os: Os::MacOs,
        };
        let applied = for_start(Harness::Opencode, plane.path(), &machine, &|_| true)
            .expect("starts")
            .expect("sandboxed");
        let confinement = applied.confine().expect("confined").expect("a wrap");
        let line = line_in(&applied, &cwd, None, &confinement).expect("starts");

        for refused in [
            "echo x >> clone/.git/config",
            "echo x > clone/.git/hooks/post-commit",
            "mv clone/.git clone/.git-old",
            "mkdir -p x && mv x clone2 && mkdir clone2/.git",
            "echo {} > .mcp.json",
            "mkdir -p .vscode",
            "echo {} > opencode.json",
            "mkdir -p .claude && echo {} > .claude/settings.local.json",
            "mkdir -p .claude/skills/x",
            "mkdir -p .opencode",
            "mkdir -p .codex",
            "mkdir -p .agents/skills/x",
            "echo {} > tui.json",
            "echo x > .zshrc",
            "echo x > charter.toml",
        ] {
            assert!(!ran_in(&line, &cwd, refused), "{refused} was let through");
        }
        assert!(ran_in(
            &line,
            &cwd,
            "echo ok > ordinary && echo x > clone/.git/objects-ok"
        ));

        let data_ok = format!(
            "cd '{}' && echo x > log/l && echo x > opencode.db",
            data.display()
        );
        std::fs::create_dir_all(data.join("log")).expect("a log");
        assert!(run(&line, &data_ok).status.success(), "a turn's own writes");
        for refused in [
            "echo {} > auth.json",
            "echo x >> snapshot/proj/abc/config",
            "echo x > snapshot/proj/abc/objects/o",
            "mkdir snapshot/proj/new",
            "mv snapshot/proj/abc snapshot/proj/abd",
            "ln -s /tmp log/link",
            "echo x > plugin.ts",
        ] {
            assert!(!ran_in(&line, &data, refused), "{refused} was let through");
        }
        // Nor is one of opencode's own directories moved out to be changed and moved back, nor
        // a link moved in where a later opencode writes.
        let config = home.path().join(".config/opencode");
        for refused in [
            format!("mv '{}' moved-data", data.display()),
            format!("mv '{}' moved-config", config.display()),
            format!("mv '{}' moved-repos", data.join("repos").display()),
            format!(
                "ln -s /tmp link && mv link '{}'",
                config.join(".gitignore").display()
            ),
        ] {
            assert!(!ran_in(&line, &cwd, &refused), "{refused} was let through");
        }
    }

    #[test]
    fn a_wrapped_chat_writes_a_manifest_only_where_it_cannot_change_a_chats_sandbox() {
        // #1336: a chat in `ws/repo` may write a manifest below its folder (a clone's fixture)
        // and in its temp folder, and never at the root, in `ws` or in its own folder, nor move
        // one there. The plane is reached through the temp folder's link (`/var` on macOS), so
        // the profile holds the folders by the names the kernel gives them.
        let plane = plane_saying(ON);
        let repo = plane.path().join("ws/repo");
        std::fs::create_dir_all(&repo).expect("a repo");
        let home = tempfile::tempdir().expect("a home");
        let machine = Machine {
            env: crate::secrets::Env::of(&[]),
            home: Some(home.path().to_path_buf()),
            os: Os::MacOs,
        };
        let applied = for_start(Harness::Opencode, plane.path(), &machine, &|_| true)
            .expect("starts")
            .expect("sandboxed");
        let confinement = applied.confine().expect("confined").expect("a wrap");
        let line = line_in(&applied, &repo, None, &confinement).expect("starts");
        for name in MANIFESTS {
            for allowed in [
                format!("mkdir -p sub && echo x > sub/{name}"),
                format!("mkdir -p \"$TMPDIR/x\" && echo x > \"$TMPDIR/x/{name}\""),
            ] {
                assert!(ran_in(&line, &repo, &allowed), "{allowed} was refused");
            }
            for refused in [
                format!("echo x > {name}"),
                format!("echo x > ../{name}"),
                format!("echo x > ../../{name}"),
                format!("mv sub/{name} {name}"),
            ] {
                assert!(!ran_in(&line, &repo, &refused), "{refused} was let through");
            }
            assert!(!repo.join(name).exists(), "{name}");
        }
    }

    #[test]
    fn a_root_manifest_that_is_a_link_is_held_by_its_own_name() {
        // #1336: an operator may keep `purlis.local.toml` as a link into a dotfiles repository.
        // A plane-root chat may neither remove the link nor put a file of its own in its place.
        let plane = plane_saying(ON);
        let dotfiles = tempfile::tempdir().expect("a dotfiles repository");
        let target = dotfiles.path().join("purlis.local.toml");
        std::fs::write(&target, "").expect("the operator's settings");
        let link = plane.path().join("purlis.local.toml");
        std::os::unix::fs::symlink(&target, &link).expect("a linked manifest");
        let home = tempfile::tempdir().expect("a home");
        let machine = Machine {
            env: crate::secrets::Env::of(&[]),
            home: Some(home.path().to_path_buf()),
            os: Os::MacOs,
        };
        let applied = for_start(Harness::Opencode, plane.path(), &machine, &|_| true)
            .expect("starts")
            .expect("sandboxed");
        let confinement = applied.confine().expect("confined").expect("a wrap");
        let line = line_in(&applied, plane.path(), None, &confinement).expect("starts");
        for refused in [
            "rm purlis.local.toml",
            "echo x > purlis.local.toml",
            "mv purlis.local.toml moved",
            "rm purlis.local.toml; printf '[chat_env]\\n' > purlis.local.toml",
            "ln -sf /tmp/elsewhere purlis.local.toml",
        ] {
            assert!(
                !ran_in(&line, plane.path(), refused),
                "{refused} was let through"
            );
            assert_eq!(
                std::fs::read_link(&link).ok(),
                Some(target.clone()),
                "{refused}"
            );
        }
        assert_eq!(std::fs::read_to_string(&target).expect("kept"), "");
    }

    #[test]
    fn opencodes_own_directories_are_made_before_the_wrap_so_a_first_run_need_not_make_them() {
        // Measured: on a home with no `~/.local`, opencode stops at its first `mkdir`. The wrap
        // lets it make none of them, since a directory made could be one moved into place.
        let plane = plane_saying(ON);
        let home = tempfile::tempdir().expect("a home");
        let machine = Machine {
            env: crate::secrets::Env::of(&[]),
            home: Some(home.path().to_path_buf()),
            os: Os::MacOs,
        };
        let applied = for_start(Harness::Opencode, plane.path(), &machine, &|_| true)
            .expect("starts")
            .expect("sandboxed");
        let confinement = applied.confine().expect("confined").expect("a wrap");
        line_in(&applied, plane.path(), None, &confinement).expect("starts");
        for dir in [
            ".local/share/opencode",
            ".local/share/opencode/repos",
            ".local/share/opencode/log",
            ".local/share/opencode/storage",
            ".config/opencode",
            ".cache/opencode",
            ".cache/opencode/bin",
        ] {
            assert!(home.path().join(dir).is_dir(), "{dir} was not made");
        }
    }
}

// -------------------------------------------------------------------------------------
// Every class, for every harness charter compiles the policy for (ruling V73b)
// -------------------------------------------------------------------------------------

/// What `harness`'s compiled sandbox for a chat in `cwd` denies writing, and denies reading,
/// as each states it.
struct Stated {
    denies_write: Box<dyn Fn(&str) -> bool>,
    denies_read: Box<dyn Fn(&str) -> bool>,
}

fn stated(harness: Harness, plane: &Path, cwd: &Path, machine: &Machine) -> Stated {
    let applied = compiled_anyway(harness, plane, machine).expect("compiles");
    let confinement = applied.confine().expect("confined");
    let line = applied
        .line(
            Words {
                program: "harness".to_owned(),
                command: Vec::new(),
                armed: Vec::new(),
                charters: Vec::new(),
            },
            &At {
                cwd: Some(cwd),
                hook_socket: None,
                confinement: confinement.as_ref(),
                no_opt_out: false,
            },
        )
        .expect("starts");
    match applied.form() {
        Form::ClaudeCode(settings) => {
            let list = |key: &str| -> Vec<String> {
                settings.sandbox["filesystem"][key]
                    .as_array()
                    .expect("a list")
                    .iter()
                    .map(|it| it.as_str().expect("a path").to_owned())
                    .collect()
            };
            let (read, write) = (list("denyRead"), list("denyWrite"));
            Stated {
                denies_write: Box::new(move |path| write.iter().any(|it| it == path)),
                denies_read: Box::new(move |path| read.iter().any(|it| it == path)),
            }
        }
        // Both are charter's own profile.
        Form::Codex(_) | Form::Opencode(_) => {
            let profile = line.args[1].clone();
            let read = profile.clone();
            Stated {
                denies_write: Box::new(move |path| profile.contains(path)),
                denies_read: Box::new(move |path| {
                    read.contains(&format!(
                        "(deny file-read* file-write* (subpath \"{path}\"))"
                    ))
                }),
            }
        }
    }
}

/// The human client scopes' credentials, and `charterd.sock` beside them, are one directory
/// every harness charter compiles a sandbox for denies a chat reading and writing (FD-27, V16a,
/// ADR 0067 §5 class 3): a chat never holds the credential of `local-ui`, `terminal`,
/// `fleet-mcp`, `approval` or `editor`.
#[test]
fn every_harness_denies_a_chat_the_human_scopes_credentials() {
    let plane = plane_saying(ON);
    let root = real(plane.path());
    let home = tempfile::tempdir().expect("a home");
    let home_dir = real(home.path());
    let machine = Machine {
        env: crate::secrets::Env::of(&[]),
        home: Some(home_dir.clone()),
        os: Os::MacOs,
    };
    let credentials = home_dir.join(".config/charter/charterd");
    assert_eq!(
        crate::machine::charterd(&home_dir.join(".config")),
        credentials,
        "where charterd keeps them"
    );
    let path = credentials.display().to_string();
    let mut held = 0;
    for harness in Harness::ALL {
        let Some(_) = compiler(harness) else { continue };
        let stated = stated(harness, &root, &root, &machine);
        assert!(
            (stated.denies_read)(&path),
            "{harness:?} lets a chat read {path}"
        );
        assert!(
            (stated.denies_write)(&path),
            "{harness:?} lets a chat write {path}"
        );
        held += 1;
    }
    assert_eq!(held, 3, "Claude Code, Codex and opencode each hold it");
}

/// A home reached through a link: every harness denies the credentials' directory by the path
/// the kernel names it by, which is what a sandbox matches, so a linked `~/.config` (or a config
/// root under `/var` or `/tmp`) cannot make a denial match nothing (FD-27).
#[test]
fn every_harness_denies_the_credentials_under_a_linked_home_by_their_real_path() {
    let plane = plane_saying(ON);
    let root = real(plane.path());
    let homes = tempfile::tempdir().expect("homes");
    let real_home = real(homes.path()).join("real");
    std::fs::create_dir_all(real_home.join(".config/charter/charterd")).expect("the directory");
    let linked_home = homes.path().join("linked");
    std::os::unix::fs::symlink(&real_home, &linked_home).expect("a linked home");
    let machine = Machine {
        env: crate::secrets::Env::of(&[]),
        home: Some(linked_home.clone()),
        os: Os::MacOs,
    };
    let path = real_home
        .join(".config/charter/charterd")
        .display()
        .to_string();
    // Claude Code's own sandbox and its tools may compare the name as written, so its settings
    // name both (D-FD27k). Codex and opencode run in charter's wrap, which Seatbelt matches by
    // the kernel's name alone.
    let as_written = linked_home
        .join(".config/charter/charterd")
        .display()
        .to_string();
    for harness in Harness::ALL {
        let Some(_) = compiler(harness) else { continue };
        let stated = stated(harness, &root, &root, &machine);
        let mut names = vec![&path];
        if harness == Harness::ClaudeCode {
            names.push(&as_written);
        }
        for path in names {
            assert!(
                (stated.denies_read)(path),
                "{harness:?} lets a chat read {path}"
            );
            assert!(
                (stated.denies_write)(path),
                "{harness:?} lets a chat write {path}"
            );
        }
    }
}

/// Each compiler states every class: Claude Code's in its own sandbox's settings, Codex's and
/// opencode's in charter's own profile (#1123).
#[test]
fn every_denial_class_is_held_by_every_harness_charter_compiles_for() {
    let plane = plane_saying(ON);
    let root = real(plane.path());
    let home = tempfile::tempdir().expect("a home");
    // As the kernel names it, which is how charter's own profile writes every path.
    let home_dir = real(home.path());
    let machine = Machine {
        env: crate::secrets::Env::of(&[]),
        home: Some(home_dir.clone()),
        os: Os::MacOs,
    };
    let config_dir = home_dir.join(".config/charter");
    let mut held = 0;
    for harness in Harness::ALL {
        let Some(_) = compiler(harness) else { continue };
        let stated = stated(harness, &root, &root, &machine);
        for class in Class::ALL {
            // Each class's representative: the path a test of its own names, and whether it is
            // denied reading as well as writing.
            let (path, read_too) = match class {
                Class::Vaults => (root.join(".charter/vaults").display().to_string(), true),
                Class::Integrity => (root.join(".charter/app").display().to_string(), false),
                Class::HumanPowers => (config_dir.display().to_string(), false),
                // Off a runner it names nothing (RR-5), which its own test holds.
                Class::RunnerInternals => continue,
                Class::LaterCode => match harness {
                    Harness::ClaudeCode => ("**/.git/config".to_owned(), false),
                    Harness::Codex | Harness::Opencode => {
                        let escaped = root.display().to_string().replace('.', "\\\\.");
                        (format!("^{escaped}/(.*/)?\\\\.git/config(/.*)?$"), false)
                    }
                },
            };
            assert!(
                (stated.denies_write)(&path),
                "{harness:?} lets a chat write {class:?}'s {path}"
            );
            if read_too {
                assert!(
                    (stated.denies_read)(&path),
                    "{harness:?} lets a chat read {class:?}'s {path}"
                );
            }
            held += 1;
        }
    }
    assert_eq!(held, 12, "four classes, for each of three harnesses");
}

/// Round three of the review (rulings V73a and V73d), applied for real.
#[cfg(target_os = "macos")]
mod live_round_three {
    use super::*;
    use std::process::Command;

    fn ran(line: &Line, script: &str) -> bool {
        crate::forklock::output(
            Command::new(&line.program)
                .args(&line.args[..2])
                .args(["/bin/sh", "-c", script])
                .envs(line.env.iter().map(|(k, v)| (k.as_str(), v.as_str()))),
        )
        .expect("sandbox-exec runs")
        .status
        .success()
    }

    #[test]
    fn nothing_a_later_program_runs_can_be_built_anywhere_the_chat_writes_or_named_by_config() {
        let plane = plane_saying(ON);
        let cwd = plane.path().join("work");
        let clone = cwd.join("clone");
        std::fs::create_dir_all(clone.join(".git/modules/sub/hooks")).expect("a clone");
        std::fs::write(clone.join(".git/modules/sub/HEAD"), "ref: x\n").expect("a submodule");
        std::fs::write(clone.join(".git/modules/sub/config"), "[core]\n").expect("its config");
        std::fs::write(
            clone.join(".git/config"),
            "[core]\n\thooksPath = tools/hooks\n",
        )
        .expect("a hooks path");
        std::fs::create_dir_all(clone.join("tools/hooks")).expect("the hooks directory");
        std::fs::create_dir_all(cwd.join(".claude")).expect(".claude");
        std::fs::write(
            cwd.join(".claude/settings.json"),
            r#"{"hooks": {"Stop": [{"hooks": [{"type": "command", "command": "sh scripts/stop.sh"}]}]}}"#,
        )
        .expect("settings");
        std::fs::create_dir_all(cwd.join("scripts")).expect("scripts");
        let home = tempfile::tempdir().expect("a home");
        let state = home.path().join(".local/state/opencode");
        std::fs::create_dir_all(&state).expect("opencode's state");
        let machine = Machine {
            env: crate::secrets::Env::of(&[]),
            home: Some(home.path().to_path_buf()),
            os: Os::MacOs,
        };
        let applied = for_start(Harness::Opencode, plane.path(), &machine, &|_| true)
            .expect("starts")
            .expect("sandboxed");
        let confinement = applied.confine().expect("confined").expect("a wrap");
        let line = line_in(&applied, &cwd, None, &confinement).expect("starts");
        let data = home.path().join(".local/share/opencode");
        let at = |dir: &Path, script: &str| format!("cd '{}' && {script}", dir.display());

        for refused in [
            // A submodule's git directory, at any depth.
            at(&clone, "echo x >> .git/modules/sub/config"),
            at(&clone, "echo x > .git/modules/sub/hooks/post-checkout"),
            at(&clone, "mkdir -p .git/modules/a/modules/b/hooks"),
            // What `core.hooksPath` names, and the hook managers' directories.
            at(&clone, "echo x > tools/hooks/pre-commit"),
            at(&cwd, "mkdir .husky"),
            at(&cwd, "mkdir .githooks"),
            // A script a harness's project config runs.
            at(&cwd, "echo x > scripts/stop.sh"),
            // Built in the temp directory, to be moved in with its parent.
            "mkdir -p \"$TMPDIR/p/.git\"".to_owned(),
            "mkdir -p \"$TMPDIR/q\" && mkdir \"$TMPDIR/q/.claude\"".to_owned(),
            // Built in a granted data directory.
            at(&data, "mkdir log/p"),
            // opencode's own state, which a later opencode reads, and its data directories.
            at(&state, "echo x > kv.json"),
            format!("mv '{}' '{}/moved'", state.display(), cwd.display()),
            at(&data, "mv log moved-log"),
            at(
                &cwd,
                &format!(
                    "mkdir d && ln -s /etc d/l && mv d '{}/log/d'",
                    data.display()
                ),
            ),
        ] {
            assert!(!ran(&line, &refused), "{refused} was let through");
        }
        assert!(ran(
            &line,
            &at(&clone, "echo x > .git/modules/sub/objects-ok")
        ));
        assert!(ran(&line, &at(&data, "echo x > log/l")));
        assert!(ran(
            &line,
            "mkdir -p \"$XDG_STATE_HOME/opencode/locks/a.lock\""
        ));
    }
}

/// What a plane-root chat could otherwise move away with a denied path inside it.
#[cfg(target_os = "macos")]
mod live_pinned {
    use super::*;
    use std::process::Command;

    fn ran(line: &Line, script: &str) -> bool {
        crate::forklock::output(
            Command::new(&line.program)
                .args(&line.args[..2])
                .args(["/bin/sh", "-c", script])
                .envs(line.env.iter().map(|(k, v)| (k.as_str(), v.as_str()))),
        )
        .expect("sandbox-exec runs")
        .status
        .success()
    }

    #[test]
    fn a_plane_root_chat_cannot_move_charters_state_aside_to_write_it() {
        let plane = plane_saying(ON);
        std::fs::create_dir_all(plane.path().join(".charter/app")).expect(".charter/app");
        std::fs::create_dir_all(plane.path().join(".charter/sessions")).expect("sessions");
        std::fs::write(plane.path().join(".charter/app/record.json"), "{}").expect("a record");
        let home = tempfile::tempdir().expect("a home");
        let machine = Machine {
            env: crate::secrets::Env::of(&[]),
            home: Some(home.path().to_path_buf()),
            os: Os::MacOs,
        };
        let applied = for_start(Harness::Opencode, plane.path(), &machine, &|_| true)
            .expect("starts")
            .expect("sandboxed");
        let confinement = applied.confine().expect("confined").expect("a wrap");
        let line = line_in(&applied, plane.path(), None, &confinement).expect("starts");
        let at = |script: &str| format!("cd '{}' && {script}", plane.path().display());
        // What is in it, and not denied, is still the chat's to write.
        assert!(ran(&line, &at("echo x > .charter/sessions/ok")));
        for refused in [
            "mv .charter moved",
            "rm -rf .charter && test ! -e .charter",
            "mv .charter moved && ln -s /tmp .charter",
            "ln -s /tmp link && mv -f link .charter && test -L .charter",
            // The chat's own directory moved whole to its temp directory, written there, and
            // moved back: every path rule under it would follow it out.
            "here=$(pwd -P) && mv \"$here\" \"$TMPDIR/moved\" && echo x > \"$TMPDIR/moved/.charter/app/record.json\" && mv \"$TMPDIR/moved\" \"$here\"",
        ] {
            assert!(!ran(&line, &at(refused)), "{refused} was let through");
        }
        assert_eq!(
            std::fs::read_to_string(plane.path().join(".charter/app/record.json")).expect("record"),
            "{}"
        );
    }
}

/// `charter.local.toml` is what the machine adds to the plane, and charter reads it unsandboxed
/// at every later start (`[chat_env] pass`, plugins, extensions): no harness writes the root's.
#[test]
fn every_harness_is_kept_from_writing_the_roots_charter_local_toml() {
    let plane = plane_saying(ON);
    let root = real(plane.path());
    let home = tempfile::tempdir().expect("a home");
    let machine = Machine {
        env: crate::secrets::Env::of(&[]),
        home: Some(real(home.path())),
        os: Os::MacOs,
    };
    for harness in Harness::ALL {
        let stated = stated(harness, &root, &root, &machine);
        // Held by path at the root (#1336), no longer by name at any depth.
        let path = match harness {
            Harness::ClaudeCode => root.join("charter.local.toml").display().to_string(),
            Harness::Codex | Harness::Opencode => format!(
                "(deny file-write* (subpath \"{}\"))",
                root.join("charter.local.toml").display()
            ),
        };
        assert!(
            (stated.denies_write)(&path),
            "{harness:?} lets a chat write {path}"
        );
    }
}

#[cfg(target_os = "macos")]
#[test]
fn a_plane_root_opencode_chat_cannot_write_charter_local_toml() {
    let plane = plane_saying(ON);
    let home = tempfile::tempdir().expect("a home");
    let machine = Machine {
        env: crate::secrets::Env::of(&[]),
        home: Some(home.path().to_path_buf()),
        os: Os::MacOs,
    };
    let applied = for_start(Harness::Opencode, plane.path(), &machine, &|_| true)
        .expect("starts")
        .expect("sandboxed");
    let confinement = applied.confine().expect("confined").expect("a wrap");
    let line = line_in(&applied, plane.path(), None, &confinement).expect("starts");
    let wrote = crate::forklock::output(
        std::process::Command::new(&line.program)
            .args(&line.args[..2])
            .args([
                "/bin/sh",
                "-c",
                &format!(
                    "cd '{}' && echo '[chat_env]' > charter.local.toml",
                    plane.path().display()
                ),
            ])
            .envs(line.env.iter().map(|(k, v)| (k.as_str(), v.as_str()))),
    )
    .expect("sandbox-exec runs");
    assert!(!wrote.status.success(), "{wrote:?}");
    assert!(!plane.path().join("charter.local.toml").exists());
}
