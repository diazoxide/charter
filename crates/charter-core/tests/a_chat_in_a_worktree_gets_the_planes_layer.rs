//! The gap ADR 0027 named, closed: a chat in a charter-cut worktree runs with the plane's
//! ask/deny rules, its persona's agents and `$CHARTER_HARNESS` — or it does not start.
//!
//! Every test here is written against a guard that can be mutated, and each says which
//! mutation it is the red light for. Six review rounds on this repo came from tests that
//! passed either way, so a test that would stay green with the guard deleted does not belong
//! here.

mod support;

use std::path::Path;

use charter_core::{guest, start, worktree};

fn layered_plane(repo: &str) -> support::Fixture {
    let f = support::plane_with_clone(repo);
    f.give_the_plane_a_layer();
    f
}

fn cut(f: &support::Fixture, piece: &str) -> worktree::Added {
    worktree::add(&f.plane, &f.ws, &f.repo, piece, None).expect("a piece is cut")
}

/// The block in the `info/exclude` this checkout actually reads.
fn exclude_of(tree: &Path) -> String {
    let path = guest::exclude_file(tree).expect("a checkout has a git directory");
    std::fs::read_to_string(path).unwrap_or_default()
}

// ---------------------------------------------------------------------------------------
// What a chat in a worktree now has                                                       #
// ---------------------------------------------------------------------------------------

#[test]
fn a_worktree_charter_cuts_carries_the_planes_rules_its_agents_and_charter_harness() {
    // The M1.4 todo, in one assertion each: "a chat in a charter-cut worktree has NO persona
    // agents, none of the plane's ask/deny rules, and no $CHARTER_HARNESS".
    let f = layered_plane("thing");

    let added = cut(&f, "piece");

    let settings = std::fs::read_to_string(added.path.join(".claude/settings.json"))
        .expect("the plane's settings reach the piece");
    assert!(
        settings.contains("CHARTER_HARNESS"),
        "the variable a hook compares to `claude-code`: {settings}"
    );
    assert!(
        settings.contains("Bash(charter handoff *)"),
        "the plane's ask rules: {settings}"
    );
    assert!(
        settings.contains("charter@charter"),
        "and the plugin the next wiring probe resolves at this directory: {settings}"
    );
    let local = std::fs::read_to_string(added.path.join(".claude/settings.local.json"))
        .expect("the plane's machine-local rules reach the piece");
    assert!(local.contains("Bash(rm -rf /*)"), "{local}");
    assert_eq!(
        std::fs::read_to_string(added.path.join(".claude/agents/steward.md")).unwrap(),
        "# steward\n\nThe control plane steward.\n",
        "the persona's agent, which the walk-up cannot carry across a git root"
    );
    let warned = added
        .warnings
        .iter()
        .any(|w| w.contains("no charter layer"));
    assert!(
        !warned,
        "the cut does not warn about a layer that is there: {:?}",
        added.warnings
    );
}

#[test]
fn a_grant_in_the_plane_does_not_travel_into_somebody_elses_repository() {
    // `allow` is the one bucket that can make something run that would not have run anyway,
    // and it is deliberately left behind. The red light for a mutation that widens
    // RESTRICTIVE to every bucket, or drops the filter entirely.
    let f = layered_plane("thing");

    let added = cut(&f, "piece");

    let settings = std::fs::read_to_string(added.path.join(".claude/settings.json")).unwrap();
    assert!(
        !settings.contains("Bash(ls *)"),
        "a grant is not charter's to put in force in a repo it is a guest in: {settings}"
    );
    assert!(
        !settings.contains("hooks"),
        "and nothing outside enabledPlugins, env and the restrictive rules travels: {settings}"
    );
}

