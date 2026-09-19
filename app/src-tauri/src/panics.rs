//! A panic is written down before the process goes.
//!
//! A shipped build aborts on any panic (`panic = "abort"`), and so does a debug build on a
//! panic inside a call from the window, which reaches Rust through the webview's own code and
//! cannot unwind back through it. Either way the process is gone before anything else can
//! look, and Rust's own hook says what happened only on standard error — which, for an app
//! started from the Dock or a desktop launcher, goes nowhere. So the one line that names the
//! panic is lost with the process.
//!
//! That is not hypothetical. A shipped build died on this project's own machine on
//! 2026-09-17: `SIGABRT` on the main thread, `abort()` called from charter-app's code inside
//! WebKit's `didPostMessage` — a call from the window. The build is stripped, so every Rust
//! frame in the crash report is `?`, and the message was on a standard error nobody had.
//! charter-app#16 is the same shape: the app gone mid-test, no crash report that named it.
//!
//! The hook here writes the thread, the place in the source, the message and a backtrace to
//! standard error, and appends the same to a file, and then hands over to the hook that was
//! there before it. It runs on the panicking thread, before the abort. The place in the source
//! survives stripping, which is what makes it worth writing down even where the backtrace is
//! only addresses.

use std::backtrace::Backtrace;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// The file panics are appended to, once the app knows where it is.
static LOG: OnceLock<PathBuf> = OnceLock::new();

/// Installs the hook. Called first, before anything that could panic.
///
/// `CHARTER_PANIC_LOG` names the file when it is set: the scenario tests point it at the logs
/// a failed CI run keeps. Otherwise panics go to standard error alone until [`keep_in`] names
/// the app's own log directory, which the app learns only once Tauri has started.
pub fn record() {
    if let Some(file) = std::env::var_os("CHARTER_PANIC_LOG") {
        let _ = LOG.set(PathBuf::from(file));
    }
    install(|| LOG.get());
}

/// Panics are appended to `panics.log` in `dir` from now on — unless the environment has
/// already named a file, which wins, because it was set by whoever is watching.
pub fn keep_in(dir: &Path) {
    let _ = LOG.set(dir.join("panics.log"));
}

fn install(log: impl Fn() -> Option<&'static PathBuf> + Send + Sync + 'static) {
    let before = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        let said = describe(
            thread.name().unwrap_or("an unnamed thread"),
            info.location()
                .map(|at| format!("{}:{}:{}", at.file(), at.line(), at.column())),
            info.payload_as_str()
                .unwrap_or("(a panic that carries no message)"),
            &Backtrace::force_capture(),
        );
        // Nothing here may panic: a panic inside the hook aborts at once and says less.
        let _ = std::io::stderr().write_all(said.as_bytes());
        if let Some(file) = log() {
            write_down(file, "");
        }
        before(info);
    }));
}

/// A panic, as one block a person can find in a log and read without anything else.
fn describe(
    thread: &str,
    place: Option<String>,
    message: &str,
    backtrace: &impl std::fmt::Display,
) -> String {
    let place = place.as_deref().unwrap_or("an unknown place");
    let when = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or(0);
    format!(
        "charter-panic pid {pid} at {when} (seconds since 1970)\n\
         thread: {thread}\n\
         place: {place}\n\
         message: {message}\n\
         backtrace:\n{backtrace}\n\
         charter-panic end\n",
        pid = std::process::id(),
    )
}

/// Appends `said` to `file`, making its directory if it has none. Anything that fails is let
/// go: the panic is already on standard error, and the process is on its way out.
fn write_down(file: &Path, said: &str) {
    if let Some(dir) = file.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(mut log) = OpenOptions::new().create(true).append(true).open(file) {
        let _ = log.write_all(said.as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        std::env::temp_dir()
            .join(format!("charter-panics-{}-{name}", std::process::id()))
            .join("nested")
            .join("panics.log")
    }

    #[test]
    fn a_panic_is_described_by_its_thread_place_and_message() {
        let said = describe(
            "charter-view",
            Some("src/sessions.rs:12:5".to_owned()),
            "the queue was poisoned",
            &"frame 0\nframe 1",
        );

        assert!(said.contains("thread: charter-view"), "{said}");
        assert!(said.contains("place: src/sessions.rs:12:5"), "{said}");
        assert!(said.contains("message: the queue was poisoned"), "{said}");
        assert!(said.contains("frame 1"), "{said}");
        assert!(
            said.starts_with("charter-panic ") && said.ends_with("charter-panic end\n"),
            "a panic is one block, found by its first and last line: {said}"
        );
    }

    #[test]
    fn a_panic_written_down_is_added_to_the_file_and_nothing_before_it_is_lost() {
        let file = scratch("appends");
        let _ = std::fs::remove_file(&file);

        write_down(&file, "the first\n");
        write_down(&file, "the second\n");

        assert_eq!(
            std::fs::read_to_string(&file).expect("the file was made, directory and all"),
            "the first\nthe second\n"
        );
    }

    #[test]
    fn a_thread_that_panics_leaves_its_panic_in_the_file_before_it_goes() {
        // The hook, installed for real, on a thread that really panics. The previous hook is
        // still called after it, so every other test in this binary reports as it always did.
        let file: &'static PathBuf = Box::leak(Box::new(scratch("hooked")));
        let _ = std::fs::remove_file(file);
        install(move || Some(file));

        let died = std::thread::Builder::new()
            .name("a-thread-that-panics".into())
            .spawn(|| panic!("a panic the test asked for"))
            .expect("the thread starts")
            .join();

        assert!(died.is_err(), "the thread was meant to panic");
        let written = std::fs::read_to_string(file).expect("the hook wrote the file");
        assert!(
            written.contains("thread: a-thread-that-panics"),
            "{written}"
        );
        assert!(
            written.contains("message: a panic the test asked for"),
            "{written}"
        );
        assert!(
            written.contains("place: app/src-tauri/src/panics.rs:"),
            "{written}"
        );
    }
}
