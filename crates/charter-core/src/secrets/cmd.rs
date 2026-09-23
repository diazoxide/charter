//! `charter secret …` and `charter persona secret …` — `charter/commands_secrets.py` and the
//! persona proxy in `charter/commands_persona.py`.
//!
//! Each command returns the exit status Python's returns, and speaks through an [`Io`] so the
//! words and the streams they go to are the CLI's business and a test can read them back.
//!
//! **The access record.** Exactly three routes take a plaintext out of this process — a child's
//! environment or temp file (`exec`), a file on disk (`cp`) and a terminal (`get --reveal`) —
//! and all three write one trace event before the value leaves, naming the vault, the keys and
//! the command. Never a value: every field is scrubbed of the values the call resolved before
//! it is written ([`trace_secret_use`]). `get` without `--reveal` records nothing, because
//! nothing left.

use std::path::Path;

use serde_json::Value;

use super::registry::{self, Vault};
use super::{Ctx, VaultError, fingerprint, onepassword, plain_file, reference};

/// One line in charter's voice — `util.info/ok/warn/err`, all on stderr.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Say {
    Info(String),
    Ok(String),
    Warn(String),
    Err(String),
}

/// Where a command's words and bytes go.
pub trait Io {
    /// A line in charter's voice, on stderr.
    fn say(&mut self, line: Say);
    /// Bytes on stdout — a command's ANSWER, or a child's redacted output.
    fn out(&mut self, bytes: &[u8]);
    /// Bytes on stderr, unadorned — a child's redacted stderr.
    fn err(&mut self, bytes: &[u8]);
    /// Whether stdout is a terminal: the one channel `--reveal` will print to.
    fn stdout_is_terminal(&self) -> bool;
    /// Whether stdin is a terminal.
    fn stdin_is_terminal(&self) -> bool;
    /// All of stdin, for `secret set`.
    fn read_stdin(&mut self) -> String;
    /// One line typed at the terminal with echo off, after `prompt` — `getpass.getpass`.
    fn read_hidden(&mut self, prompt: &str) -> String;
}

// ---------------------------------------------------------------------------------------
// The provider, whichever it is.

/// `_provider(name)`: the vault registered as `name`, ready to read. Every provider this
/// charter knows is implemented, so none is unavailable here.
pub fn provider(ctx: &Ctx, name: &str) -> Result<Vault, VaultError> {
    registry::vault(ctx, name)
}

/// The value of `key`, through the vault's own provider.
pub fn get_value(ctx: &Ctx, v: &Vault, key: &str) -> Result<String, VaultError> {
    match v.provider.as_str() {
        "plain-file" => plain_file::get(ctx, v, key),
        "reference" => reference::get(ctx, v, key),
        _ => onepassword::get(ctx, v, key),
    }
}

/// The key names, never the values.
pub fn keys(ctx: &Ctx, v: &Vault) -> Result<Vec<String>, VaultError> {
    match v.provider.as_str() {
        "plain-file" => plain_file::keys(ctx, v),
        "reference" => reference::keys(ctx, v),
        _ => onepassword::keys(ctx, v),
    }
}

/// Store a value.
pub fn set_value(ctx: &Ctx, v: &Vault, key: &str, value: &str) -> Result<(), VaultError> {
    match v.provider.as_str() {
        "plain-file" => plain_file::set(ctx, v, key, value, chrono::Local::now().date_naive()),
        "reference" => reference::set(ctx, v, key, value),
        _ => onepassword::set(ctx, v, key, value),
    }
}

/// Delete a value.
pub fn delete(ctx: &Ctx, v: &Vault, key: &str) -> Result<(), VaultError> {
    match v.provider.as_str() {
        "plain-file" => plain_file::delete(ctx, v, key),
        "reference" => reference::delete(ctx, v, key),
        _ => onepassword::delete(ctx, v, key),
    }
}

/// `health()`: `(ok, detail)`, never a value.
pub fn health(ctx: &Ctx, v: &Vault) -> (bool, String) {
    match v.provider.as_str() {
        "plain-file" => plain_file::health(ctx, v),
        "reference" => reference::health(ctx, v),
        _ => onepassword::health(ctx, v),
    }
}

// ---------------------------------------------------------------------------------------
// The access record.

/// `_value_free`: `field` with any of `values` it holds rewritten to `***`, at any depth.
fn value_free(field: &Value, values: &[String]) -> Value {
    match field {
        Value::String(s) => Value::String(super::redact_str(s, values)),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| (super::redact_str(k, values), value_free(v, values)))
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(|v| value_free(v, values)).collect()),
        other => other.clone(),
    }
}

