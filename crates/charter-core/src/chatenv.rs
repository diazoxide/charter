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
pub fn inherited(
    inherited: impl IntoIterator<Item = (OsString, OsString)>,
    harness: Option<Harness>,
    operator: &[String],
    strip: &dyn Fn(&OsStr) -> bool,
) -> Vec<(OsString, OsString)> {
    inherited
        .into_iter()
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
}
