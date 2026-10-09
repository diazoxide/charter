//! What a persona wants (#1502): that the line grants nothing, which of its names are offered
//! and which are ignored, which of them a question still offers, and what the question says a
//! persona works with. Every expected answer is written out.

use std::path::Path;

use super::*;
use crate::dispatchchain::Above;
use crate::dispatchgrant::{ChatPair, Covers, InForce, covers};
use crate::sandbox::policy::Locks;
use crate::secrets::{Ctx, Env};

/// A project with these personas, each with this definition.
fn project(personas: &[(&str, &str)]) -> tempfile::TempDir {
    let project = tempfile::tempdir().expect("a project");
    for (name, text) in personas {
        define(project.path(), name, text);
    }
    project
}

fn define(root: &Path, name: &str, text: &str) {
    let dir = root.join("personas").join(name);
    std::fs::create_dir_all(&dir).expect("made");
    std::fs::write(dir.join("persona.md"), text).expect("written");
}

/// A finished persona's definition, with `more` lines of frontmatter.
fn persona(name: &str, more: &str) -> String {
    format!("---\nname: {name}\nrole: {name}\nvault: none\n{more}---\n\n# {name}\n")
}

fn names(wants: &Wants) -> Vec<&str> {
    wants.personas.iter().map(String::as_str).collect()
}

fn asked(root: &Path, asking: &str, target: &str) -> Covers {
    covers(
        Some(asking),
        target,
        &InForce::read(root, Vec::new()),
        &Locks::none(),
    )
}

// ---- it grants nothing (V100-20) ----------------------------------------------------------------

#[test]
fn a_wants_line_alone_allows_no_dispatch() {
    let project = project(&[
        ("steward", &persona("steward", "wants: [devops]\n")),
        ("devops", &persona("devops", "")),
    ]);
    let root = project.path();
    assert_eq!(names(&of(root, "steward")), ["devops"], "it is read");
    // And the pair is asked for exactly as with no line at all.
    assert_eq!(asked(root, "steward", "devops"), Covers::NeedsGrant);
    assert_eq!(InForce::read(root, Vec::new()), InForce::default());
}

#[test]
fn a_persona_that_rewrites_its_own_wants_gains_no_reach() {
    let project = project(&[
        ("steward", &persona("steward", "")),
        ("devops", &persona("devops", "")),
        ("qa", &persona("qa", "")),
    ]);
    let root = project.path();
    let before = InForce::read(root, Vec::new());
    // What a chat editing its own persona's file can write: every name, itself, and the star.
    for written in [
        "wants: [devops, qa]\n",
        "wants: *\n",
        "wants: [\"*\"]\n",
        "wants: [steward, devops, qa, *]\n",
        "wants: devops\nwants: qa\n",
    ] {
        define(root, "steward", &persona("steward", written));
        assert_eq!(InForce::read(root, Vec::new()), before, "{written}");
        for target in ["devops", "qa"] {
            assert_eq!(
                asked(root, "steward", target),
                Covers::NeedsGrant,
                "{written}"
            );
        }
        assert!(crate::dispatchgrant::yours(root).is_empty(), "{written}");
        assert!(
            crate::dispatchgrant::any_yours(root).is_empty(),
            "{written}"
        );
        assert!(
            crate::dispatchgrant::committed_at(root).is_empty(),
            "{written}"
        );
    }
}

#[test]
fn a_chat_nobody_is_at_is_refused_whatever_its_persona_wants() {
    let project = project(&[
        ("steward", &persona("steward", "wants: [devops]\n")),
        ("devops", &persona("devops", "")),
    ]);
    let root = project.path();
    // The rule for a chat nobody is at, asked as the app asks it: a standing grant only, and
    // a line in a persona's file is none.
    let answer = crate::dispatchunattended::covers(
        Some("steward"),
        "devops",
        &InForce::read(root, Vec::new()),
        &Locks::none(),
        true,
    );
    assert!(
        matches!(answer, crate::dispatchunattended::Answer::Refused(_)),
        "{answer:?}"
    );
}