#[test]
fn the_row_stops_reading_unwired_once_the_layer_is_there() {
    // The bound ADR 0027 put on this gap. It reads the TREE, so it stops firing on its own —
    // this test is what proves that, rather than a label somebody has to remember to delete.
    let f = layered_plane("thing");

    cut(&f, "piece");

    let pieces = worktree::list(&f.plane, &f.ws, &f.repo).unwrap();
    let row = pieces.iter().find(|p| p.piece == "piece").unwrap();
    assert!(row.wired, "the layer is in the tree, so the row says so");
}

#[test]
fn a_plane_with_nothing_to_carry_writes_nothing_and_refuses_nothing() {
    // Writing an empty `{}` would look like a layer. A plane with no settings and no agents
    // has none, and a chat there is not refused over it.
    let f = support::plane_with_clone("thing");

    let added = cut(&f, "piece");

    assert!(!added.path.join(".claude").exists(), "nothing was written");
    assert!(!added.path.join(".charter-generated").exists());
    assert!(
        !exclude_of(&added.path).contains("charter"),
        "and no block was added to a file git's own template already wrote"
    );
    start::layered_or_refusal(&added.path, &f.plane).expect("and no chat is refused over it");
}

// ---------------------------------------------------------------------------------------
// The repo's own `git status` is not charter's to dirty                                   #
// ---------------------------------------------------------------------------------------

#[test]
fn the_repos_own_git_status_is_unaffected_by_everything_charter_wrote() {
    // The guarantee the whole guest design exists for. The red light for a mutation that
    // drops the `block` call, or writes the files before it.
    let f = layered_plane("thing");

    let added = cut(&f, "piece");

    assert_eq!(f.status(&added.path), "", "the piece's own status is clean");
    assert_eq!(f.status(&f.clone), "", "and so is the clone's");
    assert!(
        added.path.join(".claude/settings.json").is_file(),
        "and the files really are there to be hidden"
    );
}

#[test]
fn the_block_goes_in_the_exclude_the_clone_reads_and_not_the_worktrees_own() {
    // Git treats `info/` as shared, so a pattern written to `.git/worktrees/<id>/info/exclude`
    // is read by NOBODY — the file stays listed as untracked while the identical pattern in
    // the common directory hides it. The red light for a mutation that drops the `commondir`
    // hop and writes beside the worktree's own gitdir.
    let f = layered_plane("thing");

    let added = cut(&f, "piece");

    let common = f.clone.join(".git/info/exclude");
    let text = std::fs::read_to_string(&common).expect("the clone's exclude was written");
    assert!(text.contains("/.claude/settings.json"), "{text}");
    assert!(text.contains("/.charter-generated"), "{text}");
    assert!(
        text.contains(".charter-generated.*.tmp"),
        "the temp a kill leaves behind is hidden before the first one exists: {text}"
    );
    assert_eq!(
        std::fs::canonicalize(guest::exclude_file(&added.path).unwrap()).unwrap(),
        std::fs::canonicalize(&common).unwrap(),
        "the piece's exclude IS the clone's"
    );
    let own = f.clone.join(".git/worktrees/piece/info/exclude");
    assert!(
        !own.exists(),
        "and nothing was written where git reads nothing"
    );
}

#[test]
fn a_line_a_sibling_needs_is_never_taken_away_by_the_next_piece() {
    // One exclude, several trees. A piece that rewrote the block to its own list alone dropped
    // the line for charter's `.claude/settings.json` — the plane's rules and `env` — into
    // somebody else's repository. The red light for a mutation that replaces `already()`'s
    // union with the new list.
    let f = layered_plane("thing");
    cut(&f, "first");
    let common = f.clone.join(".git/info/exclude");
    // A line only the first piece could have needed, written into the shared block by hand.
    let text = std::fs::read_to_string(&common).unwrap();
    std::fs::write(
        &common,
        text.replace(
            "/.charter-generated\n",
            "/.claude/agents/only-the-first.md\n/.charter-generated\n",
        ),
    )
    .unwrap();

    cut(&f, "second");

    let after = std::fs::read_to_string(&common).unwrap();
    assert!(
        after.contains("/.claude/agents/only-the-first.md"),
        "a sibling's line survives the next piece's wire: {after}"
    );
    assert_eq!(
        after.matches("# >>> charter").count(),
        1,
        "and there is still exactly one block: {after}"
    );
}