/// `_trace_secret_use`: record that a credential was handed out — which command, which names,
/// never a value. Best-effort and silent: the bookkeeping must never fail the delivery.
pub fn trace_secret_use(ctx: &Ctx, event: &str, resolved: &[String], fields: &[(&str, Value)]) {
    let scrubbed: Vec<(&str, Value)> = fields
        .iter()
        .map(|(k, v)| (*k, value_free(v, resolved)))
        .collect();
    let session = crate::trace::bucket(&|name| ctx.env.get(name));
    crate::trace::record_values(
        &ctx.root,
        &session,
        event,
        &scrubbed,
        chrono::Local::now().naive_local(),
    );
}

// ---------------------------------------------------------------------------------------
// list / get / audit / set / rm

/// `cmd_secret_list`: the key names, one per line on stdout.
pub fn list(ctx: &Ctx, vault: &str, io: &mut dyn Io) -> i32 {
    let found = provider(ctx, vault).and_then(|v| keys(ctx, &v));
    match found {
        Err(e) => {
            io.say(Say::Err(e.message));
            1
        }
        Ok(keys) if keys.is_empty() => {
            io.say(Say::Info(format!("Vault '{vault}' has no secrets.")));
            0
        }
        Ok(keys) => {
            for k in keys {
                io.out(format!("{k}\n").as_bytes());
            }
            0
        }
    }
}

/// `cmd_secret_get`: masked by default; `--reveal` only to a terminal unless `--force`.
pub fn get(ctx: &Ctx, vault: &str, key: &str, reveal: bool, force: bool, io: &mut dyn Io) -> i32 {
    let value = match provider(ctx, vault).and_then(|v| get_value(ctx, &v, key)) {
        Ok(value) => value,
        Err(e) => {
            io.say(Say::Err(e.message));
            return 1;
        }
    };
    if !reveal {
        io.out(
            format!(
                "{vault}/{key}: present · {}\n(value hidden — use `charter secret exec` to hand it \
                 to a command, `secret cp` for a tool that needs a file path, or --reveal to \
                 print)\n",
                fingerprint::masked(ctx, &value)
            )
            .as_bytes(),
        );
        return 0;
    }
    if !io.stdout_is_terminal() && !force {
        io.say(Say::Err(
            "Refusing to print a secret to non-interactive stdout — this is how it would leak \
             into an agent's context. Use `charter secret exec` to hand it to a command instead \
             (`secret cp` materialises a 0600 file for a tool that needs a path — reading that \
             file back leaks the value just the same), or pass --force if you truly intend to \
             print it."
                .into(),
        ));
        return 2;
    }
    trace_secret_use(
        ctx,
        "secret-reveal",
        std::slice::from_ref(&value),
        &[
            ("vault", Value::String(vault.into())),
            ("key_names", Value::Array(vec![Value::String(key.into())])),
            ("forced", Value::Bool(force)),
        ],
    );
    io.say(Say::Warn(
        "Revealing secret plaintext to this terminal.".into(),
    ));
    let mut shown = value;
    if !shown.ends_with('\n') {
        shown.push('\n');
    }
    io.out(shown.as_bytes());
    0
}

/// `cmd_secret_audit`: secrets older than `days`, for rotation hygiene. Only a plain-file vault
/// tracks ages; the others manage rotation externally.
pub fn audit(ctx: &Ctx, vault: &str, days: i64, io: &mut dyn Io) -> i32 {
    let v = match provider(ctx, vault) {
        Ok(v) => v,
        Err(e) => {
            io.say(Say::Err(e.message));
            return 1;
        }
    };
    if v.provider != "plain-file" {
        io.say(Say::Info(format!(
            "vault '{vault}' ({}) manages rotation externally — no age tracking.",
            v.provider
        )));
        return 0;
    }
    let ages = match plain_file::ages(ctx, &v, chrono::Local::now().date_naive()) {
        Ok(a) => a,
        Err(e) => {
            io.say(Say::Err(e.message));
            return 1;
        }
    };
    if ages.is_empty() {
        io.say(Say::Info(format!("vault '{vault}' has no secrets.")));
        return 0;
    }
    let mut stale: Vec<(&String, i64)> = ages
        .iter()
        .filter_map(|(k, d)| d.filter(|d| *d >= days).map(|d| (k, d)))
        .collect();
    stale.sort_by_key(|(_, d)| -d);
    let mut unknown: Vec<&String> = ages
        .iter()
        .filter(|(_, d)| d.is_none())
        .map(|(k, _)| k)
        .collect();
    unknown.sort();
    for (k, d) in &stale {
        io.say(Say::Warn(format!(
            "{vault}/{k}: {d} days old — consider rotating"
        )));
    }
    if stale.is_empty() {
        io.say(Say::Ok(format!(
            "no secrets in '{vault}' older than {days} days."
        )));
    }
    if !unknown.is_empty() {
        let names: Vec<&str> = unknown.iter().map(|s| s.as_str()).collect();
        io.say(Say::Info(format!(
            "age unknown (set before tracking): {}",
            names.join(", ")
        )));
    }
    if stale.is_empty() { 0 } else { 1 }
}

