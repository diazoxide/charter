//! The names the command line answers to (RN-3, #1255).
//!
//! It ships as `purlis`. `charter`, its name before, runs it too for the rename's window (V93k),
//! and `edm`, the name before that, is still recognised by the guards that knew it. Every place
//! that asks "is this program the product itself?" reads these lists, so a guard cannot learn one
//! name and miss another. They move into the name module (RN-1) as they are.

/// The name the command line ships as, and the one it is linked on `PATH` as first.
pub const PRIMARY: &str = "purlis";

/// The name it had before, kept as an alias for the window (V93k). Removing it is its own
/// ticket at 1.0 (V93l).
pub const ALIAS: &str = "charter";

/// Every name the command line is installed under: the bundle's two binaries, the `.deb`'s two
/// files in `/usr/bin` and the two links the PATH install makes.
pub const INSTALLED: [&str; 2] = [PRIMARY, ALIAS];

/// Every name a guard reads as the product itself: the installed names, and `edm`, the name
/// before `charter`. A guard that knew only some of them would be stepped around by typing
/// another, on a machine where that binary is still installed.
pub const RECOGNISED: [&str; 3] = [PRIMARY, ALIAS, "edm"];

/// Whether `name`, a program's file name as written, is one the command line is installed
/// under. Exact, as the places that ask it always compared.
pub fn is_installed(name: &str) -> bool {
    INSTALLED.contains(&name)
}

/// Whether `name` is one a guard reads as the product itself, ignoring case: on a
/// case-insensitive filesystem `PURLIS` runs the same binary, and a guard that matched one
/// casing would have a Shift key for a bypass.
pub fn is_recognised(name: &str) -> bool {
    RECOGNISED.iter().any(|own| own.eq_ignore_ascii_case(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_command_line_is_purlis_first_and_charter_for_the_window() {
        assert_eq!(INSTALLED, ["purlis", "charter"]);
        assert!(is_installed("purlis") && is_installed("charter"));
        assert!(!is_installed("edm") && !is_installed("PURLIS"));
    }

    #[test]
    fn the_guards_recognise_every_name_it_has_had_in_any_case() {
        for name in ["purlis", "PURLIS", "charter", "Charter", "edm"] {
            assert!(is_recognised(name), "{name}");
        }
        for name in ["purlis2", "chart", "", "my-charter"] {
            assert!(!is_recognised(name), "{name}");
        }
    }
}
