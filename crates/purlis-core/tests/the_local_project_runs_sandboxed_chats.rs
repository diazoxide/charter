//! A sandboxed chat starts in this machine's local project, and still cannot write the person's
//! approvals (#1670).
//!
//! The local project used to live in the config home, which a chat's sandbox denies whole
//! (ADR 0067 §5, class 3), so every sandboxed chat there was refused before it started. It is
//! made in the data home now, where nothing a chat is denied is above it.

use std::path::{Path, PathBuf};

use purlis_core::harness::Harness;
use purlis_core::sandbox::{self, Access, Class, Machine, Os};

/// A machine whose config home and data home are under `base`. Its home directory is a path of
/// its own outside the temp tree, which a chat may write.
fn machine(base: &Path) -> Machine {
    let home = Path::new("/home/op");
    let config = base.join(".config");
    let data = base.join(".local/share/purlis");
    Machine {
        env: purlis_core::secrets::Env::of(&[
            ("HOME", home.to_str().expect("a UTF-8 path")),
            (
                purlis_core::machine::HOME_VAR,
                config.to_str().expect("a UTF-8 path"),
            ),
            (
                purlis_core::datahome::HOME_VAR,
                data.to_str().expect("a UTF-8 path"),
            ),
        ]),
        home: Some(home.to_path_buf()),
        os: Os::this(),
    }
}

/// The local project, made where the app makes it on `machine`.
fn local_project(machine: &Machine) -> PathBuf {
    let data = purlis_core::firstrun::local_project_home(&|name| machine.env.get(name))
        .expect("the machine has a data home");
    purlis_core::firstrun::ensure_local_plane(
        &data,
        purlis_core::firstrun::ForgeFrom::Named(purlis_core::forge::Kind::GitHub),
    )
    .expect("the local project is made")
}

#[test]
fn a_sandboxed_chat_starts_in_the_local_project() {
    let dir = tempfile::tempdir().expect("a directory");
    let base = dir.path().canonicalize().expect("the directory");
    let machine = machine(&base);
    let root = local_project(&machine);

    let started = sandbox::for_start(Harness::ClaudeCode, &root, &machine, &|_| true);

    match started {
        Ok(Some(_)) => {}
        Ok(None) => panic!("the local project does not run its chats sandboxed"),
        Err(refused) => panic!("refused: {refused}"),
    }
    // And the person's approvals stay out of its reach: the config home is still denied.
    let config = purlis_core::machine::dir(&base.join(".config"));
    let denied = sandbox::Denied::of(&root, &machine).paths;
    assert!(
        denied
            .iter()
            .any(|denial| denial.class == Class::HumanPowers
                && matches!(denial.access, Access::Write | Access::ReadWrite)
                && config.starts_with(&denial.path)),
        "nothing denies writing {}: {denied:?}",
        config.display()
    );
    assert!(
        !root.starts_with(&config),
        "the local project is in the config home: {}",
        root.display()
    );
}
