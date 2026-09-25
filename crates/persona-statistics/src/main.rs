//! `persona-statistics` — read one request line on stdin, print one answer line on stdout.
//!
//! `persona-statistics assemble <dir>` puts this program and its manifest in `<dir>`, which is
//! how the release build puts it in the app's bundle. See `lib.rs` for what it answers.

use std::io::{BufRead, Write};
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let [verb, dir] = args.as_slice()
        && verb == "assemble"
    {
        return match persona_statistics::assemble(std::path::Path::new(dir)) {
            Ok(program) => {
                println!("assembled {}", program.display());
                ExitCode::SUCCESS
            }
            Err(why) => {
                eprintln!("persona-statistics: could not assemble into {dir}: {why}");
                ExitCode::FAILURE
            }
        };
    }
    if !args.is_empty() {
        eprintln!(
            "usage: persona-statistics            (answer one request on stdin)\n       \
             persona-statistics assemble <dir>"
        );
        return ExitCode::FAILURE;
    }

    let mut line = String::new();
    match std::io::stdin().lock().read_line(&mut line) {
        // End of input before a question: charter closed its end, or is gone — it dies with
        // its end of the socket, and cannot kill this when it crashes. There is nobody to
        // answer, so this exits rather than waiting for one (charter's executor, "a program
        // ends when its stdin does").
        Ok(0) => return ExitCode::SUCCESS,
        Ok(_) => {}
        Err(why) => {
            eprintln!("persona-statistics: could not read the request: {why}");
            return ExitCode::FAILURE;
        }
    }
    let said = match serde_json::from_str(&line) {
        Ok(request) => persona_statistics::answer(&request),
        Err(why) => serde_json::json!({
            "charter": persona_statistics::PROTOCOL,
            "error": format!("the request was not JSON: {why}"),
        }),
    };
    let mut out = std::io::stdout().lock();
    if writeln!(out, "{said}").and_then(|()| out.flush()).is_err() {
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
