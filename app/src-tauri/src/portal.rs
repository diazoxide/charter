//! The desktop portal, asked before GTK asks it (charter-app#24).
//!
//! On a Linux session whose D-Bus session bus can *activate* `org.freedesktop.portal.Desktop`
//! but whose portal cannot come up — CI runners, containers with desktop packages, a remote X
//! session or a bare tiling window manager with no desktop session behind it — a launch used to
//! take 26 to 31 seconds. GTK, inside `tauri::Builder::build()`, creates a proxy for the portal
//! with auto-start on and waits out D-Bus's 25 s default; WebKitGTK then asks the same portal
//! for the colour scheme and waits out its own 5 s. charter makes neither call and cannot
//! switch either off (ADR 0026, amended).
//!
//! What charter CAN do is ask first, and briefly. Before anything touches GTK, [`listen`]
//! makes the call GTK's proxy makes — it asks the session bus to start the portal — and
//! gives it [`BUDGET`]. The bus answers "already running" at once when the portal is up, and
//! "no such service" at once when nothing can start it. Only a portal the bus is still trying
//! to start leaves it silent — and that silence is the 25 s GTK is about to wait out.
//!
//! When it is silent, the app starts again, in place (`exec`, same process), with
//! `DBUS_SESSION_BUS_ADDRESS` pointing at nothing. A machine with no session bus starts in
//! under 0.6 s, because every bus call fails at once. The bus itself is healthy, so the
//! address it had is kept in [`SESSION_BUS_KEPT`] and every chat gets that one
//! (`purlis_core::chatenv`), as does every program the app starts (`forklock::spawn`). What
//! the run loses — the tray, notifications, a keyring vault from the window, and handing a
//! second launch over — is said on standard error and in the window ([`SessionBus`]);
//! `instance.rs` is what keeps two apps off one plane without the bus. After the launch the
//! kept bus is asked again, backing off, until it answers (`listen_again`), and then the
//! window offers to start again on it. It never does so by itself.
//!
//! **X11 without a session bus.** With no `DBUS_SESSION_BUS_ADDRESS` and no
//! `$XDG_RUNTIME_DIR/bus` — `startx` into i3, say — GIO does not give up: it runs
//! `dbus-launch --autolaunch` and uses the bus the X display holds, starting one if there is
//! none. A notification daemon started from the window manager's config is on that bus. GTK
//! would find it and zbus — which the tray, notifications and single-instance use — would not,
//! so the launch looks for it the way GIO does, asks the portal there, and starts again on it
//! ([`Start::OnTheBus`]). Measured in a container (Ubuntu 24.04, i3 on Xvfb,
//! `xdg-desktop-portal` and its GTK backend installed): `dbus-launch` answered in 11 ms, and
//! the portal it could activate there came up in 114–202 ms. Each gets [`BUDGET`], so this path
//! costs at most about 600 ms and never hangs. Only when there is no bus to be had, or its
//! portal is silent too, does the launch start without one; the `dbus-daemon` `dbus-launch`
//! started then stays with the X display, as GIO leaves it.

use std::time::Duration;

pub use purlis_core::chatenv::{NO_SESSION_BUS, SESSION_BUS, SESSION_BUS_KEPT};

/// How long the bus is given to answer. For a running portal it answers in milliseconds;
/// the ticket (FR-8) and ADR 0026's 2 s cold-start limit leave room for this much and no more.
pub const BUDGET: Duration = Duration::from_millis(300);

/// How long after the launch — and after each ask that went unanswered — the kept bus is asked
/// again, and how long it is given each time. A portal that was slow at the launch (right after
/// login, a cold disk) has come up by the first; one still silent after the last is the one the
/// launch was right not to wait for, and it is left alone.
pub const ASKED_AGAIN_AFTER: [Duration; 3] = [
    Duration::from_secs(5),
    Duration::from_secs(15),
    Duration::from_secs(45),
];
pub const ASKED_AGAIN_FOR: Duration = Duration::from_secs(5);

/// The name GTK and WebKitGTK ask for.
const PORTAL: &str = "org.freedesktop.portal.Desktop";

/// What the session bus said when it was asked to start the portal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Heard {
    /// A reply of any kind inside the budget: the portal answered, or the bus answered that it
    /// cannot start it. GTK's own call will be answered as quickly.
    Answer,
    /// Nothing inside the budget: the bus is still starting a portal that is not coming up.
    Silence,
    /// No session bus could be reached at all.
    NoBus,
}

/// How this launch goes on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Start {
    /// With the session bus as it found it.
    AsItIs,
    /// Again, in place, on the bus at this address: the X display's, which GIO would have
    /// used and zbus cannot find.
    OnTheBus(String),
    /// Again, in place, with the session bus pointed at [`NO_SESSION_BUS`], saying `say` on
    /// standard error first. `kept` is the bus a chat gets instead ([`SESSION_BUS_KEPT`]):
    /// empty when there is none to give, or when a chat finds it by the standard lookup.
    WithoutTheBus { say: String, kept: String },
}

/// What a launch without the bus loses, in the words the window and standard error both use.
///
/// The keyring is named because the window's own reveal and copy of a keyring vault go through
/// the Secret Service on the session bus, in this process, which has none; a chat, and every
/// program the app starts, is given the bus and still reaches it.
const WHAT_IS_OFF: &str = "For this run there is no tray icon and no desktop notifications, \
     a keyring vault cannot be opened from the window (chats still reach the keyring), and a \
     second launch is refused instead of being handed to this one — charter-app#24.";

