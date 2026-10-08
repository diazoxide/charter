//! Dispatch limits (#1439, #1440): which level's value is in force, what each limit refuses,
//! and the sentences a refused dispatch is told.

use super::*;

const FILE: &str = "charter.toml";

fn table(text: &str) -> Table {
    let read = read(Some(text), FILE);
    assert_eq!(read.refused, Vec::<String>::new(), "{text}");
    read.table
}

fn nothing() -> Table {
    Table::default()
}

fn no_policy() -> Level {
    Level::unset()
}

/// The limits for a `steward` chat in `alpha` dispatching to `devops`, as `project` alone says.
fn committed(project: &str) -> Limits {
    in_force(
        &table(project),
        Some("alpha"),
        Some("steward"),
        Some("devops"),
        &nothing(),
        &no_policy(),
    )
}

/// A lineage with room under every default.
fn quiet() -> Lineage {
    Lineage {
        depth: 0,
        chain: Vec::new(),
        running: 0,
        lineage: 1,
        as_target: 0,
        by_asking: 0,
        tokens: 0,
    }
}

/// A chat with `above` over it, nearest first: as deep as there are chats above it.
fn under(above: &[&str]) -> Lineage {
    Lineage {
        depth: u32::try_from(above.len()).expect("a depth"),
        chain: above.iter().map(|one| Some((*one).to_owned())).collect(),
        ..quiet()
    }
}

fn refused_of(decision: &Decision) -> &Refused {
    decision.refused().expect("a refusal")
}

fn sentence_of(decision: &Decision) -> String {
    refused_of(decision).say()
}

fn limit_of(decision: &Decision) -> Option<Limit> {
    refused_of(decision).limit()
}

// ---- which level is in force ---------------------------------------------------------------------

#[test]
fn a_project_that_sets_nothing_has_the_defaults_and_no_persona_caps() {
    let limits = committed("");
    assert_eq!(limits.running, 6);
    assert_eq!(limits.lineage, 16);
    assert_eq!(limits.depth, 3);
    assert_eq!(limits.messages_per_minute, 10);
    assert_eq!(limits.may_dispatch, None);
    assert_eq!(limits.may_run_at_once, None);
    for limit in Limit::ALL {
        assert_eq!(limits.set_by(limit), &Source::Default, "{limit:?}");
    }
}

#[test]
fn the_projects_own_value_replaces_the_default() {
    let limits = committed("[dispatch]\nrunning-per-chat = 4\n");
    assert_eq!(limits.running, 4);
    assert_eq!(limits.set_by(Limit::RunningPerChat), &Source::Project);
    // What it does not set stays the default.
    assert_eq!(limits.depth, 3);
    assert_eq!(limits.set_by(Limit::Depth), &Source::Default);
}

#[test]
fn a_workspaces_value_wins_over_the_projects_and_only_in_that_workspace() {
    let project = "[dispatch]\nrunning-per-chat = 4\n\n[dispatch.workspaces.alpha]\n\
                   running-per-chat = 12\n";
    let limits = committed(project);
    assert_eq!(limits.running, 12);
    assert_eq!(
        limits.set_by(Limit::RunningPerChat),
        &Source::Workspace("alpha".to_owned())
    );
    let elsewhere = in_force(
        &table(project),
        Some("beta"),
        Some("steward"),
        Some("devops"),
        &nothing(),
        &no_policy(),
    );
    assert_eq!(elsewhere.running, 4);
    // A chat in no workspace has the project's.
    let nowhere = in_force(
        &table(project),
        None,
        Some("steward"),
        Some("devops"),
        &nothing(),
        &no_policy(),
    );
    assert_eq!(nowhere.running, 4);
}

#[test]
fn the_asking_personas_value_wins_over_the_workspaces_and_the_projects() {
    let project = "[dispatch]\ndepth = 4\n\n[dispatch.workspaces.alpha]\ndepth = 5\n\n\
                   [dispatch.personas.steward]\ndepth = 2\n";
    let limits = committed(project);
    assert_eq!(limits.depth, 2);
    assert_eq!(
        limits.set_by(Limit::Depth),
        &Source::Persona("steward".to_owned())
    );
}

#[test]
fn a_persona_over_the_project_with_no_workspace_value_between() {
    let limits = committed("[dispatch]\ndepth = 4\n\n[dispatch.personas.steward]\ndepth = 1\n");
    assert_eq!(limits.depth, 1);
}

#[test]
fn a_level_that_sets_nothing_inherits_limit_by_limit() {
    // The persona sets depth only, the workspace live-per-lineage only, the project
    // running-per-chat only: each limit comes from the most specific level that sets it.
    let project = "[dispatch]\nrunning-per-chat = 4\n\n[dispatch.workspaces.alpha]\n\
                   live-per-lineage = 9\n\n[dispatch.personas.steward]\ndepth = 2\n";
    let limits = committed(project);
    assert_eq!(limits.depth, 2);
    assert_eq!(limits.lineage, 9);
    assert_eq!(limits.running, 4);
    assert_eq!(limits.messages_per_minute, 10);
}

#[test]
fn the_target_personas_general_limits_are_not_the_asking_chats() {
    // devops is dispatched to; its depth is for chats that ask as devops.
    let limits = committed("[dispatch.personas.devops]\ndepth = 1\nrunning-per-chat = 1\n");
    assert_eq!(limits.depth, 3);
    assert_eq!(limits.running, 6);
}

#[test]
fn may_dispatch_is_the_asking_personas_and_may_run_at_once_the_targets() {
    let project = "[dispatch.personas.steward]\nmay-dispatch = 2\nmay-run-at-once = 7\n\n\
                   [dispatch.personas.devops]\nmay-dispatch = 5\nmay-run-at-once = 1\n";
    let limits = committed(project);
    assert_eq!(limits.may_dispatch, Some(2));
    assert_eq!(limits.may_run_at_once, Some(1));
    assert_eq!(
        limits.set_by(Limit::MayRunAtOnce),
        &Source::Persona("devops".to_owned())
    );
}