// ---- what of the line is offered ----------------------------------------------------------------

/// `value` read for `me`, where `known` are the project's personas and `drafts` its drafts.
fn read_as(value: &str, me: &str, known: &[&str], drafts: &[&str]) -> Wants {
    read(
        value,
        me,
        &Project {
            known: &|name| known.contains(&name),
            loads: &|_| true,
            draft: &|name| drafts.contains(&name),
        },
    )
}

#[test]
fn the_names_are_offered_in_the_order_written_and_once() {
    let wants = read_as("[devops, qa, devops]", "steward", &["devops", "qa"], &[]);
    assert_eq!(names(&wants), ["devops", "qa"]);
    assert_eq!(wants.ignored, vec![]);
    // The brackets are optional, as for every list of a persona's.
    assert_eq!(
        names(&read_as("devops, qa", "steward", &["devops", "qa"], &[])),
        ["devops", "qa"]
    );
}

#[test]
fn an_unknown_name_a_draft_itself_and_the_star_are_ignored_and_said() {
    let wants = read_as(
        "[devops, ghost, sketch, steward, *, \"*\", De vops, <b>qa</b>]",
        "steward",
        &["devops", "sketch", "steward"],
        &["sketch"],
    );
    assert_eq!(names(&wants), ["devops"]);
    assert_eq!(
        wants.ignored,
        vec![
            Ignored::Unknown("ghost".to_owned()),
            Ignored::Draft("sketch".to_owned()),
            Ignored::Itself,
            Ignored::Any,
            Ignored::Any,
            Ignored::NotAName("De vops".to_owned()),
            Ignored::NotAName("<b>qa</b>".to_owned()),
        ]
    );
    let said: Vec<String> = wants.ignored.iter().map(Ignored::said).collect();
    assert_eq!(
        said,
        [
            "wants names 'ghost', which is not a persona of this project, so it is not offered",
            "wants names 'sketch', which is a draft, so it is not offered until its `draft: true` \
             line is dropped",
            "wants names the persona itself, which is ignored: a chat dispatches to its own \
             persona with no grant",
            "wants names `*`, which is ignored: any persona is granted in Settings only, and \
             `wants` grants nothing",
            "wants names `*`, which is ignored: any persona is granted in Settings only, and \
             `wants` grants nothing",
            "wants names 'De vops', which is not a persona's name, so it is not offered",
            "wants names '<b>qa</b>', which is not a persona's name, so it is not offered",
        ]
    );
}

#[test]
fn a_name_that_only_looks_like_a_persona_s_is_not_a_name() {
    // Another alphabet's letters, a capital, a hidden character: none is a name purlis mints,
    // so none is offered under a known persona's look.
    let known = ["devops"];
    for written in [
        "dev\u{43e}ps",
        "Devops",
        "devops\u{200b}",
        "dev ops",
        "-devops",
    ] {
        let wants = read_as(written, "steward", &known, &[]);
        assert_eq!(names(&wants), [] as [&str; 0], "{written:?}");
        assert_eq!(wants.ignored.len(), 1, "{written:?}");
    }
}

#[test]
fn no_more_than_a_few_are_offered_however_long_the_line_is() {
    let all: Vec<String> = (0..500).map(|n| format!("p{n}")).collect();
    let known: Vec<&str> = all.iter().map(String::as_str).collect();
    let wants = read_as(&all.join(", "), "steward", &known, &[]);
    assert_eq!(wants.personas.len(), MOST);
    assert_eq!(names(&wants)[..2], ["p0", "p1"]);
    assert_eq!(wants.ignored, vec![Ignored::Beyond(500 - MOST)]);
    assert_eq!(wants.from, None);
    assert_eq!(
        wants.ignored[0].said(),
        format!(
            "wants names 494 more than the {MOST} purlis offers in one question, so they are \
             not offered"
        )
    );
    // And a line of nothing but junk says a bounded number of things, and why it stopped:
    // not "more than purlis offers", since none was offered.
    let junk = vec!["Not A Name"; 500].join(", ");
    let ignored = read_as(&junk, "steward", &[], &[]).ignored;
    assert_eq!(ignored.len(), MOST_SAID + 1);
    assert_eq!(ignored[MOST_SAID], Ignored::Unread(500 - MOST_SAID));
    assert_eq!(
        ignored[MOST_SAID].said(),
        "wants holds 488 more entries purlis did not read after 12 it ignored: mend the ones \
         above first"
    );
}

