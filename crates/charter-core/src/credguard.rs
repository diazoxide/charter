//! Arm A2: the golden rule — one credential per forge, its own token over HTTPS.
//!
//! A port of `charter/hooks.py`'s `_single_credential_hit` and its closure: `_url_args`,
//! `_ssh_prefix_hosts`, `_git_subcommand`, `_has_ssh_command_config`,
//! `_has_config_env_sshcommand`, `_has_git_config_env_sshcommand`, `_is_sshcommand_config_write`
//! and the tables each reads. The shell substrate is stage 1's [`crate::shellseg`] and stage 2's
//! [`crate::shellwrap`]; `_exported_env` lives with the rest of the shell's environment
//! modelling, as [`crate::shellwrap::exported_env`], because A3 asks it too.
//!
//! # Where this is called from
//!
//! [`crate::toolgate`] — `charter/hooks.py:pretooluse`'s eight refusals, in its order — and
//! through it `charter hook pretooluse` (M3.1 stage 6).
//!
//! Until that stage the switch was closed: `main.rs`'s `is_a_tool_hook` answered every word in
//! the `pretooluse`/`posttooluse` namespace with exit 2 — *block* — because a PARTIAL guard on
//! the switch turns fail-closed into allow-everything-except-the-arm-that-is-ported. It moved
//! once all eight arms were standing, and never earlier. The other eight words in that
//! namespace still block, for the same reason they always did.
//! # The shape names the TRIGGER, never the OPERAND
//!
//! `git <ssh-url>`, not the URL; `GIT_SSH_COMMAND=`, not the command it was set to. This is a
//! guard whose whole job is keeping secrets out of the transcript, so a trace that recorded the
//! matched text would rebuild the leak in a file that outlives the conversation. Every shape
//! this returns is a fixed string or a variable/flag NAME, and the differential compares them
//! character for character.
//!
//! # The forges are an ARGUMENT, and their ORDER is part of the contract
//!
//! The Python reads `config.ROOT` through `registry.known_forges`; the core holds no globals, so
//! [`single_credential_hit`] takes the forge list and [`crate::forge::known_ordered`] builds it.
//! The order is not cosmetic: the `ssh <forge>` arm reports **the first** host whose `git@<host>`
//! appears in the argv, so two hosts where one's name is a prefix of the other's
//! (`github.co` and `github.com`) are told apart only by position. `known_ordered` therefore
//! reproduces Python's dict order — each kind's default host first, then the declared hosts in
//! `charter.toml` order, a declared host that re-declares a default keeping the default's
//! position — rather than the sorted order [`crate::forge::known`] returns.
//!
//! # Case, in three places, each of which was a bypass
//!
//! * the **program**, because APFS and NTFS resolve `GIT` and `git` to the same binary:
//!   `GIT push git@host:o/r.git` walked straight past a case-sensitive compare;
//! * the **host**, because git treats hostnames case-insensitively on the wire, so matching only
//!   the canonical lowercase form is worse than no guard — it still LOOKS present;
//! * the **git config key**, because git's config keys are case-insensitive, so
//!   `CORE.SSHCOMMAND=` is the same override.

use std::sync::OnceLock;

use regex::Regex;

use crate::forge::Forge;
use crate::memstore::py_strip;
use crate::shellseg;
use crate::shellwrap::{self, base_lower};

/// The remedy, identical for every trigger — `_SINGLE_CREDENTIAL_FIX`.
///
/// Which is exactly why it cannot double as the traced label: the first 70 characters are the
/// same no matter what matched, and that is how the tally came to hold 335 denials nobody could
/// attribute (#289). The shape is the attribution; this is the prose.
pub const SINGLE_CREDENTIAL_FIX: &str = "The control plane is **token-only**: git auth is each forge's own CLI token over HTTPS \
     (`charter git-policy --apply` configures every clone; `charter save` / `charter workspace \
     save` already use it). ";

/// `_GIT_SSH_ENV_RE`: `^GIT_SSH(?:_COMMAND)?=`, case-SENSITIVE as the Python's is — an
/// environment variable name is, to every shell charter runs under.
fn git_ssh_env_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\AGIT_SSH(?:_COMMAND)?=").expect("the pattern compiles"))
}

