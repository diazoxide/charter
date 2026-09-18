//! Which harness a session is running, and the arguments that start or resume one.
//!
//! Every value here is a reading of one harness version, measured by the Python charter and
//! cited where it is declared (`charter/harness/claude_code.py`, `charter/harness/codex.py`,
//! ADR 0024). Nothing here reads a harness's output: the arguments are decided before the
//! program starts, and a resume is decided from what the app itself recorded.

use std::fmt;

/// A harness session's id, held to a shape that cannot be read as a flag.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SessionId(String);

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SessionIdError {
    #[error(
        "a session id is 1 to 128 characters of letters, digits, `_` or `-`, starting with a letter or a digit; this one is {0:?}"
    )]
    Malformed(String),
}

impl SessionId {
    /// Takes `text` as a session id, or refuses it.
    ///
    /// The shape is the Python charter's (`charter/frame/state.py:1128`), so both
    /// implementations take and refuse the same ids. It starts with a letter or a digit,
    /// which is what makes an id off disk safe to put in argv: it can never be read as a flag.
    pub fn new(text: impl Into<String>) -> Result<Self, SessionIdError> {
        let text = text.into();
        // Length first: a value that is far too long is refused without walking it.
        if text.is_empty() || text.len() > MOST {
            return Err(SessionIdError::Malformed(text));
        }
        let mut bytes = text.bytes();
        let starts = bytes.next().is_some_and(|b| b.is_ascii_alphanumeric());
        let rest = bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
        if starts && rest {
            Ok(Self(text))
        } else {
            Err(SessionIdError::Malformed(text))
        }
    }

    /// A fresh id for a harness whose session id charter chooses.
    pub fn fresh() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The longest session id both implementations take: the Python charter's regex allows a
/// first character and 127 more.
const MOST: usize = 128;

/// A harness the app knows how to start and resume.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Harness {
    ClaudeCode,
    Codex,
}

impl Harness {
    /// The harness a command is, judged by the name it is invoked under, or none for a
    /// program that is not one — a shell, or a harness charter has not measured.
    pub fn of_command(program: &str) -> Option<Self> {
        // The file name, so an absolute path to the same binary is the same harness. The
        // word is matched whole: `claude-something` is not Claude Code.
        match std::path::Path::new(program).file_name()?.to_str()? {
            "claude" => Some(Self::ClaudeCode),
            "codex" => Some(Self::Codex),
            _ => None,
        }
    }

    /// The word the plane calls this harness by (`charter.local.toml`'s `kind`).
    pub fn name(self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude",
            Self::Codex => "codex",
        }
    }

    /// Whether this harness tells a hook which process it is running under.
    ///
    /// Claude Code sets `$CLAUDE_PID` in every hook's environment, and that is the whole of
    /// what tells `/clear` (a new conversation from the same process, ADR 0024 C6) from a
    /// `claude` started inside the chat's own shell (a new conversation from a different
    /// one, C5). Codex names no such variable, so it gets the narrower rule: the first
    /// report of a chat is adopted and a later different id is ignored.
    pub fn reports_its_process(self) -> bool {
        match self {
            Self::ClaudeCode => true,
            Self::Codex => false,
        }
    }

    /// Whether charter chooses this harness's session id and hands it over at the start, so
    /// the link exists before the harness does.
    pub fn chooses_session_id(self) -> bool {
        match self {
            // Measured live on Claude Code 2.1.272 (C1): `--session-id` is reported at
            // SessionStart as given.
            Self::ClaudeCode => true,
            // Codex names no flag to choose an id (openai/codex#14482), so its id is only
            // ever what it reports itself — through a hook, inside its first turn (X1).
            Self::Codex => false,
        }
    }

    /// The arguments that start a new session under `id`, or none for a harness that
    /// chooses its own id.
    pub fn new_session_argv(self, id: &SessionId, name: &str) -> Vec<String> {
        match self {
            Self::ClaudeCode => words(["--session-id", id.as_str(), "--name", name]),
            Self::Codex => Vec::new(),
        }
    }

    /// Whether `args` already name a session themselves, so charter adds none of its own.
    ///
    /// The operator's flag wins: two `--resume` on one command line is not a harness charter
    /// has measured, and the one the operator typed is the one they meant.
    pub fn session_named_in(self, args: &[String]) -> bool {
        args.iter().any(|arg| {
            self.session_words().iter().any(|word| {
                // A flag may carry its value attached (`--resume=<id>`); a longer flag that
                // merely starts the same way (`--resume-later`) is a different flag.
                arg == word
                    || (word.starts_with('-')
                        && arg.starts_with(word)
                        && arg.as_bytes().get(word.len()) == Some(&b'='))
            })
        })
    }

    /// The arguments by which an operator already names a session themselves.
    ///
    /// `charter/harness/claude_code.py:298` and `charter/harness/codex.py:261`. Codex's are
    /// subcommands rather than flags, which is why they carry no dashes.
    fn session_words(self) -> &'static [&'static str] {
        match self {
            Self::ClaudeCode => &[
                "--session-id",
                "--resume",
                "-r",
                "--continue",
                "-c",
                "--fork-session",
            ],
            Self::Codex => &["resume", "fork"],
        }
    }

    /// The arguments that bring the conversation `id` back, or none where charter has not
    /// measured how this harness resumes.
    pub fn resume_argv(self, id: &SessionId, name: &str) -> Option<Vec<String>> {
        Some(match self {
            Self::ClaudeCode => words(["--resume", id.as_str(), "--name", name]),
            Self::Codex => words(["resume", id.as_str()]),
        })
    }
}

