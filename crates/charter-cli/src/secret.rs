//! `charter secret`, `charter persona secret` and `charter vault` — the command lines of
//! [`charter_core::secrets`], spelled as `charter/cli.py` spells them.
//!
//! **The words and exit statuses are Python charter's**, recorded in
//! `tests/fixtures/recorded/behaviour.jsonl` (the `secrets` suite). A chat an operator started
//! against the Python charter runs these exact lines — `charter secret exec devops --file
//! KUBECONFIG=PROD_KUBECONFIG -- kubectl get pods` — and 0.1.0 answered every one with a usage
//! error because the commands were not here.

use std::ffi::OsString;
use std::io::{IsTerminal, Read, Write};

use charter_core::secrets::cmd::{self, Io, Say};
use charter_core::secrets::exec::{self, Request};
use charter_core::secrets::vaultcmd::{self, AddRequest};
use charter_core::secrets::{Ctx, Env};
use clap::{Args, Subcommand};

use crate::voice;

/// `charter secret …`: read and write the secrets in a vault; values stay out of the model.
#[derive(Subcommand)]
pub enum SecretCommand {
    /// Store a secret (value via --stdin/--from-file, not argv).
    Set {
        vault: String,
        #[command(flatten)]
        set: SetArgs,
    },
    /// List secret keys in a vault (never the values).
    List { vault: String },
    /// Flag secrets older than --days for rotation.
    Audit {
        vault: String,
        /// Staleness threshold in days (default 90).
        #[arg(long, default_value_t = 90, allow_negative_numbers = true)]
        days: i64,
    },
    /// Show a secret (masked by default; --reveal for humans).
    Get {
        vault: String,
        #[command(flatten)]
        get: GetArgs,
    },
    /// Delete a secret.
    Rm { vault: String, key: String },
    /// Materialize a secret to a 0600 file (e.g. a kubeconfig).
    Cp {
        vault: String,
        #[command(flatten)]
        cp: CpArgs,
    },
    /// Run a command with secrets injected as env/files, redacted.
    Exec {
        vault: String,
        #[command(flatten)]
        exec: ExecArgs,
    },
}

/// `charter persona secret …`: the same verbs, on the ACTIVE persona's vault.
#[derive(Subcommand)]
pub enum PersonaSecretCommand {
    /// Store a secret in the persona's vault.
    Set {
        #[command(flatten)]
        who: Who,
        #[command(flatten)]
        set: SetArgs,
    },
    /// List secret keys in the persona's vault.
    List {
        #[command(flatten)]
        who: Who,
    },
    /// Flag the persona's secrets older than --days for rotation.
    Audit {
        #[command(flatten)]
        who: Who,
        /// Staleness threshold in days (default 90).
        #[arg(long, default_value_t = 90, allow_negative_numbers = true)]
        days: i64,
    },
    /// Show a secret (masked; --reveal for humans).
    Get {
        #[command(flatten)]
        who: Who,
        #[command(flatten)]
        get: GetArgs,
    },
    /// Delete a secret from the persona's vault.
    Rm {
        #[command(flatten)]
        who: Who,
        key: String,
    },
    /// Materialize a secret to a 0600 file.
    Cp {
        #[command(flatten)]
        who: Who,
        #[command(flatten)]
        cp: CpArgs,
    },
    /// Run a command with the persona's secrets injected + redacted.
    Exec {
        #[command(flatten)]
        who: Who,
        #[command(flatten)]
        exec: ExecArgs,
    },
}

/// `--persona`, on every `persona secret` verb.
#[derive(Args)]
pub struct Who {
    /// Override the active persona.
    #[arg(long)]
    persona: Option<String>,
}

#[derive(Args)]
pub struct SetArgs {
    key: String,
    /// Read the value from stdin.
    #[arg(long)]
    stdin: bool,
    /// Read the value verbatim from a file.
    #[arg(long)]
    from_file: Option<String>,
    /// Inline value (discouraged: visible in shell history).
    #[arg(long, allow_hyphen_values = true)]
    value: Option<String>,
    /// Permit storing an empty value. Refused by default: an empty secret reads as present and
    /// healthy everywhere charter looks, so the mistake only surfaces later as a 401.
    #[arg(long)]
    allow_empty: bool,
}

