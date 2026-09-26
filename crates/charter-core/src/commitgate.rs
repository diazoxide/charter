//! The commitment gate on `userpromptsubmit` — `charter/hooks.py`'s `_commitment_nudge`,
//! restored by the operator's ruling on charter#369.
//!
//! A prompt that asks for WORK and leaves a real FORK open (open-ended wording, a broad scope,
//! something irreversible, or a long many-part ask) is told to scout and then ask at the fork
//! before anything is built. It was measured before it was built: over 1,867 prompts, asking
//! first happened on one prompt in ten, on whim, and least of all in long grinds.
//!
//! # When it fires, by the nudge standard
//!
//! A prompt is worth its interruption only if it changes what happens. So it is deliberately
//! narrow:
//!
//! - **Never on a lookup.** A question or a status check has no fork to ask about.
//! - **Never on work with no fork.** "fix the typo on line 4" has nothing to ask.
//! - **Never on a slash command.** The operator has already picked the method.
//! - **Never unattended** (`permission_mode: bypassPermissions`). "Ask the operator" is advice
//!   nobody can follow there, and following it would block the run.
//! - **Once, then quiet for [`COOLDOWN`] prompts**, so the answers to its own question do not
//!   set it off again.
//!
//! # Where it differs from the Python, on purpose
//!
//! - **Silent unattended**, where the Python said "decide and write it down": charter#369's
//!   "done when" says never in an unattended session.
//! - **Silent on a slash command**, which the Python was not.
//! - **The quiet counts every prompt.** The Python moved its counter only on prompts that
//!   would have fired, so a gate followed by ordinary answers stayed armed and then swallowed
//!   the next real fork.
//! - **Three steps, not four, and no preamble.** Scout, ask at the fork, and say so when there
//!   is none. The Python's "name the method" step named one harness's skills, and its framing
//!   step named slash commands; charter is harness-agnostic. The workspace-placement block and
//!   the persona roster that led the Python's message are not restored: `routing:` is retired,
//!   and a handoff is the `charter:handoff` skill's to offer.
//! - **Pasted evidence ends at the line's end.** The Python's `.*` after a `curl` or a stack
//!   line ran to the end of the prompt under `re.S`, taking the ask after it with it.

use std::path::PathBuf;
use std::sync::LazyLock;

use regex::Regex;

use crate::hookstate::{self, State};
use crate::toolhooks::Hook;

/// Prompts the gate stays quiet for after it fires.
pub const COOLDOWN: u32 = 3;

/// The permission mode a harness reports for a run nobody is watching.
const UNATTENDED: &str = "bypassPermissions";

/// Characters of actual prose, pasted evidence removed, that make an ask "many-part".
const PROSE_LONG: usize = 240;

fn pattern(source: &str) -> Regex {
    Regex::new(source).expect("a valid pattern")
}

/// Asking for work to happen, not for information.
static ACTION: LazyLock<Regex> = LazyLock::new(|| {
    pattern(
        r"(?i)\b(implement|build|create|add|write|refactor|migrate|redesign|rewrite|port|integrate|wire\s+up|set\s+up|introduce|replace|split|extract|optimi[sz]e|improve|fix|make\s+(?:it|this|our|the)\b)",
    )
});

/// The request admits more than one defensible approach.
static FUZZY: LazyLock<Regex> = LazyLock::new(|| {
    pattern(
        r"(?i)\b(somehow|some\s?how|maybe|perhaps|something\s+like|better|cleaner|nicer|more\s+\w+|not\s+sure|what\s+if|could\s+we|can\s+we|should\s+we|i\s+think|ideally|kind\s+of|sort\s+of|etc\.?|and\s+so\s+on)",
    )
});

static SCOPE: LazyLock<Regex> = LazyLock::new(|| {
    pattern(
        r"(?i)\b(across|every\s+repo|all\s+repos|multiple\s+repos|end.to.end|whole|entire|everywhere|org.wide|each\s+(?:repo|service|persona)|several)",
    )
});

static DESTRUCTIVE: LazyLock<Regex> = LazyLock::new(|| {
    pattern(
        r"(?i)\b(delete|remove|drop|wipe|purge|reset|revert|roll\s?back|force.push|overwrite|truncate|prune)",
    )
});

/// Information-seeking — never a commitment point, whatever else it matches.
static LOOKUP: LazyLock<Regex> = LazyLock::new(|| {
    pattern(
        r"(?i)^\s*(what|why|who|when|where|which|how\s+(?:many|much|does|do|did|is)|is\s|are\s|does\s|do\s|did\s|can\s+you\s+(?:see|check|read|show|find|tell)|show|list|print|explain|describe|check|status|tell\s+me|any\b)",
    )
});

/// A symptom report: the method is diagnosis, and a design question would be the wrong one.
static SYMPTOM: LazyLock<Regex> = LazyLock::new(|| {
    pattern(
        r"(?i)\b(bug|broken|error|exception|fail(?:s|ed|ing)?|crash|incident|regress|not\s+work|doesn'?t\s+work|stack\s?trace|500\b|502\b|422\b|403\b|timeout)",
    )
});

