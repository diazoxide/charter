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
    /// Armed through a plugin the app ships ([`crate::opencode`], ADR 0058).
    Opencode,
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
            "opencode" => Some(Self::Opencode),
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
    /// All three kinds `profiles` reads are started since #371, when opencode joined.
    pub fn of_kind(kind: &str) -> Option<Self> {
        match kind {
            "claude" => Some(Self::ClaudeCode),
            "codex" => Some(Self::Codex),
            "opencode" => Some(Self::Opencode),
            _ => None,
        }
    }

    /// The word the plane calls this harness by (`charter.local.toml`'s `kind`).
    pub fn name(self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude",
            Self::Codex => "codex",
            Self::Opencode => "opencode",
        }
    }

    /// Whether this harness tells a hook which process it is running under.
    ///
    /// Claude Code sets `$CLAUDE_PID` in every hook's environment, and that is the whole of
    /// what tells `/clear` (a new conversation from the same process, ADR 0024 C6) from a
    /// `claude` started inside the chat's own shell (a new conversation from a different
    /// one, C5). Codex names no such variable, so it gets the narrower rule: the first
    /// report of a chat is adopted and a later different id is ignored. opencode names none
    /// either, and gets the same rule.
    pub fn reports_its_process(self) -> bool {
        match self {
            Self::ClaudeCode => true,
            Self::Codex | Self::Opencode => false,
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
            // opencode has no flag that takes an id for a new session: `-s` resumes one that
            // exists. Its id is what its plugin reports, at the first prompt (ADR 0058).
            Self::Opencode => false,
        }
    }

    /// The arguments that start a new session under `id`, or none for a harness that
    /// chooses its own id.
    pub fn new_session_argv(self, id: &SessionId, name: &str) -> Vec<String> {
        match self {
            Self::ClaudeCode => words(["--session-id", id.as_str(), "--name", name]),
            Self::Codex | Self::Opencode => Vec::new(),
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
            // `opencode --help` on 1.18.23: `-s/--session <id>` and `-c/--continue`.
            Self::Opencode => &["-s", "--session", "-c", "--continue"],
        }
    }

    /// The arguments that bring the conversation `id` back, or none where charter has not
    /// measured how this harness resumes.
    pub fn resume_argv(self, id: &SessionId, name: &str) -> Option<Vec<String>> {
        Some(match self {
            Self::ClaudeCode => words(["--resume", id.as_str(), "--name", name]),
            Self::Codex => words(["resume", id.as_str()]),
            // `opencode -s <id>`; opencode has no flag that names a session.
            Self::Opencode => words(["-s", id.as_str()]),
        })
    }
}

/// What a harness can tell charter about its own state, and how it is asked to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StateHooks {
    /// Armed on this session alone, by the arguments and the environment given.
    ///
    /// Nothing outside this session is changed: no config folder is written, and a harness
    /// the operator runs in a terminal is untouched.
    ThisSessionOnly {
        args: Vec<String>,
        /// Variables the session's own process needs, beside the ones the app always sets.
        env: Vec<(String, String)>,
        /// Events it cannot report, by the word `charter hook` takes. Empty is the whole set.
        cannot_report: Vec<&'static str>,
    },
    /// Charter has not measured how this program reports anything, or it is not a harness.
    None,
}

/// What the app ships that a chat is armed with: its own `charter`, and its own plugin.
#[derive(Debug, Clone, Copy)]
pub struct Kit<'a> {
    /// The `charter` every hook runs.
    pub binary: &'a std::path::Path,
    /// The bundled Claude Code plugin ([`crate::plugin`]), where the app found it. Its
    /// [`crate::opencode::SHIM_IN_BUNDLE`] is the plugin an opencode chat loads.
    pub plugin: Option<&'a std::path::Path>,
}