#[test]
fn an_empty_line_and_no_line_want_nothing() {
    for value in ["", "[]", " , ,", "[ ]"] {
        assert_eq!(
            read_as(value, "steward", &["devops"], &[]),
            Wants::default()
        );
    }
    let project = project(&[("steward", &persona("steward", ""))]);
    assert_eq!(of(project.path(), "steward"), Wants::default());
    assert_eq!(of(project.path(), "nobody"), Wants::default());
    assert_eq!(of(project.path(), "../steward"), Wants::default());
}

#[test]
fn the_line_is_read_off_the_project_s_own_personas_and_a_child_s_line_wins() {
    let project = project(&[
        ("base", &persona("base", "wants: [devops]\n")),
        (
            "kid",
            "---\nname: kid\nextends: base\nrole: kid\nvault: none\n---\n\n# kid\n",
        ),
        (
            "own",
            "---\nname: own\nextends: base\nrole: own\nvault: none\nwants: [qa]\n---\n\n# own\n",
        ),
        ("devops", &persona("devops", "")),
        ("qa", &persona("qa", "")),
        ("sketch", &persona("sketch", "draft: true\n")),
        ("wide", &persona("wide", "wants: [sketch, ghost, devops]\n")),
    ]);
    let root = project.path();
    assert_eq!(names(&of(root, "kid")), ["devops"]);
    assert_eq!(names(&of(root, "own")), ["qa"]);
    let wide = of(root, "wide");
    assert_eq!(names(&wide), ["devops"]);
    assert_eq!(
        wide.ignored,
        vec![
            Ignored::Draft("sketch".to_owned()),
            Ignored::Unknown("ghost".to_owned())
        ]
    );
}

#[test]
fn a_reserved_name_one_that_does_not_load_and_one_too_long_to_show_are_not_offered() {
    let long = "docs.nothing-else-is-allowed-by-this-box-at-all";
    assert!(long.len() > MOST_NAME);
    let wants = read(
        &format!("[charter, purlis, broken, {long}, devops]"),
        "steward",
        &Project {
            known: &|_| true,
            loads: &|name| name != "broken",
            draft: &|_| false,
        },
    );
    assert_eq!(names(&wants), ["devops"]);
    assert_eq!(
        wants.ignored,
        vec![
            Ignored::Reserved("charter".to_owned()),
            Ignored::Reserved("purlis".to_owned()),
            Ignored::Unloadable("broken".to_owned()),
            Ignored::TooLong(long.to_owned()),
        ]
    );
    assert_eq!(
        wants.ignored[0].said(),
        "wants names 'charter', a name purlis keeps for itself, so it is not offered"
    );
    assert_eq!(
        wants.ignored[2].said(),
        "wants names 'broken', whose definition does not load, so it is not offered"
    );
    let said = wants.ignored[3].said();
    assert!(
        said.ends_with(
            "which is longer than the 40 characters a question shows a name in, so it is not \
             offered"
        ),
        "{said}"
    );
}

#[test]
fn a_reserved_persona_written_by_hand_and_a_definition_that_is_not_text_are_not_offered() {
    let project = project(&[
        (
            "steward",
            &persona("steward", "wants: [charter, broken, devops]\n"),
        ),
        ("charter", &persona("charter", "")),
        ("devops", &persona("devops", "")),
    ]);
    let dir = project.path().join("personas/broken");
    std::fs::create_dir_all(&dir).expect("made");
    std::fs::write(dir.join("persona.md"), [0xff, 0xfe, 0x00, 0xff]).expect("written");
    let wants = of(project.path(), "steward");
    assert_eq!(names(&wants), ["devops"]);
    assert_eq!(
        wants.ignored,
        vec![
            Ignored::Reserved("charter".to_owned()),
            Ignored::Unloadable("broken".to_owned()),
        ]
    );
}