#[test]
fn a_chat_on_no_persona_has_the_workspaces_and_the_projects() {
    let project = "[dispatch]\ndepth = 4\n\n[dispatch.personas.steward]\ndepth = 1\n\
                   may-dispatch = 1\n";
    let limits = in_force(
        &table(project),
        None,
        None,
        Some("devops"),
        &nothing(),
        &no_policy(),
    );
    assert_eq!(limits.depth, 4);
    assert_eq!(limits.may_dispatch, None);
}

// ---- this machine's own table --------------------------------------------------------------------

fn with_mine(project: &str, mine: &str) -> Limits {
    in_force(
        &table(project),
        Some("alpha"),
        Some("steward"),
        Some("devops"),
        &table(mine),
        &no_policy(),
    )
}

#[test]
fn your_own_limit_lowers_the_projects() {
    let limits = with_mine(
        "[dispatch]\nrunning-per-chat = 6\n",
        "[dispatch]\nrunning-per-chat = 2\n",
    );
    assert_eq!(limits.running, 2);
    assert_eq!(limits.set_by(Limit::RunningPerChat), &Source::You);
    assert!(limits.ignored.is_empty());
}

#[test]
fn your_own_limit_above_the_projects_is_ignored_and_said_so() {
    let limits = with_mine("[dispatch]\ndepth = 2\n", "[dispatch]\ndepth = 5\n");
    assert_eq!(limits.depth, 2);
    assert_eq!(limits.set_by(Limit::Depth), &Source::Project);
    assert_eq!(
        limits.ignored,
        vec![Ignored {
            limit: Limit::Depth,
            yours: 5,
            committed: 2
        }]
    );
    assert_eq!(
        limits.ignored[0].to_string(),
        "Your own limit of 5 for depth is above the project's 2, so it is ignored: a limit on \
         this machine can only lower one."
    );
}

#[test]
fn your_own_limit_above_a_default_is_ignored_too() {
    let limits = with_mine("", "[dispatch]\nlive-per-lineage = 40\n");
    assert_eq!(limits.lineage, 16);
    assert_eq!(limits.ignored.len(), 1);
}

#[test]
fn your_own_limit_cannot_lift_a_persona_or_a_workspace_value() {
    // The committed value in force is the persona's 1; yours of 4 is under the project's 6
    // and still above what is in force, so it is ignored.
    let limits = with_mine(
        "[dispatch]\nrunning-per-chat = 6\n\n[dispatch.personas.steward]\nrunning-per-chat = 1\n",
        "[dispatch]\nrunning-per-chat = 4\n",
    );
    assert_eq!(limits.running, 1);
    assert_eq!(limits.ignored.len(), 1);
}

#[test]
fn your_own_limit_caps_a_persona_that_has_no_cap() {
    let limits = with_mine("", "[dispatch.personas.devops]\nmay-run-at-once = 1\n");
    assert_eq!(limits.may_run_at_once, Some(1));
    assert_eq!(limits.set_by(Limit::MayRunAtOnce), &Source::You);
}

#[test]
fn your_own_zero_switches_dispatch_off_for_you() {
    let limits = with_mine("", "[dispatch]\nrunning-per-chat = 0\n");
    let decision = decide(&limits, &quiet());
    assert_eq!(limit_of(&decision), Some(Limit::RunningPerChat));
    assert_eq!(
        sentence_of(&decision),
        "dispatch is off on this machine, by your own limit: running per chat is set to 0. \
         Only the person can change it, in Settings › Project › Dispatch."
    );
}

// ---- policy --------------------------------------------------------------------------------------

#[test]
fn policy_caps_what_every_level_gives() {
    let policy = Level::unset()
        .with(Limit::RunningPerChat, 3)
        .with(Limit::Depth, 2);
    let project = "[dispatch]\nrunning-per-chat = 4\n\n[dispatch.workspaces.alpha]\n\
                   running-per-chat = 12\n\n[dispatch.personas.steward]\ndepth = 8\n";
    let limits = in_force(
        &table(project),
        Some("alpha"),
        Some("steward"),
        Some("devops"),
        &nothing(),
        &policy,
    );
    assert_eq!(limits.running, 3);
    assert_eq!(limits.set_by(Limit::RunningPerChat), &Source::Policy);
    assert_eq!(limits.depth, 2);
    // What it does not name is not capped.
    assert_eq!(limits.lineage, 16);
}

#[test]
fn policy_above_the_value_in_force_changes_nothing() {
    let policy = Level::unset().with(Limit::RunningPerChat, 30);
    let limits = in_force(
        &table("[dispatch]\nrunning-per-chat = 4\n"),
        Some("alpha"),
        Some("steward"),
        Some("devops"),
        &nothing(),
        &policy,
    );
    assert_eq!(limits.running, 4);
    assert_eq!(limits.set_by(Limit::RunningPerChat), &Source::Project);
}

#[test]
fn policy_caps_a_persona_that_has_no_cap() {
    let policy = Level::unset().with(Limit::MayRunAtOnce, 2);
    let limits = in_force(&nothing(), None, None, Some("devops"), &nothing(), &policy);
    assert_eq!(limits.may_run_at_once, Some(2));
    assert_eq!(limits.set_by(Limit::MayRunAtOnce), &Source::Policy);
}

