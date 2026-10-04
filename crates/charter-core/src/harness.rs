//! Which harness a session is running, and the arguments that start or resume one.
//!
//! Every value here is a reading of one harness version, measured by the Python charter and
//! cited where it is declared (`charter/harness/claude_code.py`, `charter/harness/codex.py`,
//! ADR 0024). Nothing here reads a harness's output: the arguments are decided before the
//! program starts, and a resume is decided from what the app itself recorded.

use std::fmt;

pub mod adapter;
pub mod asked;
pub mod asks;
pub mod claude;
pub mod codex;
pub mod hooked;
pub mod model;
pub mod opencode;
#[cfg(test)]
pub(crate) mod testing;

pub use adapter::HarnessAdapter;
pub use claude::SMART_CLOSE_ALLOW;

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
    /// Every harness charter starts, each named once.
    pub const ALL: [Harness; 3] = [Harness::ClaudeCode, Harness::Codex, Harness::Opencode];

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

    /// The variables of the app's own environment this harness reads for itself, passed to a
    /// chat that runs it on top of [`crate::chatenv::PASSED`]. A trailing `*` is a prefix.
    ///
    /// Each harness's own configuration namespace, and nothing else: a name in it that reads
    /// as a credential is still held back unless the operator lists it
    /// ([`crate::chatenv::inherited`]), and a harness's identity is never passed at all.
    pub fn env_passed(self) -> &'static [&'static str] {
        match self {
            // `CLAUDE_CONFIG_DIR`, `CLAUDE_CODE_*`.
            Self::ClaudeCode => &["CLAUDE_*"],
            // `CODEX_HOME`.
            Self::Codex => &["CODEX_*"],
            // `OPENCODE_CONFIG`, `OPENCODE_CONFIG_DIR`.
            Self::Opencode => &["OPENCODE_*"],
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

    /// Whether this harness finds a conversation by the directory it ran in, so a chat whose
    /// directory moves can no longer be resumed by its id (charter#367, D10).
    ///
    /// - **Claude Code** files each transcript under `~/.claude/projects/<encoded cwd>/`, and
    ///   `--resume <id>` looks there, under the directory it is started in. After
    ///   `charter workspace rename` the chat starts in the new directory and its conversation
    ///   stays under the old one. charter does not move the harness's files (ADR 0050).
    /// - **Codex** keeps its rollouts by date under `~/.codex/sessions/`, and
    ///   `codex resume <id>` finds one by its id from any directory.
    /// - **opencode** keys a session by its project, which is the repository's first commit
    ///   (or `global` outside git), not by a path, and `-s <id>` names the session itself.
    pub fn keeps_conversations_by_directory(self) -> bool {
        match self {
            Self::ClaudeCode => true,
            Self::Codex | Self::Opencode => false,
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
/// When a chat just opened on a harness can have a curation prompt typed into it
/// ([`Harness::ready_to_type`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadyToType {
    /// On the first `SessionStart` hook report that began a session, once the terminal is raw.
    WhenItReportsItsStart,
    /// Once the terminal is raw and the harness has then written nothing for a quiet period.
    WhenRawAndQuiet,
}

/// The biggest paste a harness draws whole in its input ([`Harness::longest_paste_drawn_whole`]):
/// at most `lines` lines and at most `chars` characters. `usize::MAX` is no limit measured.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DrawnWhole {
    pub lines: usize,
    pub chars: usize,
}

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
        sandbox: Option<&crate::sandbox::Applied>,
    ) -> StateHooks {
        // The adapter refuses a sandbox compiled for another harness ([`HarnessAdapter::arm`]).
        self.adapter().arm(kit, cwd, plugins, sandbox)
    }

    /// Why a chat of this harness started with `command` and `env` would run without charter's
    /// hooks — `None` when it would not. Only opencode has such a switch charter can see
    /// ([`crate::opencode::disarmed_by`]): Claude Code loads `--plugin-dir` whatever else it is
    /// told, and Codex's session flags are charter's own.
    pub fn disarmed_by(self, command: &[String], env: &[(String, String)]) -> Option<String> {
        self.adapter().disarmed_by(command, env)
    }

    /// What the app arms a chat of this harness with, as `charter doctor` says it.
    pub fn armed_with(self) -> String {
        self.adapter().armed_with()
    }

    /// The arguments that start this harness's program as an ACP agent, for a chat at level 3
    /// over [`crate::acp`], or `None` where charter does not run it over ACP yet. A fact, as
    /// the `[levels] acp` argv of its declaration will be once FD-14 moves it there.
    ///
    /// opencode's ACP agent is opencode itself, `opencode acp` (ADR 0080 §3, measured on
    /// 1.18.33: protocol version 1, `loadSession`, and a session run with `fs` and `terminal`
    /// both off). Codex and Claude Code reach ACP through ACP adapter programs, which HP-3 and
    /// HP-4 decide.
    pub fn acp_args(self) -> Option<&'static [&'static str]> {
        match self {
            Self::Opencode => Some(&["acp"]),
            Self::ClaudeCode | Self::Codex => None,
        }
    }

    /// The adapter that arms this harness at level 2 (ADR 0073 §3): the enum is their
    /// registry, and every harness has one.
    pub fn adapter(self) -> &'static dyn HarnessAdapter {
        match self {
            Self::ClaudeCode => &claude::ADAPTER,
            Self::Codex => &codex::ADAPTER,
            Self::Opencode => &opencode::ADAPTER,
        }
    }

    /// How a chat on this harness comes to know charter's skills, for that chat alone and with
    /// nothing written (ADR 0063). [`Self::state_hooks`] is where each route is taken.
    ///
    /// - **Claude Code** loads the bundled plugin with `--plugin-dir`, and a plugin's `skills/`
    ///   comes with it.
    /// - **opencode** discovers every `SKILL.md` under the paths in its config's `skills.paths`
    ///   (`packages/opencode/src/skill/index.ts`, v1.18.32), and a plugin's `config` hook is
    ///   handed "the live merged config" to change. The shim appends the bundle's directory
    ///   there, measured on 1.18.32: `opencode debug skill` listed charter's skills beside a
    ///   path the operator's own config named.
    /// - **Codex** has no such switch. codex-cli 0.147.0 finds skills only under
    ///   `$CODEX_HOME/skills`, `~/.agents/skills`, a trusted project's `.codex/skills`, the
    ///   `.agents/skills` folders between the project root and the directory it runs in,
    ///   `/etc/codex/skills`, and installed plugins (`codex-rs/ext/skills/src/host_roots.rs` at
    ///   `rust-v0.147.0`). `skills.config` only turns a skill it found on or off. Each root is
    ///   the operator's home, their tree or Codex's plugin cache, which ADR 0050 keeps charter
    ///   from writing, so a Codex chat is briefed on the skills instead.
    pub fn skills(self) -> crate::skills::Route {
        match self {
            Self::ClaudeCode => crate::skills::Route::Plugin,
            Self::Opencode => crate::skills::Route::Config,
            Self::Codex => crate::skills::Route::Briefing,
        }
    }

    /// What a chat's pane sends for Shift+Enter: the bytes this harness reads as "a new line
    /// in what I am typing", rather than "send it" (SI-4).
    ///
    /// A terminal has no Shift+Enter of its own — xterm.js 6.0.0 sends a bare CR for it, the
    /// same as Enter, so every harness submitted on it. ESC CR is Alt/Option+Enter, the
    /// sequence each of these harnesses already reads as a newline. Measured, not assumed: in a
    /// pseudo-terminal, `hello`, ESC CR, `world` left both words in the input on two lines,
    /// unsent, on Claude Code 2.1.283, codex-cli 0.147.0 and opencode 1.18.23.
    ///
    /// A harness's and not the pane's, because it is a fact about the program reading the
    /// keys; a chat that runs no harness — a shell — keeps the terminal's own Enter.
    pub fn newline(self) -> &'static str {
        match self {
            Self::ClaudeCode | Self::Codex | Self::Opencode => "\x1b\r",
        }
    }

    /// Whether this harness reports `SessionStart` when it starts, before anyone has typed a
    /// thing — the moment charter types a curation action's prompt into a Claude Code chat it
    /// has just opened (ADR 0061), knowing from a hook rather than from the harness's output
    /// that the program is there to read it.
    ///
    /// Measured, not assumed: Claude Code fires it at launch. Codex fires it inside the FIRST
    /// TURN (codex-cli 0.147.0, see [`Harness::unreported`]), and opencode's shim asks for it at
    /// the first prompt because opencode names no hook for a session's creation (ADR 0058). A
    /// prompt typed on either one's `SessionStart` would land after the operator had already
    /// sent something, so that report never types one ([`Harness::ready_to_type`] says what
    /// does).
    pub fn reports_its_start_before_the_first_prompt(self) -> bool {
        match self {
            Self::ClaudeCode => true,
            Self::Codex | Self::Opencode => false,
        }
    }

    /// How charter tells that a chat it has just opened on this harness is ready to have a
    /// curation prompt typed into it (ADR 0061), or `None` where it has no way to tell. Every
    /// harness is named, so one added later is given an answer on purpose, once measured.
    ///
    /// Claude Code reports `SessionStart` at launch, so it is typed into on that hook. Codex
    /// says nothing until the first prompt, so it is typed into once its terminal has gone raw
    /// and it has then written nothing for a while — the kernel's line discipline and the
    /// moment bytes last arrived, never what it drew. Measured on codex-cli 0.147.0: it took a
    /// paste at any moment after going raw, even mid-draw.
    ///
    /// opencode has neither. Measured on 1.18.32, it draws its input box, goes raw, and then
    /// boots silently before that box takes a key: 0.83 s idle, 1.4 s with every core busy,
    /// more than 8 s on a machine at load 50 — and a paste in that silence vanished. No quiet
    /// period long enough to cover it is short enough to wait for (ADR 0061, amended
    /// 2026-09-27).
    pub fn ready_to_type(self) -> Option<ReadyToType> {
        match self {
            Self::ClaudeCode => Some(ReadyToType::WhenItReportsItsStart),
            Self::Codex => Some(ReadyToType::WhenRawAndQuiet),
            Self::Opencode => None,
        }
    }

    /// The biggest paste this harness draws whole in its input: a paste with more lines or
    /// more characters is drawn as a placeholder the operator cannot read. A curation prompt is
    /// typed to be read before it is sent (ADR 0061, and its amendment of 2026-09-27), so one
    /// that would be drawn as a placeholder is not typed.
    ///
    /// Measured, each with no setting a chat can be started with to stop it:
    ///
    /// - Claude Code 2.1.283: `[Pasted text #N +M lines]` over 800 characters or at 4 lines or
    ///   more. A line feed counts among the characters, a trailing one starts a line, and a
    ///   character is a character, not a byte.
    /// - Codex 0.147.0: `[Pasted Content N chars]` over 1,000 characters, line feeds among
    ///   them, however many lines (200 were drawn whole).
    /// - opencode 1.18.32: `[Pasted ~N lines]` at 3 lines or more, or over 150 characters.
    ///   opencode is never typed into ([`Harness::ready_to_type`]); its limit is here so that a
    ///   harness is judged the moment it can be.
    pub fn longest_paste_drawn_whole(self) -> DrawnWhole {
        match self {
            Self::ClaudeCode => DrawnWhole {
                lines: 3,
                chars: 800,
            },
            Self::Codex => DrawnWhole {
                lines: usize::MAX,
                chars: 1000,
            },
            Self::Opencode => DrawnWhole {
                lines: 2,
                chars: 150,
            },
        }
    }

    /// Why `pasted`, handed to this harness as one paste, would be drawn as a placeholder in
    /// its input — or `None` when it is drawn whole. `pasted` is the text inside the paste
    /// (`curation::pasted`), and this is the one place that judges it, for `curate`'s refusal
    /// and `charter persona lint`'s warning alike.
    pub fn why_drawn_as_a_placeholder(self, pasted: &str) -> Option<String> {
        let most = self.longest_paste_drawn_whole();
        let lines = pasted.split('\n').count();
        let chars = pasted.chars().count();
        let title = self.title();
        if lines > most.lines {
            return Some(format!(
                "{title} draws a paste of more than {} lines as a placeholder, and this one is \
                 {lines} lines",
                most.lines
            ));
        }
        (chars > most.chars).then(|| {
            format!(
                "{title} draws a paste over {} characters as a placeholder, and this one is \
                 {chars}",
                most.chars
            )
        })
    }

    /// `reference` in this harness's own syntax ([`HarnessAdapter::reference`]).
    pub fn reference(self, reference: &crate::reference::Reference) -> String {
        self.adapter().reference(reference)
    }

    /// What the operator calls this harness: `Claude Code`, `Codex`, `opencode`.
    pub fn title(self) -> &'static str {
        self.adapter().plugins().title()
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
                 for your approval, and a sub-agent reads working until the chat ends.",
            ),
            Self::Opencode => Some(
                "opencode says nothing until your first prompt, and nothing when it quits; once \
                 you answer its permission prompt, the chat reads waiting until the turn ends, \
                 a session you open inside it with /new is not followed, and its sub-agents \
                 are not shown under it.",
            ),
        }
    }
}

