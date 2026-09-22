//! The Rust side of the `planeroot` differential run: a case in, every answer A3 and A3b stand on
//! out.
//!
//! One JSON object per line of stdin, one JSON object per line of stdout, in the same order. The
//! harness (`tests/differential/planeroot.py`) builds the git-repository fixture, writes the
//! cases, and derives every PROBE a case carries (the operands, the reset targets, the
//! directories) — so this program has no table of its own to drift from the harness's, and does
//! only what the Rust would do with each.
//!
//! **Cases run on several threads.** Four of the answers ask a real git, and a git process costs
//! milliseconds where everything else here costs microseconds; each case is independent, and the
//! output order is the input order. The one piece of shared state is the scratch directory the
//! config probes write into, which is per thread.
//!
//! An EXAMPLE rather than a binary because nothing ships it; the guards it reports on are wired to
//! nothing yet either way.

use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use charter_core::gitconfig;
use charter_core::planeroot::{self, OptKind};
use charter_core::pypath;
use charter_core::shellseg;
use charter_core::shellwrap;
use serde_json::{Value, json};

fn main() {
    let base =
        PathBuf::from(std::env::var("PLANEROOT_BASE").expect("PLANEROOT_BASE names the fixture"));
    let mut input = String::new();
    std::io::stdin()
        .read_to_string(&mut input)
        .expect("stdin is readable");
    let cases: Vec<Value> = input
        .split('\n')
        .filter(|l| !l.is_empty())
        .map(|l| serde_json::from_str(l).expect("each line is one JSON object"))
        .collect();
    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .clamp(1, 16);
    let chunk = cases.len().div_ceil(threads).max(1);
    let answers: Vec<Value> = std::thread::scope(|s| {
        let handles: Vec<_> = cases
            .chunks(chunk)
            .enumerate()
            .map(|(t, part)| {
                let scratch = base.join(format!("cfgrs-{t}"));
                s.spawn(move || part.iter().map(|c| answer(c, &scratch)).collect::<Vec<_>>())
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().expect("a worker thread finished"))
            .collect()
    });
    let stdout = std::io::stdout();
    let mut out = BufWriter::new(stdout.lock());
    for a in answers {
        writeln!(out, "{a}").expect("stdout is writable");
    }
    out.flush().expect("stdout is writable");
}

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

pub fn answer(case: &Value, scratch: &Path) -> Value {
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
        if shellwrap::basename(&prog) != "git" {
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
            .map(|w| json!([w, planeroot::checkout_operand_kind(&root, w).as_str()]))
            .collect();
        let upr: Vec<Value> = strs(&case["targets"])
            .iter()
            .map(|t| match planeroot::unpushed_at_risk(&root, t) {
                None => json!([t, null]),
                Some((n, up)) => json!([t, [n, up]]),
            })
            .collect();
        let extra = json!({
            "rga": rga,
            "okind": okind,
            "upr": upr,
            "pdb": opt(planeroot::default_branch(&root)),
            "bra": opt(planeroot::plane_root_branch_reason(cmd, cwd, root_cfg)),
            "rst": opt(planeroot::plane_root_reset_reason(cmd, cwd, root_cfg)),
        });
        for (k, v) in extra.as_object().expect("object") {
            out[k] = v.clone();
        }
    }
    out
}
