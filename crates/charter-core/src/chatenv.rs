//! What a chat is started with, of the app's own environment.
//!
//! **A keep-list, not a strip-list.** A chat's program starts from an empty environment plus
//! the variables named here, taken from the app's own when the app has them. The extension
//! executor has started its programs this way from the start ([`crate::executor`]'s
//! `environment`), and a chat now does too: a variable the app happened to inherit — from a
//! terminal, `launchctl setenv`, a login item — is not the chat's unless something here
//! names it.
//!
//! Three sources name one, and nothing else does:
//!
//! 1. [`PASSED`]: what any program needs to run as the operator on this machine — `PATH`,
//!    `HOME`, the locale, the proxies, the XDG directories, charter's own `CHARTER_*`.
//! 2. **The harness's own**, as that harness declares them ([`Harness::env_passed`]): data per
//!    harness, never a list here.
//! 3. **The operator**, per plane, in `charter.local.toml`'s `[chat_env] pass` ([`read`]).
//!
//! **A variable named like a credential is not passed by the first two.** Cloud, forge and
//! model-provider credentials ([`CREDENTIALS`]), and any name holding one of the words a
//! profile may not use ([`crate::profiles::named_like_a_credential`]), pass only when the
//! operator lists them. `charter secret exec` stays the way a command gets a credential.
//!
//! **Some are never passed, whoever lists them**: a harness's identity and where the launcher's
//! chat was ([`crate::hookwire::NOT_INHERITED`]), every `OP_*` and every identity variable a
//! vault declares (the caller's `strip`), and `TERM`, which is always the chat's own.
//!
//! **One is replaced**: an app that started itself without the session bus hands its chats the
//! bus it was given ([`SESSION_BUS_KEPT`]) as `DBUS_SESSION_BUS_ADDRESS`, not the dead one.

use std::ffi::{OsStr, OsString};
use std::path::Path;

use crate::harness::Harness;

/// The table of `charter.local.toml` the operator extends the keep-list in.
pub const TABLE: &str = "chat_env";

/// The key under [`TABLE`]: a list of names, each exact or ending in `*` for a prefix.
pub const KEY: &str = "pass";

/// What every chat is started with, of the app's own environment. A trailing `*` is a prefix.
///
/// What a program needs to run as the operator here, and to reach the network the way the
/// operator's own terminal does. `TERM` is not among them: a chat's is charter's own
/// (`xterm-256color`, [`crate::session`]), whatever started the app.
pub const PASSED: &[&str] = &[
    "PATH",
    "HOME",
    "USER",
    "LOGNAME",
    "SHELL",
    "ZDOTDIR",
    "COLORTERM",
    "LANG",
    "LC_*",
    "TZ",
    "TMPDIR",
    "EDITOR",
    "VISUAL",
    "PAGER",
    "SSH_AUTH_SOCK",
    "XDG_*",
    "DISPLAY",
    "WAYLAND_DISPLAY",
    "DBUS_SESSION_BUS_ADDRESS",
    "__CF_USER_TEXT_ENCODING",
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "ALL_PROXY",
    "NO_PROXY",
    "http_proxy",
    "https_proxy",
    "all_proxy",
    "no_proxy",
    "SSL_CERT_FILE",
    "SSL_CERT_DIR",
    "NODE_EXTRA_CA_CERTS",
    "CHARTER_*",
    // What a Windows program cannot start without, and where it keeps the operator's files.
    "SystemRoot",
    "SystemDrive",
    "windir",
    "ComSpec",
    "PATHEXT",
    "USERPROFILE",
    "USERNAME",
    "HOMEDRIVE",
    "HOMEPATH",
    "APPDATA",
    "LOCALAPPDATA",
    "ProgramData",
    "ProgramFiles",
    "ProgramFiles(x86)",
    "ProgramW6432",
    "CommonProgramFiles",
    "TEMP",
    "TMP",
    "OS",
    "NUMBER_OF_PROCESSORS",
    "PROCESSOR_ARCHITECTURE",
];

/// The session bus the app was given, kept aside when the app started itself again without
/// it (charter-app `portal.rs`, charter-app#24). Empty when it was given none.
///
/// The app points its own `DBUS_SESSION_BUS_ADDRESS` at nothing when the desktop portal is
/// silent, because GTK would otherwise wait 25 s on it. The bus itself is healthy, so a chat
/// gets the address kept here instead of the dead one ([`inherited`]), and this name itself
/// is never a chat's.
pub const SESSION_BUS_KEPT: &str = "CHARTER_SESSION_BUS_KEPT";

/// The variable [`SESSION_BUS_KEPT`] stands in for: where a D-Bus client finds the session bus.
pub const SESSION_BUS: &str = "DBUS_SESSION_BUS_ADDRESS";

