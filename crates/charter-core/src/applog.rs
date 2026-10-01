//! The diagnostic log (#647): what the app notices and nobody asked to see, kept in a file.
//!
//! An app started from the Dock or a desktop launcher has a standard error that goes nowhere,
//! so every `eprintln!` it made was lost: a chat that would not start, a plane that could not
//! be watched, a record that was not written. They are `tracing` events now, and [`install`]
//! sends them to `charter.<YYYY-MM-DD>.log` in the app's log directory and, unchanged, to
//! standard error. One file a day, the newest seven kept. `docs/plane-format.md` records the
//! store: Machine, device-bound, transient (ADR 0069), beside `panics.log`.
//!
//! **Output is not a log.** What a `charter` command prints to its terminal on purpose is that
//! command's answer, replayed against the recorded fixtures (ADR 0046), and it never goes
//! through here. Only diagnostics do, and `clippy.toml` refuses `eprintln!` in this crate and
//! the app so a new one cannot slip back to standard error alone.
//!
//! **No secrets.** Every event is held whole and asked [`crate::secretshape::found`] before it
//! is written, to the file and to standard error alike; one that looks like it holds a
//! credential is replaced by a line naming the kind. Callers still say a chat's number and
//! never its token: this is the net under that rule, not a licence to break it.
//!
//! **Not the audit and not telemetry** (O1, ADR 0075). Nothing reads this file but a person.

use std::borrow::Cow;
use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};

use tracing::Level;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::filter::Targets;
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::layer::SubscriberExt;

/// The directory the log is kept in: `$CHARTER_LOG_DIR` when it is set, else the app's own log
/// directory, where `panics.log` already is.
pub fn dir() -> Option<PathBuf> {
    dir_from(std::env::var_os("CHARTER_LOG_DIR"))
}

fn dir_from(named: Option<OsString>) -> Option<PathBuf> {
    match named.filter(|named| !named.is_empty()) {
        Some(named) => Some(PathBuf::from(named)),
        None => app_log_dir(),
    }
}

/// Tauri's `app_log_dir` for charter's identifier, worked out without Tauri: the core never
/// depends on it, and the app wants its log before Tauri has started.
pub fn app_log_dir() -> Option<PathBuf> {
    const APP: &str = "dev.charter.app";
    if cfg!(target_os = "macos") {
        Some(dirs::home_dir()?.join("Library/Logs").join(APP))
    } else {
        Some(dirs::data_local_dir()?.join(APP).join("logs"))
    }
}

/// How many daily files are kept, today's among them: a week is long enough to look back at
/// what went wrong before somebody noticed, and short enough that nobody has to clean up.
const DAYS_KEPT: usize = 7;

/// Sends every diagnostic this process raises to the log in [`dir`] and to standard error, from
/// now on. The app calls it first, before anything that could have something to say.
///
/// A log that cannot be opened is not a reason to stop: standard error still gets every
/// event, which is what the app had before, and the first thing it says is why the file is
/// missing. Called a second time, it changes nothing.
pub fn install() {
    let file = dir()
        .ok_or_else(|| std::io::Error::other("no home directory to keep it under"))
        .and_then(|dir| daily_file(&dir));
    let (file, missing) = match file {
        Ok(file) => (Some(file), None),
        Err(why) => (None, Some(why)),
    };
    if tracing::subscriber::set_global_default(subscriber_to(file, std::io::stderr)).is_ok()
        && let Some(why) = missing
    {
        tracing::warn!(
            "charter: no log file ({why}); what the app notices goes to standard error alone"
        );
    }
}

/// The file in `dir` that today's events go to.
///
/// **Blocking, on purpose.** `tracing-appender`'s non-blocking writer hands lines to a thread
/// and loses whatever it still holds when the process ends without dropping its guard, which
/// is how this app ends: `panic = "abort"`, and `std::process::exit` on a second launch. The
/// app says little, so a write on the thread that said it costs nothing anyone can notice.
fn daily_file(dir: &Path) -> std::io::Result<RollingFileAppender> {
    RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix("charter")
        .filename_suffix("log")
        .max_log_files(DAYS_KEPT)
        .build(dir)
        .map_err(std::io::Error::other)
}