#[test]
fn policy_is_read_from_the_policy_file_beside_the_sandbox_locks() {
    let locks = crate::sandbox::policy::Locks::parse(
        r#"{"sandbox": {"opt-out": false}, "dispatch": {"running-per-chat": 2, "may-run-at-once": 1}}"#,
        Path::new("/etc/purlis/policy.json"),
    );
    assert_eq!(locks.refused_because(), None);
    assert!(locks.forbids_opt_out());
    let ceiling = locks.dispatch_ceiling();
    assert_eq!(ceiling.get(Limit::RunningPerChat), Some(2));
    assert_eq!(ceiling.get(Limit::MayRunAtOnce), Some(1));
    assert_eq!(ceiling.get(Limit::Depth), None);
    // No policy caps nothing.
    assert!(
        crate::sandbox::policy::Locks::none()
            .dispatch_ceiling()
            .is_unset()
    );
}

#[test]
fn a_refused_policy_file_switches_dispatch_off_and_says_the_file_is_refused() {
    for json in [
        r#"{"dispatch": {"depth": 9}}"#,
        r#"{"dispatch": {"depth": -1}}"#,
        r#"{"dispatch": {"depth": "3"}}"#,
        r#"{"dispatch": {"speed": 3}}"#,
        r#"{"dispatch": 3}"#,
        // Nothing about dispatch at all: the file is refused for something else.
        r#"{"sandbox": {"speed": 3}}"#,
        "not json",
    ] {
        let locks =
            crate::sandbox::policy::Locks::parse(json, Path::new("/etc/purlis/policy.json"));
        assert!(locks.refused_because().is_some(), "{json}");
        assert!(locks.dispatch_ceiling().is_refused(), "{json}");
        // Whatever the project, a workspace, a persona or the person says.
        let limits = in_force(
            &table(
                "[dispatch]\nrunning-per-chat = 6\n\n[dispatch.personas.steward]\n\
                 running-per-chat = 4\n",
            ),
            Some("alpha"),
            Some("steward"),
            Some("devops"),
            &nothing(),
            locks.dispatch_ceiling(),
        );
        let decision = decide(&limits, &quiet());
        assert_eq!(
            refused_of(&decision),
            &Refused::Off {
                limit: Limit::RunningPerChat,
                by: Source::PolicyRefused,
                target: Some("devops".to_owned())
            },
            "{json}"
        );
        // No limit "is set to 0": nobody set one.
        assert_eq!(
            sentence_of(&decision),
            "dispatch is off on this machine: its policy file is refused, so purlis cannot \
             tell what an administrator allows. Only an administrator can fix it.",
            "{json}"
        );
        assert_eq!(
            sentence_of(&may_send(&limits, 0)),
            "messages between chats are off on this machine: its policy file is refused, so \
             purlis cannot tell what an administrator allows. Only an administrator can fix \
             it.",
            "{json}"
        );
    }
    // A policy that is read and names no limit caps none, and is not a refused one.
    let read = crate::sandbox::policy::Locks::parse(
        r#"{"owner": "IT"}"#,
        Path::new("/etc/purlis/policy.json"),
    );
    assert!(!read.dispatch_ceiling().is_refused());
    assert!(read.dispatch_ceiling().is_unset());
}

// ---- each limit refuses at its number ------------------------------------------------------------

#[test]
fn a_dispatch_with_room_under_every_limit_is_allowed() {
    assert_eq!(decide(&committed(""), &quiet()), Decision::Allowed);
}

#[test]
fn running_per_chat_refuses_at_its_number_and_allows_one_below() {
    let limits = committed("");
    let at = |running| Lineage { running, ..quiet() };
    assert_eq!(decide(&limits, &at(5)), Decision::Allowed);
    let refused = decide(&limits, &at(6));
    assert_eq!(
        refused_of(&refused),
        &Refused::TooManyRunning {
            limit: 6,
            running: 6
        }
    );
    assert_eq!(limit_of(&refused), Some(Limit::RunningPerChat));
    assert_eq!(
        sentence_of(&refused),
        "this chat already has 6 persona chats running, and it may have 6 at once. Wait for \
         one to report, then dispatch again."
    );
    // The count said is the real one, where a limit was lowered under what is running.
    assert_eq!(
        sentence_of(&decide(&limits, &at(9))),
        "this chat already has 9 persona chats running, and it may have 6 at once. Wait for \
         one to report, then dispatch again."
    );
}

#[test]
fn live_per_lineage_refuses_at_its_number_and_allows_one_below() {
    let limits = committed("");
    let at = |lineage| Lineage { lineage, ..quiet() };
    assert_eq!(decide(&limits, &at(15)), Decision::Allowed);
    let refused = decide(&limits, &at(16));
    assert_eq!(
        refused_of(&refused),
        &Refused::LineageFull {
            limit: 16,
            lineage: 16
        }
    );
    assert_eq!(limit_of(&refused), Some(Limit::LivePerLineage));
    assert_eq!(
        sentence_of(&refused),
        "this chat's lineage already holds 16 running chats, and it may hold 16. Wait for one \
         to finish, then dispatch again."
    );
    assert_eq!(
        sentence_of(&decide(&limits, &at(20))),
        "this chat's lineage already holds 20 running chats, and it may hold 16. Wait for one \
         to finish, then dispatch again."
    );
}

#[test]
fn depth_refuses_at_its_number_and_allows_one_below() {
    let limits = committed("");
    // Two dispatches below the chat the person started: the new chat is the third deep.
    assert_eq!(
        decide(&limits, &under(&["reviewer", "steward"])),
        Decision::Allowed
    );
    let refused = decide(&limits, &under(&["writer", "reviewer", "steward"]));
    assert_eq!(
        refused_of(&refused),
        &Refused::TooDeep { limit: 3, depth: 3 }
    );
    assert_eq!(limit_of(&refused), Some(Limit::Depth));
    assert_eq!(
        sentence_of(&refused),
        "this chat is 3 dispatches below the chat the person started, and a chain may go 3 \
         deep here. Do the work in this chat, or say in your report what is left."
    );
    // A chain already deeper than a limit since lowered says how deep it really is.
    let lowered = committed("[dispatch]\ndepth = 1\n");
    assert_eq!(
        sentence_of(&decide(&lowered, &under(&["reviewer", "steward"]))),
        "this chat is 2 dispatches below the chat the person started, and a chain may go 1 \
         deep here. Do the work in this chat, or say in your report what is left."
    );
}

