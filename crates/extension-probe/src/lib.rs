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

/// The protocol this program speaks: 2, which added events and the briefing section
/// (charter-app#343).
pub const PROTOCOL: u64 = 2;

/// The manifest this program is installed with, so `assemble` writes the one that is tested.
pub const MANIFEST: &str = include_str!("../charter-extension.json");

/// The file in the state directory a test writes to make the probe misbehave on purpose —
/// `{"sleep_ms": 3000}`, `{"fail": true}`, `{"section": "…"}`. The state directory is outside
/// the fingerprint, so this changes nothing the operator approved.
pub const BEHAVE: &str = "behave.json";

/// The file in the state directory the probe appends one line to for every event it hears, so a
/// test reads off what it was told.
pub const HEARD: &str = "heard.txt";

/// What the probe was told to do by [`BEHAVE`], read fresh for each question.
#[derive(Debug, Default)]
pub struct Behave {
    /// Sleep this long before answering, to be the slow extension.
    pub sleep_ms: u64,
    /// Answer an `error`, to be the failing one.
    pub fail: bool,
    /// The briefing section to answer, in place of the probe's own.
    pub section: Option<String>,
}

impl Behave {
    /// Read from `state`, or the ordinary probe when there is nothing to read.
    pub fn read(state: Option<&Path>) -> Self {
        let Some(doc) = state
            .and_then(|state| std::fs::read_to_string(state.join(BEHAVE)).ok())
            .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        else {
            return Self::default();
        };
        Self {
            sleep_ms: doc.get("sleep_ms").and_then(Value::as_u64).unwrap_or(0),
            fail: doc.get("fail").and_then(Value::as_bool).unwrap_or(false),
            section: doc
                .get("section")
                .and_then(Value::as_str)
                .map(str::to_owned),
        }
    }
}

/// The answer to one request line, as the JSON value to print. Never panics on what it is
/// given: a request it cannot read is an `error` answer.
///
/// `state` is the state directory charter named, when it named one: an event is recorded there
/// ([`HEARD`]), and what [`BEHAVE`] says there is done.
pub fn answer(request: &Value, state: Option<&Path>) -> Value {
    let behave = Behave::read(state);
    if behave.sleep_ms > 0 {
        std::thread::sleep(std::time::Duration::from_millis(behave.sleep_ms));
    }
    if behave.fail {
        return json!({ "charter": PROTOCOL, "error": "the probe was told to fail" });
    }
    match said(request, state, &behave) {
        Ok(Said::View(text)) => {
            json!({ "charter": PROTOCOL, "blocks": [{ "kind": "note", "text": text }] })
        }
        Ok(Said::Heard) => json!({ "charter": PROTOCOL }),
        Ok(Said::Section(text)) => json!({ "charter": PROTOCOL, "section": text }),
        Err(why) => json!({ "charter": PROTOCOL, "error": why }),
    }
}

/// What the probe answers, by the kind of question.
enum Said {
    View(String),
    Heard,
    Section(String),
}

/// What the probe says back: which view it was asked, in which protocol, and how much it was
/// handed — so a test reads off the answer that charter asked what it meant to ask. An event is
/// written down in [`HEARD`]; a briefing is a line naming the workspace it was asked about.
fn said(request: &Value, state: Option<&Path>, behave: &Behave) -> Result<Said, String> {
    let protocol = request
        .get("charter")
        .and_then(Value::as_u64)
        .ok_or("the request does not say which protocol it is in")?;
    if protocol != PROTOCOL {
        return Err(format!(
            "this program speaks protocol {PROTOCOL} and was asked in protocol {protocol}"
        ));
    }
    if let Some(event) = request.get("event").and_then(Value::as_str) {
        let mut line = event.to_owned();
        for key in ["workspace", "from"] {
            if let Some(value) = request.get(key).and_then(Value::as_str) {
                line.push(' ');
                line.push_str(value);
            }
        }
        if let Some(state) = state {
            write_heard(state, &line).map_err(|why| why.to_string())?;
        }
        return Ok(Said::Heard);
    }
    if let Some(briefing) = request.get("briefing") {
        let workspace = briefing
            .get("workspace")
            .and_then(Value::as_str)
            .unwrap_or("none");
        return Ok(Said::Section(behave.section.clone().unwrap_or_else(|| {
            format!("extension-probe briefs a chat in workspace {workspace}")
        })));
    }
    let view = request
        .get("view")
        .and_then(Value::as_str)
        .ok_or("the request names no view, no event and no briefing")?;
    let personas = request
        .pointer("/given/personas")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let noun = if personas == 1 { "persona" } else { "personas" };
    Ok(Said::View(format!(
        "extension-probe answered the view '{view}' in protocol {protocol}, handed {personas} \
         {noun}"
    )))
}

/// Append one heard event to [`HEARD`] in `state`.
fn write_heard(state: &Path, line: &str) -> std::io::Result<()> {
    use std::io::Write;
    std::fs::create_dir_all(state)?;
    let mut log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(state.join(HEARD))?;
    writeln!(log, "{line}")
}

/// The repo the probe fills its column for. It knows nothing of the plane — a program is handed
/// its view's subject and nothing else — so it names the one repo its tests clone.
pub const REPO: &str = "svc";

/// The facts file's name inside the state directory (`charter_core::extension::facts::FILE`,
/// which a stranger's extension would know from the documentation, not by linking the core).
pub const FACTS: &str = "facts.json";

/// The facts file after one more answer: its `asked` badge and its `asked` cell for [`REPO`]
/// count the questions answered, as of `now` (Unix seconds). `previous` is the file as it was,
/// when there was one charter could read.
pub fn facts_after(previous: Option<&Value>, now: u64) -> Value {
    let asked = previous
        .and_then(|it| it.pointer("/badges/asked/value"))
        .and_then(Value::as_str)
        .and_then(|it| it.parse::<u64>().ok())
        .unwrap_or(0)
        + 1;
    let field = json!({ "value": asked.to_string(), "at": now });
    json!({
        "badges": { "asked": field },
        "repo-columns": { "asked": { REPO: field } },
    })
}

/// Rewrite the facts file in `state` after an answer — **whenever it answers a question**, as
/// the facts file's contract asks. By rename, so charter never reads half of one.
pub fn refresh_facts(state: &Path, now: u64) -> std::io::Result<()> {
    std::fs::create_dir_all(state)?;
    let at = state.join(FACTS);
    let previous = std::fs::read_to_string(&at)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok());
    let beside = state.join(".facts.json.new");
    std::fs::write(&beside, facts_after(previous.as_ref(), now).to_string())?;
    std::fs::rename(&beside, &at)
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