/// Where `secret set` takes its value from. Never argv, unless the operator insists.
#[derive(Clone, Default)]
pub struct SetFrom {
    pub stdin: bool,
    pub from_file: Option<String>,
    pub value: Option<String>,
    pub allow_empty: bool,
}

impl std::fmt::Debug for SetFrom {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SetFrom")
            .field("stdin", &self.stdin)
            .field("from_file", &self.from_file)
            .field("value", &self.value.as_ref().map(|_| "***"))
            .field("allow_empty", &self.allow_empty)
            .finish()
    }
}

/// `_read_value`: a file, an inline value (warned), a pipe on stdin, or — with a terminal on
/// stdin and no `--stdin` — a hidden prompt. Never argv unless the operator insists.
fn read_value(ctx: &Ctx, key: &str, from: &SetFrom, io: &mut dyn Io) -> Result<String, VaultError> {
    if let Some(file) = &from.from_file {
        let p = super::expanduser(file, &ctx.env);
        return std::fs::read_to_string(&p)
            .map_err(|e| VaultError::new(format!("cannot read {}: {e}", p.display())));
    }
    if let Some(v) = &from.value {
        io.say(Say::Warn(
            "Value passed via --value is visible in shell history / process list; prefer --stdin \
             or --from-file."
                .into(),
        ));
        return Ok(v.clone());
    }
    if from.stdin || !io.stdin_is_terminal() {
        let data = io.read_stdin();
        return Ok(data.strip_suffix('\n').map(str::to_owned).unwrap_or(data));
    }
    Ok(io.read_hidden(&format!("Value for '{key}' (hidden): ")))
}

/// `cmd_secret_set`: store a value, refusing an empty one unless `--allow-empty`.
pub fn set(ctx: &Ctx, vault: &str, key: &str, from: &SetFrom, io: &mut dyn Io) -> i32 {
    let outcome = (|| -> Result<String, VaultError> {
        let v = provider(ctx, vault)?;
        // The rule `vault add` applies, applied again where the plaintext is written: a
        // registry is hand-editable and half of it is committed, so a plain-file vault can
        // come to point inside the plane at a path git would take.
        if v.provider == "plain-file"
            && let Some(file) = super::config_str(&v.config, "file")
            && let Some(unignored) = super::vaultcmd::unignored_plaintext(ctx, file)
        {
            return Err(VaultError::new(format!(
                "refusing to write: '{unignored}' is inside the control plane and NOT gitignored \
                 — a plain-file vault stores plaintext, so the next `charter save` would commit \
                 it. Add it to .gitignore, or re-register the vault with a --file under .charter/ \
                 or outside the plane."
            )));
        }
        let value = read_value(ctx, key, from, io)?;
        if value.is_empty() && !from.allow_empty {
            let how = if io.stdin_is_terminal() {
                ""
            } else {
                "\n  Nothing arrived on stdin. An agent's shell, a CI step and `< /dev/null` \
                 all\n  look like a pipe with no data behind them."
            };
            return Err(VaultError::new(format!(
                "refusing to store an empty value for '{key}' — it would read as a present, \
                 healthy secret everywhere charter looks.{how}\n  If that is genuinely what you \
                 want: --allow-empty"
            )));
        }
        set_value(ctx, &v, key, &value)?;
        Ok(value)
    })();
    match outcome {
        Ok(value) => {
            io.say(Say::Ok(format!(
                "Set '{key}' in vault '{vault}' ({}). Value not shown.",
                fingerprint::size_band(&value)
            )));
            0
        }
        Err(e) => {
            io.say(Say::Err(e.message));
            1
        }
    }
}