impl Harness {
    /// How this harness is asked to report its state and run charter's guard, with what the
    /// app ships in `kit`, for a chat that will run in `cwd`.
    ///
    /// `cwd` decides one thing only: whether charter may also fill Claude Code's status line
    /// for this chat ([`crate::footerclaim`]). It is the chat's own directory because project
    /// settings are read from the session's own directory and the host does not walk up.
    ///
    /// `plugins` is the harness's own plugins the project turned on or off for this chat
    /// ([`crate::harness_plugin::chosen`], charter-app#274), by the harness's id. Only an
    /// adapter that applies per chat hands it anything, so for Codex it is always empty; it is
    /// taken here all the same, so no harness can be armed past it.
    pub fn state_hooks(
        self,
        kit: Kit<'_>,
        cwd: Option<&std::path::Path>,
        plugins: &crate::harness_plugin::Chosen,
    ) -> StateHooks {
        match self {
            // **The bundled plugin, loaded for this session alone** (`crate::plugin` has the
            // measurements). Its `hooks.json` holds every hook — the six that report state and
            // the Bash guard — and names the binary by `$CHARTER_HOOK_BINARY`, so that is
            // handed over in the environment. Without the plugin there is nothing to arm: a
            // chat reads `unknown`, rather than being half-armed from a second place.
            //
            // `--settings` MERGES with the settings already in force rather than replacing
            // them (measured on claude 2.1.276), and a key it names wins over the project's
            // (measured on 2.1.280). It carries two keys: the Python charter's plugin turned
            // off for this session, and the status line where charter may fill it.
            Self::ClaudeCode => {
                let Some(plugin) = kit.plugin else {
                    return StateHooks::None;
                };
                StateHooks::ThisSessionOnly {
                    args: words([
                        "--plugin-dir",
                        &plugin.display().to_string(),
                        "--settings",
                        &claude_code_settings(
                            kit.binary,
                            crate::footerclaim::status_line(cwd).free(),
                            plugins,
                        ),
                    ]),
                    env: vec![(
                        crate::plugin::BINARY_ENV.to_owned(),
                        kit.binary.display().to_string(),
                    )],
                    cannot_report: Vec::new(),
                }
            }
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
            //   permission, and the app arms no such hook. So a Codex chat that stops
            //   mid-turn for approval cannot say so.
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
            //
            // Codex has no plugin here: the guard used to reach a Codex chat through the
            // Python charter's Codex plugin, and now rides on the same `-c` flags as the state
            // hooks, from the same registry ([`crate::plugin::CODEX`] out of [`crate::hookreg`]).
            //
            // And no plugin: Codex 0.147.0 takes a plugin's `enabled` from its `config.toml`
            // alone and ignores the same key given with `-c` (measured, charter-app#274), so
            // `plugins` is empty for it and nothing here would carry it.
            Self::Codex => StateHooks::ThisSessionOnly {
                args: codex_session_flags(kit.binary),
                env: Vec::new(),
                cannot_report: vec!["notification"],
            },
            // **The bundled shim, loaded for this session alone** ([`crate::opencode`] has the
            // measurements, opencode 1.18.23). opencode reads a whole config from
            // `OPENCODE_CONFIG_CONTENT` and concatenates its `plugin` list with every other
            // config's, so naming the shim there loads it beside whatever the operator loads,
            // writes nothing, and a project file cannot take it out. It runs the binary the
            // environment names, as the Claude Code plugin's hooks do. `OPENCODE_PURE=0`
            // because `1` would load no plugin at all; the flag that does the same is refused
            // before the chat starts ([`crate::opencode::disarmed_by`]).
            //
            // No plugin choice: opencode has no switch that turns one plugin off.
            //
            // It cannot report `SessionEnd`: quitting opencode fires no event and runs no exit
            // handler in a plugin (measured). The app sees the process end.
            Self::Opencode => {
                let Some(shim) = kit
                    .plugin
                    .map(|plugin| plugin.join(crate::opencode::SHIM_IN_BUNDLE))
                    .filter(|shim| shim.is_file())
                else {
                    return StateHooks::None;
                };
                StateHooks::ThisSessionOnly {
                    args: Vec::new(),
                    env: vec![
                        (
                            crate::plugin::BINARY_ENV.to_owned(),
                            kit.binary.display().to_string(),
                        ),
                        (
                            crate::opencode::CONFIG_ENV.to_owned(),
                            crate::opencode::session_config(&shim),
                        ),
                        (crate::opencode::PURE_ENV.to_owned(), "0".to_owned()),
                    ],
                    cannot_report: vec!["sessionend"],
                }
            }
        }
    }

