//! `weekly-users`: the weekly-user estimate from release listings (OB-17).
//!
//! ```text
//! gh api --paginate repos/diazoxide/charter/releases > now.json
//! cargo run -p release-manifest --bin weekly-users -- now.json [a-week-ago.json]
//! ```
//!
//! Prints each release's weekly-manifest count and download count, and, given an earlier
//! listing, the growth of each and the estimate per channel. The logic is in
//! [`release_manifest::weekly`]; this reads two files and prints.

fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let read = |path: &String| {
        std::fs::read_to_string(path).map_err(|why| format!("{path} could not be read: {why}"))
    };
    let done = match args.as_slice() {
        [now] => read(now).and_then(|now| release_manifest::weekly::report(&now, None)),
        [now, earlier] => read(now).and_then(|now| {
            read(earlier).and_then(|earlier| release_manifest::weekly::report(&now, Some(&earlier)))
        }),
        _ => Err("usage: weekly-users <listing.json> [<earlier-listing.json>]".to_owned()),
    };
    match done {
        Ok(text) => {
            print!("{text}");
            std::process::ExitCode::SUCCESS
        }
        Err(why) => {
            eprintln!("weekly-users: {why}");
            std::process::ExitCode::FAILURE
        }
    }
}