/// `cmd_secret_rm`.
pub fn rm(ctx: &Ctx, vault: &str, key: &str, io: &mut dyn Io) -> i32 {
    match provider(ctx, vault).and_then(|v| delete(ctx, &v, key)) {
        Ok(()) => {
            io.say(Say::Ok(format!("Removed '{key}' from vault '{vault}'.")));
            0
        }
        Err(e) => {
            io.say(Say::Err(e.message));
            1
        }
    }
}

// ---------------------------------------------------------------------------------------
// persona secret

/// The persona's effective `vault:` — the most-derived non-empty value up its `extends:` chain.
fn declared_vault(root: &Path, name: &str) -> Option<String> {
    let chain = crate::personas::lineage(root, name);
    let mut vault: Option<String> = None;
    for a in chain.iter().rev() {
        if let Some(pairs) = crate::personas::load(root, a)
            && let Some((_, v)) = pairs.iter().rev().find(|(k, _)| k == "vault")
            && !v.is_empty()
        {
            vault = Some(v.clone());
        }
    }
    vault
}

/// `persona.NO_VAULT`: the declared "this persona holds no credentials".
pub const NO_VAULT: &str = "none";

/// `commands_persona._resolve_vault`: the vault a persona's secrets live in, or the sentence
/// saying why there is none. `name` is the persona already resolved (flag, then the ladder).
pub fn persona_vault(ctx: &Ctx, name: &str) -> Result<String, String> {
    let declared = declared_vault(&ctx.root, name);
    let vault = match declared.as_deref().map(crate::memstore::py_strip) {
        Some(NO_VAULT) => None,
        Some(v) if !v.is_empty() => Some(v.to_string()),
        _ => registry::load_registry(ctx)
            .ok()
            .and_then(|doc| registry::vaults_for_persona(&doc, name).into_iter().next()),
    };
    let declares_none = declared
        .as_deref()
        .map(crate::memstore::py_strip)
        .is_some_and(|v| v == NO_VAULT);
    let Some(vault) = vault else {
        if declares_none {
            return Err(format!(
                "persona '{name}' declares `vault: {NO_VAULT}` — it holds no credentials by \
                 design. Use a persona that owns this secret, or replace that line with a real \
                 vault name."
            ));
        }
        return Err(format!(
            "persona '{name}' has no vault. Add `vault:` to its file, or `charter vault add <v> \
             --persona {name}`."
        ));
    };
    let registered = registry::load_registry(ctx)
        .map(|doc| registry::vaults(&doc).contains_key(&vault))
        .map_err(|e| e.message)?;
    if !registered {
        return Err(format!(
            "persona '{name}' vault '{vault}' isn't set up on this machine. Create it: charter \
             vault add {vault} --provider plain-file --persona {name}."
        ));
    }
    Ok(vault)
}

// ---------------------------------------------------------------------------------------
// cp

/// `e.strerror`: the OS's sentence, without Rust's ` (os error N)` suffix.
#[cfg(unix)]
fn strerror(e: &std::io::Error) -> String {
    let text = e.to_string();
    match text.rfind(" (os error ") {
        Some(at) if text.ends_with(')') => text[..at].to_string(),
        _ => text,
    }
}

/// What a destination turned out to be, for the refusal — `_DEST_KINDS`.
#[cfg(unix)]
fn dest_kind(ft: std::fs::FileType) -> &'static str {
    use std::os::unix::fs::FileTypeExt;
    if ft.is_dir() {
        "a directory"
    } else if ft.is_fifo() {
        "a FIFO"
    } else if ft.is_socket() {
        "a socket"
    } else if ft.is_char_device() {
        "a character device"
    } else if ft.is_block_device() {
        "a block device"
    } else {
        "not a file"
    }
}

/// `_own_stream_identities`: `(st_dev, st_ino)` → `standard output` for this process's own three
/// streams. IDENTITY, not name: `/dev/stdout`, `/dev/fd/1`, a hardlink to the transcript log and
/// the path `readlink` gives for it are five names for one inode.
#[cfg(unix)]
fn own_streams() -> Vec<((u64, u64), &'static str)> {
    let mut out: Vec<((u64, u64), &'static str)> = Vec::new();
    let mut add = |st: rustix::io::Result<rustix::fs::Stat>, name: &'static str| {
        if let Ok(st) = st {
            #[allow(clippy::unnecessary_cast)] // `st_dev` is `i32` on macOS, `u64` on Linux
            let id = (st.st_dev as u64, st.st_ino as u64);
            if !out.iter().any(|(k, _)| *k == id) {
                out.push((id, name));
            }
        }
    };
    add(rustix::fs::fstat(std::io::stdin()), "standard input");
    add(rustix::fs::fstat(std::io::stdout()), "standard output");
    add(rustix::fs::fstat(std::io::stderr()), "standard error");
    out
}

