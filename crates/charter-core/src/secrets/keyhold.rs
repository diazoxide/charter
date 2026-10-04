//! Keyring items held to charter's app (ruling V90a, ADR 0047 as amended).
//!
//! **The rule is the Keychain's own default.** An item made with no access object of its own
//! lets the program that created it read it without asking, by that program's code signature,
//! and makes every other program ask the person or fail (ADR 0047, measured). So charter holds
//! an item to its app by having **the app's own binary create it**:
//!
//! - in the app ([`this_process_is_the_app`]), the item is deleted and made again here;
//! - anywhere else (the `charter` command), the app's binary beside this one is started again
//!   with [`HOLD_ARG`], is handed the item on its standard input, makes it and exits
//!   ([`serve_if_asked`]). It writes and never reads, so starting it gives nothing back.
//!
//! The item is deleted first because a replaced item keeps the access it had, and one the
//! person once let another program read ("Always Allow") would keep letting it. **A program may
//! delete only an item it owns**, so an item the `charter` command made before (where no app sat
//! beside it, or before this rule) is moved by the command: it deletes its own item and the app
//! makes it again, the next time the command reads or writes it. The app cannot move such an
//! item itself. If the app cannot make it, the command writes it back, and says the item is not
//! held yet ([`Held::NotYet`]); the next read through the command tries again. The termination
//! signals a person or a closing terminal sends are held off for that window
//! ([`Shield`]), so only a kill that cannot be caught loses the value there.
//!
//! **What it rests on.** Signed builds (#606) make the app's identity survive an update. Until
//! then an ad-hoc signed app is trusted by its exact build, so the first read after an update
//! asks the person once, and neither the new app nor the new command owns an item an older build
//! made. Where no app sits beside the command (a command-line install, a dev build of the command
//! alone), the command writes the item itself, and it stays the command's until a command with
//! the app beside it reads it. The command is found through any link to it: the app is looked
//! for beside the real file, never beside a link on `PATH`.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde_json::{Map, Value};

use super::VaultError;
use super::keyring::{Held, SERVICE_PREFIX};

/// The argument that starts the app's binary as the one-item writer.
pub const HOLD_ARG: &str = "charter-hold-a-keyring-item";

/// The name of the app's binary, beside the `charter` command in the app's bundle and in a
/// build's target directory.
pub const APP_BINARY: &str = "charter-app";

/// The most bytes the writer reads from its standard input: an item, never more.
const MOST_INPUT: u64 = 1024 * 1024;

/// How long the command waits for the writer before it writes the item itself.
const PATIENCE: Duration = Duration::from_secs(20);

/// The writer's exit code when the item there is owned by another program.
const OWNED: i32 = 3;

static THE_APP: AtomicBool = AtomicBool::new(false);

/// Called once by the app as it starts: this process is charter's app, so it holds an item by
/// writing it itself.
pub fn this_process_is_the_app() {
    THE_APP.store(true, Ordering::SeqCst);
}

/// One item to write, as the writer is handed it.
#[derive(Debug, PartialEq, Eq)]
pub struct Item {
    pub service: String,
    pub account: String,
    pub value: String,
}

/// The item `input` names, or why the writer refuses it: only an item under charter's own
/// services, so the app's binary is never a deputy for writing another program's item.
pub fn asked(input: &[u8]) -> Result<Item, String> {
    let doc: Value =
        serde_json::from_slice(input).map_err(|_| "the item is not a JSON object".to_owned())?;
    let field = |name: &str| {
        doc.get(name)
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| format!("the item has no {name}"))
    };
    let item = Item {
        service: field("service")?,
        account: field("account")?,
        value: field("value")?,
    };
    if !item.service.starts_with(SERVICE_PREFIX)
        || item.service.chars().any(char::is_control)
        || item.account.is_empty()
        || item.account.chars().any(char::is_control)
    {
        return Err(format!(
            "charter writes only its own items, under '{SERVICE_PREFIX}…'"
        ));
    }
    Ok(item)
}

