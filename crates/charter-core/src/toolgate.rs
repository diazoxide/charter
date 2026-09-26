//! The Bash guard, assembled: eight arms, one verdict, in the order `pretooluse` runs them.
//!
//! A port of `charter/hooks.py:pretooluse`'s REFUSALS — the arms and the order alone, with
//! nothing that writes. Each arm already stands on its own: [`crate::leakguard`] (A),
//! [`crate::credguard`] (A2), [`crate::planeroot`] (A3, A3b), [`crate::floorguard`] (A4),
//! [`crate::proseguard`] (A5, A6) and [`crate::handoffguard`] (A7). What is here is the one
//! thing none of them could carry: **which of two firing arms the chat is told about**.
//!
//! # The name is Python's other module, and that is worth knowing
//!
//! `charter/toolgate.py` is the persona tool-gate: an ALLOW-only gate that reads a persona's
//! declared tools and tells the harness not to prompt. It is the last thing `pretooluse` does,
//! after every refusal here, and **it is not ported**. This module is the refusals, and it
//! takes the name because in this binary the refusals are what a tool call meets. The gap is
//! stated rather than hidden: an operator running this charter gets every denial and no
//! smoothing, which is the safe half to have first — `toolgate.py`'s own promise is that "a
//! bug here can't block work, only fail to smooth it", and the inverse is true of its absence.
//!
//! # The order is MEASURED, not chosen
//!
//! Every pair of arms that can fire on one command line has a decided winner, and the Python
//! comments say why for each. The three that are arguments rather than accidents:
//!
//! * **A (the leak guard) is first and is ungated.** Not printing a secret into the transcript
//!   is a safety invariant, not a policy a plane happens to hold. `pretooluse` has carried that
//!   sentence in a comment since 0.42.
//! * **A6 runs after A5**, so a line that is both — `charter …` piped into `gh issue create` —
//!   is explained by the guard that publishes to a forge.
//! * **A7 runs after A6**, so a line that also persists prose through a live substitution is
//!   explained by that guard; and A7 is GATED where A5 and A6 beside it are not, because a
//!   handoff opens a chat in one of this plane's workspaces and outside a plane there is no
//!   such chat to consent to.
//!
//! # The plane gate, read once
//!
//! `_in_a_plane` (charter#852) divides the arms: A2, A3, A3b, A4 and A7 are about a control
//! plane and are silent without one, while A, A5 and A6 are facts about the shell and run in
//! any directory. Outside a plane the gated arms denied in every unrelated repository on the
//! machine, explaining a control plane that does not exist there.
//!
//! Here the gate is the TYPE: [`Plane`] carries the three things the gated arms need — the
//! root, the forge list and the state directory — and [`verdict`] takes it as an `Option`. An
//! arm cannot be reached without the facts it judges against, so the gate cannot be forgotten
//! at one call site the way `config.HAS_CONTROL_PLANE` was forgotten at six.
//!
//! # What is NOT here, and is a declared gap
//!
//! `pretooluse` also does bookkeeping — `_record_reported_session`, `_mark_guard_seen`,
//! `_touch_piece`, `_turn_bump`, `_memnudge_reset`, `_route_mark_clear` and a `_trace` row per
//! verdict. None of it changes a verdict and none of it is ported; this module is pure, takes
//! no clock and writes nothing. [`Verdict::reason`] and [`Verdict::shape`] are the two fields a
//! trace row would carry, kept so the port can be compared on them and so a later stage that
//! writes traces has nothing to re-derive.

use std::path::Path;

use crate::forge::Forge;
use crate::handoffguard::{self, Caller};
use crate::{credguard, floorguard, leakguard, planeroot, proseguard, pyjson};

/// What to do when a guard is WRONG about your case (charter#370) — `_OVERRIDE_NOTE`.
///
/// Every denial named a remedy for the workflow the operator was supposed to be doing, and none
/// named this. Appended to every arm's sentence by [`said`], which is why it is spelled once.
pub const OVERRIDE_NOTE: &str = " — Wrong about this case? There is deliberately no config key, environment variable or \
     switch that lifts a charter denial: one charter could read is one a committed file could \
     flip. Run it yourself, in your own terminal — these guards bound what an AGENT does with \
     your authority, never what you do. See `docs/hooks.md` → When a guard is wrong.";

/// The event word a `PreToolUse` verdict names back to the harness.
pub const EVENT: &str = "PreToolUse";