#[test]
fn an_inherited_line_says_whose_it_is() {
    let project = project(&[
        ("base", &persona("base", "wants: [ghost]\n")),
        (
            "kid",
            "---\nname: kid\nextends: base\nrole: kid\nvault: none\n---\n\n# kid\n",
        ),
    ]);
    let root = project.path();
    assert_eq!(of(root, "base").from, None);
    assert_eq!(
        of(root, "base").said(),
        ["wants names 'ghost', which is not a persona of this project, so it is not offered"]
    );
    assert_eq!(of(root, "kid").from.as_deref(), Some("base"));
    assert_eq!(
        of(root, "kid").said(),
        [
            "wants names 'ghost', which is not a persona of this project, so it is not offered, \
          in the line it inherits from 'base'"
        ]
    );
}

// ---- which of them a question offers ------------------------------------------------------------

#[test]
fn a_question_offers_each_wanted_persona_not_asked_for_granted_or_refused() {
    let project = project(&[
        (
            "steward",
            &persona("steward", "wants: [devops, qa, docs, legal, ops]\n"),
        ),
        ("devops", &persona("devops", "")),
        ("qa", &persona("qa", "")),
        ("docs", &persona("docs", "")),
        ("legal", &persona("legal", "")),
        ("ops", &persona("ops", "")),
    ]);
    let root = project.path();
    let offered = |grants: &InForce, locks: &Locks| {
        also(
            root,
            Some("steward"),
            "devops",
            grants,
            locks,
            &Above::default(),
        )
    };
    // The asked pair is the question itself, never a box under it.
    assert_eq!(
        offered(&InForce::default(), &Locks::none()),
        // In alphabetical order, whatever order the line writes them in.
        ["docs", "legal", "ops", "qa"]
    );
    let grants = InForce {
        chat: vec![ChatPair {
            asking: Some("steward".to_owned()),
            target: "qa".to_owned(),
        }],
        you: vec![crate::dispatchgrant::Pair::new("steward", "docs").expect("a pair")],
        never: vec![("steward".to_owned(), "legal".to_owned())],
        ..InForce::default()
    };
    assert_eq!(offered(&grants, &Locks::none()), ["ops"]);
    // "Any persona" covers them all: nothing is left to offer.
    let any = InForce {
        you_any: vec!["steward".to_owned()],
        ..InForce::default()
    };
    assert_eq!(offered(&any, &Locks::none()), [] as [&str; 0]);
    // A pair an administrator's policy locks is not offered either.
    let locks = Locks::parse(
        r#"{"dispatch": {"locked": [{"from": "steward", "to": "ops"}]}}"#,
        Path::new("/etc/purlis/policy.json"),
    );
    assert_eq!(
        offered(&InForce::default(), &locks),
        ["docs", "legal", "qa"]
    );
    // A chat on no persona has no definition, so nothing is offered.
    assert_eq!(
        also(
            root,
            None,
            "devops",
            &InForce::default(),
            &Locks::none(),
            &Above::default()
        ),
        [] as [&str; 0]
    );
}

