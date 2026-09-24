//! `charter vault add|list|verify|remove` — the vault half of `commands_secrets.py`.
//!
//! A registration holds NAMES only: a provider, a file path, a 1Password vault and item, the
//! names of the environment variables an identity is read through. Never a value.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use super::cmd::{self, Io, Say};
use super::registry::{self, Vault};
use super::{Ctx, onepassword};

/// Cells between one column of `vault list` and the next.
const GAP: usize = 2;

/// What `vault add` was asked to register.
#[derive(Debug, Clone, Default)]
pub struct AddRequest {
    pub name: String,
    pub provider: String,
    pub file: Option<String>,
    pub op_vault: Option<String>,
    pub op_item: Option<String>,
    pub account: Option<String>,
    pub persona: Option<String>,
    pub env: Vec<String>,
    pub token_env: Option<String>,
    pub share: bool,
    pub force: bool,
}

/// `_portable_file`: relative to the plane root when inside it, else as given (expanded).
fn portable_file(ctx: &Ctx, p: &Path) -> String {
    let p = super::expanduser(&p.to_string_lossy(), &ctx.env);
    match (super::resolve(&p), super::resolve(&ctx.root)) {
        (Some(rp), Some(rr)) => match rp.strip_prefix(&rr) {
            Ok(rel) => rel.to_string_lossy().into_owned(),
            Err(_) => p.to_string_lossy().into_owned(),
        },
        _ => p.to_string_lossy().into_owned(),
    }
}

/// `util.git_ignores`: whether git would ignore `path` inside `root`, or `None` when `root` is
/// not a repository. Asked of `git check-ignore`, the authority on the question.
pub fn git_ignores(root: &Path, path: &Path) -> Option<bool> {
    let git = |args: &[&std::ffi::OsStr]| {
        let mut command = std::process::Command::new("git");
        command
            .arg("-C")
            .arg(root)
            .args(args)
            .env("GIT_TERMINAL_PROMPT", "0")
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            .env_remove("GIT_COMMON_DIR")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        crate::forklock::status(&mut command).ok()
    };
    let repo = git(&["rev-parse".as_ref(), "--git-dir".as_ref()])?;
    if !repo.success() {
        return None;
    }
    let ignored = git(&["check-ignore".as_ref(), "-q".as_ref(), path.as_os_str()])?;
    Some(ignored.success())
}

/// `_unignored_plaintext`: the path, if a plaintext file there would be committed.
pub fn unignored_plaintext(ctx: &Ctx, configured: &str) -> Option<String> {
    let mut p = super::expanduser(configured, &ctx.env);
    if !p.is_absolute() {
        p = ctx.root.join(p);
    }
    let rp = super::resolve(&p)?;
    let rr = super::resolve(&ctx.root)?;
    let rel = rp.strip_prefix(&rr).ok()?.to_path_buf();
    match git_ignores(&ctx.root, &p) {
        None | Some(true) => None,
        Some(false) => Some(rel.to_string_lossy().into_owned()),
    }
}

/// `_env_bindings`: `{TARGET: SOURCE}` from `--env` and `--token-env`, names validated.
fn env_bindings(req: &AddRequest) -> Result<Map<String, Value>, String> {
    let mut pairs = Map::new();
    if let Some(t) = req.token_env.as_deref().filter(|t| !t.is_empty()) {
        pairs.insert("OP_SERVICE_ACCOUNT_TOKEN".into(), Value::String(t.into()));
    }
    for raw in &req.env {
        let Some((target, source)) = raw.split_once('=') else {
            return Err(format!(
                "--env expects TARGET=SOURCE, got {}",
                crate::pyrepr::repr_str(raw)
            ));
        };
        pairs.insert(
            crate::memstore::py_strip(target).to_string(),
            Value::String(crate::memstore::py_strip(source).to_string()),
        );
    }
    let ok = |v: &str| {
        let mut c = v.chars();
        c.next()
            .is_some_and(|f| f.is_ascii_alphabetic() || f == '_')
            && c.all(|x| x.is_ascii_alphanumeric() || x == '_')
    };
    for (target, source) in &pairs {
        let source = source.as_str().unwrap_or_default();
        for (label, v) in [("target", target.as_str()), ("source", source)] {
            if !ok(v) {
                return Err(format!(
                    "--env {label} {} is not a valid environment variable name",
                    crate::pyrepr::repr_str(v)
                ));
            }
        }
    }
    Ok(pairs)
}

