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

    /// The harness a profile's DECLARED `kind` names, or none for a kind this app does not
    /// start.
    ///
    /// Not [`Self::of_command`], and the difference is the whole reason both exist.
    /// `of_command` INFERS a harness from a program name and cannot tell a shell from a
    /// harness charter has not measured — both are `None`, deliberately, and no refusal may
    /// rest on telling them apart. A `kind` is not inferred: the operator wrote one of three
    /// words in `charter.local.toml` and charter's own validator already refused anything
    /// else. So this answer IS knowable, and a launch may refuse on it.
    ///
    /// `opencode` is a kind `profiles` reads and this app does not start (spec decision 6).
    pub fn of_kind(kind: &str) -> Option<Self> {
        match kind {
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
    /// Charter has not measured how this program reports anything, or it is not a harness.
    None,
}

impl Harness {
    /// How this harness is asked to report its state, with `charter` at `binary`, for a chat
    /// that will run in `cwd`.
    ///
    /// `cwd` decides one thing only: whether charter may also fill Claude Code's status line
    /// for this chat ([`crate::footerclaim`]). It is the chat's own directory because project
    /// settings are read from the session's own directory and the host does not walk up.
    pub fn state_hooks(
        self,
        binary: &std::path::Path,
        cwd: Option<&std::path::Path>,
    ) -> StateHooks {
        match self {
            // `--settings` MERGES with the settings files already in force rather than
            // replacing them — measured on claude 2.1.276 by planting a project hook beside
            // one of these and watching both fire. That is what makes this safe: the charter
            // plugin's guard hooks keep running exactly as they were.
            Self::ClaudeCode => StateHooks::ThisSessionOnly {
                args: words([
                    "--settings",
                    &claude_code_settings(binary, crate::footerclaim::status_line(cwd).free()),
                ]),
                cannot_report: Vec::new(),
            },
            // **Measured on codex-cli 0.147.0, and it refutes what this arm used to say** —
            // that Codex could only be armed in `~/.codex/config.toml` and could never say it
            // was waiting. Every fact here was taken from the real binary driving a real turn
            // against a stand-in model server, the TUI in a pane as the app runs it (#27):
            //
            // * `-c hooks.<Event>=[…]` arms a hook for ONE session. Codex lists its source as
            //   "Session flags", and it runs BESIDE the operator's own `[[hooks.<Event>]]` for
            //   the same event rather than replacing it — both fired. Nothing is written.
            // * `SessionStart`, `UserPromptSubmit`, `Stop` and `SessionEnd` each fire with
            //   `session_id` in the payload. `SessionStart` names a `source` (`startup`, and
            //   `resume` with the SAME id on `codex resume <id>`); `SessionEnd` names a
            //   `reason` (`other`, on `/quit`). `Stop` is "right before Codex ends its turn",
            //   which is the falling edge `Stop` means for Claude Code.
            // * There is no `Notification`. Codex tells a hook it is asking for approval only
            //   through `PermissionRequest` — which fired exactly when the prompt appeared,
            //   and not for a command that needed none — but that is a hook that DECIDES a
            //   permission, and the app arms no such hook (see the guard test below). So a
            //   Codex chat that stops mid-turn for approval cannot say so.
            // * **And there is no second way round it** (charter-app#52, read out of the same
            //   0.147.0 binary the measurements above were taken on). The binary carries
            //   eleven hook events and no more — `PreToolUse`, `PermissionRequest`,
            //   `PostToolUse`, `PreCompact`, `PostCompact`, `SessionStart`, `SessionEnd`,
            //   `UserPromptSubmit`, `SubagentStart`, `SubagentStop`, `Stop` — and the only
            //   one that fires when the prompt appears is the one that decides it. The
            //   `notify` program is not a second channel either: in 0.147.0 it is
            //   `legacy_notify`, a shim over `Stop` whose one payload type is
            //   `agent-turn-complete`, which is the falling edge `Stop` already gives. What
            //   is left is `PreToolUse` without a matching `PostToolUse` for long enough —
            //   which is a guess at timing over a harness's behaviour, and ADR 0018 admits
            //   no state that did not come from a hook saying so.
            // * `SessionStart` fires inside the FIRST TURN, not at launch (X1): an idle TUI
            //   reported nothing at all until a prompt was typed.
            // * A hook is inert until Codex trusts it. For these, the TUI itself asks at
            //   startup — "Hooks need review … Trust all and continue / Continue without
            //   trusting (hooks won't run)" — and Codex writes the answer into its own
            //   `[hooks.state]`, keyed by event, position and a hash of the hook. The same
            //   binary path arms the same hooks, so the operator is asked once and not once a
            //   chat. Untrusted, they do not run and nothing says so (`codex exec`).
            Self::Codex => StateHooks::ThisSessionOnly {
                args: codex_session_flags(binary),
                cannot_report: vec!["notification"],
            },
        }
    }

    /// What a chat on this harness cannot tell charter, in a sentence the chat shows — or
    /// none, where it can tell charter everything the board asks.
    ///
    /// Said ON the chat, not left for the operator to infer from a quiet sidebar: a Codex
    /// chat waiting on an approval looks exactly like one that is working, and a chat that
    /// reports nothing looks like charter is broken rather than like the harness is
    /// different.
    pub fn unreported(self) -> Option<&'static str> {
        match self {
            Self::ClaudeCode => None,
            Self::Codex => Some(
                "Codex says nothing until your first prompt, and nothing at all until you \
                 trust charter's hooks when Codex asks; it never says when it stops mid-turn \
                 for your approval.",
            ),
        }
    }
}