#[test]
fn a_question_leaves_out_a_persona_the_person_said_never_to_for_a_chat_above() {
    // #1548: the decision refuses a dispatch to a persona the person said never to for a chat
    // above the asking one (ADR 0090), so the question offers no box for it either.
    let project = project(&[
        ("steward", &persona("steward", "wants: [qa, ops]\n")),
        ("devops", &persona("devops", "")),
        ("qa", &persona("qa", "")),
        ("ops", &persona("ops", "")),
    ]);
    let root = project.path();
    let grants = InForce {
        never: vec![("lead".to_owned(), "ops".to_owned())],
        ..InForce::default()
    };
    let offered = |above: &Above| {
        also(
            root,
            Some("steward"),
            "devops",
            &grants,
            &Locks::none(),
            above,
        )
    };
    // The person's own chat: nothing above it, both are offered.
    assert_eq!(offered(&Above::default()), ["ops", "qa"]);
    // A task of a lead chat: ops is refused for it, and is no box.
    let below_lead = Above::Known(vec![Some("lead".to_owned()), None]);
    assert_eq!(offered(&below_lead), ["qa"]);
    // A chain purlis cannot read whole may hold lead: a persona any never names is no box.
    assert_eq!(offered(&Above::Unread), ["qa"]);
    // What the chat's own record keeps is what is above it.
    let mut task = crate::reopen::Chat::default();
    assert_eq!(Above::of(&task), Above::default());
    task.from = Some(crate::reopen::HandedFrom {
        chat: 1,
        name: "lead 1".to_owned(),
        workspace: crate::active::Place::PlaneRoot,
        report: crate::reopen::Owed::Due,
        mode: crate::reopen::Mode::Task,
        depth: 1,
        root: None,
        above: None,
        by_person: false,
    });
    assert_eq!(Above::of(&task), Above::Unread);
    if let Some(from) = task.from.as_mut() {
        from.above = Some(vec![Some("lead".to_owned())]);
    }
    assert_eq!(
        Above::of(&task),
        Above::Known(vec![Some("lead".to_owned())])
    );
}

// ---- what a persona works with (V100-28) --------------------------------------------------------

const SECRET_NAME: &str = "PROD_DEPLOY_TOKEN";
const SECRET_VALUE: &str = "s3cr3t-value-nobody-reads";

/// A project whose registry tags vault `team` for devops, with a secret in it.
fn with_vaults() -> tempfile::TempDir {
    let project = project(&[
        // The file names another vault: a chat can write that line, so it is not what is said.
        (
            "devops",
            &persona("devops", "").replace("vault: none", "vault: everything"),
        ),
        ("qa", &persona("qa", "")),
    ]);
    let root = project.path();
    std::fs::write(
        root.join("vaults.json"),
        serde_json::json!({ "vaults": {
            "team": {"provider": "plain-file", "config": {"file": "team.json"}, "persona": "devops"},
            "everything": {"provider": "plain-file", "config": {"file": "team.json"}},
        }})
        .to_string(),
    )
    .expect("written");
    std::fs::write(
        root.join("team.json"),
        serde_json::json!({ SECRET_NAME: SECRET_VALUE }).to_string(),
    )
    .expect("written");
    project
}

fn sandbox(text: &str) -> crate::sandbox::Plane {
    crate::sandbox::Plane::of(Some(text))
}

const SANDBOXED: &str = r#"
[sandbox]
mode = "on"
hosts = ["project.example"]

[sandbox.personas.devops]
hosts = ["10.100.39.145:6443", "*.internal.example"]
"#;

#[test]
fn the_question_names_the_target_s_vault_and_hosts_as_the_project_declares_them() {
    let project = with_vaults();
    let ctx = Ctx::new(project.path(), Env::of(&[]));
    let access = Access::of(
        &ctx,
        &sandbox(SANDBOXED),
        &Locks::none(),
        &InForce::default(),
        "devops",
    );
    assert_eq!(
        access.said(),
        "devops works with its own access: vault team; hosts 10.100.39.145:6443, \
         *.internal.example."
    );
    // The vault the registry tags, never the one the persona's own file names.
    assert!(!access.said().contains("everything"));
    let none = Access::of(
        &ctx,
        &sandbox(SANDBOXED),
        &Locks::none(),
        &InForce::default(),
        "qa",
    );
    assert_eq!(
        none.said(),
        "qa works with its own access: no vault; no hosts beyond the project's."
    );
}

