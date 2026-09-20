//! One way to write a program a test is about to run.
//!
//! Charter's tests are full of stand-ins: a `git` that refuses, a `claude` that answers the
//! wiring probe, a `gh` that replays recorded JSON, an fsmonitor that touches a marker. Every
//! one of them is a shell script the test writes and then something — charter, or git, or the
//! test itself — runs. Written the obvious way, that loses to `ETXTBSY`, *Text file busy*,
//! and it did: charter-app#81 and charter-app#39.
//!
//! **`ETXTBSY` is a property of the inode, not of the name.** The kernel refuses to `execve`
//! a file that any process holds open for writing, and it refuses to open for writing a file
//! any process is executing. Both directions bite here, and they are not the same bug.
//!
//! **Running a program this process wrote (charter-app#81).** `fs::write` closes its own
//! descriptor before it returns, so the test is not the one still holding it — but `cargo
//! test` runs the tests in one binary on many threads, and *every* `Command::spawn` on any of
//! those threads forks. A fork copies the descriptor table, so a child forked while this
//! thread had the script open for writing holds a copy of that descriptor until it reaches
//! its own `execve`, microseconds later. Exec the script inside that window and the kernel
//! answers `ETXTBSY`. A rename does **not** close that window: the descriptor the child holds
//! is on the inode, and renaming hands exec the same inode under another name. What closes it
//! is never opening the program for writing in this process at all — a fork cannot copy a
//! descriptor the forking process does not have. So the bytes are written by a child
//! (`/bin/sh`), which this function waits out, and afterwards no descriptor for that inode
//! exists anywhere.
//!
//! **Writing over a program that is running (charter-app#39).** The app's stand-in `claude`
//! ends in `sleep 600`, so an earlier chat is still executing it when the next test rewrites
//! it, and writing a running program is `ETXTBSY` too. That one a rename does fix: it
//! replaces the directory entry and leaves the running inode alone. Measured there at two
//! failures in five runs.
//!
//! Hence both halves below, and both are load-bearing: the child writes it (so exec never
//! races a descriptor), and a rename puts it in place (so the write never races an exec).

use std::path::{Path, PathBuf};

/// Writes `contents` as `dir/name`, makes it runnable, and returns its path.
///
/// `contents` is the whole file, shebang included — this is not a script template. It travels
/// to `/bin/sh` as an argument, so it has to fit in `ARG_MAX` (a megabyte and more on every
/// platform charter builds for) and hold no NUL byte. Both hold for a stand-in.
///
/// Panics rather than returning an error: every caller is a test, and a stand-in that could
/// not be written has nothing to say about the subject.
#[cfg(unix)]
pub fn program(dir: &Path, name: &str, contents: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt as _;

    let path = dir.join(name);
    // Beside the program, never in a temp directory of its own: `rename` is only atomic —
    // only a rename at all — within one filesystem, and `/tmp` need not be the one the
    // caller's directory is on.
    let parent = path
        .parent()
        .unwrap_or_else(|| panic!("{} has a parent directory", path.display()));
    let stem = path
        .file_name()
        .unwrap_or_else(|| panic!("{} names a file", path.display()))
        .to_string_lossy()
        .into_owned();
    let beside = parent.join(format!(
        ".{stem}.{}.{}.writing",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("a clock after 1970")
            .as_nanos()
    ));

    // The write happens in the CHILD, which is the whole point: this process opens no
    // descriptor on the inode it is about to hand to `execve`, so there is none for a fork on
    // another thread to copy. `printf %s` is a builtin in every /bin/sh charter runs on
    // (dash on the Linux runners, bash in sh mode on macOS), so nothing is looked up on PATH
    // and `env_clear` cannot starve it. The redirection, not the argument, is what writes:
    // `%s` is the format and the script is data, so a script full of `%` or `\` arrives
    // unchanged.
    let wrote = std::process::Command::new("/bin/sh")
        .arg("-c")
        .arg(r#"printf %s "$1" > "$2""#)
        .arg("sh")
        .arg(contents)
        .arg(&beside)
        .env_clear()
        .status()
        .expect("/bin/sh runs");
    assert!(
        wrote.success(),
        "the stand-in {} was not written: {wrote}",
        path.display()
    );

    // `chmod` takes a path and opens nothing, so it is safe to do from here.
    std::fs::set_permissions(&beside, std::fs::Permissions::from_mode(0o755))
        .unwrap_or_else(|e| panic!("{} is made runnable: {e}", beside.display()));
    std::fs::rename(&beside, &path)
        .unwrap_or_else(|e| panic!("{} is put in place: {e}", path.display()));
    path
}