/// The settings that arm Claude Code's state hooks on one session, as JSON on the argument.
///
/// On the argument and not in a file: a file would have to be written somewhere, cleaned up
/// when the chat ends, and cleaned up again after an app that crashed. What is in it is a
/// path and six event names — nothing secret, so `ps` showing it costs nothing.
fn claude_code_settings(binary: &std::path::Path, may_fill_the_footer: bool) -> String {
    let hooks: serde_json::Map<String, serde_json::Value> = CLAUDE_CODE_STATE_EVENTS
        .iter()
        .map(|(event, word)| {
            (
                (*event).to_owned(),
                serde_json::json!([{
                    "hooks": [{
                        "type": "command",
                        "command": hook_command(binary, word),
                        // Far more than the 1.8 ms this call was measured at, and far less
                        // than a turn. A hook that somehow hung must not hold the turn open.
                        "timeout": HOOK_TIMEOUT_SECONDS,
                    }],
                }]),
            )
        })
        .collect();
    let mut settings: serde_json::Map<String, serde_json::Value> =
        [("hooks".to_owned(), serde_json::Value::Object(hooks))]
            .into_iter()
            .collect();
    // **Only where nothing else fills it.** The key is one value and the flag is the last
    // writer, so arming it where the operator has their own would stop theirs running
    // ([`crate::footerclaim`], measured on 2.1.280). Where charter does not arm it, the chat
    // records no turns and its `ctx`/`cache` gauge stays dark — which `doctor` reports.
    if may_fill_the_footer {
        settings.insert("statusLine".to_owned(), claude_code_status_line(binary));
    }
    serde_json::Value::Object(settings).to_string()
}