/// What a launch does about what it heard.
///
/// `address` is `DBUS_SESSION_BUS_ADDRESS` as this process was given it, and `heard` what the
/// bus the standard lookup finds said. `x_session_bus` looks for the bus GIO would autolaunch
/// on the X display and asks its portal; it is called only when nothing else is there.
pub fn decide(
    address: Option<&str>,
    heard: Heard,
    x_session_bus: &mut dyn FnMut() -> Option<(String, Heard)>,
) -> Start {
    // A launch that is already the restart never restarts again, whatever it hears.
    if address == Some(NO_SESSION_BUS) {
        return Start::AsItIs;
    }
    match heard {
        Heard::Answer => Start::AsItIs,
        // Found by the standard lookup when nothing named it, which a chat does as well.
        Heard::Silence => silent(address.unwrap_or_default()),
        // An address that names a dead bus already fails fast, in GIO too.
        Heard::NoBus if address.is_some() => Start::AsItIs,
        Heard::NoBus => match x_session_bus() {
            Some((bus, Heard::Answer)) => Start::OnTheBus(bus),
            Some((bus, Heard::Silence)) => silent(&bus),
            Some((_, Heard::NoBus)) | None => Start::WithoutTheBus {
                say: format!(
                    "purlis: there is no session bus to be had (none named, none at \
                     $XDG_RUNTIME_DIR/bus, and none on the X display), so purlis is starting \
                     without one. {WHAT_IS_OFF}"
                ),
                kept: String::new(),
            },
        },
    }
}

fn silent(kept: &str) -> Start {
    Start::WithoutTheBus {
        say: format!(
            "purlis: the desktop portal ({PORTAL}) did not answer on the session bus within \
             {} ms, so purlis is starting without the session bus instead of waiting out \
             D-Bus's 25 s. {WHAT_IS_OFF}",
            BUDGET.as_millis()
        ),
        kept: kept.to_owned(),
    }
}

/// The address in what `dbus-launch --sh-syntax` prints, when it printed one.
pub fn autolaunched_address(printed: &str) -> Option<String> {
    printed.lines().find_map(|line| {
        let value = line
            .strip_prefix(SESSION_BUS)?
            .strip_prefix("='")?
            .strip_suffix("';")?;
        (!value.is_empty()).then(|| value.to_owned())
    })
}

/// What came of asking something on a thread of its own, within a budget ([`in_time`]).
#[cfg(target_os = "linux")]
enum Asked<T> {
    /// It answered in time.
    Answered(T),
    /// No answer in time. The thread still waiting is left behind, and ends with the process.
    OutOfTime,
    /// There was no thread to ask on, or it ended without answering.
    NoThread,
}

/// Runs `ask` on a thread of its own and gives it `budget`.
#[cfg(target_os = "linux")]
fn in_time<T: Send + 'static>(
    budget: Duration,
    ask: impl FnOnce() -> T + Send + 'static,
) -> Asked<T> {
    use std::sync::mpsc::{self, RecvTimeoutError};

    let (said, heard) = mpsc::channel();
    let asking = std::thread::Builder::new()
        .name("charter-portal-probe".into())
        .spawn(move || {
            let _ = said.send(ask());
        });
    if asking.is_err() {
        return Asked::NoThread;
    }
    match heard.recv_timeout(budget) {
        Ok(answer) => Asked::Answered(answer),
        Err(RecvTimeoutError::Timeout) => Asked::OutOfTime,
        Err(RecvTimeoutError::Disconnected) => Asked::NoThread,
    }
}

/// Asks the session bus to start the portal — the bus at `address`, or the one this process
/// would find by the standard lookup when `None` — and says what came back within `budget`.
///
/// It is the call GTK's proxy makes, so it waits on exactly what GTK would. The budget covers
/// reaching the bus as well: a bus that does not even accept a connection in time is as silent
/// as a portal.
#[cfg(target_os = "linux")]
pub fn listen(address: Option<&str>, budget: Duration) -> Heard {
    let address = address.map(str::to_owned);
    match in_time(budget, move || start_the_portal(address.as_deref())) {
        Asked::Answered(heard) => heard,
        Asked::OutOfTime => Heard::Silence,
        // No thread to ask on: go on as if it answered, which is what every launch did before.
        Asked::NoThread => Heard::Answer,
    }
}

#[cfg(target_os = "linux")]
fn start_the_portal(address: Option<&str>) -> Heard {
    use zbus::blocking::connection::Builder;

    let builder = match address {
        Some(address) => Builder::address(address),
        None => Builder::session(),
    };
    let Ok(bus) = builder.and_then(Builder::build) else {
        return Heard::NoBus;
    };
    // The call GTK's proxy makes (the bus trace in ADR 0026 shows it), and one the BUS
    // answers rather than the portal: "already running" at once when the portal is up,
    // `ServiceUnknown` at once when nothing can start it, "started" as soon as it has taken its
    // name. Any reply is an answer. Only no reply is silence, and that is the caller's to time.
    let _ = bus.call_method(
        Some("org.freedesktop.DBus"),
        "/org/freedesktop/DBus",
        Some("org.freedesktop.DBus"),
        "StartServiceByName",
        &(PORTAL, 0u32),
    );
    Heard::Answer
}