/// The trace reason each arm is tallied under. Python's `_trace(… reason=…)` literals, which
/// are the stable keys a tally reader already has — `reason` for the leak guard is the
/// denial's own first 70 characters, so it has no constant here.
pub const REASON_SINGLE_CREDENTIAL: &str = "single-credential";
/// See [`REASON_SINGLE_CREDENTIAL`].
pub const REASON_PLANE_ROOT_BRANCH: &str = "plane-root-branch";
/// See [`REASON_SINGLE_CREDENTIAL`].
pub const REASON_PLANE_ROOT_RESET: &str = "plane-root-reset";
/// See [`REASON_SINGLE_CREDENTIAL`].
pub const REASON_RELEASE_FLOOR: &str = "release-floor";
/// See [`REASON_SINGLE_CREDENTIAL`].
pub const REASON_FORGE_SUBSTITUTION: &str = "forge-substitution";
/// See [`REASON_SINGLE_CREDENTIAL`].
pub const REASON_CHARTER_SUBSTITUTION: &str = "charter-substitution";

/// How much of a leak denial becomes its trace reason — Python's `reason=leak[:70]`.
///
/// CHARACTERS, because the Python slices a `str`. A denial is prose and the tally key is a
/// prefix of it, which is not a key anybody chose; it is reproduced because a tally reader
/// already groups on it.
pub const LEAK_REASON_CHARS: usize = 70;

/// The tool call, as the hook's payload describes it.
#[derive(Debug, Clone, Copy)]
pub struct Call<'a> {
    /// `tool_input.command`. Python's `ti.get("command", "") or ""`, so an absent or null
    /// command is the empty string and every arm is still asked.
    pub command: &'a str,
    /// The payload's `cwd`, or `""`. What A, A3 and A3b judge a path against.
    pub cwd: &'a str,
    /// Where charter's machine-local state lives — `config.STATE_DIR`. The leak guard walks it
    /// and it exists outside a plane too, which is why it is here and not on [`Plane`].
    pub state_dir: &'a Path,
    /// The two payload fields and the harness word that no command can observe.
    pub caller: Caller<'a>,
}

/// The control plane the gated arms judge against — the presence of this IS `_in_a_plane`.
#[derive(Debug, Clone, Copy)]
pub struct Plane<'a> {
    /// The plane root. A3 and A3b ask whether a git write lands in its shared working tree.
    pub root: &'a str,
    /// Every host the one-credential policy covers, **in Python's dict order**: A2 reports the
    /// FIRST host whose `git@<host>` is in the argv, so a sorted list is a different answer.
    pub forges: &'a [Forge],
}

/// A refused tool call: what the chat is told, and what a tally would record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verdict {
    /// The trace key — one of the `REASON_*` constants, [`handoffguard`]'s own, or the leak
    /// denial's first [`LEAK_REASON_CHARS`] characters.
    pub reason: String,
    /// Which trigger matched, where the arm reports one (charter#289). `None` for the arms
    /// whose `_trace` row carries no `shape`.
    pub shape: Option<String>,
    /// The arm's sentence, **without** [`OVERRIDE_NOTE`] — [`said`] adds that.
    pub denial: String,
}

impl Verdict {
    fn new(reason: impl Into<String>, shape: Option<String>, denial: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
            shape,
            denial: denial.into(),
        }
    }

    /// What the harness shows the chat — `_deny`'s `f"charter guard: {reason}{_OVERRIDE_NOTE}"`.
    pub fn said(&self) -> String {
        said(&self.denial)
    }

    /// The whole answer a `PreToolUse` hook writes on stdout, as `json.dumps` writes it.
    ///
    /// [`crate::pyjson::dumps`] with Python's own separators and `ensure_ascii`, so the bytes
    /// are the frozen charter's bytes: the denials carry `—` and `…`, which Python escapes and
    /// `serde_json` would not, and the differential compares stdout byte for byte.
    pub fn emitted(&self) -> String {
        pyjson::dumps(
            &serde_json::json!({
                "hookSpecificOutput": {
                    "hookEventName": EVENT,
                    "permissionDecision": "deny",
                    "permissionDecisionReason": self.said(),
                }
            }),
            None,
            ", ",
            ": ",
        )
    }
}

/// `_deny`'s sentence: charter's own prefix, the arm's reason, and the override note.
pub fn said(reason: &str) -> String {
    format!("charter guard: {reason}{OVERRIDE_NOTE}")
}