/// Every event that passes the filter, to `file` when there is one and to `screen`.
fn subscriber_to(
    file: Option<RollingFileAppender>,
    screen: impl for<'a> MakeWriter<'a> + Send + Sync + 'static,
) -> impl tracing::Subscriber + Send + Sync {
    // Standard error gets the message alone, as `eprintln!` printed it: that is what a person
    // running the app from a terminal, and a CI log, has always read. The file has the rest.
    let terminal = tracing_subscriber::fmt::layer()
        .without_time()
        .with_level(false)
        .with_target(false)
        .with_writer(Redacting(screen));
    // charter's own crates (`charter_core`, `charter_app_lib`) at `info`, so what the app did
    // is there to read; everything it links at `warn`, so a library's routine chatter is not.
    let wanted = Targets::new()
        .with_target("charter", Level::INFO)
        .with_default(Level::WARN);
    tracing_subscriber::registry()
        .with(wanted)
        .with(file.map(|file| {
            tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_writer(Redacting(file))
        }))
        .with(terminal)
}

/// A writer that holds each event whole and lets it through only once it has been asked
/// whether it looks like it holds a credential.
///
/// `fmt` formats an event into one buffer and writes it with one `write_all`, but nothing
/// promises that, so the event is collected here and judged on drop, when it is complete.
struct Redacting<M>(M);

impl<'a, M: MakeWriter<'a>> MakeWriter<'a> for Redacting<M> {
    type Writer = Held<M::Writer>;

    fn make_writer(&'a self) -> Self::Writer {
        Held {
            event: Vec::new(),
            to: self.0.make_writer(),
        }
    }
}

/// One event on its way to `to`.
struct Held<W: Write> {
    event: Vec<u8>,
    to: W,
}

impl<W: Write> Write for Held<W> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.event.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl<W: Write> Drop for Held<W> {
    fn drop(&mut self) {
        let event = String::from_utf8_lossy(&self.event);
        let said = redacted(&event);
        // A log that cannot be written is let go: there is nowhere left to say so.
        let _ = self.to.write_all(said.as_bytes());
        let _ = self.to.flush();
    }
}

