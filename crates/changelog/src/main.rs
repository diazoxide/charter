//! `changelog`: print one version's section of `CHANGELOG.md`, or refuse.
//!
//! ```text
//! changelog 0.1.0                 # the section, from ./CHANGELOG.md
//! changelog Unreleased CHANGELOG.md
//! ```
//!
//! The release workflow runs it twice for a `v*` tag: in `plan`, where a refusal refuses the
//! tag before anything is built, and in `publish`, where the output becomes the GitHub
//! release's body and `latest.json`'s notes. About Charter reads the same section through the
//! same library, so the three cannot drift apart.

use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let (Some(version), path, None) = (args.next(), args.next(), args.next()) else {
        eprintln!("usage: changelog <version | Unreleased> [CHANGELOG.md]");
        return ExitCode::from(2);
    };
    let path = path.unwrap_or_else(|| "CHANGELOG.md".to_owned());
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(why) => {
            eprintln!("changelog: {path}: {why}");
            return ExitCode::FAILURE;
        }
    };
    match changelog::section(&text, &version) {
        Ok(section) => {
            println!("{}", section.notes);
            ExitCode::SUCCESS
        }
        Err(why) => {
            eprintln!("changelog: {why}");
            ExitCode::FAILURE
        }
    }
}