/// The other registered vault whose file is `file`, if one is. `name` itself does not count:
/// `--force` re-registering a vault over its own file is how it changes provider.
fn file_owner(ctx: &Ctx, file: &str, name: &str) -> Option<String> {
    let want = super::resolve(&ctx.vault_file_path(file))?;
    let doc = registry::load_registry(ctx).ok()?;
    let mut names: Vec<String> = registry::vaults(&doc).keys().cloned().collect();
    names.sort();
    names.into_iter().filter(|n| n != name).find(|n| {
        registry::vault_in(&doc, n)
            .ok()
            .and_then(|v| super::config_str(&v.config, "file").map(|f| ctx.vault_file_path(f)))
            .and_then(|p| super::resolve(&p))
            .is_some_and(|p| p == want)
    })
}

/// `cmd_vault_add`.
pub fn add(ctx: &Ctx, req: &AddRequest, io: &mut dyn Io) -> i32 {
    if !registry::name_ok(&req.name) {
        io.say(Say::Err(registry::name_refusal(&req.name)));
        return 1;
    }
    if let Some(p) = req.persona.as_deref().filter(|p| !p.is_empty())
        && let Some(refused) = crate::personas::name_refusal(&ctx.root, p)
    {
        io.say(Say::Err(refused));
        return 1;
    }
    let mut cfg = Map::new();
    let default_file = || ctx.vaults_dir().join(format!("{}.json", req.name));
    let file_given = req.file.as_deref().filter(|f| !f.is_empty());
    if req.provider == "plain-file" || req.provider == "reference" {
        let f = file_given.map(PathBuf::from).unwrap_or_else(default_file);
        cfg.insert("file".into(), Value::String(portable_file(ctx, &f)));
    } else if let Some(f) = file_given {
        cfg.insert(
            "file".into(),
            Value::String(portable_file(ctx, Path::new(f))),
        );
    }
    if let Some(file) = cfg.get("file").and_then(Value::as_str)
        && let Some(owner) = file_owner(ctx, file, &req.name)
    {
        io.say(Say::Err(format!(
            "'{file}' is already the file of vault '{owner}'. Two vaults over one file would read \
             one vault's secrets as the other's entries — choose another --file, or remove \
             '{owner}' first."
        )));
        return 1;
    }
    if req.provider == "plain-file"
        && let Some(file) = cfg.get("file").and_then(Value::as_str)
        && let Some(unignored) = unignored_plaintext(ctx, file)
    {
        io.say(Say::Err(format!(
            "'{unignored}' is inside the control plane and NOT gitignored — a plain-file vault \
             stores plaintext, so the next `charter save` would commit these credentials."
        )));
        let vaults_dir = ctx
            .vaults_dir()
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        io.say(Say::Info(format!(
            "  Keep it out of git (add it to .gitignore, or use the default under {vaults_dir}/), \
             or point --file outside the plane."
        )));
        io.say(Say::Info(
            "  A `reference` vault stores op:// URIs rather than values and IS safe to commit: \
             --provider reference"
                .into(),
        ));
        return 1;
    }
    if req.provider == "1password" {
        let Some(op_vault) = req.op_vault.as_deref().filter(|v| !v.is_empty()) else {
            io.say(Say::Err(format!(
                "--op-vault is required for a 1password vault: which 1Password vault should \
                 charter create its items in?\n  charter vault add {} --provider 1password \
                 --op-vault Engineering",
                req.name
            )));
            return 1;
        };
        cfg.insert("op-vault".into(), Value::String(op_vault.into()));
        if let Some(item) = req.op_item.as_deref().filter(|v| !v.is_empty()) {
            cfg.insert("op-item".into(), Value::String(item.into()));
        }
        if let Some(account) = req.account.as_deref().filter(|v| !v.is_empty()) {
            cfg.insert("account".into(), Value::String(account.into()));
        }
        for key in ["op-vault", "op-item", "account"] {
            if let Some(value) = cfg.get(key).and_then(Value::as_str)
                && let Err(e) = onepassword::not_a_flag(key, &req.name, value)
            {
                io.say(Say::Err(e.message));
                return 1;
            }
        }
    }
    let env_map = match env_bindings(req) {
        Ok(m) => m,
        Err(message) => {
            io.say(Say::Err(message));
            return 1;
        }
    };
    if !env_map.is_empty() {
        cfg.insert("env".into(), Value::Object(env_map));
    }
    let replaced = if req.force {
        registry::load_registry(ctx)
            .ok()
            .and_then(|doc| registry::vaults(&doc).get(&req.name).cloned())
    } else {
        None
    };
    let persona = req.persona.as_deref().filter(|p| !p.is_empty());
    if let Err(e) = registry::add_vault(
        ctx,
        &req.name,
        &req.provider,
        cfg.clone(),
        persona,
        req.force,
        req.share,
    ) {
        io.say(Say::Err(e.message));
        return 1;
    }
    if let Some(replaced) = replaced {
        let old_where = replaced
            .get("config")
            .and_then(|c| c.get("file"))
            .filter(|f| !f.is_null() && f.as_str() != Some(""))
            .map(super::py_str);
        let provider = replaced
            .get("provider")
            .map(super::py_str)
            .unwrap_or_else(|| "?".into());
        io.say(Say::Warn(format!(
            "Replaced the previous '{}' registration (provider: {provider}). Its secrets were NOT \
             migrated.{}",
            req.name,
            old_where
                .map(|w| format!(" They remain at {w}."))
                .unwrap_or_default()
        )));
    }
    let tag = persona
        .map(|p| format!(", persona: {p}"))
        .unwrap_or_default();
    let where_ = if req.share {
        "shared — commit vaults.json"
    } else {
        "local only"
    };
    io.say(Say::Ok(format!(
        "Vault '{}' registered (provider: {}{tag}) [{where_}].",
        req.name, req.provider
    )));
    if !req.share {
        io.say(Say::Info(format!(
            "  Teammates won't see it. Publish the wiring (never the secrets) with: charter vault \
             add {} --provider {} --share",
            req.name, req.provider
        )));
    }
    let v = match registry::vault(ctx, &req.name) {
        Ok(v) => v,
        Err(e) => {
            io.say(Say::Err(e.message));
            return 1;
        }
    };
    let (ok, detail) = cmd::health(ctx, &v);
    io.say(if ok {
        Say::Info(format!("  {detail}"))
    } else {
        Say::Warn(format!("  {detail}"))
    });
    match req.provider.as_str() {
        "1password" => {
            io.say(Say::Info(format!(
                "  charter keeps this vault in one 1Password item, '{}' in vault '{}', tagged \
                 'charter:{}' — each secret a concealed field of it.",
                onepassword::op_item(&v).unwrap_or_default(),
                cfg.get("op-vault").map(super::py_str).unwrap_or_default(),
                req.name
            )));
            io.say(Say::Info(format!(
                "  add secrets with: charter secret set {} <key> --stdin",
                req.name
            )));
        }
        "keyring" => {
            io.say(Say::Info(format!(
                "  charter keeps each secret as one item in {}, and the key names — never the \
                 values — in {}.",
                super::keyring::STORE_NAME,
                super::short_path(&ctx.root, &super::keyring::index_path(ctx, &v))
            )));
            io.say(Say::Info(format!(
                "  add secrets with: charter secret set {} <key> --stdin",
                req.name
            )));
        }
        "reference" => {
            io.say(Say::Info(
                "  stores op:// or vault:// URIs; values are fetched at read time.".into(),
            ));
            io.say(Say::Info(format!(
                "  add one with: charter secret set {} <key> --value 'op://<vault>/<item>/<field>'",
                req.name
            )));
        }
        _ => io.say(Say::Info(format!(
            "  add secrets with: charter secret set {} <key> --stdin",
            req.name
        ))),
    }
    0
}

