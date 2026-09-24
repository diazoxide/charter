//! Putting the app's own `charter` on a terminal's `PATH` — on an explicit action, never
//! silently.
//!
//! **Spec decision 21**: the installer puts `charter` on `PATH`. The `.deb` does, because
//! `dpkg` lays `/usr/bin/charter` down. Nothing did on macOS: the binary is at
//! `charter.app/Contents/MacOS/charter`, which no shell searches, so a terminal found either
//! no `charter` or the Python one. This is the standard answer to that, the one VS Code gives
//! with "Shell Command: Install 'code' command in PATH": a symlink in `/usr/local/bin`, made
//! when the operator asks for it, with the operating system's own password prompt when that
//! directory is not theirs to write. A symlink and not a copy, so the command keeps pointing
//! at the app's binary through every update the app installs in place.
//!
//! **Never over somebody else's `charter`.** A regular file, or a link to anything that is not
//! an app's bundled binary, is refused with a sentence naming it — it is most likely the Python
//! charter, and replacing it is the operator's decision, not a side effect of a menu item. A
//! link this action made before, to this app or to an older copy of it, is replaced.
//!
//! The decision is here and pure; the one privileged step (`osascript … with administrator
//! privileges`) is the app's, because it is a dialog on the operator's screen.

use std::path::{Path, PathBuf};

/// Where the command goes on macOS: a directory every shell on the platform searches, and the
/// one VS Code, Homebrew on Intel and most `.pkg` installers use.
pub const MACOS_DIR: &str = "/usr/local/bin";

/// The command's name.
pub const COMMAND: &str = "charter";

/// What putting the command at `link` would do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Plan {
    /// `link` already points at `binary`. Nothing to do.
    AlreadyThere,
    /// Make the link: nothing is there, or a link an app made before is.
    Link,
    /// Something that is not the app's is there, and it stays.
    Refused(String),
}

/// What is at `link`, and whether making it point at `binary` is this action's to do.
pub fn plan(link: &Path, binary: &Path) -> Plan {
    let Ok(meta) = std::fs::symlink_metadata(link) else {
        return Plan::Link;
    };
    if !meta.file_type().is_symlink() {
        return Plan::Refused(format!(
            "{} is already there and is not the app's — most likely a charter installed another \
             way. Remove it first if you want this one on PATH; nothing was changed.",
            link.display()
        ));
    }
    let Ok(points_at) = std::fs::read_link(link) else {
        return Plan::Link;
    };
    if points_at == binary {
        return Plan::AlreadyThere;
    }
    if an_apps_binary(&points_at) || (!points_at.exists() && !link.exists()) {
        return Plan::Link;
    }
    Plan::Refused(format!(
        "{} already points at {}, which is not this app's. Remove it first if you want this one \
         on PATH; nothing was changed.",
        link.display(),
        points_at.display()
    ))
}

/// Whether `path` is the `charter` inside a macOS app bundle — the one thing this action ever
/// links to, so a link to one is a link this action made.
fn an_apps_binary(path: &Path) -> bool {
    let mut parts = path.components().rev();
    let named = |part: Option<std::path::Component<'_>>, want: &str| {
        part.is_some_and(|p| p.as_os_str() == want)
    };
    named(parts.next(), COMMAND)
        && named(parts.next(), "MacOS")
        && named(parts.next(), "Contents")
        && parts
            .next()
            .is_some_and(|p| p.as_os_str().to_string_lossy().ends_with(".app"))
}

/// What a link attempt did.
#[derive(Debug)]
pub enum Linked {
    /// Done, with the sentence to say.
    Done(String),
    /// Refused, with the sentence to say.
    Refused(String),
    /// The directory is not this user's to write: the operating system has to ask.
    NeedsAdmin,
}