    /// Why a chat of this harness started with `command` and `env` would run without charter's
    /// hooks — `None` when it would not. Only opencode has such a switch charter can see
    /// ([`crate::opencode::disarmed_by`]): Claude Code loads `--plugin-dir` whatever else it is
    /// told, and Codex's session flags are charter's own.
    pub fn disarmed_by(self, command: &[String], env: &[(String, String)]) -> Option<String> {
        match self {
            Self::ClaudeCode | Self::Codex => None,
            Self::Opencode => crate::opencode::disarmed_by(command, env),
        }
    }

    /// What the app arms a chat of this harness with, as `charter doctor` says it.
    pub fn armed_with(self) -> String {
        match self {
            Self::ClaudeCode => format!(
                "the app arms each chat with its own plugin, {}",
                crate::plugin::LOADED_AS
            ),
            Self::Codex => {
                "the app arms each Codex chat with charter's hooks; Codex asks once to trust them"
                    .to_owned()
            }
            Self::Opencode => {
                "the app arms each opencode chat with charter's opencode plugin, for that chat alone"
                    .to_owned()
            }
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
            Self::Opencode => Some(
                "opencode says nothing until your first prompt, and nothing when it quits; once \
                 you answer its permission prompt, the chat reads waiting until the turn ends, \
                 and a session you open inside it with /new is not followed.",
            ),
        }
    }
}

/// The settings a Claude Code chat is started with, as JSON on the argument.
///
/// On the argument and not in a file: a file would have to be written somewhere, cleaned up
/// when the chat ends, and cleaned up again after an app that crashed. What is in it is a
/// plugin id and a path — nothing secret, so `ps` showing it costs nothing.
///
/// **No hooks.** They are the bundled plugin's (`hooks/hooks.json`), so a chat has one place
/// its hooks are declared and a hook is never armed twice.
fn claude_code_settings(
    binary: &std::path::Path,
    may_fill_the_footer: bool,
    plugins: &crate::harness_plugin::Chosen,
) -> String {
    let mut settings = serde_json::Map::new();
    // The project's own choice of Claude Code's plugins first (charter-app#274, ADR 0050): a
    // session `enabledPlugins` wins over the project's and the user's for the keys it names,
    // and leaves every other plugin to them.
    let mut enabled: serde_json::Map<String, serde_json::Value> = plugins
        .iter()
        .map(|(id, on)| (id.clone(), (*on).into()))
        .collect();
    // Then the pins, written last so nothing above can move them
    // ([`crate::harness_plugin::CLAUDE_CODE`] holds them and says why). The operator's ruling of
    // 2026-09-23: a chat the app starts turns the Python charter's plugin off for itself, so the
    // project files that enable it for the operator's own terminal sessions do not give an app
    // chat two sets of hooks and two `handoff` skills. And the bundled plugin pinned on: a
    // project file can turn a `--plugin-dir` plugin off by its id, a chat can write that file,
    // and the plugin carries the Bash guard. Measured on 2.1.280: this `true` wins over a
    // project's `false`.
    for pin in crate::harness_plugin::Adapter::pinned(&crate::harness_plugin::CLAUDE_CODE) {
        enabled.insert(pin.id.to_owned(), pin.on.into());
    }
    settings.insert(
        "enabledPlugins".to_owned(),
        serde_json::Value::Object(enabled),
    );
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
        "command": format!(
            "{} statusline",
            crate::plugin::shell_quoted(&binary.display().to_string())
        ),
    })
}

