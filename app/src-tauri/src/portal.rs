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
//! under 0.6 s, because every bus call fails at once. What that costs for the run is named on
//! standard error: no tray, no notifications, and a second launch refused rather than handed
//! over — `instance.rs` is what keeps two apps off one plane without the bus.
//!
//! **X11 without a session bus is the same case by another road.** With no
//! `DBUS_SESSION_BUS_ADDRESS` and no `$XDG_RUNTIME_DIR/bus` — `startx` into i3, say — GIO
//! does not give up: it runs `dbus-launch --autolaunch` and starts a bus of its own, where the
//! portal is activatable again. charter has no bus there to lose, so it pins the address to
//! nothing and GIO never launches one.

use std::time::Duration;

/// How long the bus is given to answer. For a running portal it answers in milliseconds;
/// the ticket (FR-8) and ADR 0026's 2 s cold-start limit leave room for this much and no more.
pub const BUDGET: Duration = Duration::from_millis(300);

/// Where the app points the session bus when it starts without one. Nothing can listen at a
/// path below `/dev/null`, so every bus call fails at once — which is the fast case the issue
/// measured — and the name says what happened to anyone who reads `/proc/<pid>/environ`.
pub const NO_SESSION_BUS: &str = "unix:path=/dev/null/charter-started-without-the-session-bus";

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
    /// Again, in place, with the session bus pointed at [`NO_SESSION_BUS`] — saying `Some`
    /// line on standard error first, when there is something the operator loses by it.
    WithoutTheBus(Option<String>),
}

/// What a launch does about what it heard.
///
/// `address` is `DBUS_SESSION_BUS_ADDRESS` as this process was given it.
pub fn decide(heard: Heard, address: Option<&str>) -> Start {
    // A launch that is already the restart never restarts again, whatever it hears.
    if address == Some(NO_SESSION_BUS) {
        return Start::AsItIs;
    }
    match heard {
        Heard::Answer => Start::AsItIs,
        Heard::Silence => Start::WithoutTheBus(Some(format!(
            "charter: the desktop portal ({PORTAL}) did not answer on the session bus within \
             {} ms, so charter is starting without the session bus instead of waiting out \
             D-Bus's 25 s. For this run there is no tray icon and no desktop notifications, and \
             a second launch is refused instead of being handed to this one — charter-app#24.",
            BUDGET.as_millis()
        ))),
        // No bus to lose. Pinned only when nothing names one, which is when GIO would
        // autolaunch its own on X11; an address that names a dead bus already fails fast.
        Heard::NoBus if address.is_none() => Start::WithoutTheBus(None),
        Heard::NoBus => Start::AsItIs,
    }
}

