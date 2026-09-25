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
    let said = match serde_json::from_str(&line) {
        Ok(request) => extension_probe::answer(&request),
        Err(why) => serde_json::json!({
            "charter": extension_probe::PROTOCOL,
            "error": format!("the request was not JSON: {why}"),
        }),
    };
    let mut out = std::io::stdout().lock();
    if writeln!(out, "{said}").and_then(|()| out.flush()).is_err() {
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