#[test]
fn wiring_the_same_tree_again_changes_not_one_byte() {
    // A wire runs on every launch. Appending would duplicate every line on the second pass,
    // and the operator's `info/exclude` would grow without bound while their `git status`
    // stayed clean. The red light for a mutation that appends instead of replacing.
    let f = layered_plane("thing");
    let added = cut(&f, "piece");
    let exclude = exclude_of(&added.path);
    let marker = std::fs::read_to_string(added.path.join(".charter-generated")).unwrap();

    let again = guest::wire(&f.plane, &added.path);

    assert!(again.complete(), "{again:?}");
    assert_eq!(exclude_of(&added.path), exclude);
    assert_eq!(
        std::fs::read_to_string(added.path.join(".charter-generated")).unwrap(),
        marker
    );
    let all_current = again
        .rows
        .iter()
        .all(|r| r.status == guest::Status::Current);
    assert!(
        all_current,
        "and every path reads as already current: {:?}",
        again.rows
    );
}

#[test]
fn a_piece_carrying_the_layer_is_still_clean_enough_to_remove() {
    // The guards that decide whether work survives read `git status`. A layer that showed
    // there would make every piece read as dirty and refuse its own removal.
    let f = layered_plane("thing");
    let added = cut(&f, "piece");

    worktree::remove(&f.plane, &f.ws, &f.repo, "piece", false, false)
        .expect("charter's own files are not uncommitted work");

    assert!(!added.path.exists());
}

// ---------------------------------------------------------------------------------------
// What charter may overwrite, and what it may not                                         #
// ---------------------------------------------------------------------------------------

#[test]
fn a_file_charter_did_not_write_is_never_overwritten_and_the_chat_is_refused() {
    // The M1.2b shape: a tree where the plane's rules are NOT in force does not quietly start
    // a chat. The red light for a mutation that drops the digest comparison in `planned` and
    // writes over whatever is there.
    let f = layered_plane("thing");
    let added = cut(&f, "piece");
    std::fs::write(
        added.path.join(".claude/settings.json"),
        "{\"mine\": true}\n",
    )
    .unwrap();

    let again = guest::wire(&f.plane, &added.path);

    assert!(!again.complete(), "{again:?}");
    assert_eq!(
        std::fs::read_to_string(added.path.join(".claude/settings.json")).unwrap(),
        "{\"mine\": true}\n",
        "their file is exactly as they left it"
    );
    let refusal = start::layered_or_refusal(&added.path, &f.plane)
        .expect_err("and no chat starts in a tree whose rules are somebody else's");
    assert!(refusal.contains(".claude/settings.json"), "{refusal}");
    assert!(
        refusal.contains("Move it aside"),
        "a refusal names the repair: {refusal}"
    );
}

#[test]
fn the_harnesss_own_edit_of_the_local_file_is_kept_and_does_not_refuse_the_chat() {
    // `.claude/settings.local.json` is where "Yes, and don't ask again" lands. Its digest
    // moves without the file becoming anybody else's, so charter neither rewrites it nor
    // treats it as a reason to refuse. The red light for a mutation that empties COWRITTEN.
    let f = layered_plane("thing");
    let added = cut(&f, "piece");
    let theirs = "{\"permissions\": {\"allow\": [\"Bash(ls *)\"], \"deny\": []}}\n";
    std::fs::write(added.path.join(".claude/settings.local.json"), theirs).unwrap();

    let again = guest::wire(&f.plane, &added.path);

    assert!(again.complete(), "{again:?}");
    assert_eq!(
        std::fs::read_to_string(added.path.join(".claude/settings.local.json")).unwrap(),
        theirs,
        "the approval the operator saved is still there"
    );
    start::layered_or_refusal(&added.path, &f.plane).expect("and the chat starts");
    assert_eq!(f.status(&added.path), "", "and it is still hidden");
}