/// `i` as CPython's `(?i)` reads it: the letter, and the two Turkic spellings the `regex` crate's
/// simple case folding leaves out.
///
/// #92 swept the two engines over every letter of `secretshape`'s keyword set and found they
/// differ for exactly U+0130 `İ` and U+0131 `ı` (CPython folds both to `i`, `regex` folds
/// neither). [`crate::leakguard`] and [`crate::secretshape`] carry the same class; it is repeated
/// rather than shared because these are ports of three different Python regexes and a shared
/// constant would invite the next edit to change all three.
const I_CLASS: &str = "[iıİ]";

/// `_SSH_COMMAND_CONFIG_RE`: `(?i)^core\.sshcommand=`, `GIT_SSH_COMMAND`'s exact config twin.
///
/// No [`I_CLASS`] here and that is checked rather than assumed: `core.sshcommand` spells no `i`.
/// The other two characters where the engines could part company are `k` (U+212A) and `s`
/// (U+017F), and #92 measured that both engines fold both — `s` appears here and is covered by
/// `(?i)` alone.
fn ssh_command_config_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)\Acore\.sshcommand=").expect("the pattern compiles"))
}

/// `_CONFIG_KEY_RE`: `(?i)^core\.sshcommand$`, the key with no value attached.
///
/// **CPython's `$`**, which matches at the end of the string *or just before a newline that ends
/// it*, where the `regex` crate's matches only at the end. Spelled `\n?\z`. A shell token really
/// can hold a newline, so this is reachable: `git config "core.sshCommand${NL}" ssh` is one word.
fn config_key_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)\Acore\.sshcommand\n?\z").expect("the pattern compiles"))
}

/// `_GIT_CONFIG_KEY_ENV_RE`: `(?i)^GIT_CONFIG_KEY_\d+=(.*)$`.
///
/// Three CPython facts are reproduced rather than approximated:
///
/// * **`\d` is `\p{Nd}`** on a `str` — every Unicode decimal digit, not `[0-9]`. The `regex`
///   crate's `\d` is the same `\p{Nd}`, so it is left as written.
/// * **`(?i)` over an `i`** — the name spells three, so each is [`I_CLASS`].
/// * **CPython's `$`** again, spelled `\n?\z`. Without it, `GIT_CONFIG_KEY_1=core.sshCommand\n`
///   — one trailing newline, which a shell token can carry — matched in CPython and not here,
///   and a guard that is present in one implementation and absent in the other is the whole
///   failure mode this differential exists for.
///
/// **The first two are reachable only through a WRAPPER, and that was measured rather than
/// assumed.** A bare `GIT_CONFIG_KEY_٣=…` prefix is not an assignment to the shell at all —
/// [`crate::shellwrap::is_env_assignment`] is `^[A-Za-z_][A-Za-z0-9_]*=` — so neither a Unicode
/// digit nor `ı`/`İ` can reach this pattern that way, and both measures would read as rules with
/// no effect. `env` and `sudo` carry their own operand rule, which is only "contains an `=`", so
/// `env GİT_CONFIG_KEY_٣=core.sshCommand git push` does reach it and the oracle does deny it.
/// The corpus holds all four spellings and the bare one beside them.
fn git_config_key_env_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(&format!(
            r"(?i)\AG{I_CLASS}T_CONF{I_CLASS}G_KEY_\d+=(.*)\n?\z"
        ))
        .expect("the pattern compiles")
    })
}

/// `--gpg-sign`'s `-c` twin: `(?:commit|tag)\.gpgsign=true`, full-match and case-SENSITIVE, as
/// the Python's `re.fullmatch` with no flag is. `fullmatch` does NOT allow a trailing newline,
/// unlike `$`, so this is `\A…\z` with nothing between.
fn gpgsign_true_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"\A(?:commit|tag)\.gpgsign=true\z").expect("the pattern compiles")
    })
}