fn words<const N: usize>(argv: [&str; N]) -> Vec<String> {
    argv.map(str::to_owned).to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn opencode_offers_level_3_through_its_own_acp_mode_and_the_others_wait_for_their_tickets() {
        // ADR 0080 §3: opencode's ACP agent is the same program (HP-2, first); Codex's and
        // Claude Code's are ACP adapter programs HP-3 and HP-4 decide.
        assert_eq!(Harness::Opencode.acp_args(), Some(&["acp"][..]));
        assert_eq!(Harness::Codex.acp_args(), None);
        assert_eq!(Harness::ClaudeCode.acp_args(), None);
    }

    #[test]
    fn a_sandbox_compiled_for_one_harness_arms_no_chat_of_any_other() {
        // ADR 0067, fail closed, for every pair: a chat handed another harness's sandbox
        // would start without its own.
        let plugin = testing::bundled_plugin();
        let kit = Kit {
            binary: std::path::Path::new("/bin/charter"),
            plugin: Some(&plugin),
        };
        let mut pairs = 0;
        for from in Harness::ALL {
            let (plane, applied) = testing::sandbox_compiled_for(from);
            let applied = applied.unwrap_or_else(|refused| panic!("{from:?}: {refused}"));
            for to in Harness::ALL.into_iter().filter(|to| *to != from) {
                assert_eq!(
                    to.adapter()
                        .arm(kit, Some(plane.path()), &BTreeMap::new(), Some(&applied)),
                    StateHooks::None,
                    "{from:?}'s sandbox armed a {to:?} chat"
                );
                let words = crate::sandbox::Words {
                    program: "harness".to_owned(),
                    command: Vec::new(),
                    armed: Vec::new(),
                    charters: Vec::new(),
                };
                assert!(
                    to.adapter()
                        .sandboxed_line(applied.form(), words, &crate::sandbox::At::default())
                        .is_err(),
                    "{from:?}'s sandbox gave a {to:?} chat a line"
                );
                pairs += 1;
            }
        }
        assert_eq!(pairs, 6, "each harness's sandbox, to the two others");
    }

    #[test]
    fn a_codex_chat_says_its_sub_agents_read_working_until_it_ends() {
        // FD-18: Codex is armed with no `SubagentStop` (`plugin::CODEX`, #1086), so a child is
        // never heard to finish, and the chat says so rather than showing it working for good.
        assert!(!crate::plugin::CODEX.contains(&"subagentstop"));
        assert!(
            Harness::Codex
                .unreported()
                .is_some_and(|said| said.contains("sub-agent reads working until")),
            "{:?}",
            Harness::Codex.unreported()
        );
    }

    #[test]
    fn an_opencode_chat_says_its_sub_agents_are_not_shown_under_it() {
        // FD-18: a child agent is told by the payload's `agent_id`, read only where it was
        // measured (`handoffguard::MEASURED_SUBAGENT_HARNESSES`: Claude Code and Codex).
        // opencode's payload has none, so its chat says so rather than looking childless.
        assert!(
            !crate::handoffguard::MEASURED_SUBAGENT_HARNESSES.contains(&"opencode"),
            "opencode was measured: drop this sentence"
        );
        assert!(
            Harness::Opencode
                .unreported()
                .is_some_and(|said| said.contains("sub-agents are not shown under it")),
            "{:?}",
            Harness::Opencode.unreported()
        );
    }

    #[test]
    fn every_harness_is_armed_by_the_adapter_the_registry_names() {
        for harness in Harness::ALL {
            assert_eq!(harness.adapter().harness(), harness);
        }
    }

    #[test]
    fn every_harness_takes_escape_return_as_a_newline_in_its_input() {
        // Measured live in a pseudo-terminal: `hello`, ESC CR, `world` left both words in the
        // input on two lines, unsent, on Claude Code 2.1.283, codex-cli 0.147.0 and opencode
        // 1.18.23 (SI-4).
        for harness in [Harness::ClaudeCode, Harness::Codex, Harness::Opencode] {
            assert_eq!(harness.newline(), "\x1b\r", "{harness:?}");
        }
    }

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
    fn only_claude_code_loses_a_conversation_when_its_directory_moves() {
        // charter#367, D10: Claude Code files a transcript under the directory it ran in;
        // Codex and opencode find a session by its id from anywhere.
        assert!(Harness::ClaudeCode.keeps_conversations_by_directory());
        assert!(!Harness::Codex.keeps_conversations_by_directory());
        assert!(!Harness::Opencode.keeps_conversations_by_directory());
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
        match Harness::ClaudeCode.state_hooks(kit(binary), Some(cwd), plugins, None) {
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
        assert_eq!(args[2], "--mcp-config");
        assert_eq!(args[4], "--settings");
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
        let settings: serde_json::Value = serde_json::from_str(settings_of(&args)).expect("JSON");

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
        let settings: serde_json::Value = serde_json::from_str(settings_of(&args)).expect("JSON");

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
            Harness::Codex.state_hooks(kit("/bin/charter"), None, &chosen, None)
        else {
            panic!("armed per session");
        };
        assert!(args.iter().all(|arg| !arg.contains("plugins")), "{args:?}");
    }

    #[test]
    fn a_claude_code_chat_may_run_charter_session_record_and_read_charter_without_asking() {
        // SI-8e, the operator's ruling: a Smart close never stops on a permission prompt for
        // the one command that ends it. Amended by V79 (#1050): the five read-only tools of
        // charter's own MCP server are pre-allowed beside it. Only grants, for exactly these —
        // no `ask`, no `deny`, no mode — so every rule of the operator's and the project's
        // still stands beside them (measured on 2.1.283: `--settings` permissions merge with
        // them, and a compound command holding the record command is still asked about).
        let empty = tempfile::tempdir().expect("a directory");
        let (args, _) = claude("/bin/charter", empty.path());
        let settings: serde_json::Value = serde_json::from_str(settings_of(&args)).expect("JSON");

        assert_eq!(
            settings["permissions"],
            serde_json::json!({"allow": [
                "Bash(charter session record *)",
                "mcp__charter__todo_list",
                "mcp__charter__memory_search",
                "mcp__charter__session_record_list",
                "mcp__charter__session_record_read",
                "mcp__charter__change_status",
            ]})
        );
    }

    #[test]
    fn a_claude_code_chat_is_still_asked_before_every_other_charter_tool() {
        // V79: the writes and `ask_operator` keep Claude Code's prompt. Every tool the server
        // offers that is not one of the five reads has no allow, whatever it is marked.
        let empty = tempfile::tempdir().expect("a directory");
        let (args, _) = claude("/bin/charter", empty.path());
        let settings: serde_json::Value = serde_json::from_str(settings_of(&args)).expect("JSON");
        let allowed = settings["permissions"]["allow"]
            .as_array()
            .expect("a list")
            .clone();
        let reads = [
            "todo_list",
            "memory_search",
            "session_record_list",
            "session_record_read",
            "change_status",
        ];
        let others: Vec<&str> = crate::chattools::TOOLS
            .iter()
            .map(|tool| tool.name)
            .filter(|name| !reads.contains(name))
            .collect();
        assert!(others.contains(&"ask_operator"), "{others:?}");
        assert!(others.contains(&"todo_add"), "{others:?}");
        for name in others {
            let rule = format!("mcp__charter__{name}");
            assert!(
                !allowed
                    .iter()
                    .any(|allow| allow == &serde_json::json!(rule)),
                "{rule} is pre-allowed"
            );
        }
        // Nor the server as a whole, which would allow every one of its tools.
        for allow in &allowed {
            let allow = allow.as_str().expect("a rule");
            assert!(
                allow != "mcp__charter" && !allow.starts_with("mcp__charter__*"),
                "{allow}"
            );
        }
    }

    #[test]
    fn an_unsandboxed_codex_chat_is_handed_no_approval_or_sandbox_setting() {
        // Codex has no per-session rule for one command: its approval policy and its sandbox
        // are whole-session switches, and loosening either would be far broader than the one
        // command Smart close needs (SI-8e, ADR 0064's measurements). Where the plane has not
        // turned the sandbox on, Codex keeps its own settings, as it always has.
        let StateHooks::ThisSessionOnly { args, .. } =
            Harness::Codex.state_hooks(kit("/bin/charter"), None, &BTreeMap::new(), None)
        else {
            panic!("armed per session");
        };
        for arg in &args {
            for setting in ["approval", "sandbox", "network", "rules", "permissions"] {
                assert!(!arg.contains(setting), "a Codex chat is handed {setting}");
            }
        }
    }

    /// The whole line of a Codex chat in a sandboxed plane — its hooks, then `charters` — and
    /// the flags its sandbox was compiled to.
    fn sandboxed_codex(charters: &[&str]) -> (Vec<String>, Vec<String>) {
        let (plane, applied) = testing::sandbox_compiled_for(Harness::Codex);
        let applied = applied.expect("starts");
        let crate::sandbox::Form::Codex(compiled) = applied.form() else {
            panic!("compiled for Codex");
        };
        let StateHooks::ThisSessionOnly { args, .. } = Harness::Codex.state_hooks(
            kit("/bin/charter"),
            Some(plane.path()),
            &BTreeMap::new(),
            Some(&applied),
        ) else {
            panic!("armed per session");
        };
        let charters = charters.iter().map(|word| (*word).to_owned()).collect();
        let line = applied
            .line(
                crate::sandbox::Words {
                    program: "codex".to_owned(),
                    command: Vec::new(),
                    armed: args,
                    charters,
                },
                &crate::sandbox::At {
                    cwd: Some(plane.path()),
                    ..crate::sandbox::At::default()
                },
            )
            .expect("starts")
            .args;
        let sandbox = [
            compiled.args.clone(),
            crate::sandbox::codex::later_code(compiled, plane.path()),
        ]
        .concat();
        (line, sandbox)
    }

    #[test]
    fn a_sandboxed_codex_chat_is_handed_its_sandbox_after_its_hooks() {
        let (line, compiled) = sandboxed_codex(&["resume", "0199"]);
        let at = line
            .windows(compiled.len())
            .position(|run| run == compiled.as_slice())
            .unwrap_or_else(|| panic!("{line:?}"));
        assert!(
            line[..at].iter().any(|arg| arg.starts_with("hooks.")),
            "{line:?}"
        );
        assert_eq!(line[at + compiled.len()..], ["resume", "0199"]);
    }

    #[test]
    fn a_sandboxed_codex_chat_is_only_ever_tightened_never_loosened() {
        // ADR 0067: charter may tighten Codex's sandbox and may never loosen it.
        let (args, _) = sandboxed_codex(&[]);
        for flag in [
            "-s",
            "--sandbox",
            "--dangerously-bypass-approvals-and-sandbox",
            "--yolo",
            "--add-dir",
            "-a",
            "--ask-for-approval",
            "--approve-for-me",
            "--search",
        ] {
            assert!(!args.iter().any(|arg| arg == flag), "handed {flag}");
        }
        assert!(
            args.windows(2)
                .all(|pair| pair[0] != "--enable" || pair[1] == "network_proxy"),
            "{args:?}"
        );
        // Only features turned off, never the proxy.
        assert!(
            args.windows(2)
                .all(|pair| pair[0] != "--disable" || pair[1] != "network_proxy"),
            "{args:?}"
        );
        for loosened in [
            "danger-full-access",
            "sandbox_mode",
            "network_access",
            "writable_roots",
            "dangerously",
            "sandbox_approval=true",
            "request_permissions=true",
            "auto_review",
            "guardian",
            "\"live\"",
            "\"cached\"",
            "\"write\"",
            "\"full\"",
            "allow_local_binding=true",
        ] {
            let joined = args.join(" ").replace(' ', "");
            let loosened = loosened.replace(' ', "");
            assert!(!joined.contains(&loosened), "handed {loosened}: {args:?}");
        }
    }

    #[test]
    fn a_sandboxed_claude_code_chat_carries_its_sandbox_in_the_same_settings() {
        // One `--settings`: the sandbox and its deny rules beside the Smart close allow, which
        // stays, because a deny outranks an allow and the two name different things.
        let (plane, applied) = testing::sandbox_compiled_for(Harness::ClaudeCode);
        let applied = applied.expect("starts");
        let crate::sandbox::Form::ClaudeCode(compiled) = applied.form() else {
            panic!("compiled for Claude Code");
        };
        let StateHooks::ThisSessionOnly { args, .. } = Harness::ClaudeCode.state_hooks(
            kit("/bin/charter"),
            Some(plane.path()),
            &BTreeMap::new(),
            Some(&applied),
        ) else {
            panic!("armed per session");
        };
        let settings: serde_json::Value = serde_json::from_str(settings_of(&args)).expect("JSON");

        assert_eq!(settings["sandbox"], compiled.sandbox);
        assert_eq!(
            settings["permissions"]["allow"],
            serde_json::json!([
                "Bash(charter session record *)",
                "mcp__charter__todo_list",
                "mcp__charter__memory_search",
                "mcp__charter__session_record_list",
                "mcp__charter__session_record_read",
                "mcp__charter__change_status",
            ])
        );
        assert_eq!(
            settings["permissions"]["deny"],
            serde_json::json!(compiled.deny)
        );
    }

    #[test]
    fn an_unsandboxed_claude_code_chat_carries_no_sandbox_key_at_all() {
        // Absent, not `enabled: false`: the operator's own settings decide, as before.
        let empty = tempfile::tempdir().expect("a directory");
        let (args, _) = claude("/bin/charter", empty.path());
        let settings: serde_json::Value = serde_json::from_str(settings_of(&args)).expect("JSON");

        assert!(settings.get("sandbox").is_none(), "{settings}");
        assert!(settings["permissions"].get("deny").is_none(), "{settings}");
    }

    #[test]
    fn the_settings_arm_only_the_permission_hook_so_no_hook_is_armed_twice() {
        // Every hook that reports or guards is the plugin's: one in `--settings` as well would
        // fire beside it and report every event twice. The one the settings own is the
        // permission hook (HP-6), which can ALLOW and so is never in a file a chat can write
        // (`plugin::tests`): it rides on the argument, quoted as the status line is.
        let empty = tempfile::tempdir().expect("a directory");
        let (args, _) = claude("/home/o'brien/charter", empty.path());
        let settings: serde_json::Value = serde_json::from_str(settings_of(&args)).expect("JSON");

        assert_eq!(
            settings["hooks"],
            serde_json::json!({"PermissionRequest": [{"hooks": [{
                "type": "command",
                "command": r"'/home/o'\''brien/charter' hook permissionrequest",
                "timeout": 60,
            }]}]})
        );
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
            None,
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
        let settings: serde_json::Value = serde_json::from_str(settings_of(&args)).expect("JSON");

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
        let settings: serde_json::Value = serde_json::from_str(settings_of(&args)).expect("JSON");
        assert!(
            settings.get("statusLine").is_none(),
            "charter armed a statusLine over the operator's: {}",
            settings_of(&args)
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
        let settings: serde_json::Value = serde_json::from_str(settings_of(&args)).expect("JSON");
        let plugin: serde_json::Value =
            serde_json::from_str(&crate::plugin::hooks_json()).expect("JSON");

        let mut from_settings = Vec::new();
        entries(&settings, &mut from_settings);
        // The permission hook alone (HP-6), which the plugin never carries.
        assert_eq!(
            from_settings,
            [(
                "PermissionRequest".to_owned(),
                String::new(),
                crate::harness::hooked::WORD.to_owned()
            )]
        );

        let mut all = from_settings;
        entries(&plugin, &mut all);
        let before = all.len();
        all.sort();
        all.dedup();
        assert_eq!(all.len(), before, "a hook is wired twice");
        assert_eq!(all.len(), crate::hookreg::HANDLERS.len() + 1);
    }

    /// Codex's `-c` pairs as (dotted key, parsed TOML value), failing on anything else.
    /// The `--settings` a Claude Code chat is armed with: the word after the flag.
    fn settings_of(args: &[String]) -> &str {
        let at = args
            .iter()
            .position(|arg| arg == "--settings")
            .expect("--settings");
        &args[at + 1]
    }

    /// The `-c` pairs that arm Codex's hooks, without the one that hands it charter's MCP
    /// server (HP-7, which `codex::tests` holds).
    fn codex_flags(binary: &str) -> Vec<(String, toml::Value)> {
        let StateHooks::ThisSessionOnly { args, env, .. } =
            Harness::Codex.state_hooks(kit(binary), None, &BTreeMap::new(), None)
        else {
            panic!("Codex's hooks are armed per session");
        };
        assert!(
            env.iter()
                .all(|(name, _)| name != crate::plugin::BINARY_ENV),
            "Codex's commands carry the path: {env:?}"
        );
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
            .filter(|(key, _)| !key.starts_with("mcp_servers."))
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
        let hooks = Harness::Codex.state_hooks(kit("/bin/charter"), None, &BTreeMap::new(), None);

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
    fn only_claude_code_reports_its_start_before_anyone_types() {
        // Codex and opencode report `SessionStart` at the first prompt, so a curation prompt
        // typed on it would follow what the operator had already sent.
        assert!(Harness::ClaudeCode.reports_its_start_before_the_first_prompt());
        assert!(!Harness::Codex.reports_its_start_before_the_first_prompt());
        assert!(!Harness::Opencode.reports_its_start_before_the_first_prompt());
    }

    #[test]
    fn codex_draws_a_paste_over_a_thousand_characters_as_a_placeholder() {
        // Measured on codex-cli 0.147.0: a paste of 1,000 characters is drawn whole, one of
        // 1,001 as `[Pasted Content 1001 chars]`.
        let codex = Harness::Codex;
        assert_eq!(codex.why_drawn_as_a_placeholder(&"x".repeat(1000)), None);
        let said = codex
            .why_drawn_as_a_placeholder(&"x".repeat(1001))
            .expect("1,001 characters is a placeholder");
        assert!(
            said.contains("Codex") && said.contains("1000") && said.contains("1001"),
            "{said}"
        );
    }

    #[test]
    fn claude_code_draws_a_paste_over_800_characters_or_3_lines_as_a_placeholder() {
        // Measured on Claude Code 2.1.283 (2026-09-27): 800 characters drawn whole, 801 as
        // `[Pasted text #1]`; 3 lines whole, 4 as `[Pasted text #1 +3 lines]`; the line feeds
        // count among the characters, and a character is a character, not a byte.
        let claude = Harness::ClaudeCode;
        assert_eq!(claude.why_drawn_as_a_placeholder(&"x".repeat(800)), None);
        assert_eq!(claude.why_drawn_as_a_placeholder(&"é".repeat(800)), None);
        assert_eq!(claude.why_drawn_as_a_placeholder("ab\nab\nab"), None);
        let three_long = format!(
            "{}\n{}\n{}",
            "x".repeat(266),
            "x".repeat(266),
            "x".repeat(266)
        );
        assert_eq!(claude.why_drawn_as_a_placeholder(&three_long), None);
        let chars = claude
            .why_drawn_as_a_placeholder(&"x".repeat(801))
            .expect("801 characters is a placeholder");
        assert!(
            chars.contains("Claude Code") && chars.contains("800") && chars.contains("801"),
            "{chars}"
        );
        assert!(
            claude
                .why_drawn_as_a_placeholder(&format!("x{three_long}"))
                .is_some(),
            "801 with the line feeds"
        );
        let lines = claude
            .why_drawn_as_a_placeholder("ab\nab\nab\nab")
            .expect("four lines is a placeholder");
        assert!(
            lines.contains("3 lines") && lines.contains("4 lines"),
            "{lines}"
        );
    }

    #[test]
    fn opencode_draws_a_paste_of_three_lines_or_over_150_characters_as_a_placeholder() {
        // Measured on opencode 1.18.32: `[Pasted ~N lines]` at 3 lines or more, or at more than
        // 150 characters.
        let opencode = Harness::Opencode;
        assert_eq!(opencode.why_drawn_as_a_placeholder("one\ntwo"), None);
        assert_eq!(opencode.why_drawn_as_a_placeholder(&"x".repeat(150)), None);
        let lines = opencode
            .why_drawn_as_a_placeholder("one\ntwo\nthree")
            .expect("three lines is a placeholder");
        assert!(
            lines.contains("opencode") && lines.contains("3 lines"),
            "{lines}"
        );
        let chars = opencode
            .why_drawn_as_a_placeholder(&"x".repeat(151))
            .expect("151 characters is a placeholder");
        assert!(chars.contains("150") && chars.contains("151"), "{chars}");
    }

    #[test]
    fn every_harness_names_the_paste_it_draws_whole() {
        // Each one measured, so a curation prompt is judged against every harness it could be
        // typed into; one added later is given an answer on purpose.
        for harness in Harness::ALL {
            let most = harness.longest_paste_drawn_whole();
            assert!(most.lines >= 1 && most.chars >= 1, "{harness:?}: {most:?}");
        }
    }

    #[test]
    fn claude_code_is_typed_into_on_its_start_codex_once_raw_and_quiet_and_opencode_never() {
        assert_eq!(
            Harness::ClaudeCode.ready_to_type(),
            Some(ReadyToType::WhenItReportsItsStart)
        );
        assert_eq!(
            Harness::Codex.ready_to_type(),
            Some(ReadyToType::WhenRawAndQuiet)
        );
        assert_eq!(Harness::Opencode.ready_to_type(), None);
        // The one that reports its start is exactly the one typed into on it.
        for harness in [Harness::ClaudeCode, Harness::Codex, Harness::Opencode] {
            assert_eq!(
                harness.ready_to_type() == Some(ReadyToType::WhenItReportsItsStart),
                harness.reports_its_start_before_the_first_prompt(),
                "{harness:?}"
            );
        }
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
            None,
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
            config["plugin"],
            serde_json::json!([format!("file://{}", shim.display())]),
            "the shim, and no plugin choice: opencode cannot turn one plugin off"
        );
        assert_eq!(cannot_report, ["sessionend"]);
    }

    /// The plugin the app ships, in the repository: its skills, its shim, its hooks.
    fn env_of(hooks: StateHooks) -> BTreeMap<String, String> {
        let StateHooks::ThisSessionOnly { env, .. } = hooks else {
            panic!("armed per session");
        };
        env.into_iter().collect()
    }

    #[test]
    fn each_harness_reaches_charters_skills_by_its_own_route() {
        // ADR 0063: Claude Code loads the plugin's `skills/`, opencode is told the directory,
        // and Codex, with no way to take a skills directory for one session, is briefed.
        assert_eq!(Harness::ClaudeCode.skills(), crate::skills::Route::Plugin);
        assert_eq!(Harness::Opencode.skills(), crate::skills::Route::Config);
        assert_eq!(Harness::Codex.skills(), crate::skills::Route::Briefing);
    }

    #[test]
    fn a_codex_chat_is_started_with_the_skills_its_briefing_lists() {
        let plugin = testing::bundled_plugin();
        let env = env_of(Harness::Codex.state_hooks(
            Kit {
                binary: std::path::Path::new("/bin/charter"),
                plugin: Some(&plugin),
            },
            None,
            &BTreeMap::new(),
            None,
        ));
        assert_eq!(
            env.get(crate::skills::LISTED_ENV),
            Some(&plugin.join("skills").display().to_string())
        );
    }

    #[test]
    fn only_a_harness_that_cannot_load_the_skills_is_briefed_on_them() {
        // Claude Code and opencode discover the skills themselves; a listing on top would
        // tell the model about every skill twice.
        let plugin = testing::bundled_plugin();
        let kit = Kit {
            binary: std::path::Path::new("/bin/charter"),
            plugin: Some(&plugin),
        };
        for harness in [Harness::ClaudeCode, Harness::Opencode] {
            let env = env_of(harness.state_hooks(kit, None, &BTreeMap::new(), None));
            assert!(!env.contains_key(crate::skills::LISTED_ENV), "{harness:?}");
        }
    }

    #[test]
    fn an_opencode_chat_is_told_the_bundled_skills_through_the_shim() {
        let plugin = testing::bundled_plugin();
        let env = env_of(Harness::Opencode.state_hooks(
            Kit {
                binary: std::path::Path::new("/bin/charter"),
                plugin: Some(&plugin),
            },
            None,
            &BTreeMap::new(),
            None,
        ));
        let config: serde_json::Value =
            serde_json::from_str(&env["OPENCODE_CONFIG_CONTENT"]).expect("JSON");
        assert_eq!(
            config["plugin"][0][1]["skills"].as_str(),
            Some(plugin.join("skills").display().to_string().as_str()),
            "{config}"
        );
        assert!(
            config.get("skills").is_none(),
            "a `skills` key here would replace the operator's own paths: {config}"
        );
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
            None,
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