/// The `-c` pairs that arm Codex's hooks on one session, out of [`crate::plugin::codex_handlers`].
///
/// On the argument for the reason Claude Code's are: nothing is written, so nothing is left
/// behind. Each value is TOML, because that is how Codex parses a `-c` value — and it is
/// SERIALISED rather than formatted, since a value that fails to parse is not an error to
/// Codex but a literal string, which it then rejects as the wrong type and refuses to start.
///
/// The command names the binary by its absolute path: Codex has no plugin root and no
/// variable of charter's to expand, and a Codex hook runs through a shell just the same —
/// measured, a single-quoted argument holding spaces arrived as one word.
fn codex_session_flags(binary: &std::path::Path) -> Vec<String> {
    crate::plugin::grouped(crate::plugin::codex_handlers())
        .into_iter()
        .flat_map(|(event, groups)| {
            let groups: Vec<toml::Value> = groups
                .into_iter()
                .map(|(matcher, hooks)| {
                    let hooks: Vec<toml::Value> = hooks
                        .iter()
                        .map(|hook| {
                            let table: toml::Table = [
                                ("type".to_owned(), toml::Value::from("command")),
                                (
                                    "command".to_owned(),
                                    toml::Value::from(crate::plugin::command_at(binary, hook.name)),
                                ),
                                // Codex's own default is 600 seconds (its review screen says so).
                                (
                                    "timeout".to_owned(),
                                    toml::Value::from(i64::from(hook.timeout)),
                                ),
                            ]
                            .into_iter()
                            .collect();
                            toml::Value::Table(table)
                        })
                        .collect();
                    let mut group = toml::Table::new();
                    if let Some(matcher) = matcher {
                        group.insert("matcher".to_owned(), toml::Value::from(matcher));
                    }
                    group.insert("hooks".to_owned(), toml::Value::Array(hooks));
                    toml::Value::Table(group)
                })
                .collect();
            let value = toml::Value::Array(groups);
            ["-c".to_owned(), format!("hooks.{event}={value}")]
        })
        .collect()
}