/// Subcommands where a bare `-S` is GPG signing — `_SIGN_VERBS`.
///
/// `git log -S commit` is the pickaxe content search and the word `commit` is its own search
/// string, not evidence of a `commit` subcommand, which is why membership is asked of
/// [`git_subcommand`] and never of the positional list.
const SIGN_VERBS: [&str; 7] = [
    "commit",
    "tag",
    "merge",
    "revert",
    "cherry-pick",
    "rebase",
    "am",
];

/// `--config-env`, `-c`'s documented twin — `_CONFIG_ENV_FLAG`. Git accepts it attached
/// (`--config-env=name=envvar`) and split (two argv tokens).
const CONFIG_ENV_FLAG: &str = "--config-env";

/// `git config` flags that only READ — `_CONFIG_READ_FLAGS`.
const CONFIG_READ_FLAGS: [&str; 7] = [
    "--get",
    "--get-all",
    "--get-regexp",
    "--get-urlmatch",
    "--list",
    "-l",
    "--list-all",
];

/// `git config` flags that can only be a WRITE — `_CONFIG_WRITE_ONLY_FLAGS`.
const CONFIG_WRITE_ONLY_FLAGS: [&str; 2] = ["--add", "--replace-all"];

/// Global git options that consume the NEXT token as a value —
/// `_GIT_GLOBAL_FLAGS_WITH_VALUE`, so `git -C /repo config …` is still a `config` invocation.
///
/// Best-effort: git's full global-option grammar is larger than this, and these are the shapes
/// that actually precede a subcommand.
const GIT_GLOBAL_FLAGS_WITH_VALUE: [&str; 6] = [
    "-C",
    "-c",
    "--git-dir",
    "--work-tree",
    "--namespace",
    "--config-env",
];

/// Positional-ish args that could be a repo URL, skipping free-text flag values — `_url_args`.
///
/// A commit message is free text and may legitimately quote an SSH URL; its VALUE must not be
/// read as one.
pub fn url_args(args: &[String]) -> Vec<String> {
    const SKIPPERS: [&str; 4] = ["-m", "--message", "-F", "--file"];
    const ATTACHED: [&str; 4] = ["-m=", "--message=", "-F=", "--file="];
    let mut out = Vec::new();
    let mut skip = false;
    for a in args {
        if skip {
            skip = false;
            continue;
        }
        if SKIPPERS.contains(&a.as_str()) {
            skip = true;
            continue;
        }
        if ATTACHED.iter().any(|p| a.starts_with(p)) {
            continue;
        }
        out.push(a.clone());
    }
    out
}

/// `ssh-prefix -> host` for every forge, from each forge's own `insteadof()` — `_ssh_prefix_hosts`.
///
/// The SAME SSH forms `gitpolicy` rewrites, so the guard and the rewrite it backstops can never
/// drift apart.
///
/// Returned as an ORDERED list rather than a map because the Python's is a `dict` and its order
/// decides which host a denial names. Python's dict semantics on a duplicate key — keep the first
/// POSITION, take the last VALUE — are reproduced exactly.
pub fn ssh_prefix_hosts(forges: &[Forge]) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for forge in forges {
        let (_https, ssh_forms) = forge.insteadof();
        for prefix in ssh_forms {
            match out.iter_mut().find(|(p, _)| *p == prefix) {
                Some(row) => row.1 = forge.host.clone(),
                None => out.push((prefix, forge.host.clone())),
            }
        }
    }
    out
}

/// True when `-c core.sshCommand=…` appears anywhere in `args` — `_has_ssh_command_config`.
///
/// In ANY position relative to the subcommand, not just where git's own grammar places a global
/// `-c` (strictly before). A defensive guard covers the shape wherever it lands rather than
/// relying on git's parse order: degrading to "covers more, not less" is the safe direction.
pub fn has_ssh_command_config(args: &[String]) -> bool {
    args.iter().enumerate().any(|(i, a)| {
        a == "-c" && i + 1 < args.len() && ssh_command_config_re().is_match(&args[i + 1])
    })
}