#[test]
fn may_dispatch_counts_every_chat_running_as_the_asking_persona_in_the_project() {
    let limits = committed("[dispatch.personas.steward]\nmay-dispatch = 2\n");
    let at = |by_asking| Lineage {
        by_asking,
        ..quiet()
    };
    assert_eq!(decide(&limits, &at(1)), Decision::Allowed);
    let refused = decide(&limits, &at(2));
    assert_eq!(
        refused_of(&refused),
        &Refused::PersonaDispatches {
            persona: "steward".to_owned(),
            limit: 2,
            running: 2
        }
    );
    assert_eq!(limit_of(&refused), Some(Limit::MayDispatch));
    assert_eq!(
        sentence_of(&refused),
        "chats as steward already have 2 persona chats running between them, and may have 2 \
         at once in this project. Wait for one to finish, then dispatch again."
    );
}

#[test]
fn may_dispatch_is_not_the_asking_chats_own_count() {
    // This chat has none running; two other steward chats have one each. The persona's cap
    // of 2 is reached all the same.
    let limits = committed("[dispatch.personas.steward]\nmay-dispatch = 2\n");
    let others = Lineage {
        running: 0,
        by_asking: 2,
        ..quiet()
    };
    assert_eq!(
        limit_of(&decide(&limits, &others)),
        Some(Limit::MayDispatch)
    );
    // This chat's own two alone do not reach a cap of 2 while the project's count is 1: the
    // count is the one the app hands over, never this chat's.
    let own = Lineage {
        running: 2,
        by_asking: 1,
        ..quiet()
    };
    assert_eq!(decide(&limits, &own), Decision::Allowed);
    // A chat on no persona has no persona's cap.
    let nobody = in_force(
        &table("[dispatch.personas.steward]\nmay-dispatch = 0\n"),
        None,
        None,
        Some("devops"),
        &nothing(),
        &no_policy(),
    );
    assert_eq!(decide(&nobody, &others), Decision::Allowed);
}

#[test]
fn at_most_n_chats_as_a_persona_refuses_the_next_one_wherever_they_run() {
    // The count is the project's, across workspaces: the asking chat's own workspace has no
    // say in it.
    let project = "[dispatch.personas.devops]\nmay-run-at-once = 2\n";
    for workspace in [Some("alpha"), Some("beta"), None] {
        let limits = in_force(
            &table(project),
            workspace,
            Some("steward"),
            Some("devops"),
            &nothing(),
            &no_policy(),
        );
        let at = |as_target| Lineage {
            as_target,
            ..quiet()
        };
        assert_eq!(decide(&limits, &at(1)), Decision::Allowed);
        let refused = decide(&limits, &at(2));
        assert_eq!(
            refused_of(&refused),
            &Refused::PersonaFull {
                persona: "devops".to_owned(),
                limit: 2,
                running: 2
            }
        );
        assert_eq!(limit_of(&refused), Some(Limit::MayRunAtOnce));
        // It names the persona, the count and the limit.
        assert_eq!(
            sentence_of(&refused),
            "2 chats are already running as devops, and 2 may run as it at once in this \
             project. Wait for one to finish, then dispatch again."
        );
        assert_eq!(
            sentence_of(&decide(&limits, &at(5))),
            "5 chats are already running as devops, and 2 may run as it at once in this \
             project. Wait for one to finish, then dispatch again."
        );
    }
}

#[test]
fn one_chat_as_a_persona_is_said_in_the_singular() {
    let limits = committed("[dispatch.personas.devops]\nmay-run-at-once = 1\n");
    let refused = decide(
        &limits,
        &Lineage {
            as_target: 1,
            ..quiet()
        },
    );
    assert_eq!(
        sentence_of(&refused),
        "1 chat is already running as devops, and 1 may run as it at once in this project. \
         Wait for one to finish, then dispatch again."
    );
}

#[test]
fn messages_per_minute_refuses_at_its_number_and_allows_one_below() {
    let limits = committed("");
    assert_eq!(may_send(&limits, 9), Decision::Allowed);
    let refused = may_send(&limits, 10);
    assert_eq!(
        refused_of(&refused),
        &Refused::TooManyMessages {
            limit: 10,
            sent: 10
        }
    );
    assert_eq!(limit_of(&refused), Some(Limit::MessagesPerMinute));
    assert_eq!(
        sentence_of(&refused),
        "this chat has sent that chat 10 messages in the last minute, and it may send 10. \
         Wait a minute, then send it."
    );
}

#[test]
fn the_sentences_end_on_the_constants_the_dispatch_decision_reuses() {
    assert_eq!(
        ASK_THE_PERSON,
        "Only the person can change it, in Settings › Project › Dispatch."
    );
    assert_eq!(
        REPORT_INSTEAD,
        "Send it what you found in your report instead."
    );
    assert_eq!(
        DO_IT_HERE,
        "Do the work in this chat, or say in your report what is left."
    );
    assert_eq!(
        WAIT_TO_DISPATCH,
        "Wait for one to report, then dispatch again."
    );
    assert_eq!(WAIT_FOR_ONE, "Wait for one to finish, then dispatch again.");
    assert_eq!(WAIT_TO_SEND, "Wait a minute, then send it.");
    assert_eq!(
        POLICY_REFUSED,
        "its policy file is refused, so purlis cannot tell what an administrator allows. Only \
         an administrator can fix it."
    );
    assert_eq!(DEEPEST, 8);
}

// ---- the fixed rules -----------------------------------------------------------------------------