/// Claude Code's `statusLine`, pointed at `charter statusline` — for THIS session only.
///
/// **This is how a chat's `ctx`/`cache` history gets written at all, and without it the app's
/// gauge is a renderer with nothing to render.** Claude Code hands `context_window` — the
/// context percentage and the cache numbers — to its `statusLine` command and to nothing
/// else: no hook payload carries them (charter ADR 0019, measured). `charter statusline`
/// is the command that writes them down (`usage::record`), on every render, whether or not
/// it draws anything. And since charter 0.57.0 (#895) nothing puts that command in a
/// session's settings any more, while the app arms only hooks — so, measured on 2026-09-22,
/// the operator's own plane had not had a turn recorded since 2026-09-04, and every chat the
/// app started ran with no gauge on any surface.
///
/// **What the chat SEES does not change by default.** Inside the app `charter statusline`
/// prints an empty line (ADR 0019 transposed, `charter-cli/src/statusline.rs`), which is
/// the footer the app's chats were already meant to have — and ADR 0029's per-chat checkbox,
/// which sets `CHARTER_FOOTER=show`, draws charter's footer instead. That checkbox was a
/// switch on a command nothing invoked; this is what makes it one.
///
/// **It is armed only where the operator fills the line with nothing**, and that is the
/// operator's own ruling of 2026-09-22. `--settings` merges key by key and `statusLine` is one
/// key, so arming it over somebody's own command would stop theirs from running, silently, for
/// every chat the app starts — measured on 2.1.280, four cells, one launch each
/// ([`crate::footerclaim`], which holds the measurement and the rule). Where something else
/// fills it, charter arms nothing, leaves their configuration alone, and lets `doctor` say why
/// the gauge is dark. Nothing here wraps or chains their command: charter would then own its
/// failures and its latency, every turn.
///
/// **What it costs otherwise, said:** nothing is written to any file, their own `claude` in a
/// terminal is untouched, and a command costs one process per footer render — which Claude
/// Code runs at startup and once per submitted turn, not on a timer (measured: zero
/// invocations over thirty idle seconds).
fn claude_code_status_line(binary: &std::path::Path) -> serde_json::Value {
    serde_json::json!({
        "type": "command",
        "command": format!("{} statusline", shell_quoted(&binary.display().to_string())),
    })
}

/// The `-c` pairs that arm Codex's state hooks on one session.
///
/// On the argument for the reason Claude Code's are: nothing is written, so nothing is left
/// behind. Each value is TOML, because that is how Codex parses a `-c` value — and it is
/// SERIALISED rather than formatted, since a value that fails to parse is not an error to
/// Codex but a literal string, which it then rejects as the wrong type and refuses to start.
fn codex_session_flags(binary: &std::path::Path) -> Vec<String> {
    CODEX_STATE_EVENTS
        .iter()
        .flat_map(|(event, word)| {
            let hook: toml::Table = [
                ("type".to_owned(), toml::Value::from("command")),
                (
                    "command".to_owned(),
                    toml::Value::from(hook_command(binary, word)),
                ),
                // Codex's own default is 600 seconds (its hook review screen says so).
                (
                    "timeout".to_owned(),
                    toml::Value::from(HOOK_TIMEOUT_SECONDS),
                ),
            ]
            .into_iter()
            .collect();
            let group: toml::Table = [(
                "hooks".to_owned(),
                toml::Value::Array(vec![toml::Value::Table(hook)]),
            )]
            .into_iter()
            .collect();
            let value = toml::Value::Array(vec![toml::Value::Table(group)]);
            ["-c".to_owned(), format!("hooks.{event}={value}")]
        })
        .collect()
}

/// The command a hook runs: charter's own binary and one event word.
///
/// Two words, no composition: the command is a path charter knows and a word from one of
/// the lists below. Nothing here is built from anything a harness, a plane or an operator
/// wrote. Both harnesses run it through a shell — Claude Code through `/bin/sh -c` (ADR
/// 0024), and Codex measured the same way, a single-quoted argument holding spaces arriving
/// as one word.
fn hook_command(binary: &std::path::Path, word: &str) -> String {
    format!(
        "{} hook {word}",
        shell_quoted(&binary.display().to_string())
    )
}

/// How long a state hook may take, in seconds, on every harness.
const HOOK_TIMEOUT_SECONDS: i64 = 5;