/// Asks the session bus to start the portal — the bus at `address`, or the one this process
/// would find by the standard lookup when `None` — and says what came back within `within`.
///
/// It is the call GTK's proxy makes, so it waits on exactly what GTK would. It is asked on a thread of its own, and the budget covers reaching the bus
/// as well: a bus that does not even accept a connection in time is as silent as a portal.
/// A thread still waiting when the budget runs out is left behind; it is ended with the process
/// when the launch restarts.
#[cfg(target_os = "linux")]
pub fn listen(address: Option<&str>, within: Duration) -> Heard {
    use std::sync::mpsc::{self, RecvTimeoutError};

    let address = address.map(str::to_owned);
    let (said, heard) = mpsc::channel();
    let asking = std::thread::Builder::new()
        .name("charter-portal-probe".into())
        .spawn(move || {
            let _ = said.send(start_the_portal(address.as_deref()));
        });
    if asking.is_err() {
        // No thread to ask on: go on as if it answered, which is what every launch did before.
        return Heard::Answer;
    }
    match heard.recv_timeout(within) {
        Ok(heard) => heard,
        Err(RecvTimeoutError::Timeout) => Heard::Silence,
        Err(RecvTimeoutError::Disconnected) => Heard::Answer,
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

/// Asks the portal, and when it is silent starts this launch again without the session bus.
///
/// Returns when the launch goes on as it is. When it restarts, it does not return: `exec`
/// replaces this process with the same binary, the same arguments and the same process id,
/// and the only difference is `DBUS_SESSION_BUS_ADDRESS`. A restart that cannot happen is
/// said on standard error and the launch goes on, slow, as it always did.
#[cfg(target_os = "linux")]
pub fn start_clear_of_a_silent_portal() {
    let address = std::env::var("DBUS_SESSION_BUS_ADDRESS").ok();
    if address.as_deref() == Some(NO_SESSION_BUS) {
        return;
    }
    let Start::WithoutTheBus(say) = decide(listen(None, BUDGET), address.as_deref()) else {
        return;
    };
    if let Some(line) = say {
        eprintln!("{line}");
    }
    let failed = restart_without_the_bus();
    eprintln!(
        "charter: could not start again without the session bus ({failed}); going on with it."
    );
}

/// `exec`s this binary again with the session bus pointed at nothing. Returns only on failure.
#[cfg(target_os = "linux")]
fn restart_without_the_bus() -> std::io::Error {
    use std::os::unix::process::CommandExt;

    let exe = match std::env::current_exe() {
        Ok(exe) => exe,
        Err(err) => return err,
    };
    let mut args = std::env::args_os();
    let mut again = std::process::Command::new(exe);
    if let Some(name) = args.next() {
        again.arg0(name);
    }
    again
        .args(args)
        .env("DBUS_SESSION_BUS_ADDRESS", NO_SESSION_BUS)
        .exec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_portal_that_answered_leaves_the_launch_as_it_is() {
        assert_eq!(
            decide(Heard::Answer, Some("unix:path=/run/user/1000/bus")),
            Start::AsItIs
        );
        assert_eq!(decide(Heard::Answer, None), Start::AsItIs);
    }

    #[test]
    fn a_silent_portal_starts_the_launch_again_without_the_bus_and_says_what_that_costs() {
        let Start::WithoutTheBus(Some(said)) =
            decide(Heard::Silence, Some("unix:path=/run/user/1000/bus"))
        else {
            panic!("a silent portal did not restart the launch");
        };
        assert!(said.contains("300 ms"), "{said}");
        assert!(
            said.contains("tray") && said.contains("notifications"),
            "{said}"
        );
        assert!(said.contains("second launch"), "{said}");
        assert!(said.contains("charter-app#24"), "{said}");
    }

    #[test]
    fn with_no_bus_named_the_address_is_pinned_so_gio_cannot_autolaunch_one_and_nothing_is_said() {
        assert_eq!(decide(Heard::NoBus, None), Start::WithoutTheBus(None));
    }

    #[test]
    fn a_named_bus_that_is_not_there_already_fails_fast_and_is_left_alone() {
        assert_eq!(
            decide(Heard::NoBus, Some("unix:path=/nowhere")),
            Start::AsItIs
        );
    }

    #[test]
    fn a_launch_that_is_already_the_restart_never_restarts_again() {
        for heard in [Heard::Answer, Heard::Silence, Heard::NoBus] {
            assert_eq!(
                decide(heard, Some(NO_SESSION_BUS)),
                Start::AsItIs,
                "{heard:?}"
            );
        }
    }

    /// A private `dbus-daemon` — the one every Linux desktop runs — with the services this
    /// test declares activatable, and nothing else.
    #[cfg(target_os = "linux")]
    struct Bus {
        daemon: std::process::Child,
        address: String,
        _dir: tempfile::TempDir,
    }

    #[cfg(target_os = "linux")]
    impl Bus {
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
            let mut daemon = charter_core::forklock::spawn(
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

    #[cfg(target_os = "linux")]
    impl Drop for Bus {
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
        let bus = Bus::with(&[(PORTAL, "/bin/sleep 60")]);

        let from = std::time::Instant::now();
        let heard = listen(Some(&bus.address), BUDGET);
        let took = from.elapsed();

        assert_eq!(heard, Heard::Silence);
        assert!(
            took < BUDGET + Duration::from_millis(200),
            "listening took {took:?} against a {BUDGET:?} budget"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_bus_with_no_portal_to_start_answers_at_once() {
        let bus = Bus::with(&[]);

        assert_eq!(listen(Some(&bus.address), BUDGET), Heard::Answer);
    }

    /// A portal that exits instead of hanging: the bus says it failed as soon as it has.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_portal_that_fails_to_start_is_answered_for_by_the_bus() {
        let bus = Bus::with(&[(PORTAL, "/bin/false")]);

        assert_eq!(listen(Some(&bus.address), BUDGET), Heard::Answer);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_running_portal_answers() {
        let bus = Bus::with(&[]);
        // Someone owns the name, as `xdg-desktop-portal` does on a working desktop.
        let _portal = zbus::blocking::connection::Builder::address(bus.address.as_str())
            .expect("an address")
            .name(PORTAL)
            .expect("a name")
            .build()
            .expect("the portal is on the bus");

        assert_eq!(listen(Some(&bus.address), BUDGET), Heard::Answer);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn no_bus_at_the_address_is_heard_as_no_bus() {
        assert_eq!(listen(Some(NO_SESSION_BUS), BUDGET), Heard::NoBus);
    }
}