#[test]
fn a_persona_already_above_the_asking_chat_is_refused_at_any_depth() {
    // Room everywhere: only the loop rule refuses.
    let limits = committed("[dispatch]\ndepth = 8\n");
    for above in [
        vec!["devops"],
        vec!["devops", "reviewer"],
        vec!["reviewer", "devops"],
        vec!["a", "b", "devops", "c", "d"],
    ] {
        let refused = decide(&limits, &under(&above));
        assert_eq!(
            refused_of(&refused),
            &Refused::Loop("devops".to_owned()),
            "{above:?}"
        );
        assert_eq!(limit_of(&refused), None);
        assert_eq!(
            sentence_of(&refused),
            "persona 'devops' is already in this chat's own chain of dispatches, and a persona \
             is never dispatched to from below itself. Send it what you found in your report \
             instead."
        );
    }
}

#[test]
fn the_loop_rule_is_said_before_the_depth_limit() {
    let refused = decide(&committed(""), &under(&["c", "b", "devops"]));
    assert_eq!(refused_of(&refused), &Refused::Loop("devops".to_owned()));
}

#[test]
fn a_chat_on_no_persona_above_is_no_loop() {
    let lineage = Lineage {
        depth: 1,
        chain: vec![None],
        ..quiet()
    };
    assert_eq!(decide(&committed(""), &lineage), Decision::Allowed);
}

#[test]
fn a_chat_may_split_its_own_work_and_only_as_deep_as_the_depth_allows() {
    // Its own persona is not above it, even where a chat of that persona is.
    let own = |above: &[&str]| {
        let limits = in_force(
            &nothing(),
            Some("alpha"),
            Some("devops"),
            Some("devops"),
            &nothing(),
            &no_policy(),
        );
        decide(&limits, &under(above))
    };
    assert_eq!(own(&["steward"]), Decision::Allowed);
    assert_eq!(own(&["devops", "steward"]), Decision::Allowed);
    assert_eq!(
        refused_of(&own(&["devops", "devops", "steward"])),
        &Refused::TooDeep { limit: 3, depth: 3 }
    );
}

#[test]
fn a_depth_of_nine_is_refused_as_a_setting_at_every_level() {
    for text in [
        "[dispatch]\ndepth = 9\n",
        "[dispatch.workspaces.alpha]\ndepth = 9\n",
        "[dispatch.personas.steward]\ndepth = 9\n",
    ] {
        let read = read(Some(text), FILE);
        assert_eq!(read.refused.len(), 1, "{text}");
        assert!(
            read.refused[0].ends_with(
                "depth in charter.toml is 9, and depth is never above 8, so it is not read \
                 and the level beneath it is in force"
            ),
            "{}",
            read.refused[0]
        );
        // Not read: the default stays in force.
        let limits = in_force(
            &read.table,
            Some("alpha"),
            Some("steward"),
            Some("devops"),
            &nothing(),
            &no_policy(),
        );
        assert_eq!(limits.depth, 3);
        // And the Settings tab's save is refused for it.
        assert_eq!(refusals(text, FILE), read.refused);
    }
    // Eight is the most that is read.
    assert_eq!(committed("[dispatch]\ndepth = 8\n").depth, 8);
}

#[test]
fn a_table_built_by_hand_cannot_carry_a_depth_above_eight_either() {
    let by_hand = Table {
        project: Level::unset().with(Limit::Depth, 40),
        ..Table::default()
    };
    let limits = in_force(
        &by_hand,
        None,
        None,
        Some("devops"),
        &nothing(),
        &no_policy(),
    );
    assert_eq!(limits.depth, DEEPEST);
}

// ---- 0 is off ------------------------------------------------------------------------------------

#[test]
fn zero_at_the_project_refuses_every_dispatch_and_says_dispatch_is_off_here() {
    for (word, limit) in [
        ("running-per-chat", Limit::RunningPerChat),
        ("live-per-lineage", Limit::LivePerLineage),
        ("depth", Limit::Depth),
    ] {
        let limits = committed(&format!("[dispatch]\n{word} = 0\n"));
        let refused = decide(&limits, &quiet());
        assert_eq!(
            refused_of(&refused),
            &Refused::Off {
                limit,
                by: Source::Project,
                target: Some("devops".to_owned())
            }
        );
        let label = word.replace('-', " ");
        assert_eq!(
            sentence_of(&refused),
            format!(
                "dispatch is off in this project: {label} is set to 0. Only the person can \
                 change it, in Settings › Project › Dispatch."
            )
        );
        // It stops new dispatches only: a chat already running still gets its messages.
        assert_eq!(may_send(&limits, 0), Decision::Allowed, "{word}");
    }
}

#[test]
fn zero_messages_a_minute_stops_messages_and_no_dispatch() {
    let limits = committed("[dispatch]\nmessages-per-minute = 0\n");
    assert_eq!(decide(&limits, &quiet()), Decision::Allowed);
    let refused = may_send(&limits, 0);
    assert_eq!(
        refused_of(&refused),
        &Refused::Off {
            limit: Limit::MessagesPerMinute,
            by: Source::Project,
            target: Some("devops".to_owned())
        }
    );
    assert_eq!(
        sentence_of(&refused),
        "messages between chats are off in this project: messages per minute is set to 0. \
         Only the person can change it, in Settings › Project › Dispatch."
    );
}

#[test]
fn zero_at_a_workspace_is_off_there_and_nowhere_else() {
    let project = "[dispatch.workspaces.alpha]\nrunning-per-chat = 0\n";
    let refused = decide(&committed(project), &quiet());
    assert_eq!(limit_of(&refused), Some(Limit::RunningPerChat));
    assert_eq!(
        sentence_of(&refused),
        "dispatch is off in the workspace alpha: running per chat is set to 0. Only the person \
         can change it, in Settings › Project › Dispatch."
    );
    let elsewhere = in_force(
        &table(project),
        Some("beta"),
        Some("steward"),
        Some("devops"),
        &nothing(),
        &no_policy(),
    );
    assert_eq!(decide(&elsewhere, &quiet()), Decision::Allowed);
}