/// Where the app points [`SESSION_BUS`] when it starts without the bus. Nothing can listen at a
/// path below `/dev/null`, so every bus call fails at once, and the name says what happened to
/// anyone who reads `/proc/<pid>/environ`.
pub const NO_SESSION_BUS: &str = "unix:path=/dev/null/charter-started-without-the-session-bus";

/// Credential variables no built-in entry passes, matched case-insensitively. A trailing `*`
/// is a prefix.
///
/// The classes a chat must not be handed by default: forge tokens, cloud credentials, model
/// providers' keys, package registries. [`crate::profiles::named_like_a_credential`] catches
/// most of them by their words already; these are the ones whose names do not say so
/// (`AWS_PROFILE` selects a credential, `GOOGLE_APPLICATION_CREDENTIALS` points at one), and
/// the ones a prefix of [`PASSED`] or a harness's own would otherwise admit.
pub const CREDENTIALS: &[&str] = &[
    "GITHUB_*",
    "GH_*",
    "GITLAB_*",
    "GL_TOKEN",
    "BITBUCKET_*",
    "AWS_*",
    "AZURE_*",
    "ARM_*",
    "GOOGLE_*",
    "GCLOUD_*",
    "CLOUDSDK_*",
    "ANTHROPIC_*",
    "OPENAI_*",
    "GEMINI_*",
    "MISTRAL_*",
    "HF_*",
    "HUGGING_FACE_*",
    "CLOUDFLARE_*",
    "CF_*",
    "DIGITALOCEAN_*",
    "HEROKU_*",
    "VERCEL_*",
    "NETLIFY_*",
    "NPM_*",
    "NODE_AUTH_TOKEN",
    "DOCKER_*",
    "VAULT_*",
    "KUBECONFIG",
];

/// Whether `name` is `pattern`: the same name, or one starting with a pattern's text before
/// its trailing `*`. On Windows a variable's name has no case, so neither does this.
fn matches(pattern: &str, name: &str) -> bool {
    if cfg!(windows) {
        return matches_exactly(&pattern.to_ascii_uppercase(), &name.to_ascii_uppercase());
    }
    matches_exactly(pattern, name)
}

/// [`matches`], case and all.
fn matches_exactly(pattern: &str, name: &str) -> bool {
    match pattern.strip_suffix('*') {
        Some(prefix) => name.starts_with(prefix),
        None => name == pattern,
    }
}

/// [`matches`], ignoring ASCII case: a credential spelled in lower case is still one.
fn matches_any_case(pattern: &str, name: &str) -> bool {
    matches_exactly(&pattern.to_ascii_uppercase(), &name.to_ascii_uppercase())
}

/// Whether a chat on `harness` is started with the app's variable `name`, the operator having
/// listed `operator`. The never-passed names are the caller's to hold back first
/// ([`inherited`]).
///
/// **A credential passes by its exact name only.** An operator's prefix is a prefix of
/// ordinary variables: `GO*` is written for Go's, and must not bring Google's credentials file
/// with it.
fn passes(name: &str, harness: Option<Harness>, operator: &[String]) -> bool {
    if operator
        .iter()
        .any(|entry| !entry.ends_with('*') && matches(entry, name))
    {
        return true;
    }
    if crate::profiles::named_like_a_credential(name)
        || CREDENTIALS
            .iter()
            .any(|pattern| matches_any_case(pattern, name))
    {
        return false;
    }
    PASSED
        .iter()
        .chain(harness.map(Harness::env_passed).unwrap_or_default())
        .copied()
        .chain(operator.iter().map(String::as_str))
        .any(|pattern| matches(pattern, name))
}

/// The app's variables a chat on `harness` is started with, of `inherited`, the operator
/// having listed `operator` for the chat's plane.
///
/// `strip` names what the caller holds back whoever lists it — every `OP_*` and each identity
/// variable a vault declares. A harness's identity ([`crate::hookwire::NOT_INHERITED`]) and
/// `TERM` are held back here. A name that is not text is never passed: nothing here could say
/// what it is.
///
/// An app that started itself without the session bus holds the bus it was given in
/// [`SESSION_BUS_KEPT`], and a chat gets that as its `DBUS_SESSION_BUS_ADDRESS` — or none,
/// when it was given none — never the address the app points at nothing.
pub fn inherited(
    inherited: impl IntoIterator<Item = (OsString, OsString)>,
    harness: Option<Harness>,
    operator: &[String],
    strip: &dyn Fn(&OsStr) -> bool,
) -> Vec<(OsString, OsString)> {
    let inherited: Vec<(OsString, OsString)> = inherited.into_iter().collect();
    let value_of = |wanted: &str| {
        inherited
            .iter()
            .find(|(name, _)| name == wanted)
            .map(|(_, value)| value.clone())
    };
    let kept = the_bus_kept(
        value_of(SESSION_BUS).as_deref(),
        value_of(SESSION_BUS_KEPT).as_deref(),
    )
    .map(OsStr::to_owned);
    let inherited = inherited
        .into_iter()
        .filter(|(name, _)| name != SESSION_BUS_KEPT)
        .filter_map(|variable| on_the_kept_bus(variable, kept.as_deref()));
    inherited
        .filter(|(name, _)| {
            let Some(text) = name.to_str() else {
                return false;
            };
            !strip(name)
                && text != "TERM"
                && !crate::hookwire::NOT_INHERITED.contains(&text)
                && passes(text, harness, operator)
        })
        .collect()
}

