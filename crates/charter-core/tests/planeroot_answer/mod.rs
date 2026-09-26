//! Every answer A3 and A3b stand on, for one recorded plane-root request — the Rust side of the
//! recording `the_plane_root_guards_answer_what_the_python_answers.rs` replays.
//!
//! A request carries every PROBE the recording derived (the operands, the reset targets, the
//! directories), so this has no table of its own to drift from the recording's, and does only
//! what the Rust would do with each. The answer's keys are the recorded answer's keys; a key the
//! Rust answers and the recording lacks is a failure too.
//!
//! This was the `planeroot_oracle` example the Python differential drove over stdin until
//! 2026-09-23, when the recording was frozen and the Python harness retired; its `main` went
//! with the harness, and what is left is the per-request answer the replay has always shared.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use charter_core::gitconfig;
use charter_core::planeroot::{self, OptKind};
use charter_core::pypath;
use charter_core::shellseg;
use charter_core::shellwrap;
use serde_json::{Value, json};

fn strs(v: &Value) -> Vec<String> {
    v.as_array()
        .map(|a| {
            a.iter()
                .map(|x| x.as_str().unwrap_or_default().to_string())
                .collect()
        })
        .unwrap_or_default()
}

fn opt(v: Option<String>) -> Value {
    v.map_or(Value::Null, Value::String)
}

/// The tables, as DATA — the harness's `tables()`, in its order, rotated by the case's `rot`.
fn tables(rot: usize) -> Value {
    let mut known: Vec<&str> = planeroot::GIT_KNOWN_SUBCOMMANDS.to_vec();
    known.sort_unstable();
    let n = rot;
    let rotated: Vec<&str> = (0..4).map(|k| known[(n + k) % known.len()]).collect();
    let mut restore: Vec<&str> = planeroot::RESTORE_OPTS.to_vec();
    restore.sort_unstable();
    let mut shorts: Vec<String> = planeroot::RESTORE_SHORTS
        .chars()
        .map(String::from)
        .collect();
    shorts.sort_unstable();
    let mut creators: Vec<&str> = planeroot::BRANCH_CREATOR_OPTS.to_vec();
    creators.sort_unstable();
    let mut detach: Vec<&str> = planeroot::DETACH_OPTS.to_vec();
    detach.sort_unstable();
    json!([
        planeroot::BRANCH_MOVERS,
        creators,
        detach,
        restore,
        shorts,
        planeroot::MAX_CHECKOUT_OPERANDS,
        planeroot::RESET_TREE_MODES,
        planeroot::GIT_DIR_ENV
            .iter()
            .map(|(k, v)| json!([k, v]))
            .collect::<Vec<_>>(),
        [json!(known.len()), json!(rotated)],
        planeroot::MAX_ALIAS_HOPS,
        gitconfig::MAX_CONFIG_BYTES,
    ])
}

/// `(opts, classes, wants)` the way the branch guard derives them from a `post`.
fn derive(post: &[String]) -> (Vec<String>, Vec<OptKind>, Vec<String>) {
    let opts: Vec<String> = post
        .iter()
        .filter(|a| a.starts_with('-') && *a != "-" && *a != "--")
        .cloned()
        .collect();
    let classes = opts
        .iter()
        .map(|o| planeroot::checkout_opt_kind(o))
        .collect();
    let wants = post
        .iter()
        .filter(|a| *a == "-" || !a.starts_with('-'))
        .cloned()
        .collect();
    (opts, classes, wants)
}

/// Write `text` as `<dir>/.git/config` (or as the `.git` FILE itself) and ask what it names.
fn config_probe(dir: &Path, text: &str, as_pointer: bool) -> Value {
    let _ = std::fs::remove_dir_all(dir);
    std::fs::create_dir_all(dir).expect("scratch is writable");
    if as_pointer {
        std::fs::write(dir.join(".git"), text).expect("scratch is writable");
    } else {
        std::fs::create_dir_all(dir.join(".git")).expect("scratch is writable");
        std::fs::write(dir.join(".git").join("config"), text).expect("scratch is writable");
    }
    opt(gitconfig::configured_work_tree(
        dir.to_str().expect("utf-8 fixture"),
        None,
    ))
}

/// What a git probe answered for one repository, kept for the rest of the replay.
///
/// The fixture repository is built once and nothing in the replay writes to it, so what git says
/// about an operand, a reset target or the default branch of a root is the same on every row that
/// asks. The recording asks often: 5,641 operand probes over 230 distinct ones, 3,413 target
/// probes over 188, and 1,912 default-branch probes over 6 roots, two or more git calls each. Every
/// DISTINCT question still goes to the code under test once and its answer is still compared on
/// every row that recorded it; only the repeats are answered from here. That took the replay, the
/// test every surviving mutant waits out, from about 290 to about 60 thread-seconds (#464).
///
/// The guards themselves (`bra`, `rst`) and alias resolution are never answered from here: those
/// questions are nearly all distinct, and they are the code the recording pins end to end.
#[derive(Default)]
pub struct Probes {
    operand: Mutex<HashMap<(String, String), &'static str>>,
    unpushed: Mutex<HashMap<(String, String), Value>>,
    default_branch: Mutex<HashMap<String, Value>>,
}