#[test]
fn zero_for_a_persona_stops_it_dispatching_and_zero_at_once_stops_dispatch_to_it() {
    let asks = decide(
        &committed("[dispatch.personas.steward]\nmay-dispatch = 0\n"),
        &quiet(),
    );
    assert_eq!(
        sentence_of(&asks),
        "dispatch is off for the persona steward: may dispatch is set to 0. Only the person \
         can change it, in Settings › Project › Dispatch."
    );
    let runs = decide(
        &committed("[dispatch.personas.devops]\nmay-run-at-once = 0\n"),
        &quiet(),
    );
    assert_eq!(limit_of(&runs), Some(Limit::MayRunAtOnce));
    assert_eq!(
        sentence_of(&runs),
        "dispatch to devops is off for the persona devops: may run at once is set to 0. Only \
         the person can change it, in Settings › Project › Dispatch."
    );
}

#[test]
fn a_more_specific_level_turns_dispatch_back_on_over_a_projects_zero() {
    let project = "[dispatch]\nrunning-per-chat = 0\n\n[dispatch.workspaces.alpha]\n\
                   running-per-chat = 2\n\n[dispatch.personas.reviewer]\nrunning-per-chat = 1\n";
    // The workspace's 2 over the project's 0.
    assert_eq!(decide(&committed(project), &quiet()), Decision::Allowed);
    // The persona's 1 over it too, in a workspace that says nothing.
    let reviewer = in_force(
        &table(project),
        Some("beta"),
        Some("reviewer"),
        Some("devops"),
        &nothing(),
        &no_policy(),
    );
    assert_eq!(decide(&reviewer, &quiet()), Decision::Allowed);
    // Where no level says more, the project's 0 holds.
    let elsewhere = in_force(
        &table(project),
        Some("beta"),
        Some("steward"),
        Some("devops"),
        &nothing(),
        &no_policy(),
    );
    assert_eq!(
        limit_of(&decide(&elsewhere, &quiet())),
        Some(Limit::RunningPerChat)
    );
}

#[test]
fn no_level_turns_dispatch_back_on_over_a_policys_zero() {
    let policy = Level::unset().with(Limit::RunningPerChat, 0);
    let project = "[dispatch]\nrunning-per-chat = 6\n\n[dispatch.workspaces.alpha]\n\
                   running-per-chat = 2\n\n[dispatch.personas.steward]\nrunning-per-chat = 4\n";
    let limits = in_force(
        &table(project),
        Some("alpha"),
        Some("steward"),
        Some("devops"),
        &table("[dispatch]\nrunning-per-chat = 1\n"),
        &policy,
    );
    let refused = decide(&limits, &quiet());
    assert_eq!(
        refused_of(&refused),
        &Refused::Off {
            limit: Limit::RunningPerChat,
            by: Source::Policy,
            target: Some("devops".to_owned())
        }
    );
    assert_eq!(
        sentence_of(&refused),
        "dispatch is off on this machine, by policy: running per chat is set to 0. Only an \
         administrator can change it, in this machine's policy file."
    );
}

// ---- what a file may say -------------------------------------------------------------------------

#[test]
fn what_is_not_a_limit_is_refused_with_a_sentence_and_the_rest_is_read() {
    let text = "[dispatch]\nrunning-per-chat = 4\nspeed = 3\ndepth = \"deep\"\n\
                live-per-lineage = -1\nmay-dispatch = 2\n\n[dispatch.workspaces]\n\"../x\" = \
                { depth = 1 }\n\n[dispatch.personas.steward]\nhosts = [\"a.example\"]\n\
                depth = 2\n";
    let read = read(Some(text), FILE);
    assert_eq!(read.table.project.get(Limit::RunningPerChat), Some(4));
    assert_eq!(read.table.project.get(Limit::Depth), None);
    assert_eq!(read.table.project.get(Limit::MayDispatch), None);
    assert_eq!(read.table.personas["steward"].get(Limit::Depth), Some(2));
    assert!(read.table.workspaces.is_empty());
    let said = read.refused.join("\n");
    for part in [
        "dispatch.speed in charter.toml is not a key purlis reads — a dispatch limit is one of \
         running-per-chat, live-per-lineage, depth, messages-per-minute",
        "dispatch.depth in charter.toml is not a whole number of 0 or more",
        "dispatch.live-per-lineage in charter.toml is not a whole number of 0 or more",
        "dispatch.may-dispatch in charter.toml is a persona's limit, so it is not read here — \
         write it under [dispatch.personas.<persona>]",
        "dispatch.workspaces.../x in charter.toml is not a workspace's name, so it sets nothing",
        "dispatch.personas.steward.hosts in charter.toml is not a key purlis reads",
    ] {
        assert!(said.contains(part), "missing: {part}\nin: {said}");
    }
    assert_eq!(read.refused.len(), 6, "{said}");
}

#[test]
fn a_file_with_no_dispatch_table_or_no_file_sets_nothing() {
    for text in [None, Some(""), Some("schema = 1\n"), Some("not toml [")] {
        assert_eq!(read(text, FILE), Read::default(), "{text:?}");
    }
    let wrong = read(Some("dispatch = 3\n"), FILE);
    assert_eq!(wrong.table, Table::default());
    assert_eq!(wrong.refused.len(), 1);
}

// ---- this machine's file, where git would carry it ---------------------------------------------

