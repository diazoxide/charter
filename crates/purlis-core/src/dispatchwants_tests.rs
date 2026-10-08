//! What a persona wants (#1502): that the line grants nothing, which of its names are offered
//! and which are ignored, which of them a question still offers, and what the question says a
//! persona works with. Every expected answer is written out.

use std::path::Path;

use super::*;
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
    // A chat nobody is at is covered by a standing grant only, and there is none.
    assert_eq!(
        InForce::read(root, Vec::new()).level_of(Some("steward"), "devops"),
        None
    );
}

// ---- what of the line is offered ----------------------------------------------------------------

/// `value` read for `me`, where `known` are the project's personas and `drafts` its drafts.
fn read_as(value: &str, me: &str, known: &[&str], drafts: &[&str]) -> Wants {
    read(value, me, &|name| known.contains(&name), &|name| {
        drafts.contains(&name)
    })
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
    assert_eq!(
        wants.ignored[0].said(),
        format!(
            "wants names 494 more than the {MOST} purlis offers in one question, so they are \
             not offered"
        )
    );
    // And a line of nothing but junk says a bounded number of things.
    let junk = vec!["Not A Name"; 500].join(", ");
    assert!(read_as(&junk, "steward", &[], &[]).ignored.len() <= MOST_SAID + 1);
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
    let offered =
        |grants: &InForce, locks: &Locks| also(root, Some("steward"), "devops", grants, locks);
    // The asked pair is the question itself, never a box under it.
    assert_eq!(
        offered(&InForce::default(), &Locks::none()),
        ["qa", "docs", "legal", "ops"]
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
        ["qa", "docs", "legal"]
    );
    // A chat on no persona has no definition, so nothing is offered.
    assert_eq!(
        also(root, None, "devops", &InForce::default(), &Locks::none()),
        [] as [&str; 0]
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
    let access = Access::of(&ctx, &sandbox(SANDBOXED), &Locks::none(), "devops");
    assert_eq!(
        access.said(),
        "devops works with its own access: vault team; hosts 10.100.39.145:6443, \
         *.internal.example."
    );
    // The vault the registry tags, never the one the persona's own file names.
    assert!(!access.said().contains("everything"));
    let none = Access::of(&ctx, &sandbox(SANDBOXED), &Locks::none(), "qa");
    assert_eq!(
        none.said(),
        "qa works with its own access: no vault; no hosts beyond the project's."
    );
}

#[test]
fn it_never_shows_a_secret_s_name_or_value() {
    let project = with_vaults();
    let ctx = Ctx::new(project.path(), Env::of(&[]));
    let access = Access::of(&ctx, &sandbox(SANDBOXED), &Locks::none(), "devops");
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
    let access = Access::of(&ctx, &sandbox(SANDBOXED), &Locks::none(), "devops");
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
    let access = Access::of(&ctx, &sandbox(&text), &Locks::none(), "devops");
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
        let access = Access::of(&ctx, &plane, &Locks::none(), "devops");
        assert_eq!(
            access.brief(),
            "vault team; any host, since this project's sandbox is off"
        );
    }
}

#[test]
fn a_registry_that_does_not_read_is_said_and_never_read_as_no_vault() {
    let project = with_vaults();
    std::fs::write(project.path().join("vaults.json"), "{ not json").expect("written");
    let ctx = Ctx::new(project.path(), Env::of(&[]));
    let access = Access::of(&ctx, &sandbox(SANDBOXED), &Locks::none(), "devops");
    assert_eq!(access.vaults, Vaults::Unreadable);
    assert_eq!(
        access.brief(),
        "vaults purlis could not read; hosts 10.100.39.145:6443, *.internal.example"
    );
}

// ---- what was shown is what is answered ---------------------------------------------------------

#[test]
fn the_stamp_changes_with_anything_the_question_says_and_with_what_it_clips() {
    let project = with_vaults();
    let ctx = Ctx::new(project.path(), Env::of(&[]));
    let offer = |text: &str, also: &[&str]| Offer {
        target: Access::of(&ctx, &sandbox(text), &Locks::none(), "devops"),
        also: also
            .iter()
            .map(|one| Access::of(&ctx, &sandbox(text), &Locks::none(), one))
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