#[test]
fn a_file_charter_wrote_and_nobody_touched_is_refreshed_when_the_plane_moves() {
    // The other half of the same rule. Without it charter's own stale copy of the plane's
    // ask/deny rules would sit in every piece for ever.
    let f = layered_plane("thing");
    let added = cut(&f, "piece");
    std::fs::write(
        f.plane.join(".claude/settings.json"),
        "{\"env\": {\"CHARTER_HARNESS\": \"claude-code\"}, \
         \"permissions\": {\"deny\": [\"Bash(curl *)\"]}}\n",
    )
    .unwrap();

    let again = guest::wire(&f.plane, &added.path);

    assert!(again.complete(), "{again:?}");
    let settings = std::fs::read_to_string(added.path.join(".claude/settings.json")).unwrap();
    assert!(settings.contains("Bash(curl *)"), "{settings}");
    assert!(
        !settings.contains("Bash(charter handoff *)"),
        "the rule the plane dropped is gone from the piece too: {settings}"
    );
}

#[test]
fn a_charter_generated_the_repository_commits_is_not_charters_record() {
    // Charter's marker is per-checkout and untracked. A tracked one is content somebody
    // committed, and writing over it would change a TRACKED file, which no exclude line can
    // hide. The red light for a mutation that drops the `tracked` check — without it, any
    // repository carrying a committed `.charter-generated` reads as wired, silencing the
    // `unwired` label on a repo the operator merely cloned.
    let f = layered_plane("thing");
    // Shaped exactly like charter's own, so nothing but "git tracks it" tells them apart —
    // which is the whole of the rule.
    std::fs::write(
        f.clone.join(".charter-generated"),
        "{\n  \".claude/settings.json\": \"deadbeef\"\n}\n",
    )
    .unwrap();
    support::git(&f.clone, &["add", ".charter-generated"]);
    support::git(&f.clone, &["commit", "-q", "-m", "ours"]);

    let added = cut(&f, "piece");

    assert!(
        added.path.join(".charter-generated").is_file(),
        "the repository's own file is checked out into the piece"
    );
    assert!(
        !added.path.join(".claude/settings.json").exists(),
        "and charter wrote nothing over a tree it cannot record in"
    );
    assert_eq!(f.status(&added.path), "", "nothing of charter's is showing");
    let pieces = worktree::list(&f.plane, &f.ws, &f.repo).unwrap();
    assert!(
        !pieces.iter().find(|p| p.piece == "piece").unwrap().wired,
        "and a committed marker is not charter's word that the layer is there"
    );
    let refusal = start::layered_or_refusal(&added.path, &f.plane).expect_err("no chat starts");
    assert!(refusal.contains(".charter-generated"), "{refusal}");
}

#[test]
fn a_record_naming_a_path_outside_the_checkout_is_dropped_whole() {
    // A marker is a file inside a repository charter is a guest in. A key that walks up is
    // something a repository committed, and every path a record names gets a line in an
    // `info/exclude` charter writes — so a trusted `../../../outside.json` is charter putting
    // a pattern about somebody else's tree into somebody else's repository. The red light for
    // a mutation that drops `key_ok`, or that keeps the good keys and skips only the bad one.
    let f = layered_plane("thing");
    let added = cut(&f, "piece");
    std::fs::write(
        added.path.join(".charter-generated"),
        "{\".claude/settings.json\": \"deadbeef\", \"../../../outside.json\": \"deadbeef\"}\n",
    )
    .unwrap();

    guest::wire(&f.plane, &added.path);

    let record = std::fs::read_to_string(added.path.join(".charter-generated")).unwrap();
    assert!(
        !record.contains("outside.json"),
        "charter wrote a fresh record and carried none of that one across: {record}"
    );
    let block = exclude_of(&added.path);
    assert!(
        !block.contains("outside.json"),
        "and no line for it reached the exclude: {block}"
    );
    assert!(
        !f.plane.parent().unwrap().join("outside.json").exists(),
        "and nothing was written out there"
    );
}

