//! The live run (ADR 0070 §7, FG-4): every contract case, through the same seam, against a real
//! forge over the native transport.
//!
//! It is ignored in every `cargo test`; the nightly (`.github/workflows/forge-live.yml`) asks for
//! it with `--ignored`, and only when the operator has provisioned the fixture and its token.
//! What it reads from the environment, per forge (`GITHUB` or `GITLAB`):
//!
//! - `FORGE_LIVE_<FORGE>_TOKEN`: the fixture account's token;
//! - `FORGE_LIVE_<FORGE>_OWNER`: the org or group holding `api` and `web`;
//! - `FORGE_LIVE_<FORGE>_HOST`: the instance, github.com or gitlab.com by default.
//!
//! The scene's numbers and commits are read from the forge when the run starts ([`discover`]),
//! by the harness's own reads of each forge's own fields, not by the backend under test. The
//! fixture itself is described in `crates/charter-core/docs/forges.md`, "The live nightly".

use std::sync::{Arc, OnceLock};

use charter_core::forge::http::{ApiRoot, Http, TokenSource};
use charter_core::forge::route::{HostScope, Resolver, SignIn};
use charter_core::forge::transport::{Call, Field, Method, Transport};
use charter_core::forge::{Account, Caller, Forge, ForgeError, Kind};
use secrecy::SecretString;
use serde_json::Value;

use super::Over;
use super::native::NoCli;
use super::scene::{SAVE, Scene};

/// The branch of the request that landed when the fixture was provisioned.
pub const LANDED: &str = "charter/landed";
/// The title of the issue the `create` case opens; the sweep closes every open one.
pub const ISSUE_TITLE: &str = "Port the picker";

/// The branch `open_or_update` opens its request from on a live forge: not [`SAVE`], whose
/// request's body the `body` case reads.
pub const OPENED_FROM: &str = "charter/open";

/// The environment variable `what` names for `kind`.
pub fn var(kind: Kind, what: &str) -> String {
    format!("FORGE_LIVE_{}_{what}", kind.word().to_ascii_uppercase())
}

fn required(kind: Kind, what: &str) -> String {
    let name = var(kind, what);
    match std::env::var(&name) {
        Ok(v) if !v.trim().is_empty() => v.trim().to_string(),
        _ => panic!(
            "the live contract run needs {name}: it runs against a real forge, and the nightly \
             (.github/workflows/forge-live.yml) sets it from what the operator provisioned \
             (crates/charter-core/docs/forges.md, \"The live nightly\")"
        ),
    }
}

/// The token, read when a request is sent, never held by a caller.
struct FromEnv(String);

impl TokenSource for FromEnv {
    fn token(&self) -> Result<SecretString, ForgeError> {
        std::env::var(&self.0)
            .map(SecretString::from)
            .map_err(|_| ForgeError::transport(format!("{} is not set", self.0)))
    }
}

/// Where the live run asks, and as whom.
struct Target {
    forge: Forge,
    owner: String,
    tokens: Arc<dyn TokenSource>,
}

fn target(kind: Kind) -> Target {
    let token = var(kind, "TOKEN");
    required(kind, "TOKEN");
    let owner = required(kind, "OWNER");
    let host = std::env::var(var(kind, "HOST"))
        .ok()
        .filter(|h| !h.trim().is_empty())
        .unwrap_or_else(|| kind.default_host().to_string());
    let mut forge = Forge::default_of(kind);
    forge.host = host;
    Target {
        forge,
        owner,
        tokens: Arc::new(FromEnv(token)),
    }
}

/// A repo's path, as each forge puts it in a URL: GitLab's escaped as one segment.
fn repo_path(kind: Kind, owner: &str, name: &str) -> String {
    match kind {
        Kind::GitHub => format!("repos/{owner}/{name}"),
        Kind::GitLab => format!("projects/{}", format!("{owner}/{name}").replace('/', "%2F")),
    }
}

fn branch(name: &str) -> String {
    name.replace('/', "%2F")
}