/// The refusal this tool call earns, or `None` — `charter/hooks.py:pretooluse`'s eight arms in
/// its order.
///
/// `plane` absent is `_in_a_plane()` answering no: A2, A3, A3b, A4 and A7 are skipped and A, A5
/// and A6 still run. The module header argues that division at length.
pub fn verdict(call: &Call<'_>, plane: Option<&Plane<'_>>) -> Option<Verdict> {
    let cmd = call.command;

    // A: a secret would leak into the conversation → hard DENY (a real safety invariant).
    // UNGATED, and that is the one arm whose gating was never in question.
    if let Some(leak) = leakguard::leak_reason(cmd, call.cwd, call.state_dir) {
        // Python's `reason=leak[:70]` — the tally key is a PREFIX of the prose, which nobody
        // chose and which a tally reader already groups on.
        let reason: String = leak.chars().take(LEAK_REASON_CHARS).collect();
        return Some(Verdict::new(reason, None, leak));
    }

    if let Some(plane) = plane {
        // A2: golden rule — one credential (each forge's token over HTTPS); no SSH, no signing.
        //
        // The HIT rather than [`credguard::single_credential_reason`], because the trace row
        // wants the shape as well and asking twice would walk the command line twice. The prose
        // is that function's, spelled here — which is the one thing charter#289 says must not
        // drift, so `the_golden_rules_two_answers_cannot_drift_apart` below pins the pair.
        if let Some((shape, detail)) = credguard::single_credential_hit(cmd, plane.forges) {
            return Some(Verdict::new(
                REASON_SINGLE_CREDENTIAL,
                Some(shape),
                format!("{}{detail}", credguard::SINGLE_CREDENTIAL_FIX),
            ));
        }
        // A3: the plane root is one shared working tree — refuse a branch move in it (#157).
        if let Some(branch) = planeroot::plane_root_branch_reason(cmd, call.cwd, plane.root) {
            return Some(Verdict::new(REASON_PLANE_ROOT_BRANCH, None, branch));
        }
        // A3b: and refuse a `git reset` in the root that would destroy commits no remote has
        // (#401). A separate guard from A3: different prose, different remedy, and this one
        // only speaks when it has measured that something really would be lost.
        if let Some(wipe) = planeroot::plane_root_reset_reason(cmd, call.cwd, plane.root) {
            return Some(Verdict::new(REASON_PLANE_ROOT_RESET, None, wipe));
        }
        // A4: an unattended run may not publish (#299).
        let unattended = floorguard::unattended(call.caller.permission_mode);
        if let Some(pub_) = floorguard::release_floor_reason(cmd, unattended) {
            return Some(Verdict::new(REASON_RELEASE_FLOOR, None, pub_));
        }
    }

    // A5: a forge command that publishes prose may not carry a live command substitution
    // (#703). UNGATED, with the leak guard and for its reason: what it refuses is a fact about
    // the SHELL, not a policy this plane happens to hold.
    if let Some((spelling, why)) = proseguard::forge_substitution_hit(cmd) {
        return Some(Verdict::new(
            REASON_FORGE_SUBSTITUTION,
            Some(spelling.to_string()),
            why,
        ));
    }
    // A6: and the same rule on charter's OWN text-taking commands (#778). AFTER A5 so that a
    // line which is both is explained by the one that publishes to a forge.
    if let Some((spelling, why)) = proseguard::charter_substitution_hit(cmd) {
        return Some(Verdict::new(
            REASON_CHARTER_SUBSTITUTION,
            Some(spelling.to_string()),
            why,
        ));
    }
    // A7: a `charter handoff` the operator's permission prompt cannot stand in front of.
    // GATED, unlike A5 and A6 beside it.
    if plane.is_some()
        && let Some((reason, why)) = handoffguard::handoff_refusal(cmd, call.caller)
    {
        // No `cmd` on this trace row, the one arm without it: a handoff's command line carries
        // its brief, and keeping every field of that line out of the tally is simpler to hold
        // than deciding which part of it is safe. Nothing here holds a command anyway.
        return Some(Verdict::new(reason, None, why));
    }
    // Everything below this line in the Python is a WRITE or the persona tool-gate's ALLOW,
    // and neither is ported. See the module header.
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::forge::{Forge, Kind};
    use std::path::PathBuf;

    fn forges() -> Vec<Forge> {
        vec![
            Forge::default_of(Kind::GitHub),
            Forge::default_of(Kind::GitLab),
        ]
    }

    struct Fixture {
        dir: tempfile::TempDir,
    }

    impl Fixture {
        /// A plane with a vault in it, so the leak guard has something real to refuse.
        fn new() -> Self {
            let dir = tempfile::tempdir().expect("a directory");
            let vaults = dir.path().join(".charter").join("vaults");
            std::fs::create_dir_all(&vaults).expect("the vault directory");
            std::fs::write(vaults.join("db.json"), "{}\n").expect("a vault");
            Self { dir }
        }

        fn root(&self) -> String {
            self.dir.path().display().to_string()
        }

        fn state(&self) -> PathBuf {
            self.dir.path().join(".charter")
        }
    }

    /// An attended chat in the main conversation — the caller every arm but A4 and A7's two
    /// payload rows is indifferent to.
    fn attended() -> Caller<'static> {
        Caller {
            agent_id: None,
            harness: Some("claude-code"),
            permission_mode: Some("default"),
        }
    }

    fn verdict_of(cmd: &str, fix: &Fixture, in_a_plane: bool) -> Option<Verdict> {
        let root = fix.root();
        let forges = forges();
        let state = fix.state();
        let plane = Plane {
            root: &root,
            forges: &forges,
        };
        let call = Call {
            command: cmd,
            cwd: "",
            state_dir: &state,
            caller: attended(),
        };
        verdict(&call, in_a_plane.then_some(&plane))
    }

    #[test]
    fn an_ordinary_command_earns_no_verdict() {
        let fix = Fixture::new();
        assert_eq!(verdict_of("git status", &fix, true), None);
    }

    #[test]
    fn the_leak_guard_runs_without_a_plane_and_the_golden_rule_does_not() {
        let fix = Fixture::new();
        let leak = "cat .charter/vaults/db.json";
        // A is a fact about the shell and a secret: it refuses in any directory.
        assert!(verdict_of(leak, &fix, false).is_some(), "A is ungated");
        assert!(verdict_of(leak, &fix, true).is_some());

        // A2 is a policy about a plane, and outside one it denied in every unrelated
        // repository on the machine (charter#852).
        let ssh = "git clone git@github.com:o/r.git";
        assert_eq!(verdict_of(ssh, &fix, false), None, "A2 is gated");
        assert_eq!(
            verdict_of(ssh, &fix, true).map(|v| v.reason),
            Some(REASON_SINGLE_CREDENTIAL.to_string())
        );
    }

    #[test]
    fn the_handoff_guard_is_gated_where_the_two_prose_guards_are_not() {
        let fix = Fixture::new();
        let hand = "charter handoff beta";
        assert_eq!(verdict_of(hand, &fix, false), None, "A7 is gated");
        assert_eq!(
            verdict_of(hand, &fix, true).map(|v| v.reason),
            Some(handoffguard::REASON_BRIEF_SOURCE.to_string())
        );

        // A6 is not: a live substitution in charter's own prose command refuses with no plane.
        let prose = "charter persona remember devops \"$(cat /etc/hosts)\"";
        assert_eq!(
            verdict_of(prose, &fix, false).map(|v| v.reason),
            Some(REASON_CHARTER_SUBSTITUTION.to_string()),
            "A6 is ungated"
        );
    }

    #[test]
    fn the_forge_guard_explains_a_line_that_is_both_it_and_charters_own() {
        let fix = Fixture::new();
        // A5 before A6, so the guard that publishes to a FORGE is the one that speaks.
        let both = "charter persona remember devops \"$(x)\" && gh issue create --body \"$(x)\"";
        assert_eq!(
            verdict_of(both, &fix, true).map(|v| v.reason),
            Some(REASON_FORGE_SUBSTITUTION.to_string())
        );
    }

    #[test]
    fn an_unattended_publish_is_refused_and_an_attended_one_is_not() {
        let fix = Fixture::new();
        let root = fix.root();
        let forges = forges();
        let plane = Plane {
            root: &root,
            forges: &forges,
        };
        let state = fix.state();
        let mut c = Call {
            command: "gh release create v1.0.0",
            cwd: "",
            state_dir: &state,
            caller: attended(),
        };
        assert_eq!(verdict(&c, Some(&plane)), None, "attended is untouched");
        c.caller.permission_mode = Some(floorguard::UNATTENDED_MODE);
        assert_eq!(
            verdict(&c, Some(&plane)).map(|v| v.reason),
            Some(REASON_RELEASE_FLOOR.to_string())
        );
    }

    #[test]
    fn the_golden_rules_two_answers_cannot_drift_apart() {
        // This module rebuilds A2's prose from the HIT so it can carry the shape too, and
        // `credguard::single_credential_reason` builds the same prose from the same hit. Two
        // spellings of one sentence is exactly the defect charter#289 named — the traced shape
        // and the prose disagreeing about what matched — so the pair is pinned rather than
        // trusted, and a change to either side that forgets the other goes red here.
        let fix = Fixture::new();
        let forges = forges();
        for cmd in [
            "git clone git@github.com:o/r.git",
            "git commit -S -m x",
            "ssh -T git@github.com",
            "git -c core.sshCommand=/tmp/k push",
            "git config core.sshCommand /tmp/k",
        ] {
            let mine = verdict_of(cmd, &fix, true).unwrap_or_else(|| panic!("{cmd:?} is refused"));
            assert_eq!(mine.reason, REASON_SINGLE_CREDENTIAL, "{cmd:?}");
            assert_eq!(
                Some(mine.denial.as_str()),
                credguard::single_credential_reason(cmd, &forges).as_deref(),
                "{cmd:?}: the assembled denial is not the arm's own",
            );
        }
    }

    #[test]
    fn a_leak_denials_trace_key_is_its_first_seventy_characters() {
        let fix = Fixture::new();
        let v = verdict_of("cat .charter/vaults/db.json", &fix, true).expect("refused");
        assert_eq!(v.reason.chars().count(), LEAK_REASON_CHARS);
        assert!(v.denial.starts_with(&v.reason));
        assert_eq!(v.shape, None);
    }

    /// Every guard reads the words the SHELL makes, through one reader, so a quoting form that
    /// reader learns is one every arm learns at once. Asked here at the entry the hook calls.
    #[test]
    fn a_program_the_shell_spells_out_of_quoting_is_the_program_to_every_arm() {
        let fix = Fixture::new();
        let vault = format!(".charter/{}/db.json", "vaults");
        let leak = |cmd: String| verdict_of(&cmd, &fix, true).map(|v| v.reason);
        let read = verdict_of(&format!("cat {vault}"), &fix, true).map(|v| v.reason);
        assert!(read.is_some());
        for cmd in [
            // ANSI-C escapes in the program name, in hex, octal and as a character.
            format!("$'\\x63at' {vault}"),
            format!("$'\\143at' {vault}"),
            format!(concat!("$'\\", "u0063at' {}"), vault),
            // bash's locale string.
            format!("$\"cat\" {vault}"),
            // A backslash-newline inside the name, and between `$` and `(`.
            format!("c\\\nat {vault}"),
            format!("echo $\\\n(cat {vault})"),
            // bash 5.3's `${ …; }`.
            format!("echo ${{ cat {vault}; }}"),
        ] {
            assert_eq!(leak(cmd.clone()), read, "{cmd:?}");
        }
        for cmd in [
            "$'\\x67'h issue create --body \"$(x)\"",
            "gh issue create --body \"$\\\n(x)\"",
            "gh issue create --body \"${ x; }\"",
        ] {
            assert_eq!(
                verdict_of(cmd, &fix, false).map(|v| v.reason),
                Some(REASON_FORGE_SUBSTITUTION.to_string()),
                "{cmd:?}"
            );
        }
        assert_eq!(
            verdict_of(
                "$'\\x63'harter persona remember devops \"$(x)\"",
                &fix,
                false
            )
            .map(|v| v.reason),
            Some(REASON_CHARTER_SUBSTITUTION.to_string())
        );
        assert_eq!(
            verdict_of("BASH -c 'charter handoff beta'", &fix, true).map(|v| v.reason),
            Some(handoffguard::REASON_SHELL_STRING.to_string())
        );
        assert_eq!(
            verdict_of("charter $'\\x68'andoff beta <<'B'\nx\nB", &fix, true).map(|v| v.reason),
            Some(handoffguard::REASON_SPELLING.to_string())
        );
    }

    #[test]
    fn the_emitted_json_is_pythons_bytes() {
        let v = Verdict::new("r", None, "a — b …");
        let out = v.emitted();
        // Python's separators, and `ensure_ascii`, which is its default: the em dash and the
        // ellipsis are escaped. A `serde_json::to_string` would pack the separators and write
        // both characters literally, and the differential compares stdout byte for byte.
        assert!(out.starts_with(r#"{"hookSpecificOutput": {"hookEventName": "PreToolUse", "#));
        assert!(out.contains(r#""permissionDecision": "deny""#));
        assert!(out.contains("a \\u2014 b \\u2026"), "{out}");
        assert!(out.contains("charter guard: "));
        assert!(
            out.contains("no config key"),
            "the override note rides along"
        );
    }
}