#[test]
fn your_own_lowering_holds_where_git_would_carry_this_machines_file() {
    use crate::settings::LayerText;
    let committed = "[dispatch]\nrunning-per-chat = 6\ndepth = 3\n";
    let mine = "[dispatch]\nrunning-per-chat = 2\ndepth = 8\n";
    let left_out = LayerText::LeftOut {
        why: "charter.local.toml is tracked by git".to_owned(),
        text: mine.to_owned(),
    };
    for local in [LayerText::Text(mine.to_owned()), left_out] {
        let files = Files::of(Some(committed), &local);
        let limits = files.in_force(Some("alpha"), Some("steward"), Some("devops"), &no_policy());
        // The lowering is honoured either way: it can only lower.
        assert_eq!(limits.running, 2, "{local:?}");
        assert_eq!(limits.set_by(Limit::RunningPerChat), &Source::You);
        // And it still raises nothing.
        assert_eq!(limits.depth, 3, "{local:?}");
        assert_eq!(limits.ignored.len(), 1);
    }
    // No file of this machine's lowers nothing.
    let none = Files::of(Some(committed), &LayerText::Nothing);
    assert_eq!(none.mine, Table::default());
}

#[test]
fn your_own_lowering_is_read_from_the_project_whatever_git_says_of_the_file() {
    // No repo here, or one that would carry the file: either way the file's limit holds.
    let root = tempfile::tempdir().expect("a project");
    std::fs::write(
        root.path().join("charter.local.toml"),
        "[dispatch]\nrunning-per-chat = 1\n",
    )
    .expect("this machine's file");
    let limits = of(root.path(), None, Some("steward"), Some("devops"));
    assert_eq!(limits.running, 1);
}

// ---- a persona's own file ------------------------------------------------------------------------

#[test]
fn a_chat_cannot_raise_its_personas_limits_through_its_personas_file() {
    // A project with no manifest and a persona whose own file asks for everything: the limits
    // in force are the defaults, because nothing here reads a persona's file.
    let root = tempfile::tempdir().expect("a project");
    let persona = root.path().join("personas").join("steward");
    std::fs::create_dir_all(&persona).expect("the persona's folder");
    std::fs::write(
        persona.join("persona.md"),
        "---\nname: steward\ndispatch:\n  depth: 8\n  running-per-chat: 99\n  may-dispatch: 99\n\
         running-per-chat: 99\ndepth: 8\n---\n\n[dispatch]\ndepth = 8\nrunning-per-chat = 99\n",
    )
    .expect("the persona's file");
    std::fs::write(
        persona.join("dispatch.toml"),
        "[dispatch]\ndepth = 8\nrunning-per-chat = 99\n",
    )
    .expect("a file beside it");
    let files = Files::read(root.path());
    assert_eq!(files.project, Table::default());
    assert_eq!(files.mine, Table::default());
    let limits = files.in_force(
        Some("alpha"),
        Some("steward"),
        Some("steward"),
        &no_policy(),
    );
    assert_eq!(limits, committed_as_steward_to_steward());
    assert_eq!(limits.depth, 3);
    assert_eq!(limits.running, 6);
    assert_eq!(limits.may_dispatch, None);
}

fn committed_as_steward_to_steward() -> Limits {
    in_force(
        &nothing(),
        Some("alpha"),
        Some("steward"),
        Some("steward"),
        &nothing(),
        &no_policy(),
    )
}

#[test]
fn the_entry_point_takes_no_personas_file_only_the_project_files_tables() {
    // The persona's limits are the committed file's `[dispatch.personas.<name>]`, and a
    // lower value there holds whatever else is written anywhere.
    let limits = committed("[dispatch.personas.steward]\ndepth = 1\nmay-dispatch = 1\n");
    assert_eq!(limits.depth, 1);
    assert_eq!(limits.may_dispatch, Some(1));
}

// ---- the files (CI's first run: a manifest cannot be written in the local sandbox) ---------------

#[test]
fn a_limit_changed_in_the_project_file_applies_to_the_next_dispatch() {
    let root = tempfile::tempdir().expect("a project");
    let manifest = root.path().join(FILE);
    std::fs::write(&manifest, "schema = 1\n").expect("the manifest");
    let lineage = Lineage {
        running: 2,
        ..quiet()
    };
    let before = of(root.path(), Some("alpha"), Some("steward"), Some("devops"));
    assert_eq!(decide(&before, &lineage), Decision::Allowed);
    // What Settings writes: one key, through the settings writer.
    let text = crate::settings::edited(
        "schema = 1\n",
        &[crate::settings::Edit {
            path: vec![
                crate::settings::Step::Key(TABLE.to_owned()),
                crate::settings::Step::Key(Limit::RunningPerChat.word().to_owned()),
            ],
            value: Some(crate::settings::Value::Integer(2)),
        }],
    )
    .expect("an edit");
    crate::settings::save(
        root.path(),
        crate::settings::Which::Shared,
        Some("schema = 1\n"),
        &text,
    )
    .expect("saved");
    let after = of(root.path(), Some("alpha"), Some("steward"), Some("devops"));
    assert_eq!(after.running, 2);
    assert_eq!(
        refused_of(&decide(&after, &lineage)),
        &Refused::TooManyRunning {
            limit: 2,
            running: 2
        }
    );
}

#[test]
fn the_settings_save_refuses_a_depth_of_nine() {
    let root = tempfile::tempdir().expect("a project");
    std::fs::write(root.path().join(FILE), "schema = 1\n").expect("the manifest");
    let refused = crate::settings::save(
        root.path(),
        crate::settings::Which::Shared,
        Some("schema = 1\n"),
        "schema = 1\n\n[dispatch]\ndepth = 9\n",
    )
    .expect_err("refused");
    assert!(
        refused
            .iter()
            .any(|why| why.contains("depth is never above 8")),
        "{refused:?}"
    );
}

// ---- a session's tokens and a task's time (#1512) ------------------------------------------------

#[test]
fn with_neither_limit_set_nothing_changes() {
    let limits = committed("");
    assert_eq!(limits.tokens_per_session, None);
    assert_eq!(limits.minutes_per_task, None);
    // However much a session used and however long a task worked.
    let spent = Lineage {
        tokens: u64::MAX,
        ..quiet()
    };
    assert_eq!(decide(&limits, &spent), Decision::Allowed);
    assert_eq!(time_reached(&limits, i64::MAX), None);
    assert_eq!(tokens_reached_at_work(&limits, u64::MAX), None);
}