/// The live scene on `kind`, read through `get`, which answers a REST path's JSON. Each forge's
/// own field names are read here, by the harness, so a backend that misreads one disagrees with
/// this rather than with itself.
pub fn discover(
    kind: Kind,
    host: &str,
    owner: &str,
    get: &dyn Fn(&str) -> Result<Value, String>,
) -> Result<Scene, String> {
    let id = |name: &str| -> Result<String, String> {
        let repo = get(&repo_path(kind, owner, name))?;
        match &repo["id"] {
            Value::Number(n) => Ok(n.to_string()),
            other => Err(format!("{owner}/{name} answered no numeric id: {other}")),
        }
    };
    let api = repo_path(kind, owner, "api");
    let first = |path: String, what: &str| -> Result<Value, String> {
        let list = get(&path)?;
        list.as_array()
            .and_then(|l| l.first())
            .cloned()
            .ok_or_else(|| format!("the fixture has no {what} ({path} answered {list})"))
    };
    let text = |v: &Value, field: &str, what: &str| -> Result<String, String> {
        v[field]
            .as_str()
            .map(String::from)
            .ok_or_else(|| format!("{what} has no {field}: {v}"))
    };
    let number = |v: &Value, field: &str, what: &str| -> Result<u64, String> {
        v[field]
            .as_u64()
            .ok_or_else(|| format!("{what} has no {field}: {v}"))
    };
    let open_what = format!("request open from {SAVE} into main");
    let landed_what = format!("request that landed from {LANDED}");
    let (open, head, merged, merge) = match kind {
        Kind::GitHub => {
            let open = first(
                format!(
                    "{api}/pulls?state=open&head={owner}:{}&base=main&per_page=1",
                    branch(SAVE)
                ),
                &open_what,
            )?;
            let landed = first(
                format!(
                    "{api}/pulls?state=closed&head={owner}:{}&per_page=1",
                    branch(LANDED)
                ),
                &landed_what,
            )?;
            if landed["merged_at"].is_null() {
                return Err(format!("the {landed_what} was closed, not merged"));
            }
            (
                number(&open, "number", &open_what)?,
                text(&open["head"], "sha", &open_what)?,
                number(&landed, "number", &landed_what)?,
                text(&landed, "merge_commit_sha", &landed_what)?,
            )
        }
        Kind::GitLab => {
            let open = first(
                format!(
                    "{api}/merge_requests?state=opened&source_branch={}&target_branch=main&per_page=1",
                    branch(SAVE)
                ),
                &open_what,
            )?;
            let landed = first(
                format!(
                    "{api}/merge_requests?state=merged&source_branch={}&per_page=1",
                    branch(LANDED)
                ),
                &landed_what,
            )?;
            (
                number(&open, "iid", &open_what)?,
                text(&open, "sha", &open_what)?,
                number(&landed, "iid", &landed_what)?,
                text(&landed, "merge_commit_sha", &landed_what)
                    .map_err(|e| format!("{e} (it landed without a merge commit)"))?,
            )
        }
    };
    let api_id = id("api")?;
    Ok(Scene {
        kind,
        host: host.to_string(),
        owner: owner.to_string(),
        tree_id: api_id.clone(),
        api_id,
        web_id: id("web")?,
        open,
        head,
        merged,
        merge,
        opened_from: OPENED_FROM.into(),
        opened: None,
        created: None,
        queue: false,
    })
}

