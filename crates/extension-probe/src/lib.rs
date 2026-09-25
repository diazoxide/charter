//! The extension probe: a test-only extension that asks for every capability charter grants, so
//! that each one is proven end to end through the real registry and executor
//! (charter-app#336, "seam 1").
//!
//! **It is never shipped and never installed on anyone's machine.** Its manifest asks for
//! capabilities a release build of charter may not know, and its answers say what it was
//! handed rather than anything an operator would want to read. A capability is added here in the
//! change that builds it, and the test in `tests/through_the_executor.rs` proves it declared,
//! fingerprinted, approved, run and answered — and refused when it is asked for wrongly.
//!
//! **Written as a stranger's extension would be**, as persona statistics is: it does not link
//! charter's core, and it knows one thing about charter, the protocol — one line of JSON on
//! stdin, one line back on stdout.
//!
//! What it does, by capability:
//!
//! - **a view** (`probe`) says which protocol it was asked in, how many personas it was handed
//!   and which paths it may write, and lists one row — how many notes it keeps — offering its
//!   actions;
//! - **its actions** (`actions`, `writes`, charter-app#341) keep those notes in the one plane
//!   path it declares, `notes/`: `jot` writes one, `forget` deletes them and says it deletes,
//!   `sweep` deletes them and does NOT say so, `careful` asks first and writes nothing, and
//!   `stray` writes outside `notes/` — each the case a test proves charter reports or refuses.
//!   Run from a view's row, an action answers the view refreshed.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

/// The protocol this program speaks.
pub const PROTOCOL: u64 = 2;

/// The manifest this program is installed with, so `assemble` writes the one that is tested.
pub const MANIFEST: &str = include_str!("../charter-extension.json");

/// The file `stray` writes, in the plane and outside every path the probe declares.
pub const STRAY: &str = "stray.txt";

/// The answer to one request line, as the JSON value to print. Never panics on what it is
/// given: a request it cannot read is an `error` answer.
pub fn answer(request: &Value) -> Value {
    match said(request) {
        Ok(Some(blocks)) => json!({ "charter": PROTOCOL, "blocks": blocks }),
        Ok(None) => json!({ "charter": PROTOCOL }),
        Err(why) => json!({ "charter": PROTOCOL, "error": why }),
    }
}

/// What the probe does with one request: a view's blocks, an action's refreshed blocks or
/// nothing, or why it could not.
fn said(request: &Value) -> Result<Option<Value>, String> {
    let protocol = request
        .get("charter")
        .and_then(Value::as_u64)
        .ok_or("the request does not say which protocol it is in")?;
    if protocol != PROTOCOL {
        return Err(format!(
            "this program speaks protocol {PROTOCOL} and was asked in protocol {protocol}"
        ));
    }
    let writes: Vec<&str> = request
        .get("writes")
        .and_then(Value::as_array)
        .ok_or("the request does not say where this extension may write")?
        .iter()
        .filter_map(Value::as_str)
        .collect();
    let Some(action) = request.get("action").and_then(Value::as_str) else {
        return view(request, &writes).map(Some);
    };
    let notes = notes(&writes)?;
    let done = |refreshed: Result<Value, String>| -> Result<Option<Value>, String> {
        // Run from a view's row, the view is answered again; from the palette, nothing is.
        if request.get("view").is_some_and(Value::is_string) {
            refreshed.map(Some)
        } else {
            Ok(None)
        }
    };
    match action {
        "jot" => {
            std::fs::create_dir_all(&notes).map_err(|why| format!("no notes directory: {why}"))?;
            let next = count(&notes) + 1;
            std::fs::write(notes.join(format!("note-{next}.md")), "a note\n")
                .map_err(|why| format!("could not jot: {why}"))?;
            done(view(request, &writes))
        }
        "forget" | "sweep" => {
            for entry in std::fs::read_dir(&notes).into_iter().flatten().flatten() {
                std::fs::remove_file(entry.path())
                    .map_err(|why| format!("could not forget: {why}"))?;
            }
            done(view(request, &writes))
        }
        "stray" => {
            let plane = notes
                .parent()
                .ok_or("the notes directory has no plane above it")?;
            std::fs::write(plane.join(STRAY), "outside\n")
                .map_err(|why| format!("could not stray: {why}"))?;
            Ok(None)
        }
        "careful" => Ok(Some(
            json!([{ "kind": "note", "text": "careful ran, having been said yes to" }]),
        )),
        other => Err(format!("the probe has no action {other:?}")),
    }
}

/// The view's blocks: what it was asked and handed, and its one row offering its actions.
fn view(request: &Value, writes: &[&str]) -> Result<Value, String> {
    let view = request
        .get("view")
        .and_then(Value::as_str)
        .ok_or("the request names no view")?;
    let personas = request
        .pointer("/given/personas")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let noun = if personas == 1 { "persona" } else { "personas" };
    let kept = count(&notes(writes)?);
    Ok(json!([
        {
            "kind": "note",
            "text": format!(
                "extension-probe answered the view '{view}' in protocol {PROTOCOL}, handed \
                 {personas} {noun}, and may write {}",
                writes.join(", ")
            ),
        },
        {
            "kind": "list",
            "rows": [{
                "key": "notes",
                "text": format!("{kept} notes"),
                "actions": ["jot", "sweep", "careful", "forget"],
            }],
        },
    ]))
}

/// The one path the probe declares, as charter resolved it.
fn notes(writes: &[&str]) -> Result<PathBuf, String> {
    writes
        .iter()
        .find(|path| path.ends_with("/notes/"))
        .map(PathBuf::from)
        .ok_or_else(|| "charter handed no notes directory to write".to_owned())
}

fn count(notes: &Path) -> usize {
    std::fs::read_dir(notes).map_or(0, |entries| entries.flatten().count())
}

/// Put this program and its manifest in `dir`, the folder charter is pointed at.
pub fn assemble(dir: &Path) -> std::io::Result<PathBuf> {
    let me = std::env::current_exe()?;
    std::fs::create_dir_all(dir.join("bin"))?;
    std::fs::write(dir.join("charter-extension.json"), MANIFEST)?;
    let program = dir.join("bin").join("extension-probe");
    // A rename over the old copy, never a write through it: on macOS writing a program that has
    // run invalidates its signature (stand-in's notes).
    let beside = dir.join("bin").join(".extension-probe.new");
    std::fs::copy(&me, &beside)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&beside, std::fs::Permissions::from_mode(0o755))?;
    }
    std::fs::rename(&beside, &program)?;
    Ok(program)
}