/// True when `--config-env=core.sshCommand=VAR` (attached) or `--config-env core.sshCommand=VAR`
/// (split) appears anywhere in `args` — `_has_config_env_sshcommand`.
///
/// Case-insensitive on the CONFIG KEY, and on the flag spelling too — defensively. A
/// differently-cased flag git would reject outright is not a bypass, but matching it costs
/// nothing and never narrows coverage.
pub fn has_config_env_sshcommand(args: &[String]) -> bool {
    for (i, a) in args.iter().enumerate() {
        let low = a.to_lowercase();
        if low.starts_with(&format!("{CONFIG_ENV_FLAG}=")) {
            // Python's `a.split("=", 1)[1]` — of the ORIGINAL token, not the folded one.
            let value = a.split_once('=').map(|(_, v)| v).unwrap_or("");
            if ssh_command_config_re().is_match(value) {
                return true;
            }
        } else if low == CONFIG_ENV_FLAG
            && i + 1 < args.len()
            && ssh_command_config_re().is_match(&args[i + 1])
        {
            return true;
        }
    }
    false
}

/// True when a `GIT_CONFIG_KEY_<n>=core.sshCommand` assignment appears in `env` —
/// `_has_git_config_env_sshcommand`.
///
/// Git's `GIT_CONFIG_COUNT`/`GIT_CONFIG_KEY_n`/`GIT_CONFIG_VALUE_n` mechanism: the override never
/// touches the command line at all.
pub fn has_git_config_env_sshcommand(env: &[String]) -> bool {
    env.iter().any(|e| {
        git_config_key_env_re().captures(e).is_some_and(|m| {
            py_strip(m.get(1).map_or("", |g| g.as_str())).to_lowercase() == "core.sshcommand"
        })
    })
}

/// The git SUBCOMMAND (`config`, `fetch`, …) — `_git_subcommand`.
///
/// Skips leading global options, including ones that consume a following value, so
/// `git -C /repo config …` is still recognised as a `config` invocation. `None` when no bare
/// subcommand token is found.
pub fn git_subcommand(args: &[String]) -> Option<String> {
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        // **This arm changes no answer, and that is proved rather than measured.** The mutation
        // sweep reported it INERT over 20,000 cases and over the whole recording. The reason is
        // that no entry of [`GIT_GLOBAL_FLAGS_WITH_VALUE`] contains an `=`, so a `--flag=value`
        // token can never match the exact-membership arm below; it falls through to the generic
        // `starts_with('-')` arm, which advances by the same one token. It is kept because it is
        // what the Python writes and because it says what the token IS — self-contained — where
        // the generic arm only says it is an option.
        if a.starts_with("--") && a.contains('=') {
            i += 1; // e.g. `--git-dir=x` — self-contained
            continue;
        }
        if GIT_GLOBAL_FLAGS_WITH_VALUE.contains(&a.as_str()) {
            i += 2; // consumes the NEXT token
            continue;
        }
        if a.starts_with('-') {
            i += 1;
            continue;
        }
        return Some(a.clone());
    }
    None
}

/// True when a `git config` invocation SETS `core.sshCommand` — `_is_sshcommand_config_write`.
///
/// `args` is everything after `git`, the leading `config` token included. A pure read
/// (`--get`/`--get-all`/… or the bare `git config core.sshCommand` form with no following value,
/// which is git's own default GET behaviour) is False. **Errs toward True on an ambiguous write
/// shape** — a guard that degrades to LESS coverage is the exact failure this exists to close —
/// and that is why `--add`/`--replace-all` answer True even with no value in sight.
///
/// The persistence is the point: after `git config core.sshCommand …` runs, a plain
/// `git fetch`/`push` goes over SSH with nothing on the command line to see.
pub fn is_sshcommand_config_write(args: &[String]) -> bool {
    let positional: Vec<&String> = args.iter().filter(|a| !a.starts_with('-')).collect();
    let key_positions: Vec<usize> = positional
        .iter()
        .enumerate()
        .filter(|(_, a)| config_key_re().is_match(a.split_once('=').map_or(a.as_str(), |(k, _)| k)))
        .map(|(i, _)| i)
        .collect();
    if key_positions.is_empty() {
        return false;
    }
    if args.iter().any(|a| CONFIG_READ_FLAGS.contains(&a.as_str())) {
        return false;
    }
    for i in key_positions {
        if positional[i].contains('=') {
            return true; // `core.sshCommand=value` inline
        }
        if i + 1 < positional.len() {
            return true; // a VALUE follows the key → a set
        }
    }
    args.iter()
        .any(|a| CONFIG_WRITE_ONLY_FLAGS.contains(&a.as_str()))
}