/// Close every open issue of `api` labelled `alpha` and titled [`ISSUE_TITLE`], by the numbers
/// the forge's own listing names (GitHub's `number`, GitLab's `iid`), never a pull request, and
/// answer the numbers closed. `create` runs it before and after itself, so an issue a failed run
/// left behind is closed by the next.
pub fn sweep(
    kind: Kind,
    api: &str,
    get: &dyn Fn(&str) -> Result<Value, String>,
    close: &dyn Fn(u64) -> Result<(), String>,
) -> Result<Vec<u64>, String> {
    let (path, number) = match kind {
        Kind::GitHub => (
            format!("{api}/issues?state=open&labels=alpha&per_page=100"),
            "number",
        ),
        Kind::GitLab => (
            format!("{api}/issues?state=opened&labels=alpha&per_page=100"),
            "iid",
        ),
    };
    let listed = get(&path)?;
    let open = listed
        .as_array()
        .ok_or_else(|| format!("{path} answered no list: {listed}"))?;
    let mut closed = Vec::new();
    for issue in open {
        if issue["title"].as_str() != Some(ISSUE_TITLE) || issue.get("pull_request").is_some() {
            continue;
        }
        let n = issue[number]
            .as_u64()
            .ok_or_else(|| format!("an issue with no {number}: {issue}"))?;
        close(n)?;
        closed.push(n);
    }
    Ok(closed)
}

/// A plain `GET` through `http`, as JSON.
fn get(http: &Http, forge: &Forge, path: &str) -> Result<Value, String> {
    let reply = http
        .send(forge, &Call::get(path, charter_core::forge::LIST_TIMEOUT))
        .map_err(|e| format!("{path}: {}", e.said()))?;
    if !reply.ok() {
        return Err(format!("{path}: {}", reply.said(forge.kind)));
    }
    serde_json::from_str(&reply.out).map_err(|e| format!("{path}: not JSON: {e}"))
}

/// The scene on `kind`, read once per run.
fn scene(kind: Kind, target: &Target, http: &Http) -> Scene {
    static GITHUB: OnceLock<Result<Scene, String>> = OnceLock::new();
    static GITLAB: OnceLock<Result<Scene, String>> = OnceLock::new();
    let cell = match kind {
        Kind::GitHub => &GITHUB,
        Kind::GitLab => &GITLAB,
    };
    cell.get_or_init(|| {
        discover(kind, &target.forge.host, &target.owner, &|path| {
            get(http, &target.forge, path)
        })
    })
    .clone()
    .unwrap_or_else(|e| panic!("the live fixture on {}: {e}", target.forge.host))
}

/// `kind`'s backend on the real forge, over the native transport, with the live scene.
pub fn over(kind: Kind) -> Over {
    let target = target(kind);
    let host = target.forge.host.clone();
    let http = Arc::new(
        Http::new(kind, ApiRoot::of(kind, &host), target.tokens.clone())
            .unwrap_or_else(|e| panic!("the native transport to {host}: {e:?}")),
    );
    let scene = scene(kind, &target, &http);
    let account = Account {
        kind,
        host: host.clone(),
        login: "charter-forge-live".into(),
    };
    let resolver = Resolver::new(kind, &host)
        // A live case that reached for the CLI would not be testing the native client.
        .cli_over(Arc::new(NoCli))
        .signed_in(
            &HostScope::for_a_test(),
            account.clone(),
            SignIn {
                tokens: target.tokens.clone(),
                imported_from_cli: false,
            },
        );
    let backend = target.forge.backend_for(Arc::new(resolver));
    let forge = target.forge.clone();
    let api = repo_path(kind, &target.owner, "api");
    Over {
        backend,
        caller: Caller::window().as_account(account),
        host,
        scene,
        check: Box::new(|| {}),
        // The issues `create` opens are closed again, so the fixture does not grow a backlog.
        sweep: Box::new(move || {
            let close = |number: u64| -> Result<(), String> {
                let call = match kind {
                    Kind::GitHub => Call::write(
                        Method::Patch,
                        format!("{api}/issues/{number}"),
                        vec![Field::text("state", "closed")],
                    ),
                    Kind::GitLab => Call::write(
                        Method::Put,
                        format!("{api}/issues/{number}"),
                        vec![Field::text("state_event", "close")],
                    ),
                };
                let reply = http
                    .send(&forge, &call)
                    .map_err(|e| format!("closing issue {number}: {}", e.said()))?;
                if reply.ok() {
                    Ok(())
                } else {
                    Err(format!(
                        "closing issue {number}: {}",
                        reply.said(forge.kind)
                    ))
                }
            };
            sweep(kind, &api, &|path| get(&http, &forge, path), &close)
        }),
    }
}
