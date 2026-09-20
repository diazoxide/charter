//! `charter discover`: refresh `inventory/repos.json` from every forge the plane declares,
//! then regenerate `docs/topology.md`. Python's `cmd_discover` and `cmd_docs`.
//!
//! **Nothing is saved until every forge has answered.** A forge that fails refuses the whole
//! command before the inventory is touched, so a partial multi-forge failure can never wipe
//! or half-write the file — the discipline Python's `_api_strict` split exists for. A failed
//! stack PROBE is different: the stack is descriptive, so the inventory is still written,
//! and the failure is said rather than masquerading as "no recognised build file".

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::Value;

use super::{Say, Sink};
use crate::forge::{self, Forge, py_str};
use crate::inventory;

/// How many stack probes run at once. Python's `_build_batch` worker count.
const PROBES: usize = 8;

/// `--no-probe` and `--no-docs`.
#[derive(Debug, Clone, Copy, Default)]
pub struct Options {
    pub no_probe: bool,
    pub no_docs: bool,
}

/// Run `discover` against the plane at `root`. Returns the exit status.
pub fn discover(root: &Path, options: Options, say: Sink) -> u8 {
    let cfg = match forge::load_config(root) {
        Ok(cfg) => cfg,
        Err(why) => {
            say(Say::Plain(why));
            return 1;
        }
    };
    let to_query = match forge::to_query(&cfg) {
        Ok(q) => q,
        Err(why) => {
            say(Say::Plain(why));
            return 1;
        }
    };

    let mut batches: Vec<Vec<Value>> = Vec::new();
    let mut probe_failures = 0usize;
    for (forge, owner, exclude) in &to_query {
        say(Say::Info(format!(
            "Querying {} {} `{owner}` …",
            forge.kind.word(),
            forge.kind.owner_noun()
        )));
        if let Err(why) = forge.check_auth() {
            say(Say::Plain(why.0));
            return 1;
        }
        let projects: Vec<Value> = match forge.list_repos(owner) {
            Ok(all) => all
                .into_iter()
                .filter(|p| {
                    let name = p.get("name").and_then(Value::as_str).unwrap_or_default();
                    !exclude.iter().any(|e| e == name)
                })
                .collect(),
            Err(why) => {
                say(Say::Plain(why.0));
                return 1;
            }
        };
        say(Say::Info(format!(
            "Found {} project(s) on {}. {}",
            projects.len(),
            forge.kind.word(),
            if options.no_probe {
                "Skipping stack probe."
            } else {
                "Probing repo stacks …"
            }
        )));
        let (records, failed) = build_batch(forge, &projects, options.no_probe);
        probe_failures += failed;
        batches.push(records);
    }

    let merged = match inventory::merge(&batches) {
        Ok(merged) => merged,
        Err(why) => {
            say(Say::Plain(why));
            return 1;
        }
    };

    let group = forge::group_of(&cfg, 0);
    let previous = match inventory::load(root, &group) {
        Ok(doc) => doc,
        Err(why) => {
            say(Say::Plain(why));
            return 1;
        }
    };
    let before: BTreeSet<String> = inventory::repos(root, &previous, &forge::exclude_of(&cfg, 0))
        .iter()
        .map(|r| py_str(r.get("name").unwrap_or(&Value::Null)))
        .collect();
    let doc = match inventory::save(root, &group, &merged) {
        Ok(doc) => doc,
        Err(why) => {
            say(Say::Plain(why));
            return 1;
        }
    };
    let after: BTreeSet<String> = merged
        .iter()
        .map(|r| py_str(r.get("name").unwrap_or(&Value::Null)))
        .collect();

    say(Say::Done(format!(
        "Wrote {} repos to inventory/repos.json",
        doc["count"]
    )));
    if probe_failures > 0 {
        say(Say::Warn(format!(
            "⚠ stack probe FAILED for {probe_failures} repo(s) — their `stack` was written as \
             \"unknown\" because the probe itself errored (network/auth/a GitHub secondary rate \
             limit), not because they lack a recognised build file. Re-run `charter discover` \
             to re-probe; if it keeps failing, check `charter doctor` (forge auth) or wait out \
             the rate-limit window."
        )));
    }
    let added: Vec<&String> = after.difference(&before).collect();
    let removed: Vec<&String> = before.difference(&after).collect();
    if !added.is_empty() {
        say(Say::Info(format!("New in group: {}", join(&added))));
    }
    if !removed.is_empty() {
        say(Say::Warn(format!("No longer in group: {}", join(&removed))));
    }

    if !options.no_docs {
        // The status is deliberately dropped, as Python's `cmd_discover` drops `cmd_docs`'s:
        // a discover that wrote the inventory succeeded, whatever the docs half could not
        // regenerate afterwards. `charter docs generate` is where that status is the answer.
        super::docs::docs(root, &group, say);
    }
    0
}

fn join(names: &[&String]) -> String {
    names
        .iter()
        .map(|s| s.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Every project as its inventory record, probing stacks eight at a time. Returns the
/// records in the order the forge listed them, and how many probes FAILED — as opposed to
/// found nothing.
fn build_batch(forge: &Forge, projects: &[Value], no_probe: bool) -> (Vec<Value>, usize) {
    if no_probe {
        let records = projects
            .iter()
            .map(|p| inventory::record(forge, p, "unknown"))
            .collect();
        return (records, 0);
    }
    let stacks: Mutex<Vec<Option<Result<&'static str, ()>>>> =
        Mutex::new(vec![None; projects.len()]);
    let next = AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for _ in 0..PROBES.min(projects.len()) {
            scope.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::SeqCst);
                    let Some(project) = projects.get(i) else {
                        break;
                    };
                    let git_ref = project.get("default_branch").and_then(Value::as_str);
                    let found = forge
                        .repo_tree_strict(project, git_ref)
                        .map(|files| inventory::classify_stack(&files))
                        .map_err(|_| ());
                    if let Ok(mut slots) = stacks.lock() {
                        slots[i] = Some(found);
                    }
                }
            });
        }
    });
    let stacks = stacks.into_inner().unwrap_or_default();
    let mut failed = 0;
    let records = projects
        .iter()
        .zip(stacks)
        .map(|(project, stack)| {
            let stack = match stack {
                Some(Ok(stack)) => stack,
                _ => {
                    failed += 1;
                    "unknown"
                }
            };
            inventory::record(forge, project, stack)
        })
        .collect();
    (records, failed)
}
