//! #1583 and #1460: a checkout gives back a mirrored agent or skill once the project stops
//! having it — and only a copy purlis wrote, still as written, that git does not track.
//!
//! These need real git (a checkout's own `info/exclude`, and whether git tracks a copy), so
//! they first run on CI.

mod support;

use std::path::Path;

use purlis_core::doctor::fix::persona_agents::{self, Facts};
use purlis_core::{guest, wslayer};

const AGENT: &str = ".claude/agents/steward.md";

/// The block in the `info/exclude` this checkout reads.
fn exclude_of(tree: &Path) -> String {
    let path = guest::exclude_file(tree).expect("a checkout has a git directory");
    std::fs::read_to_string(path).unwrap_or_default()
}

#[test]
fn a_copy_of_an_agent_the_project_removed_leaves_the_checkout_with_its_line() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("svc");
    f.give_the_plane_a_layer();
    guest::wire(&f.plane, &f.clone);
    assert!(f.clone.join(AGENT).exists(), "mirrored first");
    assert!(exclude_of(&f.clone).contains("/.claude/agents/steward.md"));
    // An agent of the operator's beside it, which the record never named.
    std::fs::write(f.clone.join(".claude/agents/mine.md"), "# mine\n").unwrap();

    std::fs::remove_file(f.plane.join(AGENT)).unwrap();
    let wired = guest::wire(&f.plane, &f.clone);

    let row = wired
        .rows
        .iter()
        .find(|r| r.rel == AGENT)
        .expect("a row for it");
    assert_eq!(row.status, guest::Status::Removed, "{wired:?}");
    assert!(!f.clone.join(AGENT).exists());
    assert!(!guest::marker_at(&f.clone).contains_key(AGENT));
    assert!(
        !exclude_of(&f.clone).contains("/.claude/agents/steward.md"),
        "its line leaves with it"
    );
    assert_eq!(
        std::fs::read_to_string(f.clone.join(".claude/agents/mine.md")).unwrap(),
        "# mine\n"
    );
    assert!(
        wired.complete(),
        "the rest of the layer is still in force: {wired:?}"
    );
}

#[test]
fn the_projects_last_agent_is_withdrawn_though_nothing_is_left_to_carry() {
    purlis_core::unsteered!();
    // The plane carries one agent and no settings: once it goes, `want` is empty, and the
    // wire used to return before it read the checkout's record at all.
    let f = support::plane_with_clone("svc");
    std::fs::create_dir_all(f.plane.join(".claude/agents")).unwrap();
    std::fs::write(f.plane.join(AGENT), "# steward\n").unwrap();
    guest::wire(&f.plane, &f.clone);
    assert!(f.clone.join(AGENT).exists());

    std::fs::remove_file(f.plane.join(AGENT)).unwrap();
    let wired = guest::wire(&f.plane, &f.clone);

    assert!(!f.clone.join(AGENT).exists(), "{wired:?}");
    assert!(
        !f.clone.join(".claude").exists(),
        "and the folder it left empty"
    );
    assert!(
        guest::marker_at(&f.clone).is_empty(),
        "nothing of purlis's is recorded"
    );
    assert!(
        !exclude_of(&f.clone).contains("steward.md"),
        "{}",
        exclude_of(&f.clone)
    );
}

#[test]
fn a_copy_git_tracks_or_the_operator_edited_is_never_withdrawn() {
    purlis_core::unsteered!();
    let f = support::plane_with_clone("svc");
    f.give_the_plane_a_layer();
    std::fs::create_dir_all(f.plane.join(".claude/skills")).unwrap();
    std::fs::write(f.plane.join(".claude/skills/go.md"), "# go\n").unwrap();
    guest::wire(&f.plane, &f.clone);
    // One copy the repository went on to commit, and one the operator edited.
    support::git(&f.clone, &["add", "-f", AGENT]);
    support::git(&f.clone, &["commit", "-q", "-m", "keep the agent"]);
    std::fs::write(f.clone.join(".claude/skills/go.md"), "# go, my way\n").unwrap();

    std::fs::remove_file(f.plane.join(AGENT)).unwrap();
    std::fs::remove_file(f.plane.join(".claude/skills/go.md")).unwrap();
    guest::wire(&f.plane, &f.clone);

    assert_eq!(
        std::fs::read_to_string(f.clone.join(AGENT)).unwrap(),
        "# steward\n\nThe control plane steward.\n",
        "a tracked copy is the repository's"
    );
    assert_eq!(
        std::fs::read_to_string(f.clone.join(".claude/skills/go.md")).unwrap(),
        "# go, my way\n",
        "an edited copy is the operator's"
    );
}

/// A sub-agent file as `persona sync-agents` wrote it, under the old spelling of the marker.
fn generated(name: &str) -> String {
    let marker = purlis_core::names::SYNC_AGENTS_MARKER
        .spellings()
        .last()
        .expect("an old spelling");
    format!(
        "---\nname: {name}\ndescription: \"The {name} persona.\"\n---\n<!-- {marker} from \
         personas/{name}/persona.md — edit the persona, not this file. -->\n\nThis sub-agent \
         acts as the **{name}** persona — Role — in an\nisolated context. Adopt the charter \
         below as your role.\n\n# {name}\n"
    )
}

#[test]
fn a_workspaces_mirrored_agent_is_withdrawn_once_the_fix_removes_the_projects_file() {
    purlis_core::unsteered!();
    // #1460's last line: `purlis doctor --fix persona-agents` removes the project's generated
    // `.claude/agents/<persona>.md`, and the next wire of the workspace takes every checkout's
    // copy of it away.
    let f = support::plane_with_clone("svc");
    let ws = f.plane.join("workspaces").join(&f.ws);
    let agent = ".claude/agents/ops.md";
    std::fs::create_dir_all(f.plane.join("personas/ops")).unwrap();
    std::fs::write(
        f.plane.join("personas/ops/persona.md"),
        "---\nrole: Ops\n---\n\n# Ops\n",
    )
    .unwrap();
    std::fs::create_dir_all(f.plane.join(".claude/agents")).unwrap();
    std::fs::write(f.plane.join(agent), generated("ops")).unwrap();
    wslayer::wire(&f.plane, &ws);
    assert_eq!(
        std::fs::read_to_string(f.clone.join(agent)).unwrap(),
        generated("ops"),
        "the checkout holds the project's copy first"
    );

    let fixed = persona_agents::apply_with(
        &f.plane,
        &Facts {
            offered: &[],
            travels: &[],
            restorable: &|_| true,
        },
    );
    assert!(fixed.complete(), "{fixed:?}");
    assert!(
        !f.plane.join(agent).exists(),
        "the fix removed the project's file"
    );
    let rows = wslayer::wire(&f.plane, &ws);

    assert!(!f.clone.join(agent).exists(), "{rows:?}");
    assert!(
        rows.iter()
            .any(|row| row.rel.ends_with(agent) && row.did == wslayer::Did::Removed),
        "{rows:?}"
    );
}