#[test]
fn charter_does_not_write_through_a_committed_directory_symlink() {
    // A committed `.claude -> <somewhere else>` would send every write in this layer wherever
    // it points. The red light for a mutation that drops either `no_link_on_the_way` call in
    // `write_into`.
    //
    // **The target has to EXIST**, and a first draft of this test got that wrong: pointed at
    // a directory that is not there, `create_dir_all` fails with EEXIST on the link and the
    // write is refused by accident, so the test passed with the guard deleted. A mutation
    // probe found it. Pointed at a real directory, `create_dir_all` succeeds and only the
    // link check stands between charter and a write outside every checkout.
    let f = layered_plane("thing");
    let added = cut(&f, "piece");
    std::fs::remove_dir_all(added.path.join(".claude")).unwrap();
    let elsewhere = f.plane.parent().unwrap().join("elsewhere-claude");
    std::fs::create_dir_all(&elsewhere).unwrap();
    std::os::unix::fs::symlink(&elsewhere, added.path.join(".claude")).unwrap();

    let again = guest::wire(&f.plane, &added.path);

    assert!(
        !elsewhere.join("settings.json").exists(),
        "charter wrote the plane's rules outside every checkout"
    );
    assert!(
        std::fs::read_dir(&elsewhere).unwrap().next().is_none(),
        "and nothing else of charter's landed there either"
    );
    assert!(
        !again.complete(),
        "and the chat would be refused: {again:?}"
    );
}

#[test]
fn a_dangling_directory_link_is_refused_by_the_check_and_not_by_luck() {
    // The same link, pointing at nothing. `create_dir_all` happens to fail here — so this
    // case cannot tell whether the guard ran, and it is written down as such rather than
    // counted as coverage. What it does pin is that charter never CREATES the target: without
    // the first `no_link_on_the_way` call, `create_dir_all` on a link to a missing directory
    // is one `mkdir -p` from making a directory tree outside the plane.
    let f = layered_plane("thing");
    let added = cut(&f, "piece");
    std::fs::remove_dir_all(added.path.join(".claude")).unwrap();
    let outside = f.plane.parent().unwrap().join("nowhere-claude");
    let nowhere = outside.join("deep");
    std::os::unix::fs::symlink(&nowhere, added.path.join(".claude")).unwrap();

    let again = guest::wire(&f.plane, &added.path);

    assert!(!outside.exists(), "nothing was created outside the plane");
    assert!(
        !again.complete(),
        "and the chat would be refused: {again:?}"
    );
}

// ---------------------------------------------------------------------------------------
// Nothing is written when it cannot be hidden                                             #
// ---------------------------------------------------------------------------------------

#[test]
fn a_tree_with_no_git_directory_gets_no_files_at_all() {
    // The block first, then the files. A checkout whose exclude charter cannot find gets
    // nothing rather than untracked noise in somebody else's `git status`. The red light for
    // a mutation that moves the `block` call after the write loop, or ignores its error.
    let f = layered_plane("thing");
    let bare = f.workspace().join("not-a-checkout");
    std::fs::create_dir_all(&bare).unwrap();

    let wired = guest::wire(&f.plane, &bare);

    assert!(
        matches!(wired.hidden, guest::Hidden::Blocked(_, _)),
        "{wired:?}"
    );
    assert!(
        !bare.join(".claude").exists(),
        "and not one file was written"
    );
    assert!(wired.refusal(&bare).contains("could not hide"), "{wired:?}");
}