/// `(shape, detail)` for the first golden-rule violation in `cmd`, else `None` —
/// `_single_credential_hit`.
///
/// ONE scanner behind both things the guard produces: the prose the operator reads
/// ([`single_credential_reason`]) and the label the trace records. They were allowed to drift
/// apart once — the trace hardcoded `reason="single-credential"` while the prose carried the real
/// detail — and the cost was 335 denials in ten days that nobody could break down (#289).
///
/// `forges` is the ordered host list; see the module docs for why the order is a contract.
pub fn single_credential_hit(cmd: &str, forges: &[Forge]) -> Option<(String, String)> {
    let ssh_prefix_hosts = ssh_prefix_hosts(forges);
    // git treats hostnames case-insensitively (`GITHUB.COM` == `github.com` on the wire), so the
    // guard must match that way too — matching only the canonical lowercase form is worse than no
    // guard: it still LOOKS present while a differently-cased host walks straight through it.
    let mut lower_prefix_hosts: Vec<(String, String)> = Vec::new();
    for (prefix, host) in &ssh_prefix_hosts {
        let low = prefix.to_lowercase();
        match lower_prefix_hosts.iter_mut().find(|(p, _)| *p == low) {
            Some(row) => row.1 = host.clone(),
            None => lower_prefix_hosts.push((low, host.clone())),
        }
    }

    let segments = shellseg::segment_argv(cmd);
    let before_each = shellwrap::exported_env(&segments);
    for (toks, before) in segments.iter().zip(before_each.iter()) {
        let (prog, seg_env, argv) = shellwrap::split_env(toks);
        // The environment an EARLIER segment exported reaches this git exactly as an attached
        // prefix does: `export GIT_SSH_COMMAND=/tmp/k && git push` walked past this guard while
        // the attached spelling was denied — the same defect as #496, in the guard next door.
        let mut env = before.clone();
        env.extend(seg_env);
        // Case-folded for the reason `is_charter` and `_VAULT_PATH_RE` are: APFS and NTFS resolve
        // `GIT` and `git` to the same binary, so a case-sensitive compare here is one Shift key
        // from absent.
        let base = base_lower(&prog);
        if base == "git" {
            let args: Vec<String> = argv.iter().skip(1).cloned().collect();
            if let Some(hit) = env.iter().find(|e| git_ssh_env_re().is_match(e)) {
                // The variable NAME only — its value is an arbitrary shell command and may carry
                // a key path, a host, or a secret.
                let name = hit.split_once('=').map_or(hit.as_str(), |(n, _)| n);
                return Some((
                    format!("git {name}="),
                    "This forces git through an SSH transport (GIT_SSH/GIT_SSH_COMMAND) — drop \
                     it."
                    .to_string(),
                ));
            }
            if has_git_config_env_sshcommand(&env) {
                return Some((
                    "git GIT_CONFIG_KEY_n=core.sshCommand".to_string(),
                    "`GIT_CONFIG_KEY_n=core.sshCommand`/`GIT_CONFIG_VALUE_n=…` forces the same \
                     SSH transport override, spelled entirely through environment variables \
                     (git's GIT_CONFIG_COUNT mechanism) — drop it."
                        .to_string(),
                ));
            }
            if has_ssh_command_config(&args) {
                return Some((
                    "git -c core.sshCommand=".to_string(),
                    "`-c core.sshCommand=…` forces the same SSH transport override as \
                     GIT_SSH_COMMAND (its git-config twin) — drop it."
                        .to_string(),
                ));
            }
            if has_config_env_sshcommand(&args) {
                return Some((
                    "git --config-env=core.sshCommand".to_string(),
                    "`--config-env=core.sshCommand=VAR` is `-c`'s documented twin — it reads the \
                     SSH override's VALUE from an environment variable instead of the command \
                     line — drop it."
                        .to_string(),
                ));
            }
            if git_subcommand(&args).as_deref() == Some("config")
                && is_sshcommand_config_write(&args)
            {
                return Some((
                    "git config core.sshCommand".to_string(),
                    "`git config core.sshCommand …` PERSISTS the SSH override into this repo's \
                     config — afterwards a plain `git fetch`/`push` goes over SSH with nothing on \
                     the command line to see. Drop it (a read, `git config --get \
                     core.sshCommand`, stays allowed)."
                        .to_string(),
                ));
            }
            // A URL only counts when the token IS the URL (a bare argument) — not when it is
            // mentioned inside a longer quoted string such as a commit message.
            let bad = url_args(&args).into_iter().find(|a| {
                let low = a.to_lowercase();
                lower_prefix_hosts.iter().any(|(p, _)| low.starts_with(p))
            });
            if let Some(bad) = bad {
                let low = bad.to_lowercase();
                let host = lower_prefix_hosts
                    .iter()
                    .find(|(p, _)| low.starts_with(p))
                    .map(|(_, h)| h.clone())
                    .expect("the prefix that matched above is still in the list");
                // `<ssh-url>`, not the URL: it carries the group and repo name, which is exactly
                // the private detail the trace has no business keeping.
                return Some((
                    "git <ssh-url>".to_string(),
                    format!(
                        "This hands git an SSH {host} URL — use the HTTPS form \
                         (`https://{host}/<group>/<repo>.git`); SSH remotes are auto-rewritten, \
                         so you never need to type one."
                    ),
                ));
            }
            // Signing: `--gpg-sign` / `-c (commit|tag).gpgsign=true` always deny; `-S` denies only
            // on an ACTUAL committing subcommand; and `-s`/`--sign` deny only for `tag`
            // (`git commit -s`/`--signoff` is an unrelated Signed-off-by trailer).
            let subcommand = git_subcommand(&args);
            let flag: Option<String> = if args
                .iter()
                .any(|a| a == "--gpg-sign" || a.starts_with("--gpg-sign="))
            {
                Some("--gpg-sign".to_string())
            } else if args.iter().any(|a| gpgsign_true_re().is_match(a)) {
                Some("-c gpgsign=true".to_string())
            } else if subcommand
                .as_deref()
                .is_some_and(|s| SIGN_VERBS.contains(&s))
                && args.iter().any(|a| a == "-S")
            {
                Some("-S".to_string())
            } else if subcommand.as_deref() == Some("tag") {
                args.iter().find(|a| *a == "-s" || *a == "--sign").cloned()
            } else {
                None
            };
            if let Some(flag) = flag {
                let sub = subcommand.unwrap_or_else(|| "?".to_string());
                return Some((
                    format!("git {sub} {flag}"),
                    "Commit/tag signing is disabled on purpose (a signer prompt hangs an agent) \
                     — commit unsigned; `charter save` handles control-plane commits."
                        .to_string(),
                ));
            }
        } else if base == "ssh" {
            let host = forges.iter().find(|f| {
                let needle = format!("git@{}", f.host).to_lowercase();
                argv.iter()
                    .skip(1)
                    .any(|a| a.to_lowercase().contains(&needle))
            });
            if let Some(forge) = host {
                let cli = forge.kind.cli();
                return Some((
                    "ssh <forge>".to_string(),
                    format!(
                        "SSH to {} isn't used — check the credential with `{cli} auth status` \
                         instead.",
                        forge.host
                    ),
                ));
            }
        }
    }
    None
}

