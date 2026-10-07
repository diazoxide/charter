//! **Every guard family reads a hostile command in time proportional to its length** (#1355).
//!
//! Reviews found shapes whose cost grew much faster than the command: substitutions in quotes
//! in substitutions, nested arithmetic, chains of wrappers. The caps in front of the guards
//! ([`purlis_core::guardcaps`]) bound the nesting and the size; inside them, each family must
//! grow no faster than the command does. Four times the input may take a little over four times
//! as long; a reading that grew with its square would take sixteen.
//!
//! Each shape here is built at the caps, so the guards themselves are measured, not the caps'
//! refusal. And the whole verdict, every arm in both modes, is held well inside the hook's
//! budget at the largest size and the deepest nesting a command may have, and one past it, as
//! an optimised build reads it. The hook's budget stands behind all of this in `purlis-cli`.

use std::path::Path;
use std::time::{Duration, Instant};

use purlis_core::forge::{Forge, Kind};
use purlis_core::guardcaps::{MAX_LAYERS, MAX_NESTING};
use purlis_core::handoffguard::Caller;
use purlis_core::toolgate::{self, Call, Launched, Plane};
use purlis_core::{
    commitguard, consentspelling, credguard, floorguard, guardcaps, handoffguard, leakguard,
    planeroot, projectgit, proseguard,
};

/// A hostile shape: its name, and the command at a given size in bytes.
type Shape = (&'static str, Box<dyn Fn(usize) -> String>);

/// A guard family: its name, and a call of it on one command.
type Family<'a> = (&'static str, Box<dyn Fn(&str) + 'a>);

/// `unit` repeated to about `bytes`.
fn fill(unit: &str, bytes: usize) -> String {
    unit.repeat(bytes / unit.len() + 1)
}

/// The hostile shapes, each a function of the command's size in bytes, with substitutions
/// nested `deep` where a shape nests them.
fn shapes(deep: usize) -> Vec<Shape> {
    vec![
        (
            "a chain of wrappers on every line",
            Box::new(|b| {
                fill(
                    &format!("{}ls\n", "eval env nice ".repeat(MAX_LAYERS / 3)),
                    b,
                )
            }),
        ),
        (
            "a shell's -c inside a shell's -c",
            Box::new(|b| fill(&format!("{}ls\n", "bash -c ".repeat(MAX_LAYERS)), b)),
        ),
        (
            "unclosed substitutions on every line",
            Box::new(move |b| fill(&format!("{}ls\n", "echo \"$(".repeat(deep)), b)),
        ),
        (
            "substitutions nested around a long body",
            Box::new(move |b| {
                format!(
                    "{}{}{}",
                    "echo $(".repeat(deep),
                    fill("a ", b),
                    ")".repeat(deep)
                )
            }),
        ),
        (
            "quotes around substitutions, repeated",
            Box::new(move |b| fill(&format!("{}\n", "'\"$(".repeat(deep)), b)),
        ),
        (
            "nested arithmetic",
            Box::new(move |b| {
                fill(
                    &format!("echo {}1{}\n", "$((".repeat(deep), "))".repeat(deep)),
                    b,
                )
            }),
        ),
        (
            "many escaped values",
            Box::new(|b| format!("git -C x commit -m {}", fill("a\\ \\\"b\\' ", b))),
        ),
        (
            "a long run of exports",
            Box::new(|b| {
                let mut s = String::new();
                let mut i = 0;
                while s.len() < b {
                    s += &format!("export GIT_V{i}=x; ");
                    i += 1;
                }
                s + "git checkout feature"
            }),
        ),
        ("many heredocs", Box::new(|b| fill("cat <<A\nx\nA\n", b))),
    ]
}

/// Every family the Bash guard runs, each asked on its own so one that refuses early does not
/// hide a slow one behind it.
fn families(root: &Path) -> Vec<Family<'_>> {
    let forges = vec![
        Forge::default_of(Kind::GitHub),
        Forge::default_of(Kind::GitLab),
    ];
    let caller = Caller {
        agent_id: None,
        harness: Some("claude-code"),
        permission_mode: Some("bypassPermissions"),
    };
    let state = root.join(".charter");
    let r = root.display().to_string();
    vec![
        (
            "the leak guard",
            Box::new(move |c| {
                let _ = leakguard::leak_reason(c, "", &state);
            }),
        ),
        (
            "the one-credential guard",
            Box::new(move |c| {
                let _ = credguard::single_credential_hit(c, &forges);
            }),
        ),
        ("the project-root guards", {
            let r = r.clone();
            Box::new(move |c| {
                let _ = planeroot::plane_root_branch_reason(c, &r, &r);
                let _ = planeroot::plane_root_reset_reason(c, &r, &r);
            })
        }),
        (
            "the release floor",
            Box::new(|c| {
                let _ = floorguard::release_floor_reason(c, true);
            }),
        ),
        (
            "the substitution guards",
            Box::new(|c| {
                let _ = proseguard::forge_substitution_hit(c);
                let _ = proseguard::charter_substitution_hit(c);
            }),
        ),
        (
            "the handoff guard",
            Box::new(move |c| {
                let _ = handoffguard::handoff_refusal(c, caller);
            }),
        ),
        (
            "the consent-spelling guard",
            Box::new(move |c| {
                let _ = consentspelling::refusal(c, root, &[root]);
            }),
        ),
        (
            "the hook-skip guard",
            Box::new(|c| {
                let _ = commitguard::hook_skip_hit(c, "");
            }),
        ),
        (
            "the project-lock guard",
            Box::new(move |c| {
                let _ = projectgit::refusal(c, &r, &r, &root.join("workspaces/x"));
            }),
        ),
    ]
}