/// The machine's id, from the first of `places` that holds one, as GIO reads it for
/// `--autolaunch`. A file that is there and empty — a container's `/etc/machine-id` often
/// is — holds none.
fn machine_id(places: &[&std::path::Path]) -> Option<String> {
    places.iter().find_map(|path| {
        let id = std::fs::read_to_string(path).ok()?;
        let id = id.trim();
        (!id.is_empty()).then(|| id.to_owned())
    })
}

/// The bus GIO would autolaunch on this X display, found or started the way GIO does it —
/// `dbus-launch --autolaunch=<machine id>`, which hands back the bus the display already holds
/// — and what its portal said. `None` with no X display, no machine id or no `dbus-launch`.
#[cfg(target_os = "linux")]
fn the_x_sessions_bus() -> Option<(String, Heard)> {
    std::env::var_os("DISPLAY").filter(|display| !display.is_empty())?;
    let machine = machine_id(&[
        std::path::Path::new("/etc/machine-id"),
        std::path::Path::new("/var/lib/dbus/machine-id"),
    ])?;
    let Asked::Answered(Ok(printed)) = in_time(BUDGET, move || {
        purlis_core::forklock::output(
            std::process::Command::new("dbus-launch")
                .arg(format!("--autolaunch={machine}"))
                .args(["--sh-syntax", "--close-stderr"])
                .stdin(std::process::Stdio::null()),
        )
    }) else {
        return None;
    };
    if !printed.status.success() {
        return None;
    }
    let bus = autolaunched_address(&String::from_utf8_lossy(&printed.stdout))?;
    let heard = listen(Some(&bus), BUDGET);
    Some((bus, heard))
}

/// Asks the portal, and when it is silent starts this launch again without the session bus.
///
/// Returns when the launch goes on as it is. When it restarts, it does not return: `exec`
/// replaces this process with the same binary, the same arguments and the same process id,
/// and the only difference is the session bus. A restart that cannot happen is said on
/// standard error and the launch goes on, slow, as it always did.
#[cfg(target_os = "linux")]
pub fn start_clear_of_a_silent_portal() {
    use std::os::unix::process::CommandExt;

    let address = std::env::var(SESSION_BUS).ok();
    if address.as_deref() == Some(NO_SESSION_BUS) {
        return;
    }
    let again = match decide(
        address.as_deref(),
        listen(None, BUDGET),
        &mut the_x_sessions_bus,
    ) {
        Start::AsItIs => return,
        Start::OnTheBus(bus) => Bus::At(bus),
        Start::WithoutTheBus { say, kept } => {
            tracing::warn!("{say}");
            Bus::Without { kept }
        }
    };
    let failed = match this_launch_again(&again) {
        Ok(mut command) => command.exec(),
        Err(err) => err,
    };
    tracing::warn!("purlis: could not start again ({failed}); going on with the bus as it was.");
}

/// Which session bus a launch of this binary is started on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Bus {
    /// This one; nothing kept aside.
    At(String),
    /// The default one, which the standard lookup finds at `$XDG_RUNTIME_DIR/bus`: no address
    /// named, nothing kept aside.
    Default,
    /// None, with `kept` kept aside for the chats.
    Without { kept: String },
}

impl Bus {
    /// The address to ask this bus's portal at: `runtime` is `$XDG_RUNTIME_DIR`, where the
    /// default one is looked for. `None` for no bus, and for a default one that is not there.
    pub fn where_to_ask(&self, runtime: Option<&std::path::Path>) -> Option<String> {
        match self {
            Self::At(address) => Some(address.clone()),
            Self::Default => runtime
                .map(|dir| dir.join("bus"))
                .filter(|bus| bus.exists())
                .map(|bus| format!("unix:path={}", bus.display())),
            Self::Without { .. } => None,
        }
    }
}

/// This binary with this launch's arguments, on `bus`.
pub fn this_launch_again(bus: &Bus) -> std::io::Result<std::process::Command> {
    let mut args = std::env::args_os();
    Ok(launch_on(std::env::current_exe()?, args.next(), args, bus))
}

/// `exe`, called `name`, with `args`, on `bus`.
fn launch_on(
    exe: std::path::PathBuf,
    name: Option<std::ffi::OsString>,
    args: impl IntoIterator<Item = std::ffi::OsString>,
    bus: &Bus,
) -> std::process::Command {
    let mut again = std::process::Command::new(exe);
    #[cfg(unix)]
    if let Some(name) = name {
        use std::os::unix::process::CommandExt;
        again.arg0(name);
    }
    #[cfg(not(unix))]
    let _ = name;
    again.args(args);
    // The kept bus under either of its names (V93k): an app started by an older one may
    // have inherited it as `CHARTER_SESSION_BUS_KEPT`.
    let forget_the_kept = |again: &mut std::process::Command| {
        for kept in purlis_core::envvar::spellings(SESSION_BUS_KEPT) {
            again.env_remove(kept);
        }
    };
    match bus {
        Bus::At(address) => {
            again.env(SESSION_BUS, address);
            forget_the_kept(&mut again);
        }
        Bus::Default => {
            again.env_remove(SESSION_BUS);
            forget_the_kept(&mut again);
        }
        Bus::Without { kept } => {
            again
                .env(SESSION_BUS, NO_SESSION_BUS)
                .env(SESSION_BUS_KEPT, kept);
        }
    }
    again
}

/// What the window is told about a launch without the session bus.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct BusNotice {
    /// The line it draws: why there is no bus, and what is off for the run.
    pub says: String,
    /// Whether the bus answers now, so a restart onto it is worth offering.
    pub can_restart: bool,
}