/// Put `binary` at `link` as a symlink, as far as this user can.
pub fn link(link: &Path, binary: &Path) -> Linked {
    match plan(link, binary) {
        Plan::AlreadyThere => return Linked::Done(said(link, binary, true)),
        Plan::Refused(why) => return Linked::Refused(why),
        Plan::Link => {}
    }
    if let Some(dir) = link.parent()
        && let Err(e) = std::fs::create_dir_all(dir)
    {
        return failed(link, &e);
    }
    // A link this action made before is replaced: removed, then made again. Not `rename` over
    // it, which needs a temporary name in a directory that may not be ours.
    if std::fs::symlink_metadata(link).is_ok()
        && let Err(e) = std::fs::remove_file(link)
    {
        return failed(link, &e);
    }
    match make_symlink(binary, link) {
        Ok(()) => Linked::Done(said(link, binary, false)),
        Err(e) => failed(link, &e),
    }
}

/// One function with the platform inside it, rather than two under `#[cfg]`: a unix build then
/// compiles every mutation of it, and the tests here see each one (#311).
fn make_symlink(binary: &Path, link: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(binary, link)
    }
    #[cfg(not(unix))]
    {
        let _ = (binary, link);
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "charter puts itself on PATH on macOS only",
        ))
    }
}

fn failed(link: &Path, e: &std::io::Error) -> Linked {
    if e.kind() == std::io::ErrorKind::PermissionDenied {
        return Linked::NeedsAdmin;
    }
    Linked::Refused(format!(
        "charter could not put itself at {} ({e}); nothing was changed.",
        link.display()
    ))
}

/// The sentence a link that is in place is reported with.
pub fn said(link: &Path, binary: &Path, already: bool) -> String {
    format!(
        "{} {} → {}. A new terminal finds `charter` there.",
        if already {
            "Already on PATH:"
        } else {
            "On PATH:"
        },
        link.display(),
        binary.display()
    )
}