#[test]
fn it_never_shows_a_secret_s_name_or_value() {
    let project = with_vaults();
    let ctx = Ctx::new(project.path(), Env::of(&[]));
    let access = Access::of(
        &ctx,
        &sandbox(SANDBOXED),
        &Locks::none(),
        &InForce::default(),
        "devops",
    );
    for said in [access.said(), access.brief(), format!("{access:?}")] {
        assert!(!said.contains(SECRET_NAME), "{said}");
        assert!(!said.contains(SECRET_VALUE), "{said}");
    }
}

#[test]
fn a_vault_you_let_the_persona_use_on_this_machine_is_named_too() {
    let project = with_vaults();
    let root = project.path();
    crate::sandbox::local::grant_vault(root, "everything", "devops").expect("kept");
    let ctx = Ctx::new(root, Env::of(&[]));
    let access = Access::of(
        &ctx,
        &sandbox(SANDBOXED),
        &Locks::none(),
        &InForce::default(),
        "devops",
    );
    assert_eq!(
        access.brief(),
        "vaults everything, team; hosts 10.100.39.145:6443, *.internal.example"
    );
}

#[test]
fn a_long_list_is_clipped_to_a_few_and_says_how_many_more() {
    let project = with_vaults();
    let ctx = Ctx::new(project.path(), Env::of(&[]));
    let hosts: Vec<String> = (0..40).map(|n| format!("\"h{n}.example\"")).collect();
    let text = format!(
        "[sandbox]\nmode = \"on\"\n\n[sandbox.personas.devops]\nhosts = [{}]\n",
        hosts.join(", ")
    );
    let access = Access::of(
        &ctx,
        &sandbox(&text),
        &Locks::none(),
        &InForce::default(),
        "devops",
    );
    assert_eq!(
        access.brief(),
        "vault team; hosts h0.example, h1.example, h2.example and 37 more"
    );
}

#[test]
fn where_the_sandbox_is_off_the_question_says_so_and_names_no_host() {
    let project = with_vaults();
    let ctx = Ctx::new(project.path(), Env::of(&[]));
    let off = "[sandbox.personas.devops]\nhosts = [\"a.example\"]\n";
    for text in [None, Some(off)] {
        let plane = crate::sandbox::Plane::of(text);
        let access = Access::of(&ctx, &plane, &Locks::none(), &InForce::default(), "devops");
        // Of vaults too: a vault's tag is held by the sandbox, so no list bounds the chat.
        assert_eq!(
            access.brief(),
            "any vault and any host on this machine, since this project's sandbox is off"
        );
        assert_eq!(
            access.said(),
            "devops works with its own access: any vault and any host on this machine, since \
             this project's sandbox is off."
        );
    }
}

#[test]
fn a_registry_that_does_not_read_is_said_and_never_read_as_no_vault() {
    let project = with_vaults();
    std::fs::write(project.path().join("vaults.json"), "{ not json").expect("written");
    let ctx = Ctx::new(project.path(), Env::of(&[]));
    let access = Access::of(
        &ctx,
        &sandbox(SANDBOXED),
        &Locks::none(),
        &InForce::default(),
        "devops",
    );
    assert_eq!(access.vaults, Vaults::Unreadable);
    assert_eq!(
        access.brief(),
        "vaults purlis could not read; hosts 10.100.39.145:6443, *.internal.example"
    );
}

// ---- what the persona may itself dispatch to (the dispatcher's ruling on V100-28) --------------

fn pair(asking: &str, target: &str) -> crate::dispatchgrant::Pair {
    crate::dispatchgrant::Pair::new(asking, target).expect("a pair")
}

