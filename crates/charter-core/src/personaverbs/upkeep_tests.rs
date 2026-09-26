//! `charter persona forget | dedupe | optimize | log` on the `daily` fixture plane. The Python
//! oracle recorded none of them, so these hold their sentences, copied from
//! `charter/commands_persona.py` at `cli-final`.

use super::*;
use crate::personaverbs::tests_plane::{Heard, Plane};

const PROD: &str = "personas/devops/memory/cluster-prod-1-lives-in-eu-west-1.md";

fn forget_on(plane: &Plane, name: &str, slug: &str, shared: bool) -> (u8, bool, Heard) {
    let mut heard = Heard::default();
    let (rc, changed) = forget(
        plane.root(),
        &Forget {
            name,
            slug,
            shared,
            ephemeral: false,
            session: "s-1",
        },
        &mut heard.sink(),
    );
    (rc, changed, heard)
}

fn today() -> chrono::NaiveDate {
    chrono::NaiveDate::from_ymd_opt(2026, 5, 4).unwrap()
}

#[test]
fn forget_removes_exactly_one_memory_and_its_index_line() {
    let plane = Plane::fixture("daily");
    let (rc, changed, heard) =
        forget_on(&plane, "devops", "cluster-prod-1-lives-in-eu-west-1", false);
    assert_eq!((rc, changed), (0, true), "{}", heard.err);
    assert_eq!(
        heard.err,
        "✓ Forgot 'cluster-prod-1-lives-in-eu-west-1' from devops's persistent memory.\n"
    );
    assert!(!plane.path(PROD).exists());
    assert!(
        !plane
            .read("personas/devops/memory/MEMORY.md")
            .contains("prod-1")
    );
    assert!(
        plane
            .path("personas/_shared/memory/the-plane-is-the-unit-of-work.md")
            .exists()
    );
}

#[test]
fn forget_reaches_the_shared_store_only_when_asked() {
    let plane = Plane::fixture("daily");
    let (rc, _, heard) = forget_on(&plane, "devops", "the-plane-is-the-unit-of-work", false);
    assert_eq!(
        (rc, heard.err.as_str()),
        (
            1,
            "✗ no memory 'the-plane-is-the-unit-of-work' in that store\n"
        )
    );
    let (rc, _, _) = forget_on(&plane, "devops", "the-plane-is-the-unit-of-work", true);
    assert_eq!(rc, 0);
    assert!(
        !plane
            .path("personas/_shared/memory/the-plane-is-the-unit-of-work.md")
            .exists()
    );
}

#[test]
fn forget_refuses_a_multi_segment_slug_and_deletes_nothing() {
    let plane = Plane::fixture("daily");
    for slug in ["../steward/persona", "../../charter.toml", "memory/x"] {
        let (rc, changed, heard) = forget_on(&plane, "devops", slug, false);
        assert_eq!((rc, changed), (1, false), "{slug}");
        assert!(
            heard.err.contains("is not the slug of one memory"),
            "{slug}: {}",
            heard.err
        );
    }
    assert!(plane.path("personas/steward/persona.md").exists());
    assert!(plane.path("charter.toml").exists());
    assert!(plane.path(PROD).exists());
}

#[test]
fn forget_ephemeral_changes_no_committed_store() {
    let plane = Plane::fixture("daily");
    plane.write(
        ".charter/persona-state/ephemeral/s-1/devops/scratch.md",
        "# Scratch\n\nx\n",
    );
    let mut heard = Heard::default();
    let (rc, changed) = forget(
        plane.root(),
        &Forget {
            name: "devops",
            slug: "scratch",
            shared: false,
            ephemeral: true,
            session: "s-1",
        },
        &mut heard.sink(),
    );
    assert_eq!((rc, changed), (0, false), "{}", heard.err);
    assert_eq!(
        heard.err,
        "✓ Forgot 'scratch' from devops's ephemeral memory.\n"
    );
}

fn dedupe_on(plane: &Plane, threshold: f64) -> (u8, Heard) {
    let mut heard = Heard::default();
    let rc = dedupe(plane.root(), "devops", threshold, &mut heard.sink());
    (rc, heard)
}