/// What a harness can tell charter about its own state, and how it is asked to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StateHooks {
    /// Armed on this session alone, by the arguments given.
    ///
    /// Nothing outside this session is changed, and the harness's own configuration — the
    /// charter plugin's guard included — is untouched.
    ThisSessionOnly {
        args: Vec<String>,
        /// Events it cannot report, by the word `charter hook` takes. Empty is the whole set.
        cannot_report: Vec<&'static str>,
    },
    /// The harness has state hooks, but they cannot be armed for one session: its
    /// configuration is machine-wide. Says so, so the UI can offer to arm them rather than
    /// showing a chat as `unknown` with no reason.
    MachineWideOnly { why: &'static str },
    /// Charter has not measured how this program reports anything, or it is not a harness.
    None,
}

impl Harness {
    /// How this harness is asked to report its state, with `charter` at `binary`.
    pub fn state_hooks(self, binary: &std::path::Path) -> StateHooks {
        match self {
            // `--settings` MERGES with the settings files already in force rather than
            // replacing them — measured on claude 2.1.276 by planting a project hook beside
            // one of these and watching both fire. That is what makes this safe: the charter
            // plugin's guard hooks keep running exactly as they were.
            Self::ClaudeCode => StateHooks::ThisSessionOnly {
                args: words(["--settings", &claude_code_settings(binary)]),
                cannot_report: Vec::new(),
            },
            // codex-cli 0.147.0's hook events, read from the draft-07 schema embedded in the
            // binary (`charter/harness/codex.py`): PreToolUse, PermissionRequest, PostToolUse,
            // PreCompact, PostCompact, SessionStart, SessionEnd, UserPromptSubmit,
            // SubagentStart, SubagentStop, Stop. **There is no Notification**, so a Codex chat
            // can say it is running and it is done, and can never say it is waiting on you.
            //
            // And there is nowhere to put them for one session: a `.codex/config.toml` beside
            // a project is ignored, so `~/.codex/config.toml` is the only answer and it is
            // machine-wide. A hook written there is inert until Codex trusts it by hash.
            Self::Codex => StateHooks::MachineWideOnly {
                why: "Codex reads no per-project configuration, so its hooks live in \
                      ~/.codex/config.toml, which every workspace on this machine shares — \
                      and a hook written there is inert until Codex trusts it. Codex also \
                      fires no Notification event, so a Codex chat can never say it is \
                      waiting on you.",
            },
        }
    }
}