/// Pasted evidence — a fenced block, JSON one level deep, a URL, a curl, a stack or log line.
/// A bug report is long because of what was pasted into it, not because the ask has many
/// parts, so it is removed before the length is measured.
static PASTED: LazyLock<Regex> = LazyLock::new(|| {
    pattern(
        r"(?sm)```.*?```|\{[^{}]*(?:\{[^{}]*\}[^{}]*)*\}|https?://\S+|\bcurl\s+\S[^\n]*|^\s*(?:at\s+\S+|\w+Error\b|\w+Exception\b)[^\n]*$",
    )
});

/// The fork signals in `prompt`, or none when it is not a commitment point.
///
/// It takes an action verb AND at least one fork: action alone has nothing to ask about, and a
/// question is not a commitment.
pub fn signals(prompt: &str) -> Vec<&'static str> {
    let p = prompt.trim();
    if p.is_empty() || p.starts_with('/') || LOOKUP.is_match(p) || !ACTION.is_match(p) {
        return Vec::new();
    }
    let mut found = Vec::new();
    if FUZZY.is_match(p) {
        found.push("open-ended wording");
    }
    if SCOPE.is_match(p) {
        found.push("broad scope");
    }
    if DESTRUCTIVE.is_match(p) {
        found.push("something irreversible");
    }
    if PASTED.replace_all(p, " ").trim().chars().count() > PROSE_LONG {
        found.push("a long, many-part ask");
    }
    found
}

/// What the gate says for `prompt`, or nothing. Pure: no cooldown, no plane.
pub fn message(prompt: &str) -> Option<String> {
    let found = signals(prompt);
    if found.is_empty() {
        return None;
    }
    let (shape, step2) = if SYMPTOM.is_match(prompt) {
        (
            "a **symptom to diagnose**",
            "2. **Reproduce it as a failing test** before theorising. Ask the operator only if \
             scouting finds a real fork: two plausible causes that want different fixes, or a \
             scope call that is theirs.",
        )
    } else {
        (
            "**work to be built**",
            "2. **Then ask the operator** at the fork you found: two to four concrete options, \
             your recommendation first. Not a confirmation, and not a question the code could \
             have answered.",
        )
    };
    Some(format!(
        "⬢ **Commitment point**: this reads as {shape}, with {}. Before you plan, dispatch or \
         edit:\n\
         1. **Scout first.** Read the code and what already exists, enough to know the real \
         fork.\n\
         {step2}\n\
         3. **No real fork once you have looked?** Say so in one line and do it. This is a \
         gate, not a ritual.",
        found.join(" · ")
    ))
}

/// The gate as `userpromptsubmit` runs it: [`message`], in a plane, attended, and out of its
/// cooldown.
pub fn nudge(hook: &Hook) -> Option<String> {
    if !hook.in_plane {
        return None;
    }
    let payload = hook.payload;
    if payload.get("permission_mode").and_then(|v| v.as_str()) == Some(UNATTENDED) {
        return None;
    }
    let session = payload.get("session_id").and_then(|v| v.as_str());
    let counter = Cooldown::of(hook, hookstate::session(session, hook.env));
    // Every attended prompt moves the cooldown, whether or not it would fire: the answers to
    // the gate's own question are what the quiet is for, and they rarely look like a fork.
    if counter.as_ref().is_some_and(Cooldown::counting_down) {
        return None;
    }
    let prompt = payload.get("prompt").and_then(|v| v.as_str()).unwrap_or("");
    let said = message(prompt)?;
    if let Some(counter) = counter {
        counter.start();
    }
    Some(said)
}

/// One session's count of prompts left to stay quiet for, under
/// `<state>/commit-gate/<session>`. A session charter cannot name has none, and a count it
/// cannot keep reads as zero: the gate may cost a prompt, never a turn.
struct Cooldown {
    state: State,
    file: PathBuf,
}

impl Cooldown {
    fn of(hook: &Hook, session: Option<String>) -> Option<Self> {
        // `hookstate::session` leaves `.` and `..` standing, and neither is a file.
        let session = session.filter(|id| !id.chars().all(|c| c == '.'))?;
        let state = State::of(hook.root);
        let file = state.dir().join("commit-gate").join(session);
        Some(Self { state, file })
    }

    /// Whether the gate is still quiet — and if it is, one prompt fewer to wait.
    fn counting_down(&self) -> bool {
        let left: u32 = self
            .state
            .read_text(&self.file)
            .and_then(|text| text.trim().parse().ok())
            .unwrap_or(0);
        if left == 0 {
            return false;
        }
        let _ = self
            .state
            .replace(&self.file, (left - 1).to_string().as_bytes());
        true
    }