/// Started again as the writer: write the one item on standard input and exit. `None` when
/// this process was not started as one. Called first thing by the app's `main`.
pub fn serve_if_asked() -> Option<i32> {
    if std::env::args_os().nth(1)? != HOLD_ARG {
        return None;
    }
    let mut input = Vec::new();
    if std::io::stdin()
        .take(MOST_INPUT)
        .read_to_end(&mut input)
        .is_err()
    {
        return Some(1);
    }
    let item = match asked(&input) {
        Ok(item) => item,
        Err(why) => {
            let _ = writeln!(std::io::stderr(), "{why}");
            return Some(1);
        }
    };
    Some(match write_quietly(&item) {
        Ok(()) => 0,
        Err(Refused::Owned) => OWNED,
        Err(Refused::Failed(e)) => {
            let _ = writeln!(std::io::stderr(), "{}", e.message);
            1
        }
    })
}

/// Why this process could not make an item.
enum Refused {
    /// The item there is another program's, and only its owner may delete it.
    Owned,
    Failed(VaultError),
}

/// The writer's write, with the Keychain's dialogs off: one it may not make fails, and the
/// command that started it decides what to do next.
#[cfg(target_os = "macos")]
fn write_quietly(item: &Item) -> Result<(), Refused> {
    let _quiet = security_framework::os::macos::keychain::SecKeychain::disable_user_interaction()
        .map_err(|e| Refused::Failed(VaultError::new(e.to_string())))?;
    make_here(&item.service, &item.account, &item.value)
}

#[cfg(not(target_os = "macos"))]
fn write_quietly(_item: &Item) -> Result<(), Refused> {
    Err(Refused::Failed(VaultError::new(
        "only macOS holds a keyring item to charter's app",
    )))
}

/// `errSecInvalidOwnerEdit`: the Keychain refused a delete because another program owns the
/// item (measured on a delete by a program the item does not trust).
const NOT_THE_OWNER: i32 = -25244;

/// The item deleted and made again by this process, so it is this program's alone.
fn make_here(service: &str, account: &str, value: &str) -> Result<(), Refused> {
    let entry = ::keyring::Entry::new(service, account)
        .map_err(|e| Refused::Failed(super::keyring::failure("reach", service, &e)))?;
    let _shield = Shield::up();
    match entry.delete_credential() {
        Ok(()) | Err(::keyring::Error::NoEntry) => {}
        Err(e) if owned_by_another(&e) => return Err(Refused::Owned),
        Err(e) => {
            return Err(Refused::Failed(super::keyring::failure(
                "write", service, &e,
            )));
        }
    }
    entry
        .set_password(value)
        .map_err(|e| Refused::Failed(super::keyring::failure("write", service, &e)))
}

/// Whether the store refused a delete because another program owns the item.
#[cfg(target_os = "macos")]
fn owned_by_another(e: &::keyring::Error) -> bool {
    match e {
        ::keyring::Error::NoStorageAccess(inner) | ::keyring::Error::PlatformFailure(inner) => {
            inner
                .downcast_ref::<security_framework::base::Error>()
                .is_some_and(|e| e.code() == NOT_THE_OWNER)
        }
        _ => false,
    }
}

#[cfg(not(target_os = "macos"))]
fn owned_by_another(_e: &::keyring::Error) -> bool {
    false
}

/// The termination signals a person or a closing terminal sends (SIGINT, SIGHUP, SIGTERM),
/// held off while an item is gone between its delete and its new write, and acted on once it
/// is written: the process then ends as the signal would have ended it.
pub struct Shield {
    #[cfg(unix)]
    flag: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    #[cfg(unix)]
    ids: Vec<signal_hook::SigId>,
}

impl Shield {
    /// Hold the signals off from now.
    pub fn up() -> Self {
        #[cfg(unix)]
        {
            use signal_hook::consts::{SIGHUP, SIGINT, SIGTERM};
            let flag = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let ids = [SIGINT, SIGHUP, SIGTERM]
                .into_iter()
                .filter_map(|sig| {
                    signal_hook::flag::register_usize(
                        sig,
                        std::sync::Arc::clone(&flag),
                        sig as usize,
                    )
                    .ok()
                })
                .collect();
            Self { flag, ids }
        }
        #[cfg(not(unix))]
        {
            Self {}
        }
    }