/// What one chat's program is started with, beside what the app itself has.
#[derive(Debug, Clone, Copy)]
pub struct Starting<'a> {
    /// The harness it runs, whose declared variables it keeps ([`Harness::env_passed`]).
    pub harness: Option<Harness>,
    /// The operator's own additions, from the chat's plane (`[chat_env] pass`, [`read`]).
    pub operator: &'a [String],
    /// Every identity variable a vault of the plane declares, kept out of the chat whether it
    /// is inherited or set, as every `OP_*` is.
    pub strip: &'a [String],
    /// The chat's own: its profile's, charter's and its persona's ([`crate::start::Ready::env`]).
    pub set: &'a [(String, String)],
    /// charter's git hooks (SQ-16, ADR 0074), armed once everything else is settled.
    pub git_hooks: Option<&'a crate::githooks::GitHooks>,
}

/// A chat's whole environment, from `app`, the app's own: what [`inherited`] keeps of it, then
/// the chat's own variables, then charter's git hooks armed after them, so a
/// `GIT_CONFIG_COUNT` the chat already has keeps its pairs. The program starts from this and
/// nothing else, at every level: in a terminal, or as a level-3 agent (ADR 0080 §1).
///
/// A later pair wins over an earlier one of the same name, so a profile's value wins over the
/// app's. What the host adds for the chat itself (its number, its token, its socket) goes after.
pub fn compose(
    app: impl IntoIterator<Item = (OsString, OsString)>,
    chat: &Starting<'_>,
) -> Vec<(OsString, OsString)> {
    // A profile's `OP_*`, and any identity variable a vault declares, is dropped with the
    // app's own (#237, and #271 review U6): a chat never carries one.
    let strip = |name: &OsStr| {
        crate::secrets::identity::kept_from_chats(name)
            || chat.strip.iter().any(|s| OsStr::new(s) == name)
    };
    let mut env = inherited(app, chat.harness, chat.operator, &strip);
    env.extend(
        chat.set
            .iter()
            .filter(|(name, _)| !strip(OsStr::new(name)))
            .map(|(name, value)| (name.into(), value.into())),
    );
    match chat.git_hooks {
        Some(hooks) => hooks.arm(env),
        None => env,
    }
}

/// Puts a program the app starts on the session bus the app was given, when the app is running
/// without it: `address` and `kept` are [`SESSION_BUS`] and [`SESSION_BUS_KEPT`] as the app has
/// them ([`the_bus_kept`]).
///
/// Every program the app starts goes through [`crate::forklock::spawn`], which calls this, so
/// the app's own `git`, `gh` and `charter` reach the keyring over D-Bus as a chat does
/// ([`inherited`]). A program that would inherit the app's address, or is handed the dead one by
/// name, gets the kept one instead — or none, when none was kept. One given a bus of its own is
/// left with it. The kept name itself is never passed on.
///
/// A program started from an empty environment is not told apart from one that inherits: the
/// standard library does not say which a [`std::process::Command`] is. Such a program is handed
/// the kept address too, which is where the operator's own session would have found it.
pub fn onto_the_kept_bus(
    command: &mut std::process::Command,
    address: Option<&OsStr>,
    kept: Option<&OsStr>,
) {
    let Some(kept) = the_bus_kept(address, kept) else {
        return;
    };
    command.env_remove(SESSION_BUS_KEPT);
    let named = command
        .get_envs()
        .find(|(name, _)| *name == SESSION_BUS)
        .map(|(_, value)| value.map(OsStr::to_owned));
    let inherits_or_is_dead = match named {
        None => true,
        Some(Some(address)) => address == NO_SESSION_BUS,
        Some(None) => false,
    };
    if !inherits_or_is_dead {
        return;
    }
    if kept.is_empty() {
        command.env_remove(SESSION_BUS);
    } else {
        command.env(SESSION_BUS, kept);
    }
}

