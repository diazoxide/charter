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
//! person once let another program read ("Always Allow") would keep letting it. A program may
//! delete only an item it owns, so an item the `charter` command made before is deleted by the
//! command and then made by the app. If the app cannot make it, the command writes it back
//! itself, so no value is lost, and says the item is not held yet ([`Held::NotYet`]); the next
//! read tries again.
//!
//! **What it rests on.** Signed builds (#606) make the app's identity survive an update. Until
//! then an ad-hoc signed app is trusted by its exact build, so the first read after an update
//! asks the person once. Where no app sits beside the command (a command-line install, a dev
//! build of the command alone), the command writes the item itself, and it is held to the app
//! the first time the app reads it.

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

/// The item deleted and made again by this process, so it is this program's alone.
fn make_here(service: &str, account: &str, value: &str) -> Result<(), Refused> {
    let entry = ::keyring::Entry::new(service, account)
        .map_err(|e| Refused::Failed(super::keyring::failure("reach", service, &e)))?;
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

/// Whether the store refused a delete because another program owns the item
/// (`errSecInvalidOwnerEdit`, measured on a delete by a program the item does not trust).
#[cfg(target_os = "macos")]
fn owned_by_another(e: &::keyring::Error) -> bool {
    match e {
        ::keyring::Error::NoStorageAccess(inner) | ::keyring::Error::PlatformFailure(inner) => {
            inner
                .downcast_ref::<security_framework::base::Error>()
                .is_some_and(|e| e.code() == -25244)
        }
        _ => false,
    }
}

#[cfg(not(target_os = "macos"))]
fn owned_by_another(_e: &::keyring::Error) -> bool {
    false
}

/// The app's binary beside this one, where there is one and this process is not it.
fn the_app() -> Option<PathBuf> {
    let here = std::env::current_exe().ok()?;
    let app = here.parent()?.join(APP_BINARY);
    (app != here && app.is_file()).then_some(app)
}

/// Write the item held to charter's app, from whichever charter this is. See the module header.
pub fn set(service: &str, account: &str, value: &str) -> Result<Held, VaultError> {
    if THE_APP.load(Ordering::SeqCst) {
        return match make_here(service, account, value) {
            Ok(()) => Ok(Held::ToTheApp),
            Err(Refused::Owned) => {
                super::keyring::set_here(service, account, value).map(|()| Held::NotYet)
            }
            Err(Refused::Failed(e)) => Err(e),
        };
    }
    let Some(app) = the_app() else {
        return super::keyring::set_here(service, account, value).map(|()| Held::NotYet);
    };
    let item = Item {
        service: service.to_owned(),
        account: account.to_owned(),
        value: value.to_owned(),
    };
    match through(&app, &item) {
        Ok(()) => return Ok(Held::ToTheApp),
        Err(code) if code != OWNED => {
            return super::keyring::set_here(service, account, value).map(|()| Held::NotYet);
        }
        Err(_) => {}
    }
    // This command's own item, made before: it deletes it, the app makes it again, and if the
    // app cannot, the command writes it back so the value is never lost.
    if let Ok(entry) = ::keyring::Entry::new(service, account) {
        match entry.delete_credential() {
            Ok(()) | Err(::keyring::Error::NoEntry) => {}
            Err(_) => {
                return super::keyring::set_here(service, account, value).map(|()| Held::NotYet);
            }
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

    #[test]
    fn a_process_not_started_as_the_writer_is_left_alone() {
        assert_eq!(serve_if_asked(), None);
    }
}