#[derive(Args)]
pub struct GetArgs {
    key: String,
    /// Print plaintext (interactive terminals only).
    #[arg(long)]
    reveal: bool,
    /// Allow --reveal to a non-interactive stdout.
    #[arg(long)]
    force: bool,
}

#[derive(Args)]
pub struct CpArgs {
    key: String,
    /// Path of a REAL FILE to create. A device, FIFO, directory or symlink is refused —
    /// /dev/stdout is this conversation.
    dest: String,
    /// Overwrite an existing file (destroying its contents and setting it to 0600). Refused
    /// without this.
    #[arg(long)]
    force: bool,
}

#[derive(Args)]
pub struct ExecArgs {
    /// Inject secret <key> as env var NAME (repeatable).
    #[arg(long, value_name = "NAME=key")]
    env: Vec<String>,
    /// Write secret <key> to a temp 0600 file; set ENVVAR to its path (repeatable).
    #[arg(long, value_name = "ENVVAR=key")]
    file: Vec<String>,
    /// Add secret <key> as NAME to a temp 0600 dotenv file; set ENVVAR to its path (repeatable
    /// — repeats sharing an ENVVAR merge into one file).
    #[arg(long, value_name = "ENVVAR=NAME:key")]
    dotenv: Vec<String>,
    /// Run the command as a CHILD with stdio inherited, wait for it, then delete any
    /// --file/--dotenv temp files. Output is NOT redacted (nothing is captured).
    #[arg(long = "stream")]
    stream_mode: bool,
    /// Replace this process with the command, so stdio streams through — for a long-running
    /// child such as an MCP stdio server. Output is NOT redacted; incompatible with --file and
    /// --dotenv — use --stream for those.
    #[arg(long = "exec")]
    exec_mode: bool,
    /// Command to run; put it after `--`, e.g. -- kubectl get pods.
    #[arg(num_args = 0.., trailing_var_arg = true)]
    command: Vec<String>,
}

/// `charter vault …`: manage secret vaults (provider + config + persona).
#[derive(Subcommand)]
pub enum VaultCommand {
    /// Register a vault.
    Add(VaultAdd),
    /// List configured vaults (names/status only, never values).
    List,
    /// Resolve every reference for real and report what does NOT resolve.
    Verify {
        /// One vault (default: all of them).
        name: Option<String>,
    },
    /// Unregister a vault (leaves its file on disk).
    Remove { name: String },
}

#[derive(Args)]
pub struct VaultAdd {
    name: String,
    /// Vault backend (default: plain-file).
    #[arg(long, default_value = "plain-file", value_parser = ["1password", "plain-file", "reference"])]
    provider: String,
    /// File path for plain-file/reference vaults (default: .charter/vaults/<name>.json).
    #[arg(long)]
    file: Option<String>,
    /// 1Password vault charter keeps its item in (provider: 1password).
    #[arg(long, value_name = "NAME")]
    op_vault: Option<String>,
    /// 1Password item whose fields are this vault's secrets (provider: 1password). Default:
    /// charter-<vault>.
    #[arg(long)]
    op_item: Option<String>,
    /// 1Password account to pin to (provider: 1password); needed when signed into more than one.
    #[arg(long)]
    account: Option<String>,
    /// Tag this vault for a persona (e.g. devops, qa). It must be one this plane defines.
    #[arg(long)]
    persona: Option<String>,
    /// Bind the identity this vault is read through: TARGET=SOURCE. Only NAMES are stored,
    /// never values. Repeatable.
    #[arg(long, value_name = "TARGET=SOURCE")]
    env: Vec<String>,
    /// Shorthand for --env OP_SERVICE_ACCOUNT_TOKEN=SOURCE.
    #[arg(long, value_name = "SOURCE")]
    token_env: Option<String>,
    /// Record it in the COMMITTED registry (vaults.json at the plane root).
    #[arg(long)]
    share: bool,
    /// Replace an existing registration of this name. Does NOT migrate its secrets.
    #[arg(long)]
    force: bool,
}

/// Where the secrets commands speak: `util.py`'s four marks on stderr, the answer on stdout.
struct Console;

impl Io for Console {
    fn say(&mut self, line: Say) {
        match line {
            Say::Info(t) => voice::info(&t),
            Say::Ok(t) => voice::ok(&t),
            Say::Warn(t) => voice::warn(&t),
            Say::Err(t) => voice::err(&t),
        }
    }

