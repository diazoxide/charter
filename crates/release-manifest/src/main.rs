//! `release-manifest`: the release workflow's one call into the rules in [`release_manifest`].
//!
//! Arguments in, one file out, and every decision it could get wrong is in the library beside
//! it where a test can watch it get it right. This file reads argv, reads the `.sig` files off
//! the disk and prints what it wrote — nothing else, deliberately: a release tool whose logic
//! lives in its `main` is a release tool nothing tests.
//!
//! ```text
//! release-manifest --channel dev --version 0.2.0-dev.42 \
//!                  --pub-date 2026-09-22T10:00:00Z --notes "..." \
//!                  --dir dist --base-url https://github.com/…/releases/download/dev \
//!                  --entry darwin-aarch64=charter.app.tar.gz \
//!                  --entry linux-x86_64-appimage=charter_0.2.0_amd64.AppImage \
//!                  --out dist
//! ```
//!
//! Each `--entry` names a file in `--dir`; its signature is read from `<file>.sig` beside it,
//! which is where the bundler puts one — and **a missing `.sig` is an error rather than an
//! empty signature**, because "the file is not there" and "the file is empty" are the same
//! unsigned release and only one of them says so.

use std::path::{Path, PathBuf};

use charter_core::updates::Channel;
use release_manifest::{Entry, assemble};

fn main() -> std::process::ExitCode {
    match run(std::env::args().skip(1).collect()) {
        Ok(wrote) => {
            println!("{}", wrote.display());
            std::process::ExitCode::SUCCESS
        }
        Err(why) => {
            eprintln!("release-manifest: {why}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn run(args: Vec<String>) -> Result<PathBuf, String> {
    let mut channel = None;
    let mut version = String::new();
    let mut pub_date = String::new();
    let mut notes = String::new();
    let mut dir = PathBuf::new();
    let mut base_url = String::new();
    let mut out = PathBuf::new();
    let mut entries: Vec<(String, String)> = Vec::new();

    let mut rest = args.into_iter();
    while let Some(flag) = rest.next() {
        let mut value = || {
            rest.next()
                .ok_or_else(|| format!("{flag} wants a value after it"))
        };
        match flag.as_str() {
            "--channel" => {
                let word = value()?;
                channel = Some(
                    Channel::named(&word)
                        .ok_or_else(|| format!("{word:?} is not a channel charter publishes"))?,
                );
            }
            "--version" => version = value()?,
            "--pub-date" => pub_date = value()?,
            "--notes" => notes = value()?,
            "--dir" => dir = PathBuf::from(value()?),
            "--base-url" => base_url = value()?,
            "--out" => out = PathBuf::from(value()?),
            "--entry" => {
                let pair = value()?;
                let (target, file) = pair
                    .split_once('=')
                    .ok_or_else(|| format!("--entry wants <target>=<file>, got {pair:?}"))?;
                entries.push((target.to_owned(), file.to_owned()));
            }
            other => return Err(format!("{other} is not a flag this tool has")),
        }
    }

    let channel = channel.ok_or("--channel is required")?;
    let rows = entries
        .into_iter()
        .map(|(target, file)| {
            Ok(Entry {
                signature: signature_beside(&dir, &file)?,
                // The url is the base and the artifact's own name, joined here so the
                // workflow cannot name one file and link another.
                url: format!("{}/{}", base_url.trim_end_matches('/'), file),
                target,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;

    let (name, text) = assemble(channel, &version, &pub_date, &notes, &rows)
        .map_err(|refused| refused.to_string())?;
    let path = out.join(name);
    std::fs::write(&path, text).map_err(|why| format!("{} could not be written: {why}", path.display()))?;
    Ok(path)
}

/// The `.sig` the bundler wrote beside an artifact.
///
/// A missing one is named as missing. Tauri's bundler writes no `.sig` at all when
/// `TAURI_SIGNING_PRIVATE_KEY` is unset — it does not write an empty one — so this is the
/// first place an unsigned release shows up, and it must not be turned into an empty string
/// that some later check might forgive.
fn signature_beside(dir: &Path, file: &str) -> Result<String, String> {
    let path = dir.join(format!("{file}.sig"));
    std::fs::read_to_string(&path).map_err(|why| {
        format!(
            "{} is not there ({why}) — the bundler writes no .sig when TAURI_SIGNING_PRIVATE_KEY \
             is unset, so this is an unsigned build",
            path.display()
        )
    })
}