/// `event`, or in its place a line naming the kind of credential it looks like it holds.
///
/// The whole event goes, and not the one line: an event can span lines (a PEM block's body
/// is lines of base64 no rule recognises), and a log is read to find out what happened, which
/// a timestamp without its message does not say anyway.
fn redacted(event: &str) -> Cow<'_, str> {
    match crate::secretshape::found(event) {
        Some(found) => Cow::Owned(format!(
            "[redacted: an event that looks like it holds {}]\n",
            found.kind
        )),
        None => Cow::Borrowed(event),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A standard error a test can read back.
    #[derive(Clone, Default)]
    struct Screen(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

    impl Screen {
        fn said(&self) -> String {
            String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
        }
    }

    impl Write for Screen {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    /// The subscriber, with standard error kept off the test run's own.
    fn quiet(dir: &Path) -> impl tracing::Subscriber + Send + Sync {
        subscriber_to(Some(daily_file(dir).unwrap()), Screen::default)
    }

    /// Everything written to the log files in `dir`, oldest file first.
    fn logged(dir: &Path) -> String {
        let mut files: Vec<_> = std::fs::read_dir(dir)
            .expect("the log directory exists")
            .map(|entry| entry.expect("an entry").path())
            .collect();
        files.sort();
        files
            .iter()
            .map(|file| std::fs::read_to_string(file).expect("a log file reads"))
            .collect()
    }

    #[test]
    fn a_warning_from_charter_lands_in_a_file_in_the_log_directory() {
        let dir = tempfile::tempdir().unwrap();
        let logs = dir.path().join("logs");

        tracing::subscriber::with_default(quiet(&logs), || {
            tracing::warn!("charter: no tray icon (the desktop has none)");
        });

        let said = logged(&logs);
        assert!(
            said.contains("charter: no tray icon (the desktop has none)"),
            "{said:?}"
        );
        assert!(said.contains("WARN"), "the file names the level: {said:?}");
    }

    #[test]
    fn an_event_that_carries_a_secret_never_reaches_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let logs = dir.path().join("logs");
        let token = concat!("ghp_", "0123456789abcdefghijABCDEFGHIJ012345");

        tracing::subscriber::with_default(quiet(&logs), || {
            tracing::warn!("charter: the forge answered 401 for {token}");
            tracing::warn!("charter: password: hunter2is was refused");
            tracing::warn!("charter: the next line is still written");
        });

        let said = logged(&logs);
        assert!(
            !said.contains(token),
            "the token was written down: {said:?}"
        );
        assert!(
            !said.contains("hunter2is"),
            "the password was written down: {said:?}"
        );
        assert!(
            said.contains(
                "[redacted: an event that looks like it holds a token by its forge's prefix]"
            ),
            "the redaction says what kind it removed, so the gap is not a mystery: {said:?}"
        );
        assert!(
            said.contains("charter: the next line is still written"),
            "one redaction does not silence the log: {said:?}"
        );
    }

    #[test]
    fn standard_error_still_gets_the_message_as_it_was_printed_and_redacted_the_same() {
        let dir = tempfile::tempdir().unwrap();
        let screen = Screen::default();
        let token = concat!("ghp_", "0123456789abcdefghijABCDEFGHIJ012345");

        let seen = screen.clone();
        tracing::subscriber::with_default(
            subscriber_to(Some(daily_file(dir.path()).unwrap()), move || seen.clone()),
            || {
                tracing::warn!("charter: no tray icon; its menu is still on the click");
                tracing::warn!("charter: the forge answered 401 for {token}");
            },
        );

        assert_eq!(
            screen.said(),
            "charter: no tray icon; its menu is still on the click\n\
             [redacted: an event that looks like it holds a token by its forge's prefix]\n",
            "a person running the app from a terminal reads what eprintln! printed, and no \
             more; the time and level are the file's"
        );
    }

    #[test]
    fn charter_says_what_it_noticed_and_a_library_only_what_went_wrong() {
        let dir = tempfile::tempdir().unwrap();
        let logs = dir.path().join("logs");

        tracing::subscriber::with_default(quiet(&logs), || {
            tracing::info!("charter: plane /p, 2 of 2 chats back");
            tracing::debug!("charter: a detail nobody asked for");
            tracing::info!(target: "zbus::connection", "a library's routine chatter");
            tracing::warn!(target: "zbus::connection", "a library's own trouble");
        });

        let said = logged(&logs);
        assert!(said.contains("2 of 2 chats back"), "{said:?}");
        assert!(said.contains("a library's own trouble"), "{said:?}");
        assert!(!said.contains("a detail nobody asked for"), "{said:?}");
        assert!(!said.contains("routine chatter"), "{said:?}");
    }

    #[test]
    fn a_week_of_logs_is_kept_and_the_panic_log_beside_it_is_never_touched() {
        let dir = tempfile::tempdir().unwrap();
        let logs = dir.path();
        std::fs::write(logs.join("panics.log"), "charter-panic\n").unwrap();
        // Nine days of logs, oldest first: the appender orders them by when each was made.
        for day in 1..=9 {
            std::fs::write(logs.join(format!("charter.2026-09-0{day}.log")), "a day\n").unwrap();
            std::thread::sleep(std::time::Duration::from_millis(20));
        }

        tracing::subscriber::with_default(quiet(logs), || {
            tracing::warn!("charter: today");
        });

        let mut kept: Vec<String> = std::fs::read_dir(logs)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name.starts_with("charter."))
            .collect();
        kept.sort();
        assert_eq!(kept.len(), 7, "today and the six days before it: {kept:?}");
        assert!(
            !kept.contains(&"charter.2026-09-03.log".to_owned()),
            "the oldest days went first: {kept:?}"
        );
        assert!(
            logs.join("panics.log").exists(),
            "the panic log is not the appender's"
        );
    }

    #[test]
    fn the_log_is_kept_beside_the_panic_log_unless_the_environment_names_a_directory() {
        let app = crate::report::panic_log().and_then(|file| file.parent().map(Path::to_path_buf));
        // Under `$CHARTER_PANIC_LOG` the panic log is wherever that says, so only compare
        // with the app's own directory when it is the one the panic log is in.
        if std::env::var_os("CHARTER_PANIC_LOG").is_none() {
            assert_eq!(
                dir_from(None),
                app,
                "the same directory `charter report` reads"
            );
        }
        assert_eq!(
            dir_from(Some("".into())),
            app_log_dir(),
            "an empty value names nothing"
        );
        assert_eq!(
            dir_from(Some("/elsewhere/logs".into())),
            Some(std::path::PathBuf::from("/elsewhere/logs"))
        );
        let app = app_log_dir().expect("a home to find it under");
        let app = app.to_string_lossy();
        assert!(
            app.ends_with("Library/Logs/dev.charter.app") || app.ends_with("dev.charter.app/logs"),
            "Tauri's `app_log_dir` for charter's identifier: {app}"
        );
    }
}