fn words<const N: usize>(argv: [&str; N]) -> Vec<String> {
    argv.map(str::to_owned).to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

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
        // A shell is the app's own default program: "no harness" rather than a guess.
        assert_eq!(Harness::of_command("/bin/zsh"), None);
        assert_eq!(Harness::of_command("opencode-something"), None);
    }

    #[test]
    fn opencode_is_recognised_by_the_name_its_command_is_invoked_under() {
        assert_eq!(
            Harness::of_command("/Users/o/.opencode/bin/opencode"),
            Some(Harness::Opencode)
        );
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

    /// The app's kit, with the plugin at `/app/plugin` and the binary at `binary`.
    fn kit(binary: &str) -> Kit<'_> {
        Kit {
            binary: std::path::Path::new(binary),
            plugin: Some(std::path::Path::new("/app/plugin")),
        }
    }

    /// Claude Code's arguments and environment for a chat in `cwd`.
    fn claude(binary: &str, cwd: &std::path::Path) -> (Vec<String>, Vec<(String, String)>) {
        claude_with(binary, cwd, &BTreeMap::new())
    }

    /// The same, for a project that chose `plugins`.
    fn claude_with(
        binary: &str,
        cwd: &std::path::Path,
        plugins: &BTreeMap<String, bool>,
    ) -> (Vec<String>, Vec<(String, String)>) {
        match Harness::ClaudeCode.state_hooks(kit(binary), Some(cwd), plugins) {
            StateHooks::ThisSessionOnly {
                args,
                env,
                cannot_report,
            } => {
                assert!(cannot_report.is_empty(), "{cannot_report:?}");
                (args, env)
            }
            StateHooks::None => panic!("Claude Code is armed per session"),
        }
    }

    #[test]
    fn a_claude_code_chat_loads_the_bundled_plugin_for_this_session_alone() {
        // `--plugin-dir` loads a plugin for one session with no marketplace and no install
        // record (measured on 2.1.280), and its hooks name the binary by the variable handed
        // over here — so the plugin's `hooks.json` is the one place the hooks are declared.
        let empty = tempfile::tempdir().expect("a directory with no settings in it");
        let (args, env) = claude("/usr/local/bin/charter", empty.path());

        assert_eq!(args[0], "--plugin-dir");
        assert_eq!(args[1], "/app/plugin");
        assert_eq!(args[2], "--settings");
        assert_eq!(
            env,
            [(
                "CHARTER_HOOK_BINARY".to_owned(),
                "/usr/local/bin/charter".to_owned()
            )]
        );
    }

    #[test]
    fn a_claude_code_chat_turns_the_python_charters_plugin_off_and_its_own_on() {
        // The operator's ruling: app chats disable it. A session `enabledPlugins` wins over a
        // project file that enables it (measured on 2.1.280), and only for this session. And
        // the bundled plugin is pinned on, because a project file a chat can write could
        // otherwise turn the guard off by the plugin's id — measured, and this wins over it.
        let empty = tempfile::tempdir().expect("a directory");
        let (args, _) = claude("/bin/charter", empty.path());
        let settings: serde_json::Value = serde_json::from_str(&args[3]).expect("JSON");

        assert_eq!(
            settings["enabledPlugins"],
            serde_json::json!({
                "charter@charter": false,
                "charter@inline": true,
                "charter-app@inline": false,
            })
        );
    }

    #[test]
    fn a_claude_code_chat_is_handed_the_projects_plugins_and_the_pins_win() {
        // charter-app#274: the set a project chose rides in the same `enabledPlugins` as the two
        // values every chat has always carried, and those two are written last, so a set that
        // somehow holds the opposite cannot move them.
        let empty = tempfile::tempdir().expect("a directory");
        let chosen = BTreeMap::from([
            ("figma@claude-plugins-official".to_owned(), false),
            ("serena@claude-plugins-official".to_owned(), true),
            ("charter@inline".to_owned(), false),
            ("charter@charter".to_owned(), true),
            ("charter-app@inline".to_owned(), true),
        ]);
        let (args, _) = claude_with("/bin/charter", empty.path(), &chosen);
        let settings: serde_json::Value = serde_json::from_str(&args[3]).expect("JSON");

        assert_eq!(
            settings["enabledPlugins"],
            serde_json::json!({
                "charter@inline": true,
                "charter@charter": false,
                "charter-app@inline": false,
                "figma@claude-plugins-official": false,
                "serena@claude-plugins-official": true,
            })
        );
    }

    #[test]
    fn a_codex_chat_is_handed_no_plugin_flag_whatever_the_project_chose() {
        // Codex 0.147.0 ignores a plugin's `enabled` given with `-c` (measured, charter-app#274),
        // so nothing is added: its adapter says "not supported yet" instead.
        let chosen = BTreeMap::from([("charter@charter".to_owned(), false)]);
        let StateHooks::ThisSessionOnly { args, .. } =
            Harness::Codex.state_hooks(kit("/bin/charter"), None, &chosen)
        else {
            panic!("armed per session");
        };
        assert!(args.iter().all(|arg| !arg.contains("plugins")), "{args:?}");
    }

    #[test]
    fn the_settings_arm_no_hook_so_no_hook_is_armed_twice() {
        // Every hook is the plugin's. A hook in `--settings` as well would fire beside it and
        // report every event twice.
        let empty = tempfile::tempdir().expect("a directory");
        let (args, _) = claude("/bin/charter", empty.path());
        let settings: serde_json::Value = serde_json::from_str(&args[3]).expect("JSON");

        assert!(settings.get("hooks").is_none(), "{settings}");
    }

    #[test]
    fn without_the_bundled_plugin_a_claude_code_chat_is_armed_with_nothing() {
        // Nothing is half-armed from a second place: the chat reads `unknown`.
        let hooks = Harness::ClaudeCode.state_hooks(
            Kit {
                binary: std::path::Path::new("/bin/charter"),
                plugin: None,
            },
            None,
            &BTreeMap::new(),
        );
        assert_eq!(hooks, StateHooks::None);
    }

    #[test]
    fn a_claude_code_chat_runs_charter_statusline_so_its_turns_are_recorded() {
        // Claude Code hands the context and cache numbers to its `statusLine` command and to
        // nothing else, so a chat whose settings name none records nothing and the app's
        // gauge has nothing to draw. Quoted as a hook command is, because Claude Code runs it
        // through `/bin/sh -c` too.
        let empty = tempfile::tempdir().expect("a directory with no settings in it");
        let (args, _) = claude("/home/o'brien/charter", empty.path());
        let settings: serde_json::Value = serde_json::from_str(&args[3]).expect("JSON");

        assert_eq!(
            settings["statusLine"],
            serde_json::json!({
                "type": "command",
                "command": r"'/home/o'\''brien/charter' statusline",
            })
        );
    }

    #[test]
    fn a_chat_whose_directory_already_fills_the_footer_keeps_its_own() {
        // The operator's ruling: charter never replaces a `statusLine` somebody else wrote.
        // The plugin is loaded exactly as before — only the footer key is left off.
        let dir = tempfile::tempdir().expect("a directory");
        std::fs::create_dir_all(dir.path().join(".claude")).expect(".claude");
        std::fs::write(
            dir.path().join(".claude/settings.json"),
            r#"{"statusLine": {"type": "command", "command": "my-own-line"}}"#,
        )
        .expect("their settings");

        let (args, _) = claude("/bin/charter", dir.path());
        let settings: serde_json::Value = serde_json::from_str(&args[3]).expect("JSON");
        assert!(
            settings.get("statusLine").is_none(),
            "charter armed a statusLine over the operator's: {}",
            args[3]
        );
        assert_eq!(args[0], "--plugin-dir");
    }

    #[test]
    fn every_hook_word_is_wired_exactly_once_across_the_settings_and_the_plugin() {
        // One owner per word. The plugin's `hooks.json` owns every charter hook; the session
        // `--settings` owns none. A word wired in both would brief a chat twice and report
        // every state event twice. So the union is collected as (event, matcher, word) and
        // must hold no duplicate — and the settings contribute nothing to it.
        fn entries(doc: &serde_json::Value, out: &mut Vec<(String, String, String)>) {
            let Some(events) = doc.get("hooks").and_then(serde_json::Value::as_object) else {
                return;
            };
            for (event, groups) in events {
                for group in groups.as_array().into_iter().flatten() {
                    let matcher = group["matcher"].as_str().unwrap_or("").to_owned();
                    for hook in group["hooks"].as_array().into_iter().flatten() {
                        let command = hook["command"].as_str().unwrap_or("");
                        let word = command.rsplit(" hook ").next().unwrap_or("").to_owned();
                        out.push((event.clone(), matcher.clone(), word));
                    }
                }
            }
        }
        let empty = tempfile::tempdir().expect("a directory");
        let (args, _) = claude("/bin/charter", empty.path());
        let settings: serde_json::Value = serde_json::from_str(&args[3]).expect("JSON");
        let plugin: serde_json::Value =
            serde_json::from_str(&crate::plugin::hooks_json()).expect("JSON");

        let mut from_settings = Vec::new();
        entries(&settings, &mut from_settings);
        assert!(from_settings.is_empty(), "{from_settings:?}");

        let mut all = from_settings;
        entries(&plugin, &mut all);
        let before = all.len();
        all.sort();
        all.dedup();
        assert_eq!(all.len(), before, "a hook is wired twice");
        assert_eq!(all.len(), crate::hookreg::HANDLERS.len());
    }

    /// Codex's `-c` pairs as (dotted key, parsed TOML value), failing on anything else.
    fn codex_flags(binary: &str) -> Vec<(String, toml::Value)> {
        let StateHooks::ThisSessionOnly { args, env, .. } =
            Harness::Codex.state_hooks(kit(binary), None, &BTreeMap::new())
        else {
            panic!("Codex's hooks are armed per session");
        };
        assert!(env.is_empty(), "Codex's commands carry the path: {env:?}");
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
    fn codex_hooks_are_armed_on_this_session_by_its_own_flag() {
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
                "hooks.PreToolUse",
                "hooks.Stop",
                "hooks.SessionEnd"
            ]
        );
        for (key, value) in &flags {
            let hook = &value[0]["hooks"][0];
            assert_eq!(hook["type"].as_str(), Some("command"), "{key}");
            let word = crate::plugin::codex_handlers()
                .find(|h| format!("hooks.{}", h.event) == *key)
                .expect("from the registry")
                .name;
            assert_eq!(
                hook["command"].as_str(),
                Some(format!("'/usr/local/bin/charter' hook {word}").as_str()),
                "{key} does not run charter's own binary with its word"
            );
            // Codex's default is 600 seconds. A hook that somehow hung must not be able to
            // hold a turn open behind it for ten minutes.
            assert!(
                hook["timeout"].as_integer().is_some_and(|t| t <= 10),
                "{key}"
            );
        }
    }

    #[test]
    fn codex_gets_the_bash_guard_under_its_matcher() {
        // The guard used to reach a Codex chat through the Python charter's Codex plugin. It
        // rides on the session's own flags now, under the matcher the plugin gave it.
        let flags = codex_flags("/bin/charter");
        let (_, guard) = flags
            .iter()
            .find(|(key, _)| key == "hooks.PreToolUse")
            .expect("the guard is armed");
        assert_eq!(guard[0]["matcher"].as_str(), Some("Bash"));
        assert_eq!(
            guard[0]["hooks"][0]["command"].as_str(),
            Some("'/bin/charter' hook pretooluse")
        );
    }

    #[test]
    fn a_codex_chat_says_it_cannot_report_a_question_asked_mid_turn() {
        // Codex has no `Notification`. It fires `PermissionRequest` when it asks for an
        // approval — measured — but that hook decides a permission, and nothing arms it.
        let hooks = Harness::Codex.state_hooks(kit("/bin/charter"), None, &BTreeMap::new());

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
    fn no_permission_hook_is_ever_armed_on_codex() {
        // `PermissionRequest` is where Codex would say it is asking for approval, and it
        // is armed nowhere: a hook there can ALLOW or DENY a permission.
        for (key, _) in codex_flags("/bin/charter") {
            for guarded in ["PostToolUse", "PermissionRequest"] {
                assert!(!key.contains(guarded), "{key} is armed by the app");
            }
        }
    }

    #[test]
    fn a_path_with_a_quote_in_it_cannot_break_out_of_a_codex_hook() {
        // Codex runs a hook's command through a shell too — measured: a single-quoted
        // argument holding spaces arrived as one word. And the TOML around it must survive
        // the quote, which is why the value is serialised and not formatted.
        let flags = codex_flags("/home/o'brien/char\"ter");
        let (_, stop) = flags
            .iter()
            .find(|(key, _)| key == "hooks.Stop")
            .expect("stop");

        assert_eq!(
            stop[0]["hooks"][0]["command"].as_str(),
            Some(r#"'/home/o'\''brien/char"ter' hook stop"#)
        );
    }

    /// An opencode chat's arming, with the bundled shim at `<plugin>/opencode/charter.ts`.
    fn opencode_hooks(binary: &str) -> (tempfile::TempDir, StateHooks) {
        let plugin = tempfile::tempdir().expect("a plugin directory");
        let shim = plugin.path().join(crate::opencode::SHIM_IN_BUNDLE);
        std::fs::create_dir_all(shim.parent().expect("a parent")).expect("opencode/");
        std::fs::write(
            &shim,
            crate::opencode::shim(crate::opencode::Arming::Session),
        )
        .expect("the shim");
        let hooks = Harness::Opencode.state_hooks(
            Kit {
                binary: std::path::Path::new(binary),
                plugin: Some(plugin.path()),
            },
            None,
            &BTreeMap::from([("some-plugin".to_owned(), false)]),
        );
        (plugin, hooks)
    }

    #[test]
    fn an_opencode_chat_loads_the_bundled_shim_for_this_session_alone() {
        // Measured on opencode 1.18.23: `OPENCODE_CONFIG_CONTENT` naming the shim loads it for
        // that process, beside every plugin the operator has, and writes nothing.
        let (plugin, hooks) = opencode_hooks("/usr/local/bin/charter");
        let StateHooks::ThisSessionOnly {
            args,
            env,
            cannot_report,
        } = hooks
        else {
            panic!("opencode is armed per session");
        };
        assert!(args.is_empty(), "nothing on the command line: {args:?}");
        let env: BTreeMap<String, String> = env.into_iter().collect();
        assert_eq!(env["CHARTER_HOOK_BINARY"], "/usr/local/bin/charter");
        assert_eq!(env["OPENCODE_PURE"], "0", "`1` would load no plugin at all");
        let config: serde_json::Value =
            serde_json::from_str(&env["OPENCODE_CONFIG_CONTENT"]).expect("JSON");
        let shim = plugin.path().join("opencode/charter.ts");
        assert_eq!(
            config,
            serde_json::json!({ "plugin": [format!("file://{}", shim.display())] }),
            "the shim, and no plugin choice: opencode cannot turn one plugin off"
        );
        assert_eq!(cannot_report, ["sessionend"]);
    }

    #[test]
    fn without_the_shim_an_opencode_chat_is_armed_with_nothing() {
        let plugin = tempfile::tempdir().expect("a plugin directory with no shim");
        let hooks = Harness::Opencode.state_hooks(
            Kit {
                binary: std::path::Path::new("/bin/charter"),
                plugin: Some(plugin.path()),
            },
            None,
            &BTreeMap::new(),
        );
        assert_eq!(hooks, StateHooks::None);
    }

    #[test]
    fn opencode_resumes_by_its_session_flag_and_is_given_no_id_to_start() {
        let id = SessionId::new("ses_f232c39feffecxEyWLvftyLSYU").expect("opencode's id shape");
        assert!(!Harness::Opencode.chooses_session_id());
        assert!(!Harness::Opencode.reports_its_process());
        assert!(Harness::Opencode.new_session_argv(&id, "ide.7").is_empty());
        assert_eq!(
            Harness::Opencode.resume_argv(&id, "ide.7"),
            Some(vec!["-s".to_owned(), id.to_string()])
        );
        for word in ["-s", "--session", "-c", "--continue", "--session=x"] {
            assert!(
                Harness::Opencode.session_named_in(&[word.to_owned()]),
                "{word}"
            );
        }
        assert!(!Harness::Opencode.session_named_in(&["--prompt".to_owned()]));
    }

    #[test]
    fn opencode_is_the_kind_and_the_command_of_the_same_harness() {
        assert_eq!(Harness::of_kind("opencode"), Some(Harness::Opencode));
        assert_eq!(Harness::Opencode.name(), "opencode");
        let said = Harness::Opencode
            .unreported()
            .expect("it says what it cannot report");
        assert!(
            said.contains("first prompt") && said.contains("quits"),
            "{said}"
        );
    }
}
