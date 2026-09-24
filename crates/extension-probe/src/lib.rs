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

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

/// The protocol this program speaks.
pub const PROTOCOL: u64 = 1;

/// The manifest this program is installed with, so `assemble` writes the one that is tested.
pub const MANIFEST: &str = include_str!("../charter-extension.json");

/// The answer to one request line, as the JSON value to print. Never panics on what it is
/// given: a request it cannot read is an `error` answer.
pub fn answer(request: &Value) -> Value {
    match said(request) {
        Ok(text) => json!({ "charter": PROTOCOL, "blocks": [{ "kind": "note", "text": text }] }),
        Err(why) => json!({ "charter": PROTOCOL, "error": why }),
    }
}

/// What the probe says back: which view it was asked, in which protocol, and how much it was
/// handed — so a test reads off the answer that charter asked what it meant to ask.
fn said(request: &Value) -> Result<String, String> {
    let protocol = request
        .get("charter")
        .and_then(Value::as_u64)
        .ok_or("the request does not say which protocol it is in")?;
    if protocol != PROTOCOL {
        return Err(format!(
            "this program speaks protocol {PROTOCOL} and was asked in protocol {protocol}"
        ));
    }
    let view = request
        .get("view")
        .and_then(Value::as_str)
        .ok_or("the request names no view")?;
    let personas = request
        .pointer("/given/personas")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let noun = if personas == 1 { "persona" } else { "personas" };
    Ok(format!(
        "extension-probe answered the view '{view}' in protocol {protocol}, handed {personas} \
         {noun}"
    ))
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