#[test]
fn at_the_token_limit_a_dispatch_is_refused_with_the_figure_and_where_to_change_it() {
    let limits = committed("[dispatch]\ntokens-per-session = 500000\n");
    assert_eq!(limits.tokens_per_session, Some(500_000));
    let under = Lineage {
        tokens: 499_999,
        ..quiet()
    };
    assert_eq!(decide(&limits, &under), Decision::Allowed);
    let at = Lineage {
        tokens: 512_000,
        ..quiet()
    };
    let refused = decide(&limits, &at);
    assert_eq!(limit_of(&refused), Some(Limit::TokensPerSession));
    let said = sentence_of(&refused);
    assert!(said.contains("512k tokens"), "{said}");
    assert!(said.contains("may use 500k here"), "{said}");
    assert!(said.contains(ASK_THE_PERSON), "{said}");
    // And the tasks at work are asked for their report at the same figure.
    assert_eq!(
        tokens_reached_at_work(&limits, 512_000),
        Some(Reached::Tokens {
            limit: 500_000,
            used: 512_000
        })
    );
}

#[test]
fn a_task_past_its_minutes_is_reached_and_one_inside_them_is_not() {
    let limits = committed("[dispatch.personas.steward]\nminutes-per-task = 30\n");
    assert_eq!(limits.minutes_per_task, Some(30));
    assert_eq!(
        limits.set_by(Limit::MinutesPerTask),
        &Source::Persona("steward".to_owned())
    );
    assert_eq!(time_reached(&limits, 29 * 60 + 59), None);
    assert_eq!(
        time_reached(&limits, 31 * 60),
        Some(Reached::Time {
            limit: 30,
            worked: 31
        })
    );
    // A time limit never refuses a dispatch.
    assert_eq!(decide(&limits, &quiet()), Decision::Allowed);
}

#[test]
fn a_zero_of_tokens_or_minutes_is_refused_as_written_and_switches_nothing_off() {
    let read = read(
        Some("[dispatch]\ntokens-per-session = 0\nminutes-per-task = 0\n"),
        FILE,
    );
    assert_eq!(read.table.project.get(Limit::TokensPerSession), None);
    assert_eq!(read.table.project.get(Limit::MinutesPerTask), None);
    let said = read.refused.join("\n");
    assert!(
        said.contains("dispatch.tokens-per-session in charter.toml is 0"),
        "{said}"
    );
    assert!(said.contains("leave it out for no limit"), "{said}");
    assert_eq!(read.refused.len(), 2, "{said}");
    // A billion tokens is the most; above it is refused.
    let big = super::read(Some("[dispatch]\ntokens-per-session = 2000000000\n"), FILE);
    assert_eq!(big.table.project.get(Limit::TokensPerSession), None);
    assert!(
        big.refused[0].contains("never above 1000000000"),
        "{:?}",
        big.refused
    );
}

#[test]
fn your_own_token_limit_lowers_where_the_project_sets_none() {
    let mine = table("[dispatch]\ntokens-per-session = 100000\n");
    let limits = in_force(
        &nothing(),
        Some("alpha"),
        Some("steward"),
        Some("devops"),
        &mine,
        &no_policy(),
    );
    assert_eq!(limits.tokens_per_session, Some(100_000));
    assert_eq!(limits.set_by(Limit::TokensPerSession), &Source::You);
    // And above the project's it is ignored, and said so.
    let higher = in_force(
        &table("[dispatch]\ntokens-per-session = 50000\n"),
        None,
        None,
        None,
        &mine,
        &no_policy(),
    );
    assert_eq!(higher.tokens_per_session, Some(50_000));
    assert_eq!(higher.ignored.len(), 1);
}

#[test]
fn a_policy_caps_the_two_and_a_refused_policy_sets_neither() {
    let mut map = serde_json::Map::new();
    map.insert("minutes-per-task".to_owned(), serde_json::json!(15));
    let ceiling = ceiling(&map).expect("read");
    let limits = in_force(&nothing(), None, None, None, &nothing(), &ceiling);
    assert_eq!(limits.minutes_per_task, Some(15));
    assert_eq!(limits.set_by(Limit::MinutesPerTask), &Source::Policy);
    let mut zero = serde_json::Map::new();
    zero.insert("tokens-per-session".to_owned(), serde_json::json!(0));
    assert!(super::ceiling(&zero).is_err());

    let refused = in_force(
        &nothing(),
        None,
        None,
        None,
        &nothing(),
        &ceiling_when_refused(),
    );
    assert_eq!(refused.minutes_per_task, None, "no task is stopped for it");
    assert_eq!(refused.tokens_per_session, None);
    assert!(matches!(
        refused_of(&decide(&refused, &quiet())),
        Refused::Off { .. }
    ));
}

#[test]
fn what_was_reached_is_said_with_the_figure_and_where_the_person_changes_it() {
    let time = Reached::Time {
        limit: 30,
        worked: 31,
    }
    .say();
    assert!(time.contains("worked 31 minutes"), "{time}");
    assert!(time.contains("may work 30 minutes"), "{time}");
    assert!(time.contains("Settings › Project › Dispatch"), "{time}");
    let tokens = Reached::Tokens {
        limit: 1_000_000,
        used: 1_200_000,
    }
    .say();
    assert!(tokens.contains("1.2M tokens"), "{tokens}");
    // Kept in the word left for the asking chat, and read back as written.
    let kept = serde_json::to_string(&Reached::Time {
        limit: 30,
        worked: 31,
    })
    .unwrap();
    assert_eq!(kept, r#"{"kind":"time","limit":30,"worked":31}"#);
}