    /// The signal that arrived while it was up, if one did.
    pub fn caught(&self) -> Option<i32> {
        #[cfg(unix)]
        {
            match self.flag.load(Ordering::SeqCst) {
                0 => None,
                n => i32::try_from(n).ok(),
            }
        }
        #[cfg(not(unix))]
        {
            None
        }
    }
}

impl Drop for Shield {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            for id in self.ids.drain(..) {
                signal_hook::low_level::unregister(id);
            }
            if let Some(sig) = self.caught() {
                // What the signal would have done had it not been held off: a handler another
                // part of charter registered still runs, and with none the process ends.
                let _ = signal_hook::low_level::raise(sig);
            }
        }
    }
}

/// The app's binary beside this one, where there is one and this process is not it.
fn the_app() -> Option<PathBuf> {
    app_beside(&std::env::current_exe().ok()?)
}

/// The app's binary beside the program at `exe`, as the kernel names it, where there is one
/// and `exe` is not it. A link on `PATH` (`/usr/local/bin/charter`, made by the app's Install
/// on PATH) is followed to the bundle, and nothing beside the link is ever looked at.
pub fn app_beside(exe: &std::path::Path) -> Option<PathBuf> {
    let here = exe.canonicalize().ok()?;
    let app = here.parent()?.join(APP_BINARY);
    (app != here && app.is_file()).then_some(app)
}

/// Why an item is written: a new value, or the same value moved under the rule (ruling V90d),
/// which is pointless to write again in place when it cannot be held.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Why {
    Write,
    Move,
}

/// Write the item held to charter's app, from whichever charter this is. See the module header.
pub fn set(service: &str, account: &str, value: &str, why: Why) -> Result<Held, VaultError> {
    // Where the item cannot be held: write the value in place, or, for a move, leave it.
    let in_place = || match why {
        Why::Write => super::keyring::set_here(service, account, value).map(|()| Held::NotYet),
        Why::Move => Ok(Held::NotYet),
    };
    if THE_APP.load(Ordering::SeqCst) {
        return match make_here(service, account, value) {
            Ok(()) => Ok(Held::ToTheApp),
            // The command's own item, which only the command can delete.
            Err(Refused::Owned) => in_place(),
            Err(Refused::Failed(e)) => Err(e),
        };
    }
    let Some(app) = the_app() else {
        return in_place();
    };
    let item = Item {
        service: service.to_owned(),
        account: account.to_owned(),
        value: value.to_owned(),
    };
    match through(&app, &item) {
        Ok(()) => return Ok(Held::ToTheApp),
        Err(code) if code != OWNED => return in_place(),
        Err(_) => {}
    }
    // This command's own item, made before: it deletes it, the app makes it again, and if the
    // app cannot, the command writes it back. No caught signal ends it in between.
    let _shield = Shield::up();
    if let Ok(entry) = ::keyring::Entry::new(service, account) {
        match entry.delete_credential() {
            Ok(()) | Err(::keyring::Error::NoEntry) => {}
            Err(_) => return in_place(),
        }
    }
    match through(&app, &item) {
        Ok(()) => Ok(Held::ToTheApp),
        Err(_) => super::keyring::set_here(service, account, value).map(|()| Held::NotYet),
    }
}