#[test]
fn an_exclude_charter_cannot_write_stops_the_layer_rather_than_leaking_it() {
    // The same rule, reached the way an operator reaches it: a checkout whose git directory
    // is read-only.
    //
    // The piece is cut by PLAIN GIT, so no block exists yet and one has to be written. A
    // charter-cut piece would leave its clone's block already current, `block` would find
    // nothing to change, and the read-only directory would never be touched — a test that
    // passed without reaching the guard at all.
    let f = layered_plane("thing");
    let piece = f.workspace().join(".worktrees").join(&f.repo).join("hand");
    std::fs::create_dir_all(piece.parent().unwrap()).unwrap();
    support::git(
        &f.clone,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "hand",
            &piece.display().to_string(),
        ],
    );
    let info = f.clone.join(".git/info");
    std::fs::create_dir_all(&info).unwrap();
    let was = std::fs::metadata(&info).unwrap().permissions();
    std::fs::set_permissions(
        &info,
        <std::fs::Permissions as std::os::unix::fs::PermissionsExt>::from_mode(0o555),
    )
    .unwrap();
    // root writes straight through the mode bits, so on a runner that is root this case
    // cannot be built at all — and a test that silently passed there would be reporting a
    // guard it never reached.
    let probe = info.join("charter-permission-probe");
    if std::fs::write(&probe, "x").is_ok() {
        let _ = std::fs::remove_file(&probe);
        std::fs::set_permissions(&info, was).unwrap();
        return;
    }

    let wired = guest::wire(&f.plane, &piece);

    std::fs::set_permissions(&info, was).unwrap();
    assert!(
        matches!(wired.hidden, guest::Hidden::Blocked(_, _)),
        "{wired:?}"
    );
    assert!(
        !piece.join(".claude/settings.json").exists(),
        "a file charter cannot hide is a file charter does not write"
    );
}

// ---------------------------------------------------------------------------------------
// A tree charter did not cut, and a directory that is not a tree at all                   #
// ---------------------------------------------------------------------------------------

#[test]
fn a_worktree_somebody_else_cut_gets_the_layer_when_a_chat_starts_in_it() {
    // `charter workspace reinit` was the only repair for this, and it is the Python. The
    // repair is now the start itself.
    let f = layered_plane("thing");
    let by_hand = f.workspace().join(".worktrees").join(&f.repo).join("hand");
    std::fs::create_dir_all(by_hand.parent().unwrap()).unwrap();
    support::git(
        &f.clone,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "hand",
            &by_hand.display().to_string(),
        ],
    );
    assert!(
        !by_hand.join(".claude/settings.json").exists(),
        "plain git carries no layer"
    );

    start::layered_or_refusal(&by_hand, &f.plane).expect("the start writes it");

    assert!(by_hand.join(".claude/settings.json").is_file());
    assert!(by_hand.join(".claude/agents/steward.md").is_file());
    assert_eq!(f.status(&by_hand), "", "and hides it");
}

#[test]
fn a_chat_outside_every_worktree_is_left_alone() {
    // The one guard standing between this and charter writing a `.claude/` into whatever
    // directory a chat was pointed at. The red light for a mutation that drops the `locate`
    // check and wires every cwd.
    let f = layered_plane("thing");

    for here in [f.plane.clone(), f.workspace(), f.clone.clone()] {
        start::layered_or_refusal(&here, &f.plane).expect("nothing to do");
        assert!(
            !here.join(".charter-generated").exists(),
            "charter wrote a record into {}",
            here.display()
        );
    }
    assert_eq!(f.status(&f.clone), "", "the clone is untouched");
}

#[test]
fn a_directory_shaped_like_a_piece_is_still_asked_whether_charter_may_write_there() {
    // `locate` is path arithmetic over names anything can create under `.worktrees/`, and it
    // hands back three strings. Those strings are asked of the same gate `add`, `remove` and
    // `merge` use before charter writes a byte — never joined straight on. The red light for
    // a mutation that drops `path_for`/`within_workspace` from the start path and wires
    // whatever `locate` named.
    //
    // The symlink half of that gate cannot be reached from here, and saying so is the point:
    // `locate` canonicalises before it matches, so a `.worktrees/<repo>` that is a link names
    // the workspace it really points into rather than smuggling a write into this one. The
    // check stays because the names are the half that IS reachable, and because the module's
    // rule is that the path charter checked is the path charter uses.
    let f = layered_plane("thing");
    let odd = f
        .workspace()
        .join(".worktrees")
        .join("-not-a-repo-name")
        .join("piece");
    std::fs::create_dir_all(&odd).unwrap();

    let refusal = start::layered_or_refusal(&odd, &f.plane)
        .expect_err("charter does not write into a path it will not name");

    assert!(refusal.contains("does not name a repo"), "{refusal}");
    assert!(
        !odd.join(".charter-generated").exists(),
        "and nothing was written there"
    );
}