/// The PERSONA cell: the label, marked when it names no persona this plane defines (#1057).
fn persona_cell(ctx: &Ctx, label: Option<&Value>) -> String {
    let label = match label {
        None | Some(Value::Null) => return "—".into(),
        Some(Value::String(s)) if s.is_empty() => return "—".into(),
        Some(v) => super::py_str(v),
    };
    let shown = crate::personas::one_line(&label);
    if crate::personas::load(&ctx.root, &label).is_none() {
        format!("{shown} (no such persona)")
    } else {
        shown
    }
}

/// `cmd_vault_list`: VAULT, PROVIDER, PERSONA, SCOPE and a STATUS that never holds a value.
pub fn list(ctx: &Ctx, io: &mut dyn Io) -> i32 {
    let doc = match registry::load_registry(ctx) {
        Ok(d) => d,
        Err(e) => {
            io.say(Say::Err(e.message));
            return 1;
        }
    };
    let vs = registry::vaults(&doc);
    if vs.is_empty() {
        io.say(Say::Info(
            "No vaults configured. Add one: charter vault add <name>".into(),
        ));
        return 0;
    }
    let heads = ["VAULT", "PROVIDER", "PERSONA", "SCOPE"];
    let mut names: Vec<&String> = vs.keys().collect();
    names.sort();
    let body: Vec<[String; 4]> = names
        .iter()
        .map(|name| {
            let entry = &vs[name.as_str()];
            [
                (*name).clone(),
                entry
                    .get("provider")
                    .map(super::py_str)
                    .unwrap_or_else(|| "?".into()),
                persona_cell(ctx, entry.get("persona")),
                registry::scope_of(ctx, name),
            ]
        })
        .collect();
    let widths: Vec<usize> = heads
        .iter()
        .enumerate()
        .map(|(i, h)| crate::tui::column(h, body.iter().map(|row| row[i].as_str()), GAP, None))
        .collect();
    let line = |cells: [&str; 4], last: &str| {
        let mut out = String::new();
        for (c, w) in cells.iter().zip(&widths) {
            out.push_str(&crate::tui::pad(c, *w, crate::tui::Align::Left));
        }
        out.push_str(last);
        format!("{}\n", crate::memstore::py_rstrip(&out))
    };
    io.out(line(heads, "STATUS").as_bytes());
    let rules: Vec<String> = widths
        .iter()
        .map(|w| "-".repeat(w.saturating_sub(GAP).max(1)))
        .collect();
    io.out(
        line(
            [&rules[0], &rules[1], &rules[2], &rules[3]],
            &"-".repeat("STATUS".len()),
        )
        .as_bytes(),
    );
    for row in &body {
        let detail = match registry::vault_in(&doc, &row[0])
            // Where the identity is, never the identity: a moved one is not read from the
            // keyring to draw a row.
            .and_then(|v| super::identity_missing(ctx, &v).map_or(Ok(v), Err))
        {
            Ok(v) => cmd::health(ctx, &v).1,
            Err(e) => e.message.split('\n').next().unwrap_or_default().to_string(),
        };
        io.out(line([&row[0], &row[1], &row[2], &row[3]], &detail).as_bytes());
    }
    0
}