/// `item` written by the app's binary at `app`: `Err` with its exit code (`-1` when it could
/// not be run or did not finish in time).
fn through(app: &std::path::Path, item: &Item) -> Result<(), i32> {
    let mut doc = Map::new();
    doc.insert("service".into(), Value::String(item.service.clone()));
    doc.insert("account".into(), Value::String(item.account.clone()));
    doc.insert("value".into(), Value::String(item.value.clone()));
    let input = serde_json::to_vec(&Value::Object(doc)).map_err(|_| -1)?;
    let mut command = Command::new(app);
    command
        .arg(HOLD_ARG)
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(home) = std::env::var_os("HOME") {
        command.env("HOME", home);
    }
    // Its own process group: a Ctrl-C to the terminal's group never ends it mid-write.
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
    let mut child = crate::forklock::spawn(&mut command).map_err(|_| -1)?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(&input);
    }
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                return if status.success() {
                    Ok(())
                } else {
                    Err(status.code().unwrap_or(-1))
                };
            }
            Ok(None) if started.elapsed() < PATIENCE => {
                std::thread::sleep(Duration::from_millis(20));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(-1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_writer_takes_an_item_under_charters_own_services() {
        let item = asked(br#"{"service": "charter/ops/3f9a2c1b", "account": "K", "value": "v"}"#)
            .expect("taken");
        assert_eq!(
            item,
            Item {
                service: "charter/ops/3f9a2c1b".into(),
                account: "K".into(),
                value: "v".into(),
            }
        );
    }

    #[test]
    fn the_writer_refuses_another_programs_item_and_a_name_it_could_not_state() {
        for input in [
            br#"{"service": "Claude Code-credentials", "account": "K", "value": "v"}"#.as_slice(),
            br#"{"service": "charter/ops/x\n", "account": "K", "value": "v"}"#,
            br#"{"service": "charter/ops/x", "account": "", "value": "v"}"#,
            br#"{"service": "charter/ops/x", "account": "K"}"#,
            b"not json",
        ] {
            assert!(asked(input).is_err(), "{}", String::from_utf8_lossy(input));
        }
    }

    #[cfg(unix)]
    #[test]
    fn the_command_run_through_a_link_on_path_finds_the_app_in_its_bundle() {
        // `/usr/local/bin/charter` is a link into the bundle (`clipath`), and macOS reports the
        // program as run, by the link's path, so the app is looked for beside the real file.
        let dir = tempfile::tempdir().expect("a directory");
        let bundle = dir.path().join("charter.app/Contents/MacOS");
        let bin = dir.path().join("bin");
        std::fs::create_dir_all(&bundle).expect("the bundle");
        std::fs::create_dir_all(&bin).expect("a bin");
        std::fs::write(bundle.join("charter"), "").expect("the command");
        std::fs::write(bundle.join(APP_BINARY), "").expect("the app");
        std::os::unix::fs::symlink(bundle.join("charter"), bin.join("charter")).expect("a link");
        assert_eq!(
            app_beside(&bin.join("charter")),
            Some(bundle.canonicalize().unwrap().join(APP_BINARY))
        );
        // A program planted beside the link is never the one handed a value.
        std::fs::write(bin.join(APP_BINARY), "").expect("a planted app");
        assert_eq!(
            app_beside(&bin.join("charter")),
            Some(bundle.canonicalize().unwrap().join(APP_BINARY))
        );
    }

    #[test]
    fn the_app_is_not_its_own_writer_and_a_lone_command_has_none() {
        let dir = tempfile::tempdir().expect("a directory");
        std::fs::write(dir.path().join(APP_BINARY), "").expect("the app");
        assert_eq!(app_beside(&dir.path().join(APP_BINARY)), None);
        assert_eq!(app_beside(&dir.path().join("missing")), None);
        let lone = tempfile::tempdir().expect("a directory");
        std::fs::write(lone.path().join("charter"), "").expect("the command");
        assert_eq!(app_beside(&lone.path().join("charter")), None);
    }

    #[cfg(unix)]
    #[test]
    fn a_signal_that_arrives_while_an_item_is_gone_waits_until_it_is_written() {
        use signal_hook::consts::SIGHUP;
        // What stands in for the rest of charter's handlers, so the signal acted on at the end
        // does not end the test.
        let after = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let id = signal_hook::flag::register(SIGHUP, std::sync::Arc::clone(&after)).unwrap();
        let shield = Shield::up();
        signal_hook::low_level::raise(SIGHUP).unwrap();
        assert_eq!(shield.caught(), Some(SIGHUP));
        drop(shield);
        assert!(
            after.load(Ordering::SeqCst),
            "the signal was dropped, not delayed"
        );
        signal_hook::low_level::unregister(id);
    }

    #[test]
    fn a_process_not_started_as_the_writer_is_left_alone() {
        assert_eq!(serve_if_asked(), None);
    }
}