/// Other `charter`s a terminal could find before this one, for the sentence after a link.
///
/// Only the user directories charter already knows a shell puts on `PATH`
/// ([`crate::programs::USER_BIN`]): which of those comes first is the operator's shell's
/// decision, and charter cannot read it without running their dotfiles. So it names them and
/// says what decides.
pub fn others(home: Option<&Path>, link: &Path) -> Vec<PathBuf> {
    let Some(home) = home else {
        return Vec::new();
    };
    crate::programs::USER_BIN
        .iter()
        .map(|dir| home.join(dir).join(COMMAND))
        .filter(|other| other != link && std::fs::symlink_metadata(other).is_ok())
        .collect()
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn app(dir: &Path) -> PathBuf {
        let binary = dir.join("charter.app/Contents/MacOS/charter");
        std::fs::create_dir_all(binary.parent().unwrap()).unwrap();
        std::fs::write(&binary, "").unwrap();
        binary
    }

    #[test]
    fn an_empty_place_gets_a_link_to_the_apps_binary() {
        let dir = tempfile::tempdir().unwrap();
        let binary = app(dir.path());
        let link = dir.path().join("bin/charter");

        let Linked::Done(said) = link_it(&link, &binary) else {
            panic!("not linked");
        };
        assert_eq!(std::fs::read_link(&link).unwrap(), binary);
        assert!(said.starts_with("On PATH:"), "{said}");
    }

    #[test]
    fn a_link_already_there_is_left_and_said() {
        let dir = tempfile::tempdir().unwrap();
        let binary = app(dir.path());
        let link = dir.path().join("charter");
        std::os::unix::fs::symlink(&binary, &link).unwrap();

        assert_eq!(plan(&link, &binary), Plan::AlreadyThere);
        let Linked::Done(said) = link_it(&link, &binary) else {
            panic!("not done");
        };
        assert!(said.starts_with("Already on PATH:"), "{said}");
    }

    #[test]
    fn a_link_to_an_older_copy_of_the_app_is_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let old = app(&dir.path().join("old"));
        let binary = app(dir.path());
        let link = dir.path().join("charter");
        std::os::unix::fs::symlink(&old, &link).unwrap();

        assert_eq!(plan(&link, &binary), Plan::Link);
        assert!(matches!(link_it(&link, &binary), Linked::Done(_)));
        assert_eq!(std::fs::read_link(&link).unwrap(), binary);
    }

    #[test]
    fn a_charter_somebody_else_installed_is_never_replaced() {
        // Most likely the Python charter. Replacing it is the operator's decision.
        let dir = tempfile::tempdir().unwrap();
        let binary = app(dir.path());
        let theirs = dir.path().join("charter");
        std::fs::write(&theirs, "#!/bin/sh\n").unwrap();

        let Linked::Refused(why) = link_it(&theirs, &binary) else {
            panic!("replaced somebody else's charter");
        };
        assert!(why.contains("nothing was changed"), "{why}");
        assert_eq!(std::fs::read(&theirs).unwrap(), b"#!/bin/sh\n");
    }

    #[test]
    fn a_link_to_somebody_elses_charter_is_never_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let binary = app(dir.path());
        let python = dir.path().join("venv/bin/charter");
        std::fs::create_dir_all(python.parent().unwrap()).unwrap();
        std::fs::write(&python, "").unwrap();
        let link = dir.path().join("charter");
        std::os::unix::fs::symlink(&python, &link).unwrap();

        assert!(matches!(plan(&link, &binary), Plan::Refused(_)));
        assert_eq!(std::fs::read_link(&link).unwrap(), python);
    }

    #[test]
    fn a_directory_that_is_not_ours_to_write_asks_the_operating_system() {
        let dir = tempfile::tempdir().unwrap();
        let binary = app(dir.path());
        let locked = dir.path().join("locked");
        std::fs::create_dir(&locked).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o555)).unwrap();

        let answer = link_it(&locked.join("charter"), &binary);

        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(matches!(answer, Linked::NeedsAdmin), "{answer:?}");
    }

    #[test]
    fn another_charter_in_a_user_directory_is_named() {
        let home = tempfile::tempdir().unwrap();
        let python = home.path().join(".local/bin/charter");
        std::fs::create_dir_all(python.parent().unwrap()).unwrap();
        std::fs::write(&python, "").unwrap();

        assert_eq!(
            others(Some(home.path()), Path::new("/usr/local/bin/charter")),
            [python]
        );
    }

    fn link_it(link: &Path, binary: &Path) -> Linked {
        super::link(link, binary)
    }

    /// A link to a file that is gone was left by an app that has since been moved or deleted:
    /// nothing is lost by replacing it, so it is replaced.
    #[test]
    fn a_link_to_nothing_is_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let binary = app(dir.path());
        let link = dir.path().join("charter");
        std::os::unix::fs::symlink(dir.path().join("gone/charter"), &link).unwrap();

        assert_eq!(plan(&link, &binary), Plan::Link);
    }

    /// A relative link is resolved beside the link, not against charter's working directory:
    /// one to somebody else's `charter` next to it is theirs, although nothing by that name
    /// exists where charter happens to be standing.
    #[test]
    fn a_relative_link_to_a_file_beside_it_is_somebody_elses() {
        let dir = tempfile::tempdir().unwrap();
        let binary = app(dir.path());
        let bin = dir.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let name = "charter-installed-another-way-311";
        std::fs::write(bin.join(name), "").unwrap();
        let link = bin.join("charter");
        std::os::unix::fs::symlink(name, &link).unwrap();
        assert!(!Path::new(name).exists(), "the test's cwd holds {name}");

        assert!(matches!(plan(&link, &binary), Plan::Refused(_)));
    }

    /// Only the whole shape `<name>.app/Contents/MacOS/charter` is an app's binary: a link to a
    /// file that matches all but one part of it is somebody else's.
    #[test]
    fn a_link_to_most_of_an_apps_shape_is_somebody_elses() {
        let dir = tempfile::tempdir().unwrap();
        let binary = app(dir.path());
        for near in [
            "Other.app/Contents/Resources/charter",
            "Other.app/Contents/MacOS/other",
            "Other.app/Other/MacOS/charter",
            "Other/Contents/MacOS/charter",
            "Other.app/x/y/z",
        ] {
            let target = dir.path().join(near);
            std::fs::create_dir_all(target.parent().unwrap()).unwrap();
            std::fs::write(&target, "").unwrap();
            let link = dir.path().join("charter");
            let _ = std::fs::remove_file(&link);
            std::os::unix::fs::symlink(&target, &link).unwrap();

            assert!(
                matches!(plan(&link, &binary), Plan::Refused(_)),
                "{near} was taken for an app's binary"
            );
        }
    }
}