/// Codex's state-carrying events, and the word `charter hook` takes for each.
///
/// Each was seen to fire on codex-cli 0.147.0 (see [`Harness::state_hooks`]). Not
/// `SubagentStop`, which Codex has and the board ignores: every hook armed here is one more
/// the operator is asked to trust, and this one would change nothing they can see.
const CODEX_STATE_EVENTS: &[(&str, &str)] = &[
    ("SessionStart", "sessionstart"),
    ("UserPromptSubmit", "userpromptsubmit"),
    ("Stop", "stop"),
    ("SessionEnd", "sessionend"),
];

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
/// Both harnesses run a hook's `command` through a shell (ADR 0024 measured the shells in
/// between for Claude Code), so the path to the binary goes in single quotes. It is
/// charter's own executable path and not anything a plane wrote, but a person with a quote
/// in their home directory name is not a security model.
fn shell_quoted(path: &str) -> String {
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
        let empty = tempfile::tempdir().expect("a directory with no settings in it");
        let hooks = Harness::ClaudeCode.state_hooks(
            std::path::Path::new("/usr/local/bin/charter"),
            Some(empty.path()),
        );

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
    fn a_claude_code_chat_runs_charter_statusline_so_its_turns_are_recorded() {
        // Claude Code hands the context and cache numbers to its `statusLine` command and to
        // nothing else, so a chat whose settings name none records nothing and the app's
        // gauge has nothing to draw. Quoted as a hook command is, because Claude Code runs it
        // through `/bin/sh -c` too.
        let empty = tempfile::tempdir().expect("a directory with no settings in it");
        let hooks = Harness::ClaudeCode.state_hooks(
            std::path::Path::new("/home/o'brien/charter"),
            Some(empty.path()),
        );
        let StateHooks::ThisSessionOnly { args, .. } = hooks else {
            panic!("armed per session");
        };
        let settings: serde_json::Value = serde_json::from_str(&args[1]).expect("JSON");

        assert_eq!(
            settings["statusLine"],
            serde_json::json!({
                "type": "command",
                "command": r"'/home/o'\''brien/charter' statusline",
            })
        );
    }

    #[test]
    fn a_chat_whose_directory_already_fills_the_footer_is_armed_with_hooks_alone() {
        // The operator's ruling: charter never replaces a `statusLine` somebody else wrote.
        // The hooks are armed exactly as before — only the footer key is left off.
        let dir = tempfile::tempdir().expect("a directory");
        std::fs::create_dir_all(dir.path().join(".claude")).expect(".claude");
        std::fs::write(
            dir.path().join(".claude/settings.json"),
            r#"{"statusLine": {"type": "command", "command": "my-own-line"}}"#,
        )
        .expect("their settings");

        let hooks =
            Harness::ClaudeCode.state_hooks(std::path::Path::new("/bin/charter"), Some(dir.path()));

        let StateHooks::ThisSessionOnly { args, .. } = hooks else {
            panic!("armed per session");
        };
        let settings: serde_json::Value = serde_json::from_str(&args[1]).expect("JSON");
        assert!(
            settings.get("statusLine").is_none(),
            "charter armed a statusLine over the operator's: {}",
            args[1]
        );
        assert_eq!(
            settings["hooks"].as_object().expect("an object").len(),
            6,
            "the state hooks are not armed for a chat that keeps its own footer"
        );
    }

    #[test]
    fn no_guard_hook_is_ever_armed_by_the_app() {
        // The guard is the Python charter's, it is security-critical, and it is not this
        // milestone's to answer. Arming one here would run a Rust hook that decides less
        // than the guard does in front of every tool call.
        let empty = tempfile::tempdir().expect("a directory with no settings in it");
        let hooks = Harness::ClaudeCode
            .state_hooks(std::path::Path::new("/bin/charter"), Some(empty.path()));
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
        let empty = tempfile::tempdir().expect("a directory with no settings in it");
        let hooks = Harness::ClaudeCode.state_hooks(
            std::path::Path::new("/home/o'brien/charter"),
            Some(empty.path()),
        );
        let StateHooks::ThisSessionOnly { args, .. } = hooks else {
            panic!("armed per session");
        };
        let settings: serde_json::Value = serde_json::from_str(&args[1]).expect("JSON");

        assert_eq!(
            settings["hooks"]["Stop"][0]["hooks"][0]["command"],
            serde_json::json!(r"'/home/o'\''brien/charter' hook stop")
        );
    }

    /// Codex's `-c` pairs as (dotted key, parsed TOML value), failing on anything else.
    fn codex_flags(binary: &str) -> Vec<(String, toml::Value)> {
        let StateHooks::ThisSessionOnly { args, .. } =
            Harness::Codex.state_hooks(std::path::Path::new(binary), None)
        else {
            panic!("Codex's hooks are armed per session");
        };
        args.chunks(2)
            .map(|pair| {
                assert_eq!(pair[0], "-c", "not a -c pair: {pair:?}");
                let (key, value) = pair[1].split_once('=').expect("key=value");
                // How Codex reads it: the value is TOML. Parsed the same way here, so a value
                // Codex would take as a literal string fails this test instead of the chat.
                let parsed: toml::Table =
                    toml::from_str(&format!("v = {value}")).expect("the value is TOML");
                (key.to_owned(), parsed["v"].clone())
            })
            .collect()
    }

    #[test]
    fn codex_state_hooks_are_armed_on_this_session_by_its_own_flag() {
        // Measured on codex-cli 0.147.0 (#27): `-c hooks.<Event>=[…]` arms a hook for one
        // session, listed by Codex as "Session flags", and it runs BESIDE the operator's own
        // hook for the same event rather than replacing it. Nothing is written anywhere.
        let flags = codex_flags("/usr/local/bin/charter");

        let keys: Vec<&str> = flags.iter().map(|(key, _)| key.as_str()).collect();
        assert_eq!(
            keys,
            [
                "hooks.SessionStart",
                "hooks.UserPromptSubmit",
                "hooks.Stop",
                "hooks.SessionEnd"
            ]
        );
        for (key, value) in &flags {
            let word = key.trim_start_matches("hooks.").to_lowercase();
            let hook = &value[0]["hooks"][0];
            assert_eq!(hook["type"].as_str(), Some("command"), "{key}");
            assert_eq!(
                hook["command"].as_str(),
                Some(format!("'/usr/local/bin/charter' hook {word}").as_str()),
                "{key} does not run charter's own binary with its word"
            );
            // Codex's default is 600 seconds. A hook that somehow hung must not be able to
            // hold a turn open behind it for ten minutes.
            assert_eq!(hook["timeout"].as_integer(), Some(5), "{key}");
        }
    }

    #[test]
    fn a_codex_chat_says_it_cannot_report_a_question_asked_mid_turn() {
        // Codex has no `Notification`. It fires `PermissionRequest` when it asks for an
        // approval — measured — but that hook decides a permission, and the app arms none
        // (the guard test below). So the one event missing is named, not hidden.
        let hooks = Harness::Codex.state_hooks(std::path::Path::new("/bin/charter"), None);

        let StateHooks::ThisSessionOnly { cannot_report, .. } = hooks else {
            panic!("armed per session");
        };
        assert_eq!(cannot_report, ["notification"]);
        let said = Harness::Codex
            .unreported()
            .expect("a Codex chat says what it cannot report");
        assert!(said.contains("mid-turn"), "{said}");
        // The two reasons a Codex chat reads `unknown`, both measured: `SessionStart` fires
        // inside the first turn, and a hook is inert until Codex's own review trusts it.
        assert!(said.contains("first prompt"), "{said}");
        assert!(said.contains("trust"), "{said}");
    }

    #[test]
    fn a_claude_code_chat_leaves_nothing_unsaid() {
        assert_eq!(Harness::ClaudeCode.unreported(), None);
    }

    #[test]
    fn no_guard_hook_is_ever_armed_on_codex_either() {
        // `PermissionRequest` is where Codex would say it is asking for approval, and it
        // is armed nowhere: a hook there can ALLOW or DENY. That is the guard's question,
        // not a state board's, whatever the hook happens to answer.
        for (key, _) in codex_flags("/bin/charter") {
            for guarded in ["PreToolUse", "PostToolUse", "PermissionRequest"] {
                assert!(!key.contains(guarded), "{key} is armed by the app");
            }
        }
    }

    #[test]
    fn a_path_with_a_quote_in_it_cannot_break_out_of_a_codex_hook_either() {
        // Codex runs a hook's command through a shell too — measured: a single-quoted
        // argument holding spaces arrived as one word. And the TOML around it must survive
        // the quote, which is why the value is serialised and not formatted.
        let flags = codex_flags("/home/o'brien/char\"ter");

        assert_eq!(
            flags[2].1[0]["hooks"][0]["command"].as_str(),
            Some(r#"'/home/o'\''brien/char"ter' hook stop"#)
        );
    }
}