    fn out(&mut self, bytes: &[u8]) {
        let mut out = std::io::stdout().lock();
        let _ = out.write_all(bytes);
        let _ = out.flush();
    }

    fn err(&mut self, bytes: &[u8]) {
        let mut err = std::io::stderr().lock();
        let _ = err.write_all(bytes);
        let _ = err.flush();
    }

    fn stdout_is_terminal(&self) -> bool {
        std::io::stdout().is_terminal()
    }

    fn stdin_is_terminal(&self) -> bool {
        std::io::stdin().is_terminal()
    }

    fn read_stdin(&mut self) -> String {
        let mut text = String::new();
        let _ = std::io::stdin().read_to_string(&mut text);
        text
    }

    fn read_hidden(&mut self, prompt: &str) -> String {
        charter_core::secrets::tty::read_hidden(prompt).unwrap_or_default()
    }
}

/// The prefixes whose trailing `-- <command…>` is the child's, untouched —
/// `cli._EXEC_PREFIXES`.
const EXEC_PREFIXES: [&[&str]; 2] = [&["secret", "exec"], &["persona", "secret", "exec"]];

/// `cli._split_exec_command`: peel everything after the FIRST `--` of an `exec` invocation
/// off before the parser sees it. Anything after it is the child's, flags included — and it
/// replaces whatever the parser would have taken as the command.
pub fn split_exec(argv: Vec<OsString>) -> (Vec<OsString>, Option<Vec<String>>) {
    let args = &argv[1.min(argv.len())..];
    for prefix in EXEC_PREFIXES {
        if args.len() >= prefix.len()
            && args.iter().zip(prefix.iter()).all(|(a, p)| a == p)
            && let Some(at) = args[prefix.len()..].iter().position(|a| a == "--")
        {
            let cut = 1 + prefix.len() + at;
            let tail = argv[cut + 1..]
                .iter()
                .map(|a| a.to_string_lossy().into_owned())
                .collect();
            let mut head = argv;
            head.truncate(cut);
            return (head, Some(tail));
        }
    }
    (argv, None)
}

fn ctx(here: &crate::Here) -> Ctx {
    Ctx::new(here.plane.root(), Env::from_process())
}

fn request(vault: String, exec: ExecArgs, graft: Option<Vec<String>>) -> Request {
    Request {
        vault,
        env: exec.env,
        file: exec.file,
        dotenv: exec.dotenv,
        stream: exec.stream_mode,
        exec: exec.exec_mode,
        command: graft.unwrap_or(exec.command),
    }
}

fn set_from(set: &SetArgs) -> cmd::SetFrom {
    cmd::SetFrom {
        stdin: set.stdin,
        from_file: set.from_file.clone(),
        value: set.value.clone(),
        allow_empty: set.allow_empty,
    }
}

/// `charter secret <verb> <vault> …`.
pub fn secret(here: &crate::Here, command: SecretCommand, graft: Option<Vec<String>>) -> u8 {
    let ctx = ctx(here);
    let io = &mut Console;
    let code = match command {
        SecretCommand::Set { vault, set } => cmd::set(&ctx, &vault, &set.key, &set_from(&set), io),
        SecretCommand::List { vault } => cmd::list(&ctx, &vault, io),
        SecretCommand::Audit { vault, days } => cmd::audit(&ctx, &vault, days, io),
        SecretCommand::Get { vault, get } => {
            cmd::get(&ctx, &vault, &get.key, get.reveal, get.force, io)
        }
        SecretCommand::Rm { vault, key } => cmd::rm(&ctx, &vault, &key, io),
        SecretCommand::Cp { vault, cp } => cmd::cp(&ctx, &vault, &cp.key, &cp.dest, cp.force, io),
        SecretCommand::Exec { vault, exec } => exec::exec(&ctx, &request(vault, exec, graft), io),
    };
    status(code)
}