/// The denial `_single_credential_reason` produces — a thin wrapper over
/// [`single_credential_hit`] since #289, so the traced shape and the prose can never disagree
/// about what matched.
pub fn single_credential_reason(cmd: &str, forges: &[Forge]) -> Option<String> {
    single_credential_hit(cmd, forges)
        .map(|(_shape, detail)| format!("{SINGLE_CREDENTIAL_FIX}{detail}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::forge::Kind;

    fn defaults() -> Vec<Forge> {
        vec![
            Forge::default_of(Kind::GitHub),
            Forge::default_of(Kind::GitLab),
        ]
    }

    fn shape(cmd: &str) -> Option<String> {
        single_credential_hit(cmd, &defaults()).map(|(s, _)| s)
    }

    /// The #496 shape, in this guard: an export in an EARLIER segment reaches this git.
    #[test]
    fn an_exported_override_reaches_a_later_git() {
        assert_eq!(
            shape("export GIT_SSH_COMMAND=/tmp/k && git push"),
            Some("git GIT_SSH_COMMAND=".to_string())
        );
        assert_eq!(
            shape("GIT_SSH_COMMAND=/tmp/k git push"),
            Some("git GIT_SSH_COMMAND=".to_string())
        );
    }

    /// `GIT push` is `git push` on APFS and NTFS.
    #[test]
    fn the_program_is_matched_case_folded() {
        assert_eq!(
            shape("GIT push git@github.com:o/r.git"),
            Some("git <ssh-url>".to_string())
        );
    }

    /// The host is matched case-folded too, because git treats hostnames that way on the wire.
    #[test]
    fn the_host_is_matched_case_folded() {
        assert_eq!(
            shape("git clone GIT@GITHUB.COM:o/r.git"),
            Some("git <ssh-url>".to_string())
        );
    }

    /// A URL inside a commit message is not an operand. `-m`'s value is free text.
    #[test]
    fn a_url_in_a_message_value_is_not_an_operand() {
        assert_eq!(shape("git commit -m git@github.com:o/r.git"), None);
        assert_eq!(shape("git commit --message=git@github.com:o/r.git"), None);
    }

    /// The three env-and-flag spellings of the same SSH transport override.
    #[test]
    fn every_spelling_of_the_ssh_override_is_the_same_override() {
        assert_eq!(
            shape("git -c core.sshCommand=/tmp/k push"),
            Some("git -c core.sshCommand=".to_string())
        );
        assert_eq!(
            shape("git --config-env=core.sshCommand=K push"),
            Some("git --config-env=core.sshCommand".to_string())
        );
        assert_eq!(
            shape("git --config-env core.sshCommand=K push"),
            Some("git --config-env=core.sshCommand".to_string())
        );
        assert_eq!(
            shape(
                "GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=core.sshCommand GIT_CONFIG_VALUE_0=x git push"
            ),
            Some("git GIT_CONFIG_KEY_n=core.sshCommand".to_string())
        );
    }

    /// CPython's `$` matches before a trailing newline and the crate's does not. A shell token
    /// really can hold one — a QUOTED newline, since a bare one is a segment boundary — so the
    /// key-env pattern is spelled `\n?\z`. With the crate's own `$` the pattern does not match at
    /// all here (`.` cannot cross the newline and `$` is end-of-haystack), which is an ALLOW
    /// where the oracle denies.
    #[test]
    fn a_trailing_newline_in_the_key_env_assignment_still_matches() {
        assert_eq!(
            shape("GIT_CONFIG_KEY_0='core.sshCommand\n' git push"),
            Some("git GIT_CONFIG_KEY_n=core.sshCommand".to_string())
        );
        // …and the strip the Python does after the capture is not what saves it: a newline in
        // the MIDDLE is not a trailing one, and neither engine matches.
        assert_eq!(shape("GIT_CONFIG_KEY_0='core.ssh\nCommand' git push"), None);
    }

    /// A `git config` READ stays allowed; a write, in any of its shapes, does not.
    #[test]
    fn reading_the_override_is_allowed_and_writing_it_is_not() {
        assert_eq!(shape("git config --get core.sshCommand"), None);
        assert_eq!(shape("git config core.sshCommand"), None);
        assert_eq!(
            shape("git config core.sshCommand 'ssh -i /tmp/k'"),
            Some("git config core.sshCommand".to_string())
        );
        assert_eq!(
            shape("git config core.sshCommand=x"),
            Some("git config core.sshCommand".to_string())
        );
        assert_eq!(
            shape("git config --add core.sshCommand"),
            Some("git config core.sshCommand".to_string())
        );
        // `git -C /repo config …` is still a `config` invocation.
        assert_eq!(
            shape("git -C /repo config core.sshCommand x"),
            Some("git config core.sshCommand".to_string())
        );
    }

    /// `-S` is the pickaxe on `log` and GPG signing on a committing verb; `-s` is a trailer on
    /// `commit` and a signature on `tag`.
    #[test]
    fn the_signing_flags_are_read_by_subcommand() {
        assert_eq!(shape("git log -S commit"), None);
        assert_eq!(shape("git commit -s -m x"), None);
        assert_eq!(
            shape("git commit -S -m x"),
            Some("git commit -S".to_string())
        );
        assert_eq!(shape("git tag -s v1"), Some("git tag -s".to_string()));
        assert_eq!(
            shape("git commit --gpg-sign=KEY -m x"),
            Some("git commit --gpg-sign".to_string())
        );
        assert_eq!(
            shape("git -c commit.gpgsign=true commit -m x"),
            Some("git commit -c gpgsign=true".to_string())
        );
    }

    /// `ssh -T git@github.com` names the forge, and the denial names its CLI.
    #[test]
    fn ssh_to_a_known_forge_is_refused_by_name() {
        let hit = single_credential_hit("ssh -T git@gitlab.com", &defaults()).expect("a hit");
        assert_eq!(hit.0, "ssh <forge>");
        assert!(hit.1.contains("gitlab.com"), "{}", hit.1);
        assert!(hit.1.contains("glab auth status"), "{}", hit.1);
        assert_eq!(shape("ssh -T git@example.invalid"), None);
    }

    /// **The shape never carries a value from the command line.** Everything this returns is a
    /// fixed string or a variable/flag NAME, because a trace outlives the conversation.
    #[test]
    fn no_shape_quotes_the_operand() {
        for cmd in [
            "GIT_SSH_COMMAND='ssh -i /tmp/secret-key' git push",
            "git push git@github.com:private-org/private-repo.git",
            "git -c core.sshCommand='ssh -i /tmp/secret-key' push",
            "ssh -T git@github.com",
        ] {
            let (shape, _detail) = single_credential_hit(cmd, &defaults()).expect("a hit");
            for leak in ["/tmp/secret-key", "private-org", "private-repo"] {
                assert!(!shape.contains(leak), "{shape} quoted {leak}");
            }
        }
    }

    /// A declared self-hosted host is covered — the case no class default can match.
    #[test]
    fn a_declared_host_is_covered() {
        let mut forges = defaults();
        forges.push(Forge::build("gitlab", Some("git.internal")).expect("a forge"));
        let hit = single_credential_hit("git clone git@git.internal:o/r.git", &forges);
        assert_eq!(hit.map(|h| h.0), Some("git <ssh-url>".to_string()));
    }

    /// The forge ORDER is a contract: with one host's name a prefix of another's, the denial
    /// names whichever comes FIRST, and that is what `known_ordered` reproduces.
    #[test]
    fn the_ssh_arm_names_the_first_matching_host() {
        let shorter = Forge::build("gitlab", Some("github.co")).expect("a forge");
        let longer = Forge::default_of(Kind::GitHub);
        let a = single_credential_hit("ssh -T git@github.com", &[shorter.clone(), longer.clone()]);
        let b = single_credential_hit("ssh -T git@github.com", &[longer, shorter]);
        assert!(
            a.expect("a hit").1.contains("github.co "),
            "the shorter host wins when first"
        );
        assert!(
            b.expect("a hit").1.contains("github.com"),
            "the longer host wins when first"
        );
    }

    /// The prose is the fix plus the detail, and nothing else — so a caller that prints the
    /// reason and a trace that records the shape stay two views of one answer.
    #[test]
    fn the_reason_is_the_fix_and_the_detail() {
        let cmd = "git push git@github.com:o/r.git";
        let (_shape, detail) = single_credential_hit(cmd, &defaults()).expect("a hit");
        assert_eq!(
            single_credential_reason(cmd, &defaults()),
            Some(format!("{SINGLE_CREDENTIAL_FIX}{detail}"))
        );
        assert_eq!(single_credential_reason("echo hi", &defaults()), None);
    }
}
