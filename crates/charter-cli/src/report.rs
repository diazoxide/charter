//! `charter report bug|feature`: the command line over [`charter_core::report`].
//!
//! Every run drafts, scrubs and prints the draft. Only the reporter's yes files it: `y` at a
//! prompt on a terminal, or `--yes <digest>` naming the digest the preview printed, so a chat
//! can run this safely — what it would send is on screen before anything is sent.

use std::io::{IsTerminal, Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use charter_core::report::{self, Draft, Kind, Known};

#[derive(clap::Subcommand)]
pub enum ReportCommand {
    /// Draft a bug report on charter's own tracker. Shows the draft; files nothing without
    /// your yes.
    Bug {
        #[command(flatten)]
        text: TextArgs,
        /// Draft the newest panic the app saved: where it panicked, its message and its
        /// charter version. Your own words, if given, go in front of it.
        #[arg(long)]
        panic: bool,
    },
    /// Draft a feature request on charter's own tracker. Shows the draft; files nothing
    /// without your yes.
    #[command(alias = "gap")]
    Feature {
        #[command(flatten)]
        text: TextArgs,
    },
}

#[derive(clap::Args)]
pub struct TextArgs {
    /// What happened, or what charter should do. The first line is the title.
    text: Option<String>,
    /// Read the description from this file, so backticks and `$` reach it as written.
    #[arg(long, value_name = "PATH", conflicts_with_all = ["text", "stdin"])]
    from_file: Option<PathBuf>,
    /// Read the description from standard input.
    #[arg(long, conflicts_with = "text")]
    stdin: bool,
    /// The issue's title, instead of the description's first line.
    #[arg(long)]
    title: Option<String>,
    /// File the draft, if its digest is this one: the digest the preview of the same draft
    /// printed. Anything else files nothing.
    #[arg(long, value_name = "DIGEST")]
    yes: Option<String>,
}

pub fn run(command: &ReportCommand) -> ExitCode {
    match draft(command) {
        Ok((draft, args)) => decide(&draft, args),
        Err(said) => {
            eprintln!("charter report: {said}");
            ExitCode::FAILURE
        }
    }
}

fn draft(command: &ReportCommand) -> Result<(Draft, &TextArgs), String> {
    // No working directory means no plane, and a report needs none.
    let plane = std::env::current_dir()
        .ok()
        .and_then(|cwd| charter_core::plane::resolve(&cwd).ok());
    let known = Known::here(plane.as_deref());
    let (kind, args, panic) = match command {
        ReportCommand::Bug { text, panic } => (Kind::Bug, text, *panic),
        ReportCommand::Feature { text } => (Kind::Feature, text, false),
    };
    let words = words(args)?;
    let saved = report::panic_log()
        .and_then(|log| std::fs::read_to_string(log).ok())
        .and_then(|log| report::latest_panic(&log));
    let made = if panic {
        let Some(saved) = saved else {
            return Err(format!(
                "there is no saved panic to report (charter looked in {})",
                report::panic_log().map_or("no log directory".into(), |p| p.display().to_string())
            ));
        };
        Draft::of_panic(&saved, words.as_deref(), &known)
    } else {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        if kind == Kind::Bug
            && let Some(saved) = saved.as_ref().filter(|p| p.is_recent(now))
        {
            eprintln!(
                "charter report: the app saved a panic (at {}). `charter report bug --panic` \
                 drafts it as a bug.",
                saved.place.rsplit(['/', '\\']).next().unwrap_or("")
            );
        }
        Draft::described(
            kind,
            words.as_deref().unwrap_or(""),
            args.title.as_deref(),
            &known,
        )
    };
    made.map(|d| (d, args)).map_err(|refused| refused.0)
}

/// The reporter's own words, from wherever they were given.
fn words(args: &TextArgs) -> Result<Option<String>, String> {
    if let Some(path) = &args.from_file {
        return std::fs::read_to_string(path)
            .map(Some)
            .map_err(|e| format!("cannot read {}: {e}", path.display()));
    }
    if args.stdin {
        let mut text = String::new();
        std::io::stdin()
            .read_to_string(&mut text)
            .map_err(|e| format!("cannot read standard input: {e}"))?;
        return Ok(Some(text));
    }
    Ok(args.text.clone())
}

fn decide(draft: &Draft, args: &TextArgs) -> ExitCode {
    let digest = draft.digest();
    print!("{}", draft.preview());
    match &args.yes {
        Some(given) if *given == digest => return send(draft),
        Some(given) => {
            eprintln!(
                "charter report: `--yes {given}` is not the draft above (its digest is {digest}), \
                 so nothing was sent. Read the draft, then pass its digest."
            );
            return ExitCode::FAILURE;
        }
        None => {}
    }
    let query = draft.query();
    if !query.is_empty() {
        println!(
            "Searching {} for possible duplicates of: {query}",
            report::UPSTREAM
        );
    }
    match report::search_duplicates(draft) {
        Ok(hits) if hits.is_empty() => println!("Possible duplicates: none found."),
        Ok(hits) => {
            println!("Possible duplicates — if one is the same, add to it on GitHub instead:");
            for hit in hits {
                println!(
                    "  #{} {} ({}) {}",
                    hit.number,
                    charter_core::shown::readable(&hit.title, 100),
                    hit.state.to_lowercase(),
                    charter_core::shown::readable(&hit.url, 200)
                );
            }
        }
        Err(e) => println!("Possible duplicates: charter could not search ({e})."),
    }
    println!(
        "It would be filed on {} under your own `gh` login, never a token from the environment.",
        report::UPSTREAM
    );
    if !args.stdin && std::io::stdin().is_terminal() && std::io::stderr().is_terminal() {
        eprint!("File it? [y/N] ");
        let _ = std::io::stderr().flush();
        let mut answer = String::new();
        let _ = std::io::stdin().read_line(&mut answer);
        if matches!(answer.trim(), "y" | "Y" | "yes") {
            return send(draft);
        }
        eprintln!("Nothing was sent.");
        return ExitCode::SUCCESS;
    }
    println!(
        "Nothing was sent. To file exactly this draft, run the same command again with \
         --yes {digest}"
    );
    ExitCode::SUCCESS
}

fn send(draft: &Draft) -> ExitCode {
    match report::file(draft) {
        Ok(url) => {
            println!("Filed: {url}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            let (url, whole) = draft.fallback_url();
            eprintln!(
                "charter report: gh could not file it: {e}\n\
                 Open this link to file it in the browser instead{}:\n{url}",
                if whole {
                    ""
                } else {
                    ", and paste the body from the draft above"
                }
            );
            ExitCode::FAILURE
        }
    }
}