fn own_stream_refusal(dest: &str, which: &str) -> String {
    format!(
        "{dest} is charter's own {which} — the channel this conversation is read from, whatever \
         it is called here. Writing a credential to it puts the plaintext straight into the \
         transcript, which is the leak `secret cp` exists to avoid. Name a real file that is not \
         one of these streams."
    )
}

/// `_identify_dest`: `fstat` of `raw` opened for writing without creating, truncating,
/// following a link or blocking on a FIFO.
#[cfg(unix)]
fn identify_dest(raw: &Path) -> Option<rustix::fs::Stat> {
    use rustix::fs::{Mode, OFlags};
    let fd = rustix::fs::open(
        raw,
        OFlags::WRONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK,
        Mode::empty(),
    )
    .ok()?;
    rustix::fs::fstat(&fd).ok()
}

/// `_cp_dest_refusal`: why `dest` is not somewhere a secret may be materialised — or `None`.
#[cfg(unix)]
fn cp_dest_refusal(dest: &Path, force: bool) -> Option<String> {
    use std::os::unix::fs::MetadataExt;
    let shown = dest.display().to_string();
    if dest.as_os_str().is_empty() {
        return Some("the destination path is empty.".into());
    }
    let st = match std::fs::symlink_metadata(dest) {
        Ok(st) => st,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(e) => return Some(format!("{shown} cannot be inspected ({}).", strerror(&e))),
    };
    let ft = st.file_type();
    if ft.is_symlink() {
        return Some(format!(
            "{shown} is a symlink, and charter will not follow one to write a credential — the \
             link decides where the plaintext lands, not you. Name the real path."
        ));
    }
    if !ft.is_file() {
        return Some(format!(
            "{shown} is {}, not a regular file. `secret cp` materialises a credential *to a \
             file*; writing it to a device or a pipe puts the plaintext on whatever is reading \
             that — and /dev/stdout, /dev/stderr and /dev/fd/* are this agent's own transcript.",
            dest_kind(ft)
        ));
    }
    let streams = own_streams();
    let mut which = streams
        .iter()
        .find(|(id, _)| *id == (st.dev(), st.ino()))
        .map(|(_, n)| *n);
    if which.is_none()
        && let Some(opened) = identify_dest(dest)
    {
        #[allow(clippy::unnecessary_cast)] // `st_dev` is `i32` on macOS, `u64` on Linux
        let id = (opened.st_dev as u64, opened.st_ino as u64);
        which = streams.iter().find(|(k, _)| *k == id).map(|(_, n)| *n);
    }
    if let Some(which) = which {
        return Some(own_stream_refusal(&shown, which));
    }
    if !force {
        return Some(format!(
            "{shown} already exists. Writing would destroy its contents and set it to 0600. Pass \
             --force to overwrite it deliberately, or choose a path that does not exist."
        ));
    }
    None
}

/// `os.path.abspath`: joined to the working directory and normalised, links NOT resolved.
#[cfg(unix)]
fn abspath(p: &Path) -> std::path::PathBuf {
    let joined = if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(p)
    };
    std::path::PathBuf::from(crate::pypath::normpath(&joined.to_string_lossy()))
}