// ---------------------------------------------------------------------------------------
// The document itself                                                                     #
// ---------------------------------------------------------------------------------------

#[test]
fn the_generated_settings_are_the_pythons_document_byte_for_byte() {
    // Both implementations wire the same plane until M4, and a byte of difference makes each
    // one read the other's file as the operator's own and stop maintaining it. The key order
    // is the Python's — `enabledPlugins`, `env`, then `permissions` last — and the rendering
    // is `json.dumps(doc, indent=2) + "\n"`.
    let f = layered_plane("thing");

    let want = guest::want(&f.plane);

    assert_eq!(
        want.get(".claude/settings.json").map(String::as_str),
        Some(concat!(
            "{\n",
            "  \"enabledPlugins\": {\n",
            "    \"charter@charter\": true\n",
            "  },\n",
            "  \"env\": {\n",
            "    \"CHARTER_HARNESS\": \"claude-code\"\n",
            "  },\n",
            "  \"permissions\": {\n",
            "    \"ask\": [\n",
            "      \"Bash(charter handoff *)\"\n",
            "    ]\n",
            "  }\n",
            "}\n",
        ))
    );
    assert_eq!(
        want.get(".claude/settings.local.json").map(String::as_str),
        Some(concat!(
            "{\n",
            "  \"permissions\": {\n",
            "    \"deny\": [\n",
            "      \"Bash(rm -rf /*)\"\n",
            "    ]\n",
            "  }\n",
            "}\n",
        ))
    );
}

#[test]
fn a_plane_file_that_is_a_link_out_of_the_plane_is_not_mirrored_into_a_repo() {
    // A persona agent symlinked out of the plane is content charter would otherwise copy into
    // somebody else's repository on the plane's authority — the Python reads through such a
    // link without asking. The red light for a mutation that drops the `contain::readable`
    // gate in `readable_text`; the walk hands the link over precisely so that gate decides.
    let f = layered_plane("thing");
    let secret = f.plane.parent().unwrap().join("secret.md");
    std::fs::write(&secret, "not the plane's to hand out\n").unwrap();
    std::os::unix::fs::symlink(&secret, f.plane.join(".claude/agents/leak.md")).unwrap();

    let want = guest::want(&f.plane);

    let keys: Vec<&String> = want.keys().collect();
    assert!(!want.contains_key(".claude/agents/leak.md"), "{keys:?}");
    assert!(
        want.contains_key(".claude/agents/steward.md"),
        "and the honest one is still carried"
    );
}

#[test]
fn a_plane_file_linked_from_inside_the_plane_is_still_mirrored() {
    // The other half, and the reason the walk hands a link over rather than dropping it: a
    // generator that links `personas/<who>/agent.md` into `.claude/agents/` is the ordinary
    // case, and a persona agent silently missing from every worktree is the failure this
    // whole milestone is about.
    let f = layered_plane("thing");
    let real = f.plane.join("personas").join("forge.md");
    std::fs::create_dir_all(real.parent().unwrap()).unwrap();
    std::fs::write(&real, "# forge\n").unwrap();
    std::os::unix::fs::symlink(&real, f.plane.join(".claude/agents/forge.md")).unwrap();

    let want = guest::want(&f.plane);

    assert_eq!(
        want.get(".claude/agents/forge.md").map(String::as_str),
        Some("# forge\n")
    );
}