/// The settings that arm Claude Code's state hooks on one session, as JSON on the argument.
///
/// On the argument and not in a file: a file would have to be written somewhere, cleaned up
/// when the chat ends, and cleaned up again after an app that crashed. What is in it is a
/// path and six event names — nothing secret, so `ps` showing it costs nothing.
fn claude_code_settings(binary: &std::path::Path) -> String {
    let command = serde_json::Value::String(binary.display().to_string());
    let hooks: serde_json::Map<String, serde_json::Value> = CLAUDE_CODE_STATE_EVENTS
        .iter()
        .map(|(event, word)| {
            (
                (*event).to_owned(),
                serde_json::json!([{
                    "hooks": [{
                        "type": "command",
                        // Two arguments, no shell: the command is a path charter knows and a
                        // word from the list below. Nothing here is composed from anything a
                        // harness, a plane or an operator wrote.
                        "command": format!("{} hook {word}", shell_quoted(&command)),
                        // Far more than the 1.8 ms this call was measured at, and far less
                        // than a turn. A hook that somehow hung must not hold the turn open.
                        "timeout": 5,
                    }],
                }]),
            )
        })
        .collect();
    serde_json::Value::Object(
        [("hooks".to_owned(), serde_json::Value::Object(hooks))]
            .into_iter()
            .collect(),
    )
    .to_string()
}

/// Claude Code's state-carrying events, and the word `charter hook` takes for each.
///
/// Measured live on claude 2.1.276 rather than read from documentation: `SessionStart`,
/// `UserPromptSubmit`, `Stop` and `SessionEnd` were each seen to fire with a `session_id`
/// matching `$CLAUDE_CODE_SESSION_ID`. `Notification` and `SubagentStop` did not fire in a
/// one-turn `-p` run — nothing asked for a human, and nothing dispatched a sub-agent — and
/// are armed on the harness's documented names.
const CLAUDE_CODE_STATE_EVENTS: &[(&str, &str)] = &[
    ("SessionStart", "sessionstart"),
    ("UserPromptSubmit", "userpromptsubmit"),
    ("Notification", "notification"),
    ("SubagentStop", "subagentstop"),
    ("Stop", "stop"),
    ("SessionEnd", "sessionend"),
];

/// A path as one word a shell cannot take apart.
///
/// Claude Code runs a hook's `command` through `/bin/sh -c` (ADR 0024 measured the shells in
/// between), so the path to the binary goes in single quotes. It is charter's own executable
/// path and not anything a plane wrote, but a person with a quote in their home directory
/// name is not a security model.
fn shell_quoted(path: &serde_json::Value) -> String {
    let path = path.as_str().unwrap_or_default();
    format!("'{}'", path.replace('\'', r"'\''"))
}