/// `charter persona secret <verb> …`: the persona's vault, then the same verb.
pub fn persona_secret(
    here: &crate::Here,
    command: PersonaSecretCommand,
    graft: Option<Vec<String>>,
) -> u8 {
    let ctx = ctx(here);
    let io = &mut Console;
    let who = match &command {
        PersonaSecretCommand::Set { who, .. }
        | PersonaSecretCommand::List { who }
        | PersonaSecretCommand::Audit { who, .. }
        | PersonaSecretCommand::Get { who, .. }
        | PersonaSecretCommand::Rm { who, .. }
        | PersonaSecretCommand::Cp { who, .. }
        | PersonaSecretCommand::Exec { who, .. } => who.persona.clone(),
    };
    // The whole check first (#1059): a misspelled `--persona` must not read whatever vault
    // happens to be tagged with the misspelling.
    let flag = who.filter(|p| !p.is_empty());
    if let Some(name) = &flag
        && let Some(refused) = charter_core::personas::name_refusal(here.plane.root(), name)
    {
        voice::err(&refused);
        return 1;
    }
    let Some(name) = here.active_persona(flag.as_deref()) else {
        voice::err(
            "no active persona. Select one: charter persona use <name>  (or pass --persona).",
        );
        return 1;
    };
    let vault = match cmd::persona_vault(&ctx, &name) {
        Ok(v) => v,
        Err(message) => {
            voice::err(&message);
            return 1;
        }
    };
    let code = match command {
        PersonaSecretCommand::Set { set, .. } => {
            cmd::set(&ctx, &vault, &set.key, &set_from(&set), io)
        }
        PersonaSecretCommand::List { .. } => cmd::list(&ctx, &vault, io),
        PersonaSecretCommand::Audit { days, .. } => cmd::audit(&ctx, &vault, days, io),
        PersonaSecretCommand::Get { get, .. } => {
            cmd::get(&ctx, &vault, &get.key, get.reveal, get.force, io)
        }
        PersonaSecretCommand::Rm { key, .. } => cmd::rm(&ctx, &vault, &key, io),
        PersonaSecretCommand::Cp { cp, .. } => {
            cmd::cp(&ctx, &vault, &cp.key, &cp.dest, cp.force, io)
        }
        PersonaSecretCommand::Exec { exec, .. } => {
            exec::exec(&ctx, &request(vault, exec, graft), io)
        }
    };
    status(code)
}

/// `charter vault <verb> …`.
pub fn vault(here: &crate::Here, command: VaultCommand) -> u8 {
    let ctx = ctx(here);
    let io = &mut Console;
    let code = match command {
        VaultCommand::Add(a) => vaultcmd::add(
            &ctx,
            &AddRequest {
                name: a.name,
                provider: a.provider,
                file: a.file,
                op_vault: a.op_vault,
                op_item: a.op_item,
                account: a.account,
                persona: a.persona,
                env: a.env,
                token_env: a.token_env,
                share: a.share,
                force: a.force,
            },
            io,
        ),
        VaultCommand::List => vaultcmd::list(&ctx, io),
        VaultCommand::Verify { name } => vaultcmd::verify(&ctx, name.as_deref(), io),
        VaultCommand::Remove { name } => vaultcmd::remove(&ctx, &name, io),
    };
    status(code)
}

/// `sys.exit(n)`: the low byte of whatever the command returned.
fn status(code: i32) -> u8 {
    (code & 0xff) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(words: &[&str]) -> Vec<OsString> {
        words.iter().map(OsString::from).collect()
    }

    fn strings(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| w.to_string()).collect()
    }

    #[test]
    fn everything_after_the_first_separator_is_the_childs() {
        let (head, tail) = split_exec(argv(&[
            "charter", "secret", "exec", "v", "--env", "A=B", "--", "kubectl", "--", "-n", "x",
        ]));
        assert_eq!(
            head,
            argv(&["charter", "secret", "exec", "v", "--env", "A=B"])
        );
        assert_eq!(tail, Some(strings(&["kubectl", "--", "-n", "x"])));
    }

    #[test]
    fn the_persona_form_is_split_the_same_way() {
        let (head, tail) = split_exec(argv(&[
            "charter",
            "persona",
            "secret",
            "exec",
            "--persona",
            "p",
            "--",
            "true",
        ]));
        assert_eq!(
            head,
            argv(&["charter", "persona", "secret", "exec", "--persona", "p"])
        );
        assert_eq!(tail, Some(strings(&["true"])));
    }

    #[test]
    fn no_other_command_is_touched() {
        let words = argv(&["charter", "secret", "list", "--", "x"]);
        assert_eq!(split_exec(words.clone()), (words, None));
    }
}