#[test]
fn dedupe_lists_a_near_duplicate_pair_above_the_threshold() {
    let plane = Plane::fixture("daily");
    let (rc, heard) = dedupe_on(&plane, 0.5);
    assert_eq!(
        (rc, heard.err.as_str()),
        (
            0,
            "✓ no near-duplicate memories for 'devops' (threshold 0.5).\n"
        )
    );
    plane.write(
        "personas/devops/memory/prod-1-cluster-region.md",
        "# Prod-1 cluster region\n\n_2026-03-03 10:00 · persistent_\n\nCluster prod-1 lives in eu-west-1 region\n",
    );
    let (rc, heard) = dedupe_on(&plane, 0.5);
    assert_eq!(rc, 0);
    assert!(
        heard
            .out
            .starts_with("Near-duplicate memory pairs for 'devops' (Jaccard ≥ 0.5):\n\n"),
        "{}",
        heard.out
    );
    assert!(
        heard.out.contains(&format!("        {PROD}\n")),
        "{}",
        heard.out
    );
    assert!(
        heard
            .out
            .contains("personas/devops/memory/prod-1-cluster-region.md\n")
    );
    assert_eq!(
        heard.err,
        "• Review, then drop one: charter persona forget <name> <slug> [--shared]\n"
    );
    assert!(plane.path(PROD).exists(), "dedupe deletes nothing");
    // Above the pair's overlap, nothing is flagged.
    let (_, heard) = dedupe_on(&plane, 0.99);
    assert!(heard.out.is_empty(), "{}", heard.out);
}

fn optimize_on(plane: &Plane, name: Option<&str>, apply: bool) -> (u8, usize, Heard) {
    let mut heard = Heard::default();
    let mut changed = 0;
    let rc = optimize(
        plane.root(),
        &Optimize {
            name,
            all: false,
            apply,
            stale_days: 90,
            today: today(),
        },
        &mut || changed += 1,
        &mut heard.sink(),
    );
    (rc, changed, heard)
}

#[test]
fn optimize_without_apply_changes_nothing_and_names_what_apply_would_do() {
    let plane = Plane::fixture("daily");
    // An exact duplicate the index does not list: two safe ops for --apply.
    let copy = plane.read(PROD);
    plane.write("personas/devops/memory/copy.md", &copy);
    let before = plane.read("personas/devops/memory/MEMORY.md");
    let (rc, changed, heard) = optimize_on(&plane, Some("devops"), false);
    assert_eq!((rc, changed), (0, 0));
    assert!(
        heard.out.starts_with(
            "\n◆ devops  (2 memories · 0% verified · 1 exact-dup group(s) · 0 near-dup pair(s) · \
             0 stale)\n  would auto-apply (re-run with --apply):\n    + collapse 1 exact-duplicate \
             group(s)"
        ),
        "{}",
        heard.out
    );
    assert!(heard.err.ends_with(
        "• \nRead-only. Re-run with --apply to auto-apply the safe/reversible ops (exact-dup \
         collapse + index repair); proposals always stay manual.\n"
    ));
    assert!(plane.path("personas/devops/memory/copy.md").exists());
    assert_eq!(plane.read("personas/devops/memory/MEMORY.md"), before);

    let (rc, changed, heard) = optimize_on(&plane, Some("devops"), true);
    assert_eq!((rc, changed), (0, 1), "{}", heard.err);
    assert!(
        heard.err.contains("✓   auto: archived exact-duplicate"),
        "{}",
        heard.err
    );
}

#[test]
fn optimize_with_no_name_curates_every_persona_and_the_shared_store() {
    let plane = Plane::fixture("daily");
    let (rc, _, heard) = optimize_on(&plane, None, false);
    assert_eq!(rc, 0);
    assert!(
        heard.out.contains("\n◆ devops  (1 memories"),
        "{}",
        heard.out
    );
    assert!(
        heard.out.contains("\n◆ _shared  (1 memories"),
        "{}",
        heard.out
    );
    let (rc, _, heard) = optimize_on(&plane, Some("_shared"), false);
    assert_eq!(rc, 0, "{}", heard.err);
    let (rc, _, heard) = optimize_on(&plane, Some("ghost"), false);
    assert_eq!(
        (rc, heard.err.as_str()),
        (
            1,
            "✗ no persona 'ghost' (create it: charter persona create ghost)\n"
        )
    );
}

fn log_on(plane: &Plane, message: Option<&str>) -> (u8, Heard) {
    let mut heard = Heard::default();
    let rc = log(
        plane.root(),
        &Log {
            name: "devops",
            message,
            n: 20,
            session: "s-1",
            now: today().and_hms_opt(11, 32, 17).unwrap(),
        },
        &mut heard.sink(),
    );
    (rc, heard)
}

#[test]
fn log_notes_into_this_sessions_activity_and_shows_it_back() {
    let plane = Plane::fixture("daily");
    let (rc, heard) = log_on(&plane, None);
    assert_eq!(
        (rc, heard.err.as_str()),
        (0, "• no activity for 'devops' in this session yet.\n")
    );
    let (rc, heard) = log_on(&plane, Some("rotated the kubeconfig"));
    assert_eq!(rc, 0);
    assert_eq!(
        heard.err,
        "✓ Noted to devops's session activity (see `charter persona log devops` / `charter \
         persona recall devops`).\n"
    );
    let (rc, heard) = log_on(&plane, None);
    assert_eq!(rc, 0);
    assert_eq!(
        heard.out,
        "2026-05-04T11:32:17  note       msg=rotated the kubeconfig\n"
    );
}