fn words<const N: usize>(argv: [&str; N]) -> Vec<String> {
    argv.map(str::to_owned).to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_code_is_started_under_the_id_charter_chose() {
        // `charter/harness/claude_code.py:300` — `--session-id <uuid> --name <name>`, so the
        // link exists before the harness starts.
        let id = SessionId::new("11111111-2222-4333-8444-555555555555").unwrap();

        assert_eq!(
            Harness::ClaudeCode.new_session_argv(&id, "ide.7"),
            vec![
                "--session-id",
                "11111111-2222-4333-8444-555555555555",
                "--name",
                "ide.7"
            ]
        );
    }

    #[test]
    fn claude_code_resumes_by_id_and_is_given_its_name_again() {
        // `charter/harness/claude_code.py:305` — the same id comes back (C3).
        let id = SessionId::new("11111111-2222-4333-8444-555555555555").unwrap();

        assert_eq!(
            Harness::ClaudeCode.resume_argv(&id, "ide.7"),
            Some(vec![
                "--resume".to_owned(),
                "11111111-2222-4333-8444-555555555555".to_owned(),
                "--name".to_owned(),
                "ide.7".to_owned(),
            ])
        );
    }

    #[test]
    fn codex_chooses_its_own_id_so_nothing_is_added_at_the_start() {
        // `charter/harness/codex.py:255` — Codex names no flag to choose an id or a name
        // (openai/codex#14482), so a new Codex session is started plain.
        let id = SessionId::new("11111111-2222-4333-8444-555555555555").unwrap();

        assert!(!Harness::Codex.chooses_session_id());
        assert_eq!(
            Harness::Codex.new_session_argv(&id, "ide.7"),
            Vec::<String>::new()
        );
    }

    #[test]
    fn codex_resumes_by_id_alone_and_is_never_given_a_name() {
        // `charter/harness/codex.py:263` — `codex resume <id>`; Codex has no flag to set a
        // name, so a name charter did not set cannot identify a session.
        let id = SessionId::new("11111111-2222-4333-8444-555555555555").unwrap();

        assert_eq!(
            Harness::Codex.resume_argv(&id, "ide.7"),
            Some(vec![
                "resume".to_owned(),
                "11111111-2222-4333-8444-555555555555".to_owned(),
            ])
        );
    }

    #[test]
    fn a_launch_that_already_names_a_session_is_left_alone() {
        // `charter/harness/claude_code.py:298` and `codex.py:261` — every flag by which an
        // operator names a session themselves. Adding charter's beside one of these gives a
        // command line no harness has been measured against.
        for flag in [
            "--session-id",
            "--resume",
            "-r",
            "--continue",
            "-c",
            "--fork-session",
        ] {
            assert!(
                Harness::ClaudeCode.session_named_in(&[flag.to_owned()]),
                "{flag} was not read as the operator naming a session"
            );
        }
        for word in ["resume", "fork"] {
            assert!(Harness::Codex.session_named_in(&[word.to_owned()]));
        }
    }

    #[test]
    fn an_ordinary_launch_does_not_look_like_one_that_names_a_session() {
        assert!(!Harness::ClaudeCode.session_named_in(&["--model".to_owned(), "opus".to_owned()]));
        // Codex's are subcommands, not flags, so a word that merely contains one is not one.
        assert!(!Harness::Codex.session_named_in(&["--resume".to_owned()]));
        assert!(!Harness::ClaudeCode.session_named_in(&[]));
    }

    #[test]
    fn a_flag_written_with_its_value_attached_still_names_a_session() {
        // `--resume=<id>` is one argument, and a harness reads it as the flag it is.
        assert!(Harness::ClaudeCode.session_named_in(&["--resume=abc".to_owned()]));
        // But a longer flag that merely starts the same way is a different flag.
        assert!(!Harness::ClaudeCode.session_named_in(&["--resume-later".to_owned()]));
    }

    #[test]
    fn a_harness_is_recognised_by_the_name_its_command_is_invoked_under() {
        assert_eq!(Harness::of_command("claude"), Some(Harness::ClaudeCode));
        assert_eq!(Harness::of_command("codex"), Some(Harness::Codex));
        assert_eq!(
            Harness::of_command("/Users/aharon/.local/bin/claude"),
            Some(Harness::ClaudeCode)
        );
    }

    #[test]
    fn a_program_that_is_not_a_measured_harness_is_not_one() {
        // A shell is the app's own default program, and opencode is a harness the Python
        // charter measured but this one has not — both answer "no harness" rather than a guess.
        assert_eq!(Harness::of_command("/bin/zsh"), None);
        assert_eq!(Harness::of_command("opencode"), None);
    }

    #[test]
    fn a_session_id_charter_chose_is_a_uuid_nothing_has_to_quote() {
        let id = SessionId::fresh();

        assert_eq!(id.as_str().len(), 36);
        assert!(
            SessionId::new(id.as_str()).is_ok(),
            "a fresh id is one the guard takes back: {id}"
        );
    }

    #[test]
    fn two_fresh_session_ids_are_never_the_same() {
        assert_ne!(SessionId::fresh(), SessionId::fresh());
    }

    #[test]
    fn a_session_id_that_could_be_read_as_a_flag_is_refused() {
        // This is the whole reason the newtype exists: an id comes back off disk and goes
        // straight into argv, so one starting with `-` would be a flag the operator never typed.
        for bad in [
            "--dangerously-skip-permissions",
            "-r",
            "",
            "a b",
            "a/../b",
            "a\nb",
        ] {
            assert_eq!(
                SessionId::new(bad),
                Err(SessionIdError::Malformed(bad.to_owned())),
                "{bad:?} was taken as a session id"
            );
        }
    }

    #[test]
    fn a_session_id_longer_than_the_python_charter_takes_is_refused() {
        // `charter/frame/state.py:1128` — `[A-Za-z0-9][A-Za-z0-9_-]{0,127}`, so 128 is the
        // longest both implementations accept.
        assert!(SessionId::new("a".repeat(128)).is_ok());
        assert!(SessionId::new("a".repeat(129)).is_err());
    }

    #[test]
    fn claude_code_state_hooks_are_armed_on_this_session_and_change_nothing_else() {
        // `--settings` MERGES rather than replaces (measured on claude 2.1.276), which is the
        // whole reason this is safe: the charter plugin's guard hooks keep running. If this
        // ever became a replacement, arming state here would silently disarm the guard.
        let hooks = Harness::ClaudeCode.state_hooks(std::path::Path::new("/usr/local/bin/charter"));

        let StateHooks::ThisSessionOnly {
            args,
            cannot_report,
        } = hooks
        else {
            panic!("Claude Code's hooks are armed per session");
        };
        assert_eq!(args[0], "--settings");
        assert!(cannot_report.is_empty(), "{cannot_report:?}");

        let settings: serde_json::Value =
            serde_json::from_str(&args[1]).expect("the settings are JSON");
        let armed = settings["hooks"].as_object().expect("an object");
        assert_eq!(armed.len(), 6);
        for event in [
            "SessionStart",
            "UserPromptSubmit",
            "Notification",
            "SubagentStop",
            "Stop",
            "SessionEnd",
        ] {
            assert_eq!(
                armed[event][0]["hooks"][0]["command"],
                serde_json::json!(format!(
                    "'/usr/local/bin/charter' hook {}",
                    event.to_lowercase()
                )),
                "{event} is not armed"
            );
            // Far more than the 1.8 ms this call was measured at, and far less than a turn:
            // a hook that somehow hung must not be able to hold the turn open behind it.
            assert_eq!(
                armed[event][0]["hooks"][0]["timeout"],
                serde_json::json!(5),
                "{event}'s hook is armed with no short deadline"
            );
        }
    }

    #[test]
    fn no_guard_hook_is_ever_armed_by_the_app() {
        // The guard is the Python charter's, it is security-critical, and it is not this
        // milestone's to answer. Arming one here would run a Rust hook that decides less
        // than the guard does in front of every tool call.
        let hooks = Harness::ClaudeCode.state_hooks(std::path::Path::new("/bin/charter"));
        let StateHooks::ThisSessionOnly { args, .. } = hooks else {
            panic!("armed per session");
        };

        for guarded in [
            "PreToolUse",
            "PostToolUse",
            "PermissionRequest",
            "pretooluse",
        ] {
            assert!(
                !args[1].contains(guarded),
                "{guarded} appears in the settings the app arms"
            );
        }
    }

    #[test]
    fn a_path_with_a_quote_in_it_cannot_break_out_of_the_hook_command() {
        // Claude Code runs the command through `/bin/sh -c`.
        let hooks = Harness::ClaudeCode.state_hooks(std::path::Path::new("/home/o'brien/charter"));
        let StateHooks::ThisSessionOnly { args, .. } = hooks else {
            panic!("armed per session");
        };
        let settings: serde_json::Value = serde_json::from_str(&args[1]).expect("JSON");

        assert_eq!(
            settings["hooks"]["Stop"][0]["hooks"][0]["command"],
            serde_json::json!(r"'/home/o'\''brien/charter' hook stop")
        );
    }

    #[test]
    fn codex_says_why_its_state_hooks_cannot_be_armed_here() {
        // codex-cli 0.147.0: no project-level config exists at all, and no Notification
        // event exists either. A Codex chat that showed `unknown` with no reason would look
        // like charter was broken rather than like Codex is different.
        let hooks = Harness::Codex.state_hooks(std::path::Path::new("/bin/charter"));

        let StateHooks::MachineWideOnly { why } = hooks else {
            panic!("Codex has no per-session answer");
        };
        assert!(why.contains("~/.codex/config.toml"));
        assert!(why.contains("Notification"));
    }
}
