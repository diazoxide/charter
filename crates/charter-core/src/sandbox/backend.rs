//! Whether this machine can apply a sandbox at all: the operating system's mechanism, and the
//! programs a harness's sandbox runs through. Asked before a chat starts, so a machine that
//! cannot is told what is missing, and how to install it, rather than handed a harness that
//! exits.

use std::fmt;

use super::Os;

/// Where macOS keeps Seatbelt's front end. Always there on a supported macOS.
pub const SANDBOX_EXEC: &str = "/usr/bin/sandbox-exec";

/// What this machine lacks for a sandbox.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Missing {
    /// Programs the backend runs through, by name, on `os`.
    Programs { os: Os, absent: Vec<&'static str> },
    /// No backend exists for this operating system yet (Windows: M46, #565).
    NoBackend(Os),
}

impl fmt::Display for Missing {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Programs { os, absent } => {
                let (verb, it) = if absent.len() == 1 {
                    ("is", "it")
                } else {
                    ("are", "them")
                };
                write!(f, "{} {verb} not installed", absent.join(" and "))?;
                // SD-30's prerequisite install, named where the chat is refused. The package is
                // `bubblewrap` for the `bwrap` program on every major distribution.
                if *os == Os::Linux {
                    let packages: Vec<&str> = absent
                        .iter()
                        .map(|it| if *it == "bwrap" { "bubblewrap" } else { it })
                        .collect();
                    let packages = packages.join(" ");
                    write!(
                        f,
                        " — install {it} with `sudo apt install {packages}` or \
                         `sudo dnf install {packages}`"
                    )?;
                }
                Ok(())
            }
            // M46 Windows, #565.
            Self::NoBackend(Os::Windows) => {
                f.write_str("charter has no sandbox backend on Windows yet")
            }
            Self::NoBackend(_) => {
                f.write_str("charter has no sandbox backend on this operating system")
            }
        }
    }
}

/// What is missing on `os` for a sandbox to be applied, or `None` when nothing is. `has` says
/// whether a program is installed, by its name.
///
/// - **macOS**: Seatbelt, through `sandbox-exec`.
/// - **Linux**: bubblewrap for the filesystem and `socat` for the network proxy, the two
///   programs Claude Code's sandbox runs through.
/// - **Windows**: no backend yet (ruling V21, 3; M46 Windows, #565).
pub fn missing(os: Os, has: &dyn Fn(&str) -> bool) -> Option<Missing> {
    let needs: &[&'static str] = match os {
        Os::MacOs => &["sandbox-exec"],
        Os::Linux => &["bwrap", "socat"],
        Os::Windows | Os::Other => return Some(Missing::NoBackend(os)),
    };
    let absent: Vec<&'static str> = needs.iter().copied().filter(|it| !has(it)).collect();
    (!absent.is_empty()).then_some(Missing::Programs { os, absent })
}

/// Whether `program` is installed on this machine, as [`missing`] asks it.
pub fn installed(program: &str) -> bool {
    if program == "sandbox-exec" {
        return crate::programs::runnable(std::path::Path::new(SANDBOX_EXEC));
    }
    crate::programs::on_path(program)
}
