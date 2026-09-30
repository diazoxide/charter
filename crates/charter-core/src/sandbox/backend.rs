//! Whether this machine can apply a sandbox at all: the operating system's mechanism, and the
//! programs a harness's sandbox runs through. Asked before a chat starts, so a machine that
//! cannot is told what is missing rather than handed a harness that exits.

use super::Os;

/// Where macOS keeps Seatbelt's front end. Always there on a supported macOS.
pub const SANDBOX_EXEC: &str = "/usr/bin/sandbox-exec";

/// What is missing on `os` for a sandbox to be applied, or `None` when nothing is. `has` says
/// whether a program is installed, by its name.
///
/// - **macOS**: Seatbelt, through `sandbox-exec`.
/// - **Linux**: bubblewrap for the filesystem and `socat` for the network proxy, the two
///   programs Claude Code's sandbox runs through.
/// - **Windows**: no backend yet, and the default stays on (ruling V21, 3): a chat there starts
///   only once a person turns the sandbox off for it.
pub fn missing(os: Os, has: &dyn Fn(&str) -> bool) -> Option<String> {
    let needs: &[&str] = match os {
        Os::MacOs => &["sandbox-exec"],
        Os::Linux => &["bwrap", "socat"],
        Os::Windows => return Some("charter has no sandbox backend on Windows yet".to_owned()),
        Os::Other => {
            return Some("charter has no sandbox backend on this operating system".to_owned());
        }
    };
    let absent: Vec<&str> = needs.iter().copied().filter(|it| !has(it)).collect();
    (!absent.is_empty()).then(|| format!("{} is not installed", absent.join(" and ")))
}

/// Whether `program` is installed on this machine, as [`missing`] asks it.
pub fn installed(program: &str) -> bool {
    if program == "sandbox-exec" {
        return crate::programs::runnable(std::path::Path::new(SANDBOX_EXEC));
    }
    crate::programs::on_path(program)
}
