//! `persona-statistics` — read one request line on stdin, print one answer line on stdout.
//!
//! `persona-statistics assemble <dir>` puts this program and its manifest in `<dir>`, which is
//! the folder to point charter's Extensions dialog at. See `lib.rs` for what it answers.

use std::io::{BufRead, Write};
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let [verb, dir] = args.as_slice()
        && verb == "assemble"
    {
        return match persona_statistics::assemble(std::path::Path::new(dir)) {
            Ok(program) => {
                println!(
                    "assembled {} — point charter's Extensions dialog at {dir}",
                    program.display()
                );
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
    if let Err(why) = std::io::stdin().lock().read_line(&mut line) {
        eprintln!("persona-statistics: could not read the request: {why}");
        return ExitCode::FAILURE;
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