/// This launch and the session bus, as the window is told about it (`session_bus`) and as the
/// ask after the launch finds it (`listen_again`).
#[derive(Debug)]
pub struct SessionBus {
    /// The line the window draws, when the launch is without the bus.
    says: Option<String>,
    /// The bus to ask again and to go back onto: `At` the kept address, or the `Default` one.
    /// `None` when there is nothing to go back onto.
    back: Option<Bus>,
    /// `$XDG_RUNTIME_DIR`, where the default bus is asked.
    runtime: Option<std::path::PathBuf>,
    /// Whether it answered when it was asked again.
    answers: std::sync::atomic::AtomicBool,
    /// The bus the operator asked to start again on, once they have.
    restart: std::sync::Mutex<Option<Bus>>,
}

impl SessionBus {
    /// `address` and `kept` as this process was given `DBUS_SESSION_BUS_ADDRESS` and
    /// [`SESSION_BUS_KEPT`]; `runtime` is `$XDG_RUNTIME_DIR`, where the standard lookup looks.
    pub fn of(
        address: Option<&str>,
        kept: Option<&str>,
        runtime: Option<&std::path::Path>,
    ) -> Self {
        if address != Some(NO_SESSION_BUS) {
            return Self {
                says: None,
                back: None,
                runtime: None,
                answers: false.into(),
                restart: std::sync::Mutex::default(),
            };
        }
        let kept = kept.unwrap_or_default();
        let back = if kept.is_empty() {
            Some(Bus::Default).filter(|bus| bus.where_to_ask(runtime).is_some())
        } else {
            Some(Bus::At(kept.to_owned()))
        };
        let why = if back.is_some() {
            format!(
                "purlis started without the session bus: the desktop portal did not answer \
                 on it within {} ms, and waiting would have cost half a minute.",
                BUDGET.as_millis()
            )
        } else {
            "purlis found no session bus to start on.".to_owned()
        };
        Self {
            says: Some(format!("{why} {WHAT_IS_OFF}")),
            back,
            runtime: runtime.map(std::path::Path::to_path_buf),
            answers: false.into(),
            restart: std::sync::Mutex::default(),
        }
    }

    /// What the window draws: nothing on a launch with the bus.
    pub fn now(&self) -> Option<BusNotice> {
        self.says.as_ref().map(|says| BusNotice {
            says: says.clone(),
            can_restart: self.back_onto().is_some(),
        })
    }

    /// The address to ask again after the launch, when there is a bus to go back onto.
    pub fn where_to_ask_again(&self) -> Option<String> {
        self.back
            .as_ref()
            .and_then(|bus| bus.where_to_ask(self.runtime.as_deref()))
    }