/// `key`'s answer from `memo`, asking `ask` the first time. Two threads may both ask a question
/// neither has seen; they get the same answer, so the second write changes nothing.
fn remembered<K, V>(memo: &Mutex<HashMap<K, V>>, key: K, ask: impl FnOnce() -> V) -> V
where
    K: std::hash::Hash + Eq,
    V: Clone,
{
    if let Some(v) = memo.lock().expect("a probe memo").get(&key) {
        return v.clone();
    }
    let v = ask();
    memo.lock().expect("a probe memo").insert(key, v.clone());
    v
}

pub fn answer(case: &Value, scratch: &Path, probes: &Probes) -> Value {
    let cmd = case["cmd"].as_str().expect("cmd");
    let cwd = case["cwd"].as_str().expect("cwd");
    let root_cfg = case["root"].as_str().expect("root");
    let cfg = case["cfg"].as_str().expect("cfg");
    let root = planeroot::plane_root(root_cfg);

    let (segs, _parsed) = shellseg::segment_argv_parsed(cmd);
    let befores = shellwrap::exported_env(&segs);
    let mut gt = Vec::new();
    for (toks, before) in segs.iter().zip(&befores) {
        let (prog, env, argv) = shellwrap::split_env(toks);
        if shellwrap::base_lower(&prog) != "git" {
            continue;
        }
        let (pre, _rest) = shellwrap::git_globals(&argv);
        let mut inherited = before.clone();
        inherited.extend(env);
        let aliases: Vec<Value> = planeroot::inline_aliases(&pre)
            .into_iter()
            .map(|(k, v)| json!([k, v]))
            .collect();
        gt.push(json!([
            planeroot::git_target(cwd, &pre, &inherited),
            aliases
        ]));
    }
    let prg = planeroot::plane_root_git(cmd, cwd, &root);
    let cok: Vec<Value> = strs(&case["opts"])
        .iter()
        .map(|o| json!([o, planeroot::checkout_opt_kind(o).as_str()]))
        .collect();
    let cb: Vec<Value> = prg
        .iter()
        .map(|inv| {
            let (opts, classes, wants) = derive(&inv.post);
            opt(planeroot::created_branch(&opts, &classes, &wants))
        })
        .collect();
    let cwt: Vec<Value> = case["cwt"]
        .as_array()
        .expect("cwt")
        .iter()
        .map(|p| {
            let dir = p[0].as_str().expect("dir");
            opt(gitconfig::configured_work_tree(dir, p[1].as_str()))
        })
        .collect();
    let pp: Vec<Value> = case["pp"]
        .as_array()
        .expect("pp")
        .iter()
        .map(|p| {
            let a = p[0].as_str().expect("a");
            let b = p[1].as_str().expect("b");
            let pa = pypath::pure_path(a);
            json!([pa, pypath::path_div(&pa, b)])
        })
        .collect();
    let pairs: Vec<Value> = gitconfig::pairs(cfg)
        .into_iter()
        .map(|(a, b, c, d)| json!([a, b, c, d]))
        .collect();

    let mut out = json!({
        "tbl": tables(case["rot"].as_u64().expect("rot") as usize),
        "cok": cok,
        "gt": gt,
        "prg": prg.iter().map(|i| json!([i.sub, i.post, i.pre])).collect::<Vec<_>>(),
        "cb": cb,
        "cwt": cwt,
        "pp": pp,
        "psl": gitconfig::py_splitlines(cfg),
        "gcl": gitconfig::logical_lines(cfg),
        "gcp": pairs,
        "gcs": gitconfig::scalar(cfg),
        "ccw": config_probe(scratch, cfg, false),
        "ccp": config_probe(&scratch.with_extension("f"), cfg, true),
    });
    if case["git"].as_bool() == Some(true) {
        let rga: Vec<Value> = prg
            .iter()
            .map(|i| {
                let (sub, post) = planeroot::resolve_git_alias(&root, &i.sub, &i.post, &i.pre);
                json!([sub, post])
            })
            .collect();
        let okind: Vec<Value> = strs(&case["ops"])
            .iter()
            .map(|w| {
                let kind = remembered(&probes.operand, (root.clone(), w.clone()), || {
                    planeroot::checkout_operand_kind(&root, w).as_str()
                });
                json!([w, kind])
            })
            .collect();
        let upr: Vec<Value> = strs(&case["targets"])
            .iter()
            .map(|t| {
                let risk = remembered(&probes.unpushed, (root.clone(), t.clone()), || {
                    match planeroot::unpushed_at_risk(&root, t) {
                        None => Value::Null,
                        Some((n, up)) => json!([n, up]),
                    }
                });
                json!([t, risk])
            })
            .collect();
        let extra = json!({
            "rga": rga,
            "okind": okind,
            "upr": upr,
            "pdb": remembered(&probes.default_branch, root.clone(), || {
                opt(planeroot::default_branch(&root))
            }),
            "bra": opt(planeroot::plane_root_branch_reason(cmd, cwd, root_cfg)),
            "rst": opt(planeroot::plane_root_reset_reason(cmd, cwd, root_cfg)),
        });
        for (k, v) in extra.as_object().expect("object") {
            out[k] = v.clone();
        }
    }
    out
}