/// `cmd_secret_cp`: materialise a secret to a REAL file at 0600, printing the path only. The
/// destination is checked — by path, then by the descriptor actually opened — BEFORE the value
/// is resolved, so a refused destination never brings the plaintext into this process at all.
#[cfg(unix)]
pub fn cp(ctx: &Ctx, vault: &str, key: &str, dest: &str, force: bool, io: &mut dyn Io) -> i32 {
    use rustix::fs::{Mode, OFlags};
    use std::os::unix::fs::PermissionsExt;

    let dest = super::expanduser(dest, &ctx.env);
    let shown = dest.display().to_string();
    if let Some(refusal) = cp_dest_refusal(&dest, force) {
        io.say(Say::Err(format!("Refusing to write a secret: {refusal}")));
        return 2;
    }
    if let Some(unignored) =
        super::vaultcmd::unignored_plaintext(ctx, &abspath(&dest).to_string_lossy())
    {
        io.say(Say::Err(format!(
            "Refusing to write a secret: '{unignored}' is inside the control plane and NOT \
             gitignored — the next `charter save` would commit it."
        )));
        io.say(Say::Info(
            "  Add it to .gitignore, write it under .charter/, or pick a path outside the plane."
                .into(),
        ));
        return 2;
    }
    let parent = dest
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    if !parent.is_dir() {
        let mut builder = std::fs::DirBuilder::new();
        std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
        match builder.create(parent) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => {
                io.say(Say::Err(format!(
                    "Refusing to write a secret: cannot create {} ({}). charter creates at most one \
                     missing directory level; create the parent yourself if you meant this.",
                    parent.display(),
                    strerror(&e)
                )));
                return 2;
            }
        }
    }
    let mut flags = OFlags::WRONLY | OFlags::CREATE | OFlags::NOFOLLOW | OFlags::NONBLOCK;
    if !force {
        flags |= OFlags::EXCL;
    }
    let created = std::fs::symlink_metadata(&dest).is_err();
    let fd = match rustix::fs::open(&dest, flags, Mode::from_raw_mode(0o600)) {
        Ok(fd) => fd,
        Err(e) => {
            let e = std::io::Error::from(e);
            io.say(Say::Err(format!(
                "Refusing to write a secret: cannot open {shown} ({}).",
                strerror(&e)
            )));
            return 2;
        }
    };
    let file = std::fs::File::from(fd);
    let abandon = |file: std::fs::File| {
        drop(file);
        if created {
            let _ = std::fs::remove_file(&dest);
        }
    };
    let opened = match rustix::fs::fstat(&file) {
        Ok(st) => st,
        Err(e) => {
            abandon(file);
            io.say(Say::Err(format!(
                "Refusing to write a secret: cannot inspect {shown} ({}).",
                strerror(&std::io::Error::from(e))
            )));
            return 2;
        }
    };
    let ft = file.metadata().map(|m| m.file_type()).ok();
    if !ft.is_some_and(|t| t.is_file()) {
        let kind = ft.map(dest_kind).unwrap_or("not a file");
        abandon(file);
        io.say(Say::Err(format!(
            "Refusing to write a secret: {shown} is {kind}, not a regular file — whatever the path \
             looked like before it was opened."
        )));
        return 2;
    }
    #[allow(clippy::unnecessary_cast)] // `st_dev` is `i32` on macOS, `u64` on Linux
    let id = (opened.st_dev as u64, opened.st_ino as u64);
    if let Some((_, which)) = own_streams().into_iter().find(|(k, _)| *k == id) {
        abandon(file);
        io.say(Say::Err(format!(
            "Refusing to write a secret: {}",
            own_stream_refusal(&shown, which)
        )));
        return 2;
    }
    let value = match provider(ctx, vault).and_then(|v| get_value(ctx, &v, key)) {
        Ok(v) => v,
        Err(e) => {
            abandon(file);
            io.say(Say::Err(e.message));
            return 1;
        }
    };
    trace_secret_use(
        ctx,
        "secret-cp",
        std::slice::from_ref(&value),
        &[
            ("vault", Value::String(vault.into())),
            ("key_names", Value::Array(vec![Value::String(key.into())])),
            ("dest", Value::String(shown.clone())),
            ("overwrote", Value::Bool(force && !created)),
        ],
    );
    let mut file = file;
    let _ = file.set_permissions(std::fs::Permissions::from_mode(0o600));
    let written = file
        .set_len(0)
        .and_then(|()| std::io::Write::write_all(&mut file, value.as_bytes()));
    if let Err(e) = written {
        io.say(Say::Err(format!(
            "cannot write {shown} ({}).",
            strerror(&e)
        )));
        return 1;
    }
    if force && !created {
        io.say(Say::Warn(format!("Overwrote {shown} and set it to 0600.")));
    }
    io.say(Say::Ok(format!(
        "Wrote '{vault}/{key}' to {shown} (0600). Value not shown."
    )));
    0
}

/// Windows has no `O_NOFOLLOW`, no device files to refuse by identity, and no 0600: charter
/// does not claim a guarantee it cannot keep, so `secret cp` is refused there.
#[cfg(not(unix))]
pub fn cp(_ctx: &Ctx, _vault: &str, _key: &str, _dest: &str, _force: bool, io: &mut dyn Io) -> i32 {
    io.say(Say::Err(
        "Refusing to write a secret: `secret cp` needs a filesystem with unix modes, which this \
         platform does not have. Use `charter secret exec --file` instead."
            .into(),
    ));
    2
}