    /// The bus answered when it was asked again.
    pub fn answered(&self) {
        self.answers
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// The bus a restart goes back onto: only one that has answered.
    pub fn back_onto(&self) -> Option<Bus> {
        self.back
            .clone()
            .filter(|_| self.answers.load(std::sync::atomic::Ordering::SeqCst))
    }

    /// The operator asked to start again on the bus: refused unless it has answered.
    pub fn restart_asked(&self) -> Result<(), String> {
        let onto = self.back_onto().ok_or_else(|| {
            "the session bus has not answered since the launch, so there is nothing to start \
             again on"
                .to_owned()
        })?;
        *self
            .restart
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(onto);
        Ok(())
    }

    /// The bus to start again on at `Exit`, when the operator asked for it.
    pub fn restart_wanted(&self) -> Option<Bus> {
        self.restart
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

/// The event the window hears when the kept bus answers after the launch.
pub const ANSWERS: &str = "session-bus://answers";

/// Asks the kept bus again, off the main thread, after each of [`ASKED_AGAIN_AFTER`]'s waits
/// until it answers: a portal that was only slow — right after login, a cold disk — has come up
/// by then. When it answers the window is told, and offers the restart; nothing restarts by
/// itself.
#[cfg(target_os = "linux")]
pub fn listen_again(app: &tauri::AppHandle) {
    use tauri::{Emitter, Manager};

    let Some(address) = app.state::<SessionBus>().where_to_ask_again() else {
        return;
    };
    let app = app.clone();
    let _ = std::thread::Builder::new()
        .name("charter-portal-asked-again".into())
        .spawn(move || {
            let answered =
                ask_until_answered(&ASKED_AGAIN_AFTER, &mut std::thread::sleep, &mut || {
                    listen(Some(&address), ASKED_AGAIN_FOR)
                });
            if !answered {
                return;
            }
            tracing::info!(
                "purlis: the desktop portal answers on the session bus now; the window offers \
                 to restart on it."
            );
            let bus = app.state::<SessionBus>();
            bus.answered();
            if let Some(now) = bus.now() {
                let _ = app.emit(ANSWERS, now);
            }
        });
}

/// What the window says about this launch and the session bus: nothing, on a launch with it.
#[tauri::command]
#[specta::specta]
pub fn session_bus(bus: tauri::State<'_, SessionBus>) -> Option<BusNotice> {
    bus.now()
}

/// Quit, and start again on the session bus, now that it answers. The operator's choice,
/// from the window's notice: charter never does this by itself, because a restart ends every
/// chat. It goes the way a quit goes — every plane writes what was open, so the launch after
/// it offers them back — and the new launch is started last, at `Exit` ([`restart_if_asked`]).
#[tauri::command]
#[specta::specta]
pub fn restart_on_the_session_bus(
    app: tauri::AppHandle,
    bus: tauri::State<'_, SessionBus>,
) -> Result<(), String> {
    bus.restart_asked()?;
    app.exit(0);
    Ok(())
}

/// Waits each of `waits` in turn and asks after it, until an ask is answered. Whether one was.
pub fn ask_until_answered(
    waits: &[Duration],
    wait: &mut dyn FnMut(Duration),
    ask: &mut dyn FnMut() -> Heard,
) -> bool {
    waits.iter().any(|&after| {
        wait(after);
        ask() == Heard::Answer
    })
}

/// At `Exit`, after everything else has let go: the launch the operator asked for, if any.
pub fn restart_if_asked(bus: &SessionBus) {
    restart_if_asked_with(bus, &mut |onto| {
        this_launch_again(onto)
            .and_then(|mut again| purlis_core::forklock::spawn(&mut again).map(drop))
    });
}

/// [`restart_if_asked`], with the start handed in so a test can watch it.
fn restart_if_asked_with(bus: &SessionBus, start: &mut dyn FnMut(&Bus) -> std::io::Result<()>) {
    let Some(onto) = bus.restart_wanted() else {
        return;
    };
    if let Err(why) = start(&onto) {
        tracing::warn!(
            "purlis: could not start again on the session bus ({why}); start purlis again"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RUN_USER_BUS: &str = "unix:path=/run/user/1000/bus";
    const AUTOLAUNCHED: &str =
        "unix:path=/tmp/dbus-3Awemqh910,guid=6eda85db4fed2c76c15d1ae76abd3017";

    /// No X session to autolaunch a bus on: what `decide` is handed when it must not be asked.
    fn not_asked() -> Option<(String, Heard)> {
        panic!("the X session's bus was looked for when it did not have to be")
    }

    fn nothing_there() -> Option<(String, Heard)> {
        None
    }

    fn says_what_is_off(said: &str) {
        assert!(
            said.contains("tray") && said.contains("notifications"),
            "{said}"
        );
        assert!(said.contains("second launch"), "{said}");
        assert!(said.contains("charter-app#24"), "{said}");
    }

    #[test]
    fn a_portal_that_answered_leaves_the_launch_as_it_is() {
        assert_eq!(
            decide(Some(RUN_USER_BUS), Heard::Answer, &mut not_asked),
            Start::AsItIs
        );
        assert_eq!(decide(None, Heard::Answer, &mut not_asked), Start::AsItIs);
    }

    #[test]
    fn a_silent_portal_starts_the_launch_again_without_the_bus_keeping_the_bus_for_chats() {
        let Start::WithoutTheBus { say, kept } =
            decide(Some(RUN_USER_BUS), Heard::Silence, &mut not_asked)
        else {
            panic!("a silent portal did not restart the launch");
        };
        assert!(say.contains("300 ms"), "{say}");
        says_what_is_off(&say);
        assert_eq!(kept, RUN_USER_BUS);
    }

    /// Found by the standard lookup (`$XDG_RUNTIME_DIR/bus`), which a chat does too.
    #[test]
    fn a_silent_portal_on_a_bus_nothing_named_keeps_no_address() {
        let Start::WithoutTheBus { kept, .. } = decide(None, Heard::Silence, &mut not_asked) else {
            panic!("a silent portal did not restart the launch");
        };
        assert_eq!(kept, "");
    }

    #[test]
    fn a_named_bus_that_is_not_there_already_fails_fast_and_is_left_alone() {
        assert_eq!(
            decide(Some("unix:path=/nowhere"), Heard::NoBus, &mut not_asked),
            Start::AsItIs
        );
    }

    /// `startx` into i3: no bus named and none at `$XDG_RUNTIME_DIR/bus`. GIO would autolaunch
    /// one on the X display — the one a notification daemon started from the i3 config is
    /// already on — and zbus, which the tray, notifications and single-instance use, would
    /// not find it. So the launch looks for it as GIO does, and starts again on it.
    #[test]
    fn with_no_bus_named_the_launch_starts_again_on_the_bus_of_the_x_session() {
        let mut autolaunched = || Some((AUTOLAUNCHED.to_owned(), Heard::Answer));

        assert_eq!(
            decide(None, Heard::NoBus, &mut autolaunched),
            Start::OnTheBus(AUTOLAUNCHED.to_owned())
        );
    }

    #[test]
    fn a_silent_portal_on_the_bus_of_the_x_session_starts_without_it_and_keeps_it_for_chats() {
        let mut autolaunched = || Some((AUTOLAUNCHED.to_owned(), Heard::Silence));

        let Start::WithoutTheBus { say, kept } = decide(None, Heard::NoBus, &mut autolaunched)
        else {
            panic!("a silent portal did not restart the launch");
        };
        says_what_is_off(&say);
        assert_eq!(kept, AUTOLAUNCHED);
    }

    #[test]
    fn with_no_bus_anywhere_the_launch_says_what_that_costs_and_keeps_no_address() {
        let mut unreachable = || Some((AUTOLAUNCHED.to_owned(), Heard::NoBus));
        for autolaunched in [
            &mut nothing_there as &mut dyn FnMut() -> _,
            &mut unreachable,
        ] {
            let Start::WithoutTheBus { say, kept } = decide(None, Heard::NoBus, autolaunched)
            else {
                panic!("a launch with no bus went on as if it had one");
            };
            assert!(say.contains("no session bus"), "{say}");
            says_what_is_off(&say);
            assert_eq!(kept, "");
        }
    }

    #[test]
    fn a_launch_that_is_already_the_restart_never_restarts_again() {
        for heard in [Heard::Answer, Heard::Silence, Heard::NoBus] {
            assert_eq!(
                decide(Some(NO_SESSION_BUS), heard, &mut not_asked),
                Start::AsItIs,
                "{heard:?}"
            );
        }
    }

    /// What a restart's environment says about the bus: set, removed, or left as it was.
    fn bus_of(again: &std::process::Command) -> Vec<(String, Option<String>)> {
        let mut said: Vec<(String, Option<String>)> = again
            .get_envs()
            .map(|(name, value)| {
                (
                    name.to_string_lossy().into_owned(),
                    value.map(|v| v.to_string_lossy().into_owned()),
                )
            })
            .collect();
        said.sort();
        said
    }

    fn again(bus: &Bus) -> std::process::Command {
        launch_on(
            "/usr/bin/charter-app".into(),
            Some("charter-app".into()),
            ["--no-restore".into()],
            bus,
        )
    }

    #[test]
    fn a_restart_without_the_bus_points_it_at_nothing_and_keeps_the_bus_it_had() {
        let without = again(&Bus::Without {
            kept: RUN_USER_BUS.to_owned(),
        });

        assert_eq!(
            bus_of(&without),
            [
                (SESSION_BUS.to_owned(), Some(NO_SESSION_BUS.to_owned())),
                (SESSION_BUS_KEPT.to_owned(), Some(RUN_USER_BUS.to_owned())),
            ]
        );
        assert_eq!(
            without.get_args().collect::<Vec<_>>(),
            ["--no-restore"],
            "the restart lost this launch's arguments"
        );
    }

    #[test]
    fn a_restart_onto_a_bus_names_it_and_keeps_nothing_aside() {
        assert_eq!(
            bus_of(&again(&Bus::At(AUTOLAUNCHED.to_owned()))),
            [
                ("CHARTER_SESSION_BUS_KEPT".to_owned(), None),
                (SESSION_BUS.to_owned(), Some(AUTOLAUNCHED.to_owned())),
                (SESSION_BUS_KEPT.to_owned(), None),
            ]
        );
        assert_eq!(
            bus_of(&again(&Bus::Default)),
            [
                ("CHARTER_SESSION_BUS_KEPT".to_owned(), None),
                (SESSION_BUS.to_owned(), None),
                (SESSION_BUS_KEPT.to_owned(), None),
            ]
        );
    }

    #[test]
    fn a_launch_on_the_bus_has_nothing_to_tell_the_window() {
        let bus = SessionBus::of(Some(RUN_USER_BUS), None, None);

        assert_eq!(bus.now(), None);
        assert_eq!(bus.where_to_ask_again(), None);
    }

    #[test]
    fn a_launch_away_from_a_silent_portal_tells_the_window_why_and_what_is_off() {
        let bus = SessionBus::of(Some(NO_SESSION_BUS), Some(RUN_USER_BUS), None);

        let Some(told) = bus.now() else {
            panic!("the window was not told the launch has no bus");
        };
        assert!(
            told.says.contains("desktop portal did not answer"),
            "{told:?}"
        );
        for off in ["tray icon", "notifications", "second launch", "keyring"] {
            assert!(told.says.contains(off), "{off} is not named: {told:?}");
        }
        assert!(!told.can_restart, "{told:?}");
    }

    #[test]
    fn a_launch_with_no_bus_at_all_tells_the_window_so() {
        for kept in [Some(""), None] {
            let told = SessionBus::of(Some(NO_SESSION_BUS), kept, None)
                .now()
                .expect("the window is told");
            assert!(told.says.contains("no session bus"), "{told:?}");
            assert!(told.says.contains("notifications"), "{told:?}");
        }
    }

    #[test]
    fn the_restart_is_offered_only_once_the_kept_bus_answers_and_goes_back_onto_it() {
        let bus = SessionBus::of(Some(NO_SESSION_BUS), Some(RUN_USER_BUS), None);
        assert_eq!(bus.back_onto(), None, "offered before anything answered");
        assert!(bus.restart_asked().is_err(), "a restart onto a silent bus");
        assert_eq!(bus.restart_wanted(), None);

        bus.answered();

        assert!(bus.now().expect("still told").can_restart);
        assert_eq!(bus.restart_wanted(), None, "restarted without being asked");
        bus.restart_asked().expect("the operator's restart");
        assert_eq!(bus.restart_wanted(), Some(Bus::At(RUN_USER_BUS.to_owned())));
    }

    #[test]
    fn the_kept_bus_is_asked_again_where_the_launch_found_it() {
        let runtime = tempfile::tempdir().expect("a directory");
        std::fs::write(runtime.path().join("bus"), "").expect("a socket's stand-in");

        assert_eq!(
            SessionBus::of(Some(NO_SESSION_BUS), Some(RUN_USER_BUS), None).where_to_ask_again(),
            Some(RUN_USER_BUS.to_owned())
        );
        let found = SessionBus::of(Some(NO_SESSION_BUS), Some(""), Some(runtime.path()));
        assert_eq!(
            found.where_to_ask_again(),
            Some(format!(
                "unix:path={}",
                runtime.path().join("bus").display()
            ))
        );
        found.answered();
        assert_eq!(found.back_onto(), Some(Bus::Default));

        let nowhere = tempfile::tempdir().expect("a directory");
        let none = SessionBus::of(Some(NO_SESSION_BUS), Some(""), Some(nowhere.path()));
        assert_eq!(none.where_to_ask_again(), None);
        none.answered();
        assert_eq!(none.back_onto(), None, "a restart offered onto no bus");
    }

    /// A container's `/etc/machine-id` is often there and empty; D-Bus's own copy is the next
    /// place GIO looks, and so is it here.
    #[test]
    fn the_machine_id_is_the_first_one_that_is_not_empty() {
        let dir = tempfile::tempdir().expect("a directory");
        let (etc, dbus) = (dir.path().join("etc"), dir.path().join("dbus"));
        std::fs::write(&etc, "").expect("an empty id");
        std::fs::write(&dbus, "c157283747a2ad020fa749a86abd2fb4\n").expect("an id");

        assert_eq!(
            machine_id(&[&etc, &dbus]).as_deref(),
            Some("c157283747a2ad020fa749a86abd2fb4")
        );
        assert_eq!(machine_id(&[&etc, &dir.path().join("none")]), None);
    }

    #[test]
    fn the_bus_is_asked_again_with_growing_waits_until_it_answers() {
        let mut waited = Vec::new();
        let mut heard = [Heard::Silence, Heard::Answer].into_iter();

        let answered = ask_until_answered(
            &ASKED_AGAIN_AFTER,
            &mut |wait| waited.push(wait),
            &mut || heard.next().expect("asked once too often"),
        );

        assert!(answered);
        assert_eq!(waited, ASKED_AGAIN_AFTER[..2]);
    }

    #[test]
    fn a_bus_that_never_answers_is_asked_each_time_and_then_left_alone() {
        let mut asked = 0;

        let answered = ask_until_answered(&ASKED_AGAIN_AFTER, &mut |_| {}, &mut || {
            asked += 1;
            Heard::Silence
        });

        assert!(!answered);
        assert_eq!(asked, ASKED_AGAIN_AFTER.len());
        assert!(ASKED_AGAIN_AFTER.len() > 1, "only one ask after the launch");
        assert!(
            ASKED_AGAIN_AFTER.windows(2).all(|w| w[0] < w[1]),
            "the waits do not back off"
        );
    }

    #[test]
    fn at_exit_the_launch_on_the_bus_is_started_only_when_the_operator_asked() {
        let bus = SessionBus::of(Some(NO_SESSION_BUS), Some(RUN_USER_BUS), None);
        let mut started = Vec::new();

        restart_if_asked_with(&bus, &mut |onto| {
            started.push(onto.clone());
            Ok(())
        });
        bus.answered();
        restart_if_asked_with(&bus, &mut |onto| {
            started.push(onto.clone());
            Ok(())
        });
        assert_eq!(started, [], "started without the operator asking");

        bus.restart_asked().expect("the operator's restart");
        restart_if_asked_with(&bus, &mut |onto| {
            started.push(onto.clone());
            Ok(())
        });

        assert_eq!(started, [Bus::At(RUN_USER_BUS.to_owned())]);
    }

    #[test]
    fn each_bus_says_where_it_is_asked() {
        let runtime = tempfile::tempdir().expect("a directory");
        std::fs::write(runtime.path().join("bus"), "").expect("a socket's stand-in");
        let nowhere = tempfile::tempdir().expect("a directory");

        assert_eq!(
            Bus::At(RUN_USER_BUS.to_owned()).where_to_ask(None),
            Some(RUN_USER_BUS.to_owned())
        );
        assert_eq!(
            Bus::Default.where_to_ask(Some(runtime.path())),
            Some(format!(
                "unix:path={}",
                runtime.path().join("bus").display()
            ))
        );
        assert_eq!(Bus::Default.where_to_ask(Some(nowhere.path())), None);
        assert_eq!(Bus::Default.where_to_ask(None), None);
        assert_eq!(
            Bus::Without {
                kept: RUN_USER_BUS.to_owned()
            }
            .where_to_ask(None),
            None
        );
    }

    #[test]
    fn the_address_is_read_from_what_dbus_launch_prints() {
        let printed = format!(
            "DBUS_SESSION_BUS_ADDRESS='{AUTOLAUNCHED}';\nexport DBUS_SESSION_BUS_ADDRESS;\n\
             DBUS_SESSION_BUS_PID=31;\nDBUS_SESSION_BUS_WINDOWID=2097153;\n"
        );

        assert_eq!(
            autolaunched_address(&printed).as_deref(),
            Some(AUTOLAUNCHED)
        );
        assert_eq!(autolaunched_address(""), None);
        assert_eq!(autolaunched_address("DBUS_SESSION_BUS_ADDRESS='';\n"), None);
    }

    /// A private `dbus-daemon` — the one every Linux desktop runs — with the services this
    /// test declares activatable, and nothing else.
    #[cfg(target_os = "linux")]
    struct PrivateBus {
        daemon: std::process::Child,
        address: String,
        _dir: tempfile::TempDir,
    }

    #[cfg(target_os = "linux")]
    impl PrivateBus {
        /// `services` is `(name, program)`: the bus starts `program` when `name` is asked for.
        fn with(services: &[(&str, &str)]) -> Self {
            use std::io::BufRead;

            let dir = tempfile::tempdir().expect("a directory");
            let services_dir = dir.path().join("services");
            std::fs::create_dir(&services_dir).expect("the services directory");
            for (name, program) in services {
                std::fs::write(
                    services_dir.join(format!("{name}.service")),
                    format!("[D-BUS Service]\nName={name}\nExec={program}\n"),
                )
                .expect("a service file");
            }
            let config = dir.path().join("session.conf");
            std::fs::write(
                &config,
                format!(
                    r#"<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>session</type>
  <listen>unix:path={}</listen>
  <servicedir>{}</servicedir>
  <policy context="default">
    <allow send_destination="*" eavesdrop="true"/>
    <allow eavesdrop="true"/>
    <allow own="*"/>
  </policy>
</busconfig>
"#,
                    dir.path().join("bus").display(),
                    services_dir.display()
                ),
            )
            .expect("a bus config");
            let mut daemon = purlis_core::forklock::spawn(
                std::process::Command::new("dbus-daemon")
                    .arg(format!("--config-file={}", config.display()))
                    .args(["--nofork", "--print-address"])
                    .stdout(std::process::Stdio::piped()),
            )
            .expect("dbus-daemon runs (apt: dbus-daemon)");
            let mut address = String::new();
            std::io::BufReader::new(daemon.stdout.take().expect("its stdout"))
                .read_line(&mut address)
                .expect("the bus prints its address");
            Self {
                daemon,
                address: address.trim().to_owned(),
                _dir: dir,
            }
        }
    }

    /// How long a test that expects an answer waits for one. The answer itself is not timed:
    /// the bus replies in milliseconds, but a CI runner running every crate's tests at once can
    /// leave the asking thread unscheduled past [`BUDGET`], and a test that then heard
    /// "silence" was failing on the runner's load, not on what the bus said (main, run
    /// 36772854804). Silence is still heard within [`BUDGET`]; see the test that times it.
    #[cfg(target_os = "linux")]
    const AN_ANSWER_WITHIN: Duration = Duration::from_secs(10);

    #[cfg(target_os = "linux")]
    impl PrivateBus {
        /// The names this bus can start, as the bus itself lists them.
        fn activatable(&self) -> Vec<String> {
            let bus = zbus::blocking::connection::Builder::address(self.address.as_str())
                .expect("an address")
                .build()
                .expect("the test's bus");
            let reply = bus
                .call_method(
                    Some("org.freedesktop.DBus"),
                    "/org/freedesktop/DBus",
                    Some("org.freedesktop.DBus"),
                    "ListActivatableNames",
                    &(),
                )
                .expect("the bus lists what it can start");
            reply.body().deserialize().expect("a list of names")
        }
    }

    #[cfg(target_os = "linux")]
    impl Drop for PrivateBus {
        fn drop(&mut self) {
            let _ = self.daemon.kill();
            let _ = self.daemon.wait();
        }
    }

    /// charter-app#24's machine, in miniature: the portal is activatable, and what the bus
    /// starts for it never takes the name — as `xdg-desktop-portal` does not, while it waits
    /// on a backend that needs a desktop session. GTK waited 25 s on exactly this.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_portal_the_bus_cannot_bring_up_is_heard_as_silence_within_the_budget() {
        let bus = PrivateBus::with(&[(PORTAL, "/bin/sleep 60")]);

        let from = std::time::Instant::now();
        let heard = listen(Some(&bus.address), BUDGET);
        let took = from.elapsed();

        assert_eq!(heard, Heard::Silence);
        // Well short of the 25 s it replaces, with room for a loaded runner's scheduling.
        assert!(
            took < BUDGET + Duration::from_secs(2),
            "listening took {took:?} against a {BUDGET:?} budget"
        );
    }

    /// The premise every test below stands on: the bus a test starts can start what that test
    /// declares and nothing else — whatever portals the machine running the tests has
    /// installed. Its config names one service directory, its own, and no standard ones.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_test_bus_can_start_only_what_its_test_declares() {
        assert!(
            !PrivateBus::with(&[])
                .activatable()
                .iter()
                .any(|name| name == PORTAL),
            "the machine's own portal reached a test's bus"
        );
        assert!(
            PrivateBus::with(&[(PORTAL, "/bin/false")])
                .activatable()
                .iter()
                .any(|name| name == PORTAL),
            "a declared service is not activatable"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_bus_with_no_portal_to_start_answers() {
        let bus = PrivateBus::with(&[]);

        assert_eq!(listen(Some(&bus.address), AN_ANSWER_WITHIN), Heard::Answer);
    }

    /// A portal that exits instead of hanging: the bus says it failed as soon as it has.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_portal_that_fails_to_start_is_answered_for_by_the_bus() {
        let bus = PrivateBus::with(&[(PORTAL, "/bin/false")]);

        assert_eq!(listen(Some(&bus.address), AN_ANSWER_WITHIN), Heard::Answer);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_running_portal_answers() {
        let bus = PrivateBus::with(&[]);
        // Someone owns the name, as `xdg-desktop-portal` does on a working desktop.
        let _portal = zbus::blocking::connection::Builder::address(bus.address.as_str())
            .expect("an address")
            .name(PORTAL)
            .expect("a name")
            .build()
            .expect("the portal is on the bus");

        assert_eq!(listen(Some(&bus.address), AN_ANSWER_WITHIN), Heard::Answer);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn no_bus_at_the_address_is_heard_as_no_bus() {
        assert_eq!(listen(Some(NO_SESSION_BUS), BUDGET), Heard::NoBus);
    }
}
