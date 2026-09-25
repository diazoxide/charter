//! `extension-probe` — read one request line on stdin, print one answer line on stdout.
//!
//! `extension-probe assemble <dir>` puts this program and its manifest in `<dir>`. Test-only;
//! see `lib.rs`.

use std::io::{BufRead, Write};
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let [verb, dir] = args.as_slice()
        && verb == "assemble"
    {
        return match extension_probe::assemble(std::path::Path::new(dir)) {
            Ok(_) => ExitCode::SUCCESS,
            Err(why) => {
                eprintln!("extension-probe: could not assemble into {dir}: {why}");
                ExitCode::FAILURE
            }
        };
    }
    if !args.is_empty() {
        eprintln!("usage: extension-probe | extension-probe assemble <dir>");
        return ExitCode::FAILURE;
    }

    let mut line = String::new();
    match std::io::stdin().lock().read_line(&mut line) {
        // End of input before a question: charter is gone, and there is nobody to answer.
        Ok(0) => return ExitCode::SUCCESS,
        Ok(_) => {}
        Err(why) => {
            eprintln!("extension-probe: could not read the request: {why}");
            return ExitCode::FAILURE;
        }
    }
    let state = std::env::var_os("CHARTER_EXTENSION_STATE").map(std::path::PathBuf::from);
    let said = match serde_json::from_str(&line) {
        Ok(request) => extension_probe::answer(&request, state.as_deref()),
        Err(why) => serde_json::json!({
            "charter": extension_probe::PROTOCOL,
            "error": format!("the request was not JSON: {why}"),
        }),
    };
    // The facts file is refreshed whenever the probe answers a question or hears an event
    // (charter-app#340, #343), in the state directory charter names. A failure to write it is
    // said and never fails the answer: the badge simply stays as it was.
    if said.get("error").is_none()
        && let Some(state) = &state
    {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |it| it.as_secs());
        if let Err(why) = extension_probe::refresh_facts(state, now) {
            eprintln!("extension-probe: could not refresh its facts file: {why}");
        }
    }
    let mut out = std::io::stdout().lock();
    if writeln!(out, "{said}").and_then(|()| out.flush()).is_err() {
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