/// `verify_vault`: resolve every reference, reporting which do not — never a value.
fn verify_vault(ctx: &Ctx, v: &Vault) -> Vec<(String, Option<String>)> {
    let keys = match cmd::keys(ctx, v) {
        Ok(k) => k,
        Err(e) => return vec![("*".into(), Some(e.message))],
    };
    keys.into_iter()
        .map(|k| {
            let err = cmd::get_value(ctx, v, &k).err().map(|e| e.message);
            (k, err)
        })
        .collect()
}

/// `cmd_vault_verify`: every vault (or one), resolved for real. Non-zero when anything fails.
pub fn verify(ctx: &Ctx, name: Option<&str>, io: &mut dyn Io) -> i32 {
    let names: Vec<String> = match name.filter(|n| !n.is_empty()) {
        Some(n) => vec![n.to_string()],
        None => match registry::load_registry(ctx) {
            Ok(doc) => registry::vaults(&doc).keys().cloned().collect(),
            Err(e) => {
                io.say(Say::Err(e.message));
                return 1;
            }
        },
    };
    let mut provs = Vec::new();
    for n in names {
        match registry::vault(ctx, &n) {
            Ok(v) => provs.push(v),
            Err(e) => io.say(Say::Err(format!("{n}: {}", e.message))),
        }
    }
    if provs.is_empty() {
        io.say(Say::Info("No vaults to verify.".into()));
        return 0;
    }
    let mut failed = 0;
    for v in &provs {
        let rows = verify_vault(ctx, v);
        let bad: Vec<&(String, Option<String>)> =
            rows.iter().filter(|(_, e)| e.is_some()).collect();
        failed += bad.len();
        if rows.is_empty() {
            io.say(Say::Info(format!("{}: no references to verify", v.name)));
            continue;
        }
        if bad.is_empty() {
            io.say(Say::Ok(format!(
                "{}: {} reference(s) resolved",
                v.name,
                rows.len()
            )));
            continue;
        }
        io.say(Say::Err(format!(
            "{}: {} of {} reference(s) did NOT resolve",
            v.name,
            bad.len(),
            rows.len()
        )));
        for (k, e) in bad {
            io.out(format!("    {k}: {}\n", e.as_deref().unwrap_or_default()).as_bytes());
        }
    }
    if failed > 0 {
        io.say(Say::Info(
            "A reference can be registered and still not resolve — the vault being reachable says \
             nothing about the item behind the reference."
                .into(),
        ));
        return 1;
    }
    0
}

/// `cmd_vault_remove`: unregister; the file it pointed at stays on disk.
pub fn remove(ctx: &Ctx, name: &str, io: &mut dyn Io) -> i32 {
    // Read before it goes: a keyring vault's items outlive its registration, and registering
    // the name again finds them through the index, which is worth saying.
    let keyring = registry::vault(ctx, name)
        .ok()
        .filter(|v| v.provider == "keyring");
    match registry::remove_vault(ctx, name) {
        Ok(()) => {
            io.say(Say::Ok(format!(
                "Vault '{name}' removed from the registry. (Any underlying file is left on disk \
                 untouched.)"
            )));
            if let Some(v) = keyring {
                io.say(Say::Info(format!(
                    "  Its secrets stay in {}, named in {}; registering '{name}' again as a \
                     keyring vault finds them.",
                    super::keyring::STORE_NAME,
                    super::short_path(&ctx.root, &super::keyring::index_path(ctx, &v))
                )));
            }
            0
        }
        Err(e) => {
            io.say(Say::Err(e.message));
            1
        }
    }
}

#[cfg(test)]
#[path = "vaultcmd_tests.rs"]
mod tests;