/// The bus an app running without it hands on, from its own [`SESSION_BUS`] (`address`) and
/// [`SESSION_BUS_KEPT`] (`kept`): the kept one, or empty — none — for an app pointed at
/// [`NO_SESSION_BUS`] by hand, as slowstart's hint says, which kept nothing. `None` is an app on
/// its bus, whose address is handed on as it is.
fn the_bus_kept<'a>(address: Option<&OsStr>, kept: Option<&'a OsStr>) -> Option<&'a OsStr> {
    kept.or_else(|| (address == Some(OsStr::new(NO_SESSION_BUS))).then_some(OsStr::new("")))
}

/// `variable` as a chat gets it: the app's bus address swapped for `kept` — dropped when that
/// is empty — and anything else as it was. `kept` is `None` on a launch with the bus.
fn on_the_kept_bus(
    (name, value): (OsString, OsString),
    kept: Option<&OsStr>,
) -> Option<(OsString, OsString)> {
    match kept {
        Some(kept) if name == SESSION_BUS => (!kept.is_empty()).then(|| (name, kept.to_owned())),
        _ => Some((name, value)),
    }
}

/// Whether `entry` is one the operator may list: a variable's name — letters, digits and `_`,
/// not starting with a digit — optionally ending in `*` for a prefix of at least one character.
fn well_formed(entry: &str) -> bool {
    let name = entry.strip_suffix('*').unwrap_or(entry);
    let mut bytes = name.bytes();
    bytes
        .next()
        .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// The operator's entries in `local`, the text of `charter.local.toml`: each well-formed text
/// under `[chat_env] pass`. Anything else passes nothing — a file that cannot be read, a `pass`
/// that is not a list, an entry that is not a name — and the settings tab says why
/// ([`refusals`]).
pub fn from_text(local: Option<&str>) -> Vec<String> {
    let Some(table) = local.and_then(|text| text.parse::<toml::Table>().ok()) else {
        return Vec::new();
    };
    let Some(toml::Value::Array(entries)) = table.get(TABLE).and_then(|t| t.get(KEY)) else {
        return Vec::new();
    };
    entries
        .iter()
        .filter_map(toml::Value::as_str)
        .filter(|entry| well_formed(entry))
        .map(str::to_owned)
        .collect()
}

/// What is wrong with `value`, the `[chat_env]` table: one sentence per thing [`from_text`]
/// leaves out.
pub fn refusals(value: &toml::Value) -> Vec<String> {
    let Some(table) = value.as_table() else {
        return vec![format!(
            "[{TABLE}] in charter.local.toml is not a table, so it passes nothing — write \
             [{TABLE}] then {KEY} = [\"NAME\", \"PREFIX_*\"]."
        )];
    };
    let mut said = Vec::new();
    for key in table.keys().filter(|key| key.as_str() != KEY) {
        said.push(format!(
            "[{TABLE}].{} in charter.local.toml is not read — the table holds {KEY} and \
             nothing else.",
            crate::shown::short(key)
        ));
    }
    match table.get(KEY) {
        None => {}
        Some(toml::Value::Array(entries)) => {
            for entry in entries {
                if !entry.as_str().is_some_and(well_formed) {
                    said.push(format!(
                        "[{TABLE}].{KEY} in charter.local.toml holds {}, which is not a \
                         variable's name — write a name, or a prefix ending in * \
                         (\"GO*\"). It passes nothing.",
                        crate::shown::short(&entry.to_string())
                    ));
                }
            }
        }
        Some(_) => said.push(format!(
            "[{TABLE}].{KEY} in charter.local.toml is not a list, so it passes nothing — \
             write {KEY} = [\"NAME\", \"PREFIX_*\"]."
        )),
    }
    said
}

/// The operator's entries for the plane at `root`, from `charter.local.toml` as
/// [`crate::settings::layer_text`] hands it: a file git would carry passes nothing, because a
/// teammate's commit must not decide what of this machine's environment a chat gets.
pub fn read(root: &Path) -> Vec<String> {
    use crate::settings::{Which, layer_text};
    from_text(layer_text(root, Which::Local).text())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(names: &[&str]) -> Vec<(OsString, OsString)> {
        names
            .iter()
            .map(|name| (OsString::from(name), OsString::from(format!("v-{name}"))))
            .collect()
    }

    fn kept(
        names: &[&str],
        harness: Option<Harness>,
        operator: &[&str],
        strip: &dyn Fn(&OsStr) -> bool,
    ) -> Vec<String> {
        let operator: Vec<String> = operator.iter().map(|s| (*s).to_owned()).collect();
        let mut kept: Vec<String> = inherited(env(names), harness, &operator, strip)
            .into_iter()
            .map(|(name, _)| name.into_string().unwrap())
            .collect();
        kept.sort();
        kept
    }

    fn nothing(_: &OsStr) -> bool {
        false
    }

    #[test]
    fn a_chat_keeps_only_the_names_the_keep_list_holds() {
        let kept = kept(
            &[
                "PATH",
                "HOME",
                "USER",
                "LOGNAME",
                "SHELL",
                "COLORTERM",
                "LANG",
                "LC_CTYPE",
                "TMPDIR",
                "SSH_AUTH_SOCK",
                "XDG_CONFIG_HOME",
                "HTTPS_PROXY",
                "NO_PROXY",
                "CHARTER_ROOT",
                "SOME_TOOL_SETTING",
                "JAVA_HOME",
            ],
            None,
            &[],
            &nothing,
        );

        assert_eq!(
            kept,
            [
                "CHARTER_ROOT",
                "COLORTERM",
                "HOME",
                "HTTPS_PROXY",
                "LANG",
                "LC_CTYPE",
                "LOGNAME",
                "NO_PROXY",
                "PATH",
                "SHELL",
                "SSH_AUTH_SOCK",
                "TMPDIR",
                "USER",
                "XDG_CONFIG_HOME",
            ]
        );
    }

    #[test]
    fn what_a_windows_program_needs_is_kept_too() {
        let names = [
            "SystemRoot",
            "windir",
            "ComSpec",
            "PATHEXT",
            "USERPROFILE",
            "APPDATA",
        ];

        assert_eq!(kept(&names, None, &[], &nothing).len(), names.len());
    }

    /// Windows spells one variable `Path` or `PATH`, and they are the same variable.
    #[cfg(windows)]
    #[test]
    fn on_windows_a_name_is_kept_whatever_its_case() {
        assert_eq!(
            kept(&["Path", "systemroot"], None, &[], &nothing),
            ["Path", "systemroot"]
        );
    }

    #[test]
    fn a_credential_variable_is_not_passed_unless_the_operator_lists_it() {
        let names = [
            "GITHUB_TOKEN",
            "GH_TOKEN",
            "AWS_ACCESS_KEY_ID",
            "AWS_SECRET_ACCESS_KEY",
            "ANTHROPIC_API_KEY",
            "OPENAI_API_KEY",
            "NPM_TOKEN",
        ];
        for harness in [None, Some(Harness::ClaudeCode), Some(Harness::Codex)] {
            assert_eq!(kept(&names, harness, &[], &nothing), Vec::<String>::new());
        }

        assert_eq!(
            kept(&names, None, &["GH_TOKEN", "AWS_ACCESS_KEY_ID"], &nothing),
            ["AWS_ACCESS_KEY_ID", "GH_TOKEN"]
        );
    }

    #[test]
    fn an_operator_prefix_does_not_pass_a_credential_its_exact_name_would() {
        // `GO*` is written for Go's variables, and would catch Google's credentials file too.
        assert_eq!(
            kept(
                &[
                    "GOPATH",
                    "GOOGLE_APPLICATION_CREDENTIALS",
                    "AWS_REGION",
                    "AWS_SECRET_ACCESS_KEY"
                ],
                None,
                &["GO*", "AWS_*"],
                &nothing
            ),
            ["GOPATH"]
        );
    }

    #[test]
    fn a_harness_passes_the_names_it_declares_and_no_other_harness_does() {
        let names = ["CLAUDE_CONFIG_DIR", "CODEX_HOME", "OPENCODE_CONFIG"];

        assert_eq!(
            kept(&names, Some(Harness::ClaudeCode), &[], &nothing),
            ["CLAUDE_CONFIG_DIR"]
        );
        assert_eq!(
            kept(&names, Some(Harness::Codex), &[], &nothing),
            ["CODEX_HOME"]
        );
        assert_eq!(
            kept(&names, Some(Harness::Opencode), &[], &nothing),
            ["OPENCODE_CONFIG"]
        );
        assert_eq!(kept(&names, None, &[], &nothing), Vec::<String>::new());
    }

    #[test]
    fn a_name_a_harness_prefix_admits_is_still_held_back_when_it_reads_as_a_credential() {
        assert_eq!(
            kept(
                &["CLAUDE_CODE_OAUTH_TOKEN", "CLAUDE_CONFIG_DIR"],
                Some(Harness::ClaudeCode),
                &[],
                &nothing
            ),
            ["CLAUDE_CONFIG_DIR"]
        );
        assert_eq!(
            kept(
                &["CLAUDE_CODE_OAUTH_TOKEN"],
                Some(Harness::ClaudeCode),
                &["CLAUDE_CODE_OAUTH_TOKEN"],
                &nothing
            ),
            ["CLAUDE_CODE_OAUTH_TOKEN"]
        );
    }

    #[test]
    fn the_operator_extends_the_list_by_name_and_by_prefix() {
        assert_eq!(
            kept(
                &["JAVA_HOME", "GOPATH", "GOFLAGS", "RUSTUP_HOME"],
                None,
                &["JAVA_HOME", "GO*"],
                &nothing
            ),
            ["GOFLAGS", "GOPATH", "JAVA_HOME"]
        );
    }

    #[test]
    fn what_is_never_passed_stays_out_whoever_lists_it() {
        let strip = |name: &OsStr| name == "OP_SERVICE_ACCOUNT_TOKEN" || name == "PROD_1P_TOKEN";
        assert_eq!(
            kept(
                &[
                    "OP_SERVICE_ACCOUNT_TOKEN",
                    "PROD_1P_TOKEN",
                    "CLAUDE_CODE_SESSION_ID",
                    "CLAUDE_PID",
                    "CLAUDECODE",
                    "CHARTER_WORKSPACE",
                    "CHARTER_PLANE_ROOT_SESSION",
                    "CHARTER_CHAT_TOKEN",
                    "TERM",
                ],
                Some(Harness::ClaudeCode),
                &[
                    "OP_*",
                    "PROD_1P_TOKEN",
                    "CLAUDE*",
                    "CHARTER_WORKSPACE",
                    "CHARTER_PLANE_ROOT_SESSION",
                    "CHARTER_CHAT_TOKEN",
                    "TERM",
                ],
                &strip,
            ),
            Vec::<String>::new()
        );
    }

    fn value_of(kept: &[(OsString, OsString)], name: &str) -> Option<String> {
        kept.iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.to_string_lossy().into_owned())
    }

    /// The app restarted itself without the session bus because the desktop portal was silent
    /// (charter-app#24). The bus is healthy, so a chat's own D-Bus use — a Secret Service
    /// keyring behind git or a CLI — goes to the address the app was given, not the dead one.
    #[test]
    fn a_chat_of_an_app_started_without_the_bus_gets_the_bus_the_app_was_given() {
        let app = [
            (
                OsString::from("DBUS_SESSION_BUS_ADDRESS"),
                OsString::from("unix:path=/dev/null/charter-started-without-the-session-bus"),
            ),
            (
                OsString::from(SESSION_BUS_KEPT),
                OsString::from("unix:path=/run/user/1000/bus"),
            ),
            (OsString::from("HOME"), OsString::from("/home/someone")),
        ];

        let kept = inherited(app, None, &[], &nothing);

        assert_eq!(
            value_of(&kept, "DBUS_SESSION_BUS_ADDRESS").as_deref(),
            Some("unix:path=/run/user/1000/bus")
        );
        assert_eq!(value_of(&kept, SESSION_BUS_KEPT), None, "{kept:?}");
        assert_eq!(kept.len(), 2, "{kept:?}");
    }

    /// With no bus at all, the app was given no address; neither is its chat, which is then
    /// free to find or start one the way any program in the operator's terminal would.
    #[test]
    fn a_chat_of_an_app_that_had_no_bus_to_keep_gets_no_address() {
        let app = [
            (
                OsString::from("DBUS_SESSION_BUS_ADDRESS"),
                OsString::from("unix:path=/dev/null/charter-started-without-the-session-bus"),
            ),
            (OsString::from(SESSION_BUS_KEPT), OsString::new()),
        ];

        let kept = inherited(app, None, &[], &nothing);

        assert_eq!(kept, Vec::new());
    }

    #[test]
    fn a_kept_bus_is_never_passed_under_its_own_name_even_when_listed() {
        let app = [(
            OsString::from(SESSION_BUS_KEPT),
            OsString::from("unix:path=/run/user/1000/bus"),
        )];

        let kept = inherited(app, None, &[SESSION_BUS_KEPT.to_owned()], &nothing);

        assert_eq!(kept, Vec::new());
    }

    /// What `command` says about the bus: each of the two names, set, removed, or not named.
    fn bus_of(command: &std::process::Command) -> Vec<(String, Option<String>)> {
        let mut said: Vec<(String, Option<String>)> = command
            .get_envs()
            .filter(|(name, _)| *name == SESSION_BUS || *name == SESSION_BUS_KEPT)
            .map(|(name, value)| {
                (
                    name.to_string_lossy().into_owned(),
                    value.map(|v| v.to_string_lossy().into_owned()),
                )
            })
            .collect();
        said.sort();
        said
    }

    const REAL_BUS: &str = "unix:path=/run/user/1000/bus";

    /// The app's own `git` — a worktree, a save, a `gh` call git's credential helper makes —
    /// reaches the keyring over the session bus as a chat does. `worktree::git` hands it the
    /// app's address by name, which in a run without the bus is the dead one.
    #[test]
    fn an_app_spawned_git_gets_the_bus_the_app_was_given() {
        let print_the_bus = "alias.bus=!printf %s \"$DBUS_SESSION_BUS_ADDRESS\"";
        for named in [true, false] {
            let mut git = std::process::Command::new("git");
            git.args(["-c", print_the_bus, "bus"])
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("GIT_CONFIG_SYSTEM", "/dev/null");
            if named {
                git.env(SESSION_BUS, NO_SESSION_BUS);
            }

            onto_the_kept_bus(
                &mut git,
                Some(OsStr::new(NO_SESSION_BUS)),
                Some(OsStr::new(REAL_BUS)),
            );
            let out = crate::forklock::output(&mut git).expect("git runs");

            assert!(out.status.success(), "{out:?}");
            assert_eq!(
                String::from_utf8_lossy(&out.stdout),
                REAL_BUS,
                "named: {named}"
            );
        }
    }

    #[test]
    fn a_program_the_app_starts_never_carries_the_kept_name() {
        let mut program = std::process::Command::new("true");

        onto_the_kept_bus(
            &mut program,
            Some(OsStr::new(NO_SESSION_BUS)),
            Some(OsStr::new(REAL_BUS)),
        );

        assert_eq!(
            bus_of(&program),
            [
                (SESSION_BUS_KEPT.to_owned(), None),
                (SESSION_BUS.to_owned(), Some(REAL_BUS.to_owned())),
            ]
        );
    }

    #[test]
    fn with_no_bus_kept_a_program_the_app_starts_gets_no_address() {
        let mut program = std::process::Command::new("true");
        program.env(SESSION_BUS, NO_SESSION_BUS);

        onto_the_kept_bus(
            &mut program,
            Some(OsStr::new(NO_SESSION_BUS)),
            Some(OsStr::new("")),
        );

        assert_eq!(
            bus_of(&program),
            [
                (SESSION_BUS_KEPT.to_owned(), None),
                (SESSION_BUS.to_owned(), None)
            ]
        );
    }

    #[test]
    fn a_program_given_a_bus_of_its_own_keeps_it() {
        let mut named = std::process::Command::new("true");
        named.env(SESSION_BUS, "unix:path=/elsewhere");
        let mut removed = std::process::Command::new("true");
        removed.env_remove(SESSION_BUS);

        onto_the_kept_bus(
            &mut named,
            Some(OsStr::new(NO_SESSION_BUS)),
            Some(OsStr::new(REAL_BUS)),
        );
        onto_the_kept_bus(
            &mut removed,
            Some(OsStr::new(NO_SESSION_BUS)),
            Some(OsStr::new(REAL_BUS)),
        );

        assert!(bus_of(&named).contains(&(
            SESSION_BUS.to_owned(),
            Some("unix:path=/elsewhere".to_owned())
        )));
        assert!(bus_of(&removed).contains(&(SESSION_BUS.to_owned(), None)));
    }

    #[test]
    fn an_app_on_its_bus_starts_programs_as_they_were() {
        let mut program = std::process::Command::new("true");

        onto_the_kept_bus(&mut program, Some(OsStr::new(REAL_BUS)), None);

        assert_eq!(bus_of(&program), []);
    }

    /// `DBUS_SESSION_BUS_ADDRESS=<NO_SESSION_BUS> charter`, as slowstart's hint says: the app has
    /// no bus and kept none, and its chats and programs are not handed the dead address.
    #[test]
    fn a_launch_by_hand_without_the_bus_hands_on_no_address() {
        let app = [
            (OsString::from(SESSION_BUS), OsString::from(NO_SESSION_BUS)),
            (OsString::from("HOME"), OsString::from("/home/someone")),
        ];

        let kept = inherited(app, None, &[], &nothing);

        assert_eq!(value_of(&kept, SESSION_BUS), None, "{kept:?}");

        let mut inherits = std::process::Command::new("true");
        let mut named = std::process::Command::new("true");
        named.env(SESSION_BUS, NO_SESSION_BUS);
        for program in [&mut inherits, &mut named] {
            onto_the_kept_bus(program, Some(OsStr::new(NO_SESSION_BUS)), None);
            assert_eq!(
                bus_of(program),
                [
                    (SESSION_BUS_KEPT.to_owned(), None),
                    (SESSION_BUS.to_owned(), None)
                ]
            );
        }
    }

    #[test]
    fn a_value_is_passed_as_it_was() {
        let kept = inherited(env(&["HOME"]), None, &[], &nothing);
        assert_eq!(kept, [("HOME".into(), "v-HOME".into())]);
    }

    #[test]
    fn the_operator_list_is_read_from_the_local_file_and_a_bad_entry_is_left_out() {
        let text = "[chat_env]\npass = [\"JAVA_HOME\", \"GO*\", \"*\", \"\", \"NOT A NAME\", \"A*B\", 3]\n";

        assert_eq!(from_text(Some(text)), ["JAVA_HOME", "GO*"]);
        assert_eq!(from_text(None), Vec::<String>::new());
        assert_eq!(
            from_text(Some("[chat_env]\npass = \"JAVA_HOME\"\n")),
            Vec::<String>::new()
        );
        assert_eq!(from_text(Some("not toml [")), Vec::<String>::new());
    }

    #[test]
    fn a_bad_entry_is_refused_with_a_sentence_naming_it() {
        let good: toml::Value = toml::from_str("pass = [\"JAVA_HOME\", \"GO*\"]").unwrap();
        assert_eq!(refusals(&good), Vec::<String>::new());

        let bad: toml::Value = toml::from_str("pass = [\"*\"]\nextra = 1").unwrap();
        let said = refusals(&bad);
        assert_eq!(said.len(), 2, "{said:?}");
        assert!(said.iter().any(|s| s.contains("\"*\"")), "{said:?}");
        assert!(said.iter().any(|s| s.contains("extra")), "{said:?}");

        let not_a_list: toml::Value = toml::from_str("pass = \"JAVA_HOME\"").unwrap();
        assert_eq!(refusals(&not_a_list).len(), 1);
    }

    /// A plane in a git repository of its own, whose `charter.local.toml` says `local` and is
    /// ignored by git when `ignored`.
    fn a_plane(local: &str, ignored: bool) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let git = |args: &[&str]| {
            let out = crate::forklock::output(
                std::process::Command::new("git")
                    .args(args)
                    .current_dir(dir.path())
                    .env("GIT_CONFIG_GLOBAL", "/dev/null")
                    .env("GIT_CONFIG_SYSTEM", "/dev/null"),
            )
            .unwrap();
            assert!(out.status.success(), "git {args:?}: {out:?}");
        };
        git(&["init", "-q"]);
        if ignored {
            std::fs::write(dir.path().join(".gitignore"), "/charter.local.toml\n").unwrap();
        }
        std::fs::write(dir.path().join(crate::profiles::LOCAL_FILE), local).unwrap();
        dir
    }

    #[test]
    fn a_plane_extends_the_list_from_its_ignored_local_file_only() {
        let text = "[chat_env]\npass = [\"JAVA_HOME\"]\n";

        assert_eq!(read(a_plane(text, true).path()), ["JAVA_HOME"]);
        assert_eq!(read(a_plane(text, false).path()), Vec::<String>::new());
    }

    #[test]
    fn a_chat_s_environment_is_the_kept_inherited_then_its_own_then_charter_s_git_hooks() {
        // One composition for a chat's program at every level: a terminal chat's and a level-3
        // agent's (ADR 0080 §1).
        let inherited = env(&[
            "PATH",
            "HOME",
            "OP_SESSION_me",
            "PROD_1P_TOKEN",
            "SOME_TOOL",
        ]);
        let set = [
            ("PROD_1P_TOKEN".to_owned(), "from-a-profile".to_owned()),
            (
                "OP_SERVICE_ACCOUNT_TOKEN".to_owned(),
                "from-a-profile".to_owned(),
            ),
            ("CHARTER_PERSONA".to_owned(), "steward".to_owned()),
            ("HOME".to_owned(), "/profile-home".to_owned()),
        ];
        let hooks = crate::githooks::GitHooks::at("/app/git-hooks");

        let composed = compose(
            inherited,
            &Starting {
                harness: None,
                operator: &[],
                strip: &["PROD_1P_TOKEN".to_owned()],
                set: &set,
                git_hooks: Some(&hooks),
            },
        );

        let said: Vec<(String, String)> = composed
            .into_iter()
            .map(|(name, value)| (name.into_string().unwrap(), value.into_string().unwrap()))
            .collect();
        let pair = |name: &str, value: &str| (name.to_owned(), value.to_owned());
        assert_eq!(
            said[..4],
            [
                pair("PATH", "v-PATH"),
                pair("HOME", "v-HOME"),
                pair("CHARTER_PERSONA", "steward"),
                pair("HOME", "/profile-home"),
            ]
        );
        assert!(
            said[4..]
                .iter()
                .all(|(name, _)| name.starts_with("GIT_CONFIG_")),
            "{said:?}"
        );
        assert!(said.contains(&pair("GIT_CONFIG_COUNT", "1")), "{said:?}");
    }
}