#[test]
fn the_question_says_what_the_target_may_itself_dispatch_to_and_nothing_where_that_is_nobody() {
    let project = with_vaults();
    let ctx = Ctx::new(project.path(), Env::of(&[]));
    let brief = |grants: &InForce, locks: &Locks| {
        Access::of(&ctx, &sandbox(SANDBOXED), locks, grants, "devops").brief()
    };
    let plain = "vault team; hosts 10.100.39.145:6443, *.internal.example";
    // Nobody: nothing is said.
    assert_eq!(brief(&InForce::default(), &Locks::none()), plain);
    // Named grants, yours and the project's, each once, sorted, clipped like the hosts.
    let named = InForce {
        you: vec![pair("devops", "researcher"), pair("steward", "legal")],
        project: vec![pair("devops", "qa"), pair("devops", "researcher")],
        ..InForce::default()
    };
    assert_eq!(
        brief(&named, &Locks::none()),
        format!("{plain}, and may itself dispatch to qa, researcher")
    );
    let many = InForce {
        you: ["a", "b", "c", "d", "e"]
            .iter()
            .map(|to| pair("devops", to))
            .collect(),
        ..InForce::default()
    };
    assert_eq!(
        brief(&many, &Locks::none()),
        format!("{plain}, and may itself dispatch to a, b, c and 2 more")
    );
    // The wildcard, at either level.
    for any in [
        InForce {
            you_any: vec!["devops".to_owned()],
            ..InForce::default()
        },
        InForce {
            project_any: vec!["devops".to_owned()],
            you: vec![pair("devops", "qa")],
            ..InForce::default()
        },
    ] {
        assert_eq!(
            brief(&any, &Locks::none()),
            format!("{plain}, and may itself dispatch to any persona")
        );
    }
    // Another persona's wildcard is not this one's.
    let other = InForce {
        you_any: vec!["steward".to_owned()],
        ..InForce::default()
    };
    assert_eq!(brief(&other, &Locks::none()), plain);
}

#[test]
fn onward_reach_is_only_what_a_dispatch_would_be_covered_for() {
    let grants = InForce {
        you: vec![pair("devops", "qa"), pair("devops", "researcher")],
        never: vec![("devops".to_owned(), "qa".to_owned())],
        ..InForce::default()
    };
    // A never takes the pair away.
    assert_eq!(
        Onward::of("devops", &grants, &Locks::none()),
        Onward::Named(vec!["researcher".to_owned()])
    );
    // A policy lock takes it away, and a lock on all dispatch takes everything.
    let locked = Locks::parse(
        r#"{"dispatch": {"locked": [{"from": "devops", "to": "researcher"}]}}"#,
        Path::new("/etc/purlis/policy.json"),
    );
    assert_eq!(
        Onward::of("devops", &grants, &locked),
        Onward::Named(vec![])
    );
    let none = Locks::parse(
        r#"{"dispatch": {"allow": false}}"#,
        Path::new("/etc/purlis/policy.json"),
    );
    let any = InForce {
        you_any: vec!["devops".to_owned()],
        ..InForce::default()
    };
    assert_eq!(Onward::of("devops", &any, &none), Onward::Named(vec![]));
    // While the record of nevers does not read, no grant counts: none is said.
    let unread = InForce {
        never_unread: true,
        ..any.clone()
    };
    assert_eq!(
        Onward::of("devops", &unread, &Locks::none()),
        Onward::Named(vec![])
    );
    assert_eq!(Onward::of("devops", &any, &Locks::none()), Onward::Any);
}