    /// The gate fired: quiet for the next [`COOLDOWN`] prompts.
    fn start(&self) {
        let _ = self
            .state
            .replace(&self.file, COOLDOWN.to_string().as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fork_needs_an_action_to_ask_about() {
        assert_eq!(
            signals("maybe the cache, across every repo"),
            Vec::<&str>::new()
        );
        assert_eq!(signals("build the importer"), Vec::<&str>::new());
        assert_eq!(
            signals("build something like the importer, and drop the old one"),
            vec!["open-ended wording", "something irreversible"]
        );
    }

    #[test]
    fn pasted_evidence_does_not_make_an_ask_long() {
        let log = format!(
            "fix this\n```\n{}\n```\nhttps://example.com/{}",
            "x".repeat(400),
            "y".repeat(300)
        );
        assert_eq!(signals(&log), Vec::<&str>::new());
        let long = format!("fix the importer so that {}", "it reads rows ".repeat(30));
        assert_eq!(signals(&long), vec!["a long, many-part ask"]);
    }

    #[test]
    fn nested_json_is_pasted_evidence_too() {
        let pasted = format!(
            r#"fix this {{"dd":{{"trace_id":"{}"}},"msg":"{}"}}"#,
            "1".repeat(200),
            "m".repeat(200)
        );
        assert_eq!(signals(&pasted), Vec::<&str>::new());
    }

    /// A fork-shaped ask: an action and open-ended wording.
    const FORK: &str = "build something like the importer";

    /// What `userpromptsubmit` hands the gate for `prompt` from `session`, in the plane at `root`.
    fn ask(root: &std::path::Path, in_plane: bool, payload: &serde_json::Value) -> Option<String> {
        let hook = Hook {
            root,
            in_plane,
            payload,
            env: &|_| None,
            cwd: root,
            now: chrono::Utc::now(),
            host: "test-host",
        };
        nudge(&hook)
    }

    fn prompt(session: &str, text: &str) -> serde_json::Value {
        serde_json::json!({ "session_id": session, "prompt": text })
    }

    #[test]
    fn the_gate_fires_once_then_keeps_quiet_for_its_cooldown_then_fires_again() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        assert_eq!(
            ask(root, true, &prompt("s1", FORK)),
            message(FORK),
            "a fork in an attended session of a plane is asked about"
        );
        for quiet in 1..=COOLDOWN {
            assert_eq!(
                ask(root, true, &prompt("s1", FORK)),
                None,
                "prompt {quiet} of the cooldown is quiet"
            );
        }
        assert!(
            ask(root, true, &prompt("s1", FORK)).is_some(),
            "the cooldown ends after {COOLDOWN} prompts"
        );
        // The cooldown is the session's own: another session is asked at once.
        assert!(ask(root, true, &prompt("s2", FORK)).is_some());
    }

    #[test]
    fn every_attended_prompt_counts_down_the_cooldown_whether_or_not_it_is_a_fork() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        assert!(ask(root, true, &prompt("s1", FORK)).is_some());
        for _ in 0..COOLDOWN {
            assert_eq!(ask(root, true, &prompt("s1", "yes, the first one")), None);
        }
        assert!(ask(root, true, &prompt("s1", FORK)).is_some());
    }

    #[test]
    fn a_session_whose_id_is_only_dots_has_no_cooldown_file_and_is_asked_every_time() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        for id in [".", ".."] {
            assert!(ask(root, true, &prompt(id, FORK)).is_some());
            assert!(ask(root, true, &prompt(id, FORK)).is_some(), "{id:?}");
        }
        assert!(!root.join(".charter/commit-gate").exists());
    }

    #[test]
    fn the_gate_says_nothing_outside_a_plane_or_to_a_run_nobody_is_watching() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        assert_eq!(ask(root, false, &prompt("s1", FORK)), None);
        let unattended = serde_json::json!({
            "session_id": "s1",
            "prompt": FORK,
            "permission_mode": "bypassPermissions",
        });
        assert_eq!(ask(root, true, &unattended), None);
        let attended = serde_json::json!({
            "session_id": "s1",
            "prompt": FORK,
            "permission_mode": "default",
        });
        assert!(ask(root, true, &attended).is_some());
    }

    #[test]
    fn a_slash_command_a_blank_prompt_and_a_lookup_are_never_commitment_points() {
        assert_eq!(signals("/build something like the importer"), Vec::<&str>::new());
        assert_eq!(signals("   "), Vec::<&str>::new());
        assert_eq!(
            signals("how does it build something like the importer?"),
            Vec::<&str>::new()
        );
        assert_eq!(message("/build something like the importer"), None);
    }

    #[test]
    fn the_message_names_the_signals_and_the_step_that_fits_a_build_or_a_symptom() {
        let built = message(FORK).expect("a fork");
        assert!(
            built.starts_with(
                "⬢ **Commitment point**: this reads as **work to be built**, with open-ended \
                 wording."
            ),
            "{built}"
        );
        assert!(built.contains("2. **Then ask the operator**"), "{built}");
        let symptom = message("fix the crash somehow").expect("a fork");
        assert!(
            symptom.contains("a **symptom to diagnose**, with open-ended wording"),
            "{symptom}"
        );
        assert!(symptom.contains("2. **Reproduce it as a failing test**"), "{symptom}");
    }
}