#[test]
fn every_guard_family_reads_a_hostile_command_in_time_proportional_to_its_length() {
    purlis_core::unsteered!();
    let dir = tempfile::tempdir().expect("a directory");
    let root = dir.path();
    // A project root with a git directory and a chat folder in it, so the project-lock guard
    // reads every command to its end rather than stopping at a missing folder.
    std::fs::create_dir_all(root.join(".git")).expect("the git directory");
    std::fs::create_dir_all(root.join("workspaces/x")).expect("the chat folder");
    let families = families(root);
    // At the nesting cap: the deepest a command any guard reads may go.
    for (shape, line) in shapes(MAX_NESTING) {
        let (small, large) = (line(4 * 1024), line(16 * 1024));
        // Inside the caps, so it is the guards that are measured.
        assert_eq!(guardcaps::refusal(&large), None, "{shape}");
        for (family, judge) in &families {
            // The faster of two runs each, so a moment of load on the machine is not the measure.
            let time = |cmd: &str| {
                (0..2)
                    .map(|_| {
                        let began = Instant::now();
                        judge(cmd);
                        began.elapsed()
                    })
                    .min()
                    .unwrap_or_default()
            };
            let (t_small, t_large) = (time(&small), time(&large));
            assert!(
                t_large < t_small * 10 + Duration::from_millis(100),
                "{family} on {shape}: 4 KB took {t_small:?}, 16 KB took {t_large:?}"
            );
        }
    }
}

/// The most the whole verdict on one command may take in an optimised build: half the hook's
/// two-second budget. Measured on the slowest shape here (#1355), every arm in an optimised
/// build took about 0.6 s together, the release floor about half of it.
const WHOLE_VERDICT: Duration = Duration::from_secs(1);

/// How much slower the debug build the tests run in reads these shapes than an optimised one:
/// about ten times, measured on the same shapes (#1355), with room for a loaded CI runner. A
/// debug run is held to the optimised bound times this, so a change that made the optimised
/// verdict leave the budget fails here too.
const DEBUG_SLOWDOWN: u32 = 20;

#[test]
fn the_whole_verdict_at_the_largest_and_deepest_command_stays_well_inside_the_budget() {
    purlis_core::unsteered!();
    let dir = tempfile::tempdir().expect("a directory");
    let root = dir.path();
    std::fs::create_dir_all(root.join(".git")).expect("the git directory");
    std::fs::create_dir_all(root.join("workspaces/x")).expect("the chat folder");
    let r = root.display().to_string();
    let state = root.join(".charter");
    let forges = vec![
        Forge::default_of(Kind::GitHub),
        Forge::default_of(Kind::GitLab),
    ];
    let chat_dir = root.join("workspaces/x").display().to_string();
    let plane = Plane {
        root: &r,
        forges: &forges,
        session_dir: &r,
        launched: Launched {
            sandboxed: true,
            chat_dir: Some(&chat_dir),
        },
    };
    // Just under the size cap, so every shape is read rather than refused for its size.
    let size = guardcaps::MAX_COMMAND_BYTES - 4 * 1024;
    let mut slowest = (Duration::ZERO, String::new());
    for deep in [MAX_NESTING, MAX_NESTING + 1] {
        for (shape, line) in shapes(deep) {
            let cmd = line(size);
            assert!(cmd.len() <= guardcaps::MAX_COMMAND_BYTES, "{shape}");
            for mode in ["default", "bypassPermissions"] {
                let call = Call {
                    command: &cmd,
                    cwd: &r,
                    state_dir: &state,
                    caller: Caller {
                        agent_id: None,
                        harness: Some("claude-code"),
                        permission_mode: Some(mode),
                    },
                };
                // The faster of two runs, as the family test takes, so a moment of load on the
                // machine is not the measure.
                let took = (0..2)
                    .map(|_| {
                        let began = Instant::now();
                        let _ = toolgate::verdict(&call, Some(&plane));
                        began.elapsed()
                    })
                    .min()
                    .unwrap_or_default();
                if took > slowest.0 {
                    slowest = (took, format!("{shape}, {deep} deep, {mode}"));
                }
            }
        }
    }
    let (took, which) = slowest;
    let bound = if cfg!(debug_assertions) {
        WHOLE_VERDICT * DEBUG_SLOWDOWN
    } else {
        WHOLE_VERDICT
    };
    assert!(
        took < bound,
        "the slowest whole verdict, on {which}, took {took:?}"
    );
}