#[test]
fn onward_reach_for_work_in_one_workspace_is_said_with_that_workspace() {
    // Where #1502's words meet #1505's condition: a grant limited to a workspace is reach
    // too, and leaving it out would say "nobody" of a persona that may dispatch onward.
    use crate::dispatchwithin::Limited;
    use crate::sandbox::grant::Level;
    let limited = |target: &str, workspace: &str| {
        Limited::new("devops", target, workspace).expect("a limited grant")
    };
    let grants = InForce {
        you: vec![pair("devops", "qa")],
        limited: vec![
            (Level::You, limited("researcher", "runners")),
            (Level::Project, limited("*", "web")),
            // Already said as one that holds everywhere: not said a second time.
            (Level::You, limited("qa", "runners")),
            // Another persona's is not this one's reach.
            (
                Level::You,
                Limited::new("steward", "prod", "runners").expect("a limited grant"),
            ),
        ],
        ..InForce::default()
    };
    assert_eq!(
        Onward::of("devops", &grants, &Locks::none()),
        Onward::Named(vec![
            "any persona in web".to_owned(),
            "qa".to_owned(),
            "researcher in runners".to_owned(),
        ])
    );
    // A never takes a limited pair away as it takes any other, and an unread record all.
    let refused = InForce {
        never: vec![("devops".to_owned(), "researcher".to_owned())],
        ..grants.clone()
    };
    assert_eq!(
        Onward::of("devops", &refused, &Locks::none()),
        Onward::Named(vec!["any persona in web".to_owned(), "qa".to_owned()])
    );
    let unread = InForce {
        never_unread: true,
        ..grants.clone()
    };
    assert_eq!(
        Onward::of("devops", &unread, &Locks::none()),
        Onward::Named(vec![])
    );
    // And it is part of what an answer is held to.
    let project = with_vaults();
    let ctx = Ctx::new(project.path(), Env::of(&[]));
    let stamp = |grants: &InForce| {
        Offer {
            target: Access::of(&ctx, &sandbox(SANDBOXED), &Locks::none(), grants, "devops"),
            also: Vec::new(),
        }
        .stamp()
    };
    let without = InForce {
        limited: Vec::new(),
        ..grants.clone()
    };
    assert_ne!(stamp(&grants), stamp(&without));
    assert!(
        Access::of(&ctx, &sandbox(SANDBOXED), &Locks::none(), &grants, "devops")
            .said()
            .ends_with(
                ", and may itself dispatch to any persona in web, qa, researcher in runners."
            ),
    );
}

// ---- what was shown is what is answered ---------------------------------------------------------

#[test]
fn the_stamp_changes_with_anything_the_question_says_and_with_what_it_clips() {
    let project = with_vaults();
    let ctx = Ctx::new(project.path(), Env::of(&[]));
    // What the target may itself dispatch to is part of what was said.
    let reach = |grants: &InForce| {
        Offer {
            target: Access::of(&ctx, &sandbox(SANDBOXED), &Locks::none(), grants, "devops"),
            also: Vec::new(),
        }
        .stamp()
    };
    assert_ne!(
        reach(&InForce::default()),
        reach(&InForce {
            you_any: vec!["devops".to_owned()],
            ..InForce::default()
        })
    );
    let offer = |text: &str, also: &[&str]| Offer {
        target: Access::of(
            &ctx,
            &sandbox(text),
            &Locks::none(),
            &InForce::default(),
            "devops",
        ),
        also: also
            .iter()
            .map(|one| {
                Access::of(
                    &ctx,
                    &sandbox(text),
                    &Locks::none(),
                    &InForce::default(),
                    one,
                )
            })
            .collect(),
    };
    let shown = offer(SANDBOXED, &["qa"]).stamp();
    assert_eq!(shown, offer(SANDBOXED, &["qa"]).stamp(), "the same twice");
    assert_ne!(shown, offer(SANDBOXED, &[]).stamp(), "a box gone");
    assert_ne!(
        shown,
        offer(
            &SANDBOXED.replace("*.internal.example", "*.example"),
            &["qa"]
        )
        .stamp(),
        "a host changed"
    );
    // A host past what the sentence names changes it too: what is clipped is still agreed to.
    let many = |last: &str| {
        format!(
            "[sandbox]\nmode = \"on\"\n\n[sandbox.personas.devops]\nhosts = [\"a.example\", \
             \"b.example\", \"c.example\", \"d.example\", \"{last}\"]\n"
        )
    };
    let (one, other) = (
        offer(&many("e.example"), &[]),
        offer(&many("evil.example"), &[]),
    );
    assert_eq!(one.target.said(), other.target.said(), "they read the same");
    assert_ne!(one.stamp(), other.stamp());
    // And a vault tagged since.
    let before = offer(SANDBOXED, &["qa"]).stamp();
    crate::sandbox::local::grant_vault(project.path(), "everything", "qa").expect("kept");
    assert_ne!(before, offer(SANDBOXED, &["qa"]).stamp());
}
