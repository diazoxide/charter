//! Every question charter puts to `gh` and `glab`, asked of a stand-in that answers only what
//! a test wrote down.
//!
//! `forge.rs` runs the forge's own CLI and nothing else (see its module docs), and until this
//! file nothing in `charter-core` ran that half at all: `discover` and `gl-refresh` were
//! checked against Python charter end to end, and when the Python differential was frozen into
//! recorded fixtures (#223) the half of `forge.rs` that talks to a CLI was left with no test
//! that notices a change to it. The Sunday mutation run listed 112 such changes.
//!
//! # How a test reaches the stand-in
//!
//! [`charter_core::forge::find_cli`] searches the process's own `PATH` first, and a test
//! cannot change its own `PATH`: `set_var` is `unsafe` under Rust 2024 and this workspace
//! allows one audited `unsafe` block, which is not this. So the parent test below starts this
//! same binary again with `PATH` naming a directory holding a stand-in `gh` and `glab`, and
//! with a `HOME` the stand-in reads its answers from, and runs every test in [`child`] there.
//! Outside that child, those tests return at once.
//!
//! # What the stand-in answers
//!
//! Each test works on a host of its own, so tests running side by side never read each
//! other's answers. The stand-in finds `--hostname`'s value, looks under
//! `$HOME/hosts/<host>/` for a question written down with **exactly** its argv (program name
//! included), and answers with that question's exit code, stdout and stderr. Anything else is
//! a failure the caller sees: a question nobody wrote down exits 99, and the same question
//! asked a second time exits 98 — which is what makes a pager that never advances fail at
//! once rather than loop until the mutation run's timeout.

mod support;

use std::path::{Path, PathBuf};
use std::process::Command;

/// Set in the child run; unset, every test in [`child`] is a no-op.
const CHILD: &str = "CHARTER_TEST_FORGE_CLI_CHILD";
/// The directory the child's stand-ins live in, for the tests that check the path is pinned.
const BIN: &str = "CHARTER_TEST_FORGE_CLI_BIN";

const STAND_IN: &str = r#"#!/bin/sh
# A forge CLI that answers only the questions a test wrote down, each exactly once.
host=
prev=
for a in "$@"; do
  if [ "$prev" = --hostname ]; then host=$a; fi
  prev=$a
done
dir="$HOME/hosts/$host"
if [ -z "$host" ] || [ ! -d "$dir" ]; then
  echo "stand-in: no answers for host '$host'" >&2
  exit 97
fi
q="$dir/question.$$"
printf '%s\037' "${0##*/}" > "$q"
for a in "$@"; do printf '%s\037' "$a" >> "$q"; done
for want in "$dir"/*.args; do
  [ -f "$want" ] || continue
  if cmp -s "$q" "$want"; then
    rm -f "$q"
    base=${want%.args}
    if ! mkdir "$base.asked" 2>/dev/null; then
      echo "stand-in: asked twice: $want" >&2
      exit 98
    fi
    env > "$base.env"
    cat "$base.out"
    cat "$base.err" >&2
    exit "$(cat "$base.code")"
  fi
done
echo "stand-in: nobody wrote down the question in $q" >&2
exit 99
"#;

/// Run every test whose name contains `filter` in a child of this binary, with a stand-in
/// `gh` and `glab` first on its `PATH` in a directory called `bin_name`.
fn in_a_child(filter: &str, bin_name: &str) {
    let scratch = tempfile::tempdir().expect("a scratch directory");
    let scratch = std::fs::canonicalize(scratch.path())
        .map(|p| (scratch, p))
        .expect("a resolved scratch directory");
    let bin = scratch.1.join(bin_name);
    std::fs::create_dir_all(&bin).unwrap();
    stand_in::program(&bin, "gh", STAND_IN);
    stand_in::program(&bin, "glab", STAND_IN);
    let home = scratch.1.join("home");
    std::fs::create_dir_all(home.join("hosts")).unwrap();
    // **Each stand-in is run once here, with no deadline, before anything times it**
    // (charter-app#306). macOS checks a program file the first time it runs, and on a busy
    // machine that check queues: measured beside `cargo test -p charter-core --lib`, every
    // question that reached a new `gh` before its first run had finished waited ~30 s, and
    // the auth checks and best-effort calls, on `STATUS_TIMEOUT`'s 10 s, were cut off.
    // Every later call took 20–60 ms. With no `--hostname` a stand-in exits 97 and writes
    // nothing, so this run has no side effects.
    for cli in ["gh", "glab"] {
        let ran = charter_core::forklock::output(
            Command::new(bin.join(cli)).env_clear().env("HOME", &home),
        )
        .unwrap_or_else(|e| panic!("the stand-in {cli} runs: {e}"));
        assert_eq!(ran.status.code(), Some(97), "the stand-in {cli}: {ran:?}");
    }

    let out = charter_core::forklock::output(
        Command::new(std::env::current_exe().expect("the test binary"))
            .args([filter, "--nocapture"])
            .env(CHILD, "1")
            .env(BIN, &bin)
            .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
            .env("HOME", &home)
            .env("GH_TOKEN", "tok-under-test")
            .env("CHARTER_TEST_NOT_A_CREDENTIAL", "1"),
    )
    .expect("the test binary runs again");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stdout}\n{stderr}");
    assert!(
        !stdout.contains("running 0 tests"),
        "the filter {filter} matched nothing: {stdout}"
    );
}

#[test]
fn every_question_is_asked_of_a_stand_in_cli_first_on_path() {
    charter_core::unsteered!();
    in_a_child("child::", "bin");
}

#[test]
fn a_refresh_asks_the_stand_in_too() {
    charter_core::unsteered!();
    in_a_child("refreshed::", "bin");
}

#[test]
fn a_cli_whose_path_cannot_be_quoted_is_named_bare_in_the_helper() {
    charter_core::unsteered!();
    in_a_child("quoted::", "it's-bin");
}

/// One test's host, and the questions written down for it.
struct Scene {
    host: String,
    dir: PathBuf,
    written: std::cell::Cell<usize>,
}

impl Scene {
    fn new(host: &str) -> Scene {
        let home = PathBuf::from(std::env::var_os("HOME").expect("HOME"));
        let dir = home.join("hosts").join(host);
        std::fs::create_dir_all(&dir).unwrap();
        Scene {
            host: host.to_string(),
            dir,
            written: std::cell::Cell::new(0),
        }
    }

    /// Write down that `cli args…` is answered with `code`, `out` and `err`.
    fn answers(&self, cli: &str, args: &[&str], code: i32, out: &str, err: &str) -> PathBuf {
        let n = self.written.get();
        self.written.set(n + 1);
        let base = self.dir.join(format!("q{n}"));
        let mut asked = format!("{cli}\x1f");
        for arg in args {
            asked.push_str(arg);
            asked.push('\x1f');
        }
        std::fs::write(base.with_extension("args"), asked).unwrap();
        std::fs::write(base.with_extension("code"), code.to_string()).unwrap();
        std::fs::write(base.with_extension("out"), out).unwrap();
        std::fs::write(base.with_extension("err"), err).unwrap();
        base
    }

    /// `gh api --hostname <host> <path>`, answered.
    fn gh_api(&self, path: &str, code: i32, out: &str, err: &str) -> PathBuf {
        self.answers(
            "gh",
            &["api", "--hostname", &self.host, path],
            code,
            out,
            err,
        )
    }

    /// `glab --hostname <host> api <path>`, answered.
    fn glab_api(&self, path: &str, code: i32, out: &str, err: &str) -> PathBuf {
        self.answers(
            "glab",
            &["--hostname", &self.host, "api", path],
            code,
            out,
            err,
        )
    }

    fn forge(&self, kind: &str) -> charter_core::forge::Forge {
        charter_core::forge::Forge::build(kind, Some(&self.host)).expect("a forge")
    }
}

/// Whether a question written down was asked.
fn was_asked(base: &Path) -> bool {
    base.with_extension("asked").is_dir()
}

fn in_child() -> bool {
    std::env::var_os(CHILD).is_some()
}

mod child {
    use super::*;
    use charter_core::forge::{ForgeError, Raised};
    use serde_json::{Value, json};

    /// `n` bare records named `r<i>`, as a page of a listing.
    fn page(n: usize, key: &str) -> String {
        let items: Vec<Value> = (0..n).map(|i| json!({ key: format!("r{i}") })).collect();
        Value::Array(items).to_string()
    }

    fn names(records: &[Value]) -> Vec<String> {
        records
            .iter()
            .map(|r| r["name"].as_str().unwrap_or_default().to_string())
            .collect()
    }

    #[test]
    fn a_logged_in_gh_passes_the_auth_check_and_a_logged_out_one_is_told_to_log_in() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let ok = Scene::new("auth-ok.test");
        let asked = ok.answers(
            "gh",
            &["auth", "status", "--hostname", "auth-ok.test"],
            0,
            "",
            "",
        );
        assert_eq!(ok.forge("github").check_auth(), Ok(()));
        assert!(was_asked(&asked));

        let out = Scene::new("auth-out.test");
        out.answers(
            "gh",
            &["auth", "status", "--hostname", "auth-out.test"],
            1,
            "",
            "You are not logged into any GitHub hosts.",
        );
        assert_eq!(
            out.forge("github").check_auth(),
            Err(ForgeError(
                "gh is not authenticated for auth-out.test. Run: gh auth login".into()
            ))
        );
    }

    #[test]
    fn glab_is_logged_in_only_when_it_exits_zero_and_says_so() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        // glab exits 0 while logged in to nothing, so its words are read too.
        let said = Scene::new("glab-said.test");
        said.answers(
            "glab",
            &["--hostname", "glab-said.test", "auth", "status"],
            0,
            "",
            "glab-said.test\n  ✓ Logged in to glab-said.test as someone\n",
        );
        assert_eq!(said.forge("gitlab").check_auth(), Ok(()));

        let silent = Scene::new("glab-silent.test");
        silent.answers(
            "glab",
            &["--hostname", "glab-silent.test", "auth", "status"],
            0,
            "",
            "",
        );
        assert_eq!(
            silent.forge("gitlab").check_auth(),
            Err(ForgeError(
                "glab is not authenticated for glab-silent.test. Run: glab auth login".into()
            ))
        );

        let failed = Scene::new("glab-failed.test");
        failed.answers(
            "glab",
            &["--hostname", "glab-failed.test", "auth", "status"],
            1,
            "Logged in to glab-failed.test, token invalid",
            "",
        );
        assert!(failed.forge("gitlab").check_auth().is_err());
    }

    #[test]
    fn the_cli_is_handed_its_credential_and_its_quiet_switches_and_nothing_else() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let scene = Scene::new("env.test");
        let asked = scene.answers(
            "gh",
            &["auth", "status", "--hostname", "env.test"],
            0,
            "",
            "",
        );
        scene.forge("github").check_auth().unwrap();

        let env = std::fs::read_to_string(asked.with_extension("env")).unwrap();
        let lines: Vec<&str> = env.lines().collect();
        let bin = std::env::var(BIN).unwrap();
        let home = std::env::var("HOME").unwrap();
        for want in [
            "LC_ALL=C",
            "NO_COLOR=1",
            "GH_PROMPT_DISABLED=1",
            "NO_PROMPT=1",
            "GH_NO_UPDATE_NOTIFIER=1",
            "GLAB_CHECK_UPDATE=false",
            "GH_TOKEN=tok-under-test",
            &format!("HOME={home}"),
        ] {
            assert!(lines.contains(&want), "{want} is missing from:\n{env}");
        }
        let path = lines
            .iter()
            .find_map(|l| l.strip_prefix("PATH="))
            .expect("a PATH");
        assert!(
            path.starts_with(&format!("{bin}:")),
            "the CLI's own directory first: {path}"
        );
        assert!(
            !env.contains("CHARTER_TEST_NOT_A_CREDENTIAL"),
            "only the credential environment is passed through:\n{env}"
        );
    }

    #[test]
    fn a_clones_credential_helper_names_the_cli_that_was_found_by_its_absolute_path() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let bin = std::env::var(BIN).unwrap();
        let forge = charter_core::forge::Forge::default_of(charter_core::forge::Kind::GitHub);
        assert_eq!(
            charter_core::forge::find_cli("gh"),
            Some(Path::new(&bin).join("gh"))
        );
        assert_eq!(
            charter_core::forge::helper_for(&forge),
            format!("!'{bin}/gh' auth git-credential")
        );
    }

    #[test]
    fn an_orgs_repos_are_read_page_by_page_until_a_short_page() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let scene = Scene::new("paged.test");
        scene.gh_api(
            "orgs/ac%20me/repos?per_page=100&page=1",
            0,
            &page(100, "name"),
            "",
        );
        scene.gh_api(
            "orgs/ac%20me/repos?per_page=100&page=2",
            0,
            &page(1, "name"),
            "",
        );

        let repos = scene.forge("github").list_repos("ac me").unwrap();

        assert_eq!(repos.len(), 101);
        assert_eq!(names(&repos)[100], "r0");
        assert_eq!(
            repos[0],
            json!({
                "id": null, "name": "r0", "path_with_namespace": null, "default_branch": null,
                "description": "", "web_url": "", "ssh_url": "", "topics": [], "forge": "github",
            })
        );
    }

    #[test]
    fn a_full_last_page_is_followed_by_the_empty_one_that_ends_the_listing() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let scene = Scene::new("full.test");
        scene.gh_api(
            "orgs/o/repos?per_page=100&page=1",
            0,
            &page(100, "name"),
            "",
        );
        let empty = scene.gh_api("orgs/o/repos?per_page=100&page=2", 0, "[]", "");
        assert_eq!(scene.forge("github").list_repos("o").unwrap().len(), 100);
        assert!(was_asked(&empty));

        // An empty body is a legal, successful answer, read as `[]`.
        let none = Scene::new("none.test");
        none.gh_api("orgs/o/repos?per_page=100&page=1", 0, "\n", "");
        assert_eq!(
            none.forge("github").list_repos("o").unwrap(),
            Vec::<Value>::new()
        );
    }

    #[test]
    fn a_short_first_page_is_the_whole_listing() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let scene = Scene::new("short.test");
        scene.gh_api("orgs/o/repos?per_page=100&page=1", 0, &page(3, "name"), "");
        assert_eq!(
            names(&scene.forge("github").list_repos("o").unwrap()),
            ["r0", "r1", "r2"]
        );
    }

    #[test]
    fn a_github_record_is_normalised_to_the_shape_every_backend_produces() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let scene = Scene::new("normal-gh.test");
        let raw = json!([
            {"id": 1, "name": "widget", "full_name": "acme/widget", "default_branch": "main",
             "description": null, "html_url": "https://github.com/acme/widget",
             "ssh_url": "git@github.com:acme/widget.git", "topics": ["rust"]},
            {"id": 2, "name": "gadget", "full_name": "acme/gadget", "default_branch": "dev",
             "description": "d", "html_url": "", "ssh_url": "", "topics": []},
            // `r.get("topics") or []`: a null is no topics, and never a null in the record.
            {"id": 3, "name": "bare", "topics": null},
        ]);
        scene.gh_api(
            "orgs/acme/repos?per_page=100&page=1",
            0,
            &raw.to_string(),
            "",
        );

        let repos = scene.forge("github").list_repos("acme").unwrap();

        assert_eq!(
            repos,
            vec![
                json!({"id": 1, "name": "widget", "path_with_namespace": "acme/widget",
                       "default_branch": "main", "description": "",
                       "web_url": "https://github.com/acme/widget",
                       "ssh_url": "git@github.com:acme/widget.git", "topics": ["rust"],
                       "forge": "github"}),
                json!({"id": 2, "name": "gadget", "path_with_namespace": "acme/gadget",
                       "default_branch": "dev", "description": "d", "web_url": "",
                       "ssh_url": "", "topics": [], "forge": "github"}),
                json!({"id": 3, "name": "bare", "path_with_namespace": null,
                       "default_branch": null, "description": "", "web_url": "",
                       "ssh_url": "", "topics": [], "forge": "github"}),
            ]
        );
    }

    #[test]
    fn a_personal_account_that_404s_as_an_org_is_listed_as_a_user() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        // gh reports the 404 in its own words on stderr…
        let words = Scene::new("user-words.test");
        words.gh_api(
            "orgs/solo/repos?per_page=100&page=1",
            1,
            "",
            "gh: Not Found (HTTP 404)",
        );
        words.gh_api(
            "users/solo/repos?per_page=100&page=1",
            0,
            &page(2, "name"),
            "",
        );
        assert_eq!(
            names(&words.forge("github").list_repos("solo").unwrap()),
            ["r0", "r1"]
        );

        // …or as the API's JSON body on stdout.
        let body = Scene::new("user-body.test");
        body.gh_api(
            "orgs/solo/repos?per_page=100&page=1",
            1,
            r#"{"message":"Not Found","status":"404"}"#,
            "",
        );
        body.gh_api(
            "users/solo/repos?per_page=100&page=1",
            0,
            &page(1, "name"),
            "",
        );
        assert_eq!(body.forge("github").list_repos("solo").unwrap().len(), 1);
    }

    #[test]
    fn only_the_first_page_of_the_org_probe_can_mean_not_an_org() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let scene = Scene::new("late-404.test");
        scene.gh_api(
            "orgs/o/repos?per_page=100&page=1",
            0,
            &page(100, "name"),
            "",
        );
        scene.gh_api(
            "orgs/o/repos?per_page=100&page=2",
            1,
            "",
            "gh: Not Found (HTTP 404)",
        );
        assert_eq!(
            scene.forge("github").list_repos("o"),
            Err(ForgeError(
                "listing repos for GitHub owner 'o' failed (orgs/o/repos?per_page=100&page=2): \
                 gh: Not Found (HTTP 404)"
                    .into()
            ))
        );

        // A 404 on the USER listing is a failure too, and never a second probe.
        let user = Scene::new("user-404.test");
        user.gh_api("orgs/o/repos?per_page=100&page=1", 1, "", "HTTP 404");
        user.gh_api("users/o/repos?per_page=100&page=1", 1, "", "HTTP 404");
        assert_eq!(
            user.forge("github").list_repos("o"),
            Err(ForgeError(
                "listing repos for GitHub owner 'o' failed (users/o/repos?per_page=100&page=1): \
                 HTTP 404"
                    .into()
            ))
        );
    }

    #[test]
    fn a_failed_listing_says_the_clis_own_words_or_how_it_exited() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let err = Scene::new("fail-err.test");
        err.gh_api(
            "orgs/o/repos?per_page=100&page=1",
            1,
            "ignored",
            "  gh: Server Error (HTTP 502)\n",
        );
        assert_eq!(
            err.forge("github").list_repos("o"),
            Err(ForgeError(
                "listing repos for GitHub owner 'o' failed (orgs/o/repos?per_page=100&page=1): \
                 gh: Server Error (HTTP 502)"
                    .into()
            ))
        );

        let out = Scene::new("fail-out.test");
        out.gh_api(
            "orgs/o/repos?per_page=100&page=1",
            2,
            " said on stdout \n",
            " \n",
        );
        assert_eq!(
            out.forge("github").list_repos("o"),
            Err(ForgeError(
                "listing repos for GitHub owner 'o' failed (orgs/o/repos?per_page=100&page=1): \
                 said on stdout"
                    .into()
            ))
        );

        let mute = Scene::new("fail-mute.test");
        mute.gh_api("orgs/o/repos?per_page=100&page=1", 4, "", "");
        assert_eq!(
            mute.forge("github").list_repos("o"),
            Err(ForgeError(
                "listing repos for GitHub owner 'o' failed (orgs/o/repos?per_page=100&page=1): \
                 gh exited 4"
                    .into()
            ))
        );

        let garbled = Scene::new("fail-json.test");
        garbled.gh_api("orgs/o/repos?per_page=100&page=1", 0, "{not json", "");
        let why = garbled.forge("github").list_repos("o").unwrap_err().0;
        assert!(
            why.starts_with(
                "GitHub API returned malformed JSON (orgs/o/repos?per_page=100&page=1): "
            ),
            "{why}"
        );
    }

    #[test]
    fn a_gitlab_groups_projects_are_read_page_by_page_and_normalised() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let scene = Scene::new("gl-paged.test");
        let first: Vec<Value> = (0..100)
            .map(|i| json!({"path": format!("p{i}"), "name": format!("Name {i}")}))
            .collect();
        scene.glab_api(
            "groups/grp%2Fsub/projects?per_page=100&page=1&include_subgroups=true&archived=false",
            0,
            &Value::Array(first).to_string(),
            "",
        );
        let last = json!([
            {"id": 9, "path": "", "name": "Named", "path_with_namespace": "grp/sub/named",
             "default_branch": "main", "description": "about", "web_url": "https://gl/named",
             "ssh_url_to_repo": "git@gl:grp/sub/named.git", "topics": ["x"]},
        ]);
        scene.glab_api(
            "groups/grp%2Fsub/projects?per_page=100&page=2&include_subgroups=true&archived=false",
            0,
            &last.to_string(),
            "",
        );

        let repos = scene.forge("gitlab").list_repos("grp/sub").unwrap();

        assert_eq!(repos.len(), 101);
        assert_eq!(repos[0]["name"], "p0", "`path` before `name`");
        assert_eq!(repos[0]["ssh_url"], "");
        assert_eq!(
            repos[100],
            json!({"id": 9, "name": "Named", "path_with_namespace": "grp/sub/named",
                   "default_branch": "main", "description": "about",
                   "web_url": "https://gl/named", "ssh_url": "git@gl:grp/sub/named.git",
                   "topics": ["x"], "forge": "gitlab"}),
            "an empty `path` falls back to `name`"
        );
    }

    #[test]
    fn a_gitlab_listing_that_fails_raises_rather_than_reading_as_no_repos() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let scene = Scene::new("gl-fail.test");
        scene.glab_api(
            "groups/g/projects?per_page=100&page=1&include_subgroups=true&archived=false",
            1,
            "",
            "404 Group Not Found",
        );
        assert_eq!(
            scene.forge("gitlab").list_repos("g"),
            Err(ForgeError(
                "listing repos for GitLab group 'g' failed: GitLab API call failed \
                 (groups/g/projects?per_page=100&page=1&include_subgroups=true&archived=false): \
                 404 Group Not Found"
                    .into()
            ))
        );

        let full = Scene::new("gl-full.test");
        full.glab_api(
            "groups/g/projects?per_page=100&page=1&include_subgroups=true&archived=false",
            0,
            &page(100, "path"),
            "",
        );
        full.glab_api(
            "groups/g/projects?per_page=100&page=2&include_subgroups=true&archived=false",
            0,
            "",
            "",
        );
        assert_eq!(full.forge("gitlab").list_repos("g").unwrap().len(), 100);
    }

    #[test]
    fn a_github_tree_is_read_at_the_ref_asked_else_the_default_branch_else_head() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let scene = Scene::new("tree.test");
        let tree = json!({"tree": [
            {"path": "Cargo.toml"}, {"path": "src/lib.rs"}, {"path": "README.md"}, {}
        ]})
        .to_string();
        scene.gh_api("repos/acme/wid%20get/git/trees/feature%2Fx", 0, &tree, "");
        scene.gh_api("repos/acme/wid%20get/git/trees/dev", 0, &tree, "");
        scene.gh_api("repos/acme/wid%20get/git/trees/HEAD", 0, "", "");
        let forge = scene.forge("github");
        let repo = json!({"path_with_namespace": "acme/wid get", "default_branch": "dev"});

        assert_eq!(
            forge.repo_tree_strict(&repo, Some("feature/x")).unwrap(),
            ["Cargo.toml", "README.md", ""]
        );
        assert_eq!(
            forge.repo_tree_strict(&repo, Some("")).unwrap(),
            ["Cargo.toml", "README.md", ""],
            "an empty ref is no ref"
        );
        let bare = json!({"path_with_namespace": "acme/wid get", "default_branch": ""});
        assert_eq!(
            forge.repo_tree_strict(&bare, None).unwrap(),
            Vec::<String>::new(),
            "HEAD, and an empty body is an empty tree"
        );
    }

    #[test]
    fn a_github_tree_that_cannot_be_read_raises() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let scene = Scene::new("tree-fail.test");
        scene.gh_api(
            "repos/acme/w/git/trees/main",
            1,
            "",
            "gh: Not Found (HTTP 404)",
        );
        scene.gh_api("repos/acme/w/git/trees/next", 0, "{", "");
        let forge = scene.forge("github");
        let repo = json!({"path_with_namespace": "acme/w", "default_branch": "main"});

        assert_eq!(
            forge.repo_tree_strict(&repo, None),
            Err(ForgeError(
                "listing tree for acme/w@main failed: gh: Not Found (HTTP 404)".into()
            ))
        );
        let why = forge.repo_tree_strict(&repo, Some("next")).unwrap_err().0;
        assert!(
            why.starts_with("GitHub API returned malformed JSON (tree acme/w@next): "),
            "{why}"
        );
    }

    #[test]
    fn a_gitlab_tree_is_read_page_by_page_at_the_ref_asked() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let scene = Scene::new("gl-tree.test");
        scene.glab_api(
            "projects/42/repository/tree?per_page=100&page=1&ref=feature%2Fx",
            0,
            &page(100, "name"),
            "",
        );
        scene.glab_api(
            "projects/42/repository/tree?per_page=100&page=2&ref=feature%2Fx",
            0,
            r#"[{"name": "last"}, {"path": "no-name"}]"#,
            "",
        );
        scene.glab_api(
            "projects/a%2Fb/repository/tree?per_page=100&page=1",
            1,
            "",
            "boom",
        );
        let forge = scene.forge("gitlab");

        let names = forge
            .repo_tree_strict(&json!({"id": 42}), Some("feature/x"))
            .unwrap();
        assert_eq!(names.len(), 102);
        assert_eq!(names[100..], ["last", ""]);

        assert_eq!(
            forge.repo_tree_strict(&json!({"id": "a/b"}), Some("")),
            Err(ForgeError(
                "GitLab API call failed (projects/a%2Fb/repository/tree?per_page=100&page=1): boom"
                    .into()
            ))
        );
    }

    #[test]
    fn a_github_branchs_open_pull_request_is_its_number() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let asked = "repos/acme/widget/pulls?state=open&head=acme:feature%2Fx&per_page=1";
        let scene = Scene::new("pr.test");
        scene.gh_api(asked, 0, r#"[{"number": 7, "title": "x"}]"#, "");
        assert_eq!(
            scene
                .forge("github")
                .open_change("acme/widget", "feature/x"),
            Ok(Some(json!(7)))
        );

        let none = Scene::new("pr-none.test");
        none.gh_api(asked, 0, "[]", "");
        assert_eq!(
            none.forge("github").open_change("acme/widget", "feature/x"),
            Ok(None)
        );

        // Where Python's `arr[0].get(...)` would have raised.
        let dict = Scene::new("pr-dict.test");
        dict.gh_api(asked, 0, r#"{"message": "Bad credentials"}"#, "");
        assert_eq!(
            dict.forge("github").open_change("acme/widget", "feature/x"),
            Err(Raised)
        );
        let word = Scene::new("pr-word.test");
        word.gh_api(asked, 0, r#"["x"]"#, "");
        assert_eq!(
            word.forge("github").open_change("acme/widget", "feature/x"),
            Err(Raised)
        );
    }

    #[test]
    fn a_best_effort_question_that_fails_in_any_way_is_no_answer() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let asked =
            "projects/acme%2Fwidget/merge_requests?state=opened&source_branch=main&per_page=1";
        let failed = Scene::new("mr-failed.test");
        failed.glab_api(asked, 1, r#"[{"iid": 3}]"#, "");
        assert_eq!(
            failed.forge("gitlab").open_change("acme/widget", "main"),
            Ok(None),
            "a non-zero exit, whatever it printed"
        );
        let empty = Scene::new("mr-empty.test");
        empty.glab_api(asked, 0, " \n", "");
        assert_eq!(
            empty.forge("gitlab").open_change("acme/widget", "main"),
            Ok(None)
        );
        let garbled = Scene::new("mr-garbled.test");
        garbled.glab_api(asked, 0, "[{", "");
        assert_eq!(
            garbled.forge("gitlab").open_change("acme/widget", "main"),
            Ok(None)
        );
        let found = Scene::new("mr-found.test");
        found.glab_api(asked, 0, r#"[{"iid": 3, "id": 900}]"#, "");
        assert_eq!(
            found.forge("gitlab").open_change("acme/widget", "main"),
            Ok(Some(json!(3)))
        );
    }

    #[test]
    fn a_gitlab_pipeline_status_is_read_in_charters_own_words() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let asked = "projects/acme%2Fwidget/pipelines?ref=feature%2Fx&per_page=1";
        for (i, (status, want)) in [
            (json!("running"), Some("running")),
            (json!("created"), Some("pending")),
            (json!("success"), Some("success")),
            (json!("weird"), None),
            (json!(null), None),
            (json!(7), None),
        ]
        .into_iter()
        .enumerate()
        {
            let scene = Scene::new(&format!("pipe{i}.test"));
            scene.glab_api(asked, 0, &json!([{ "status": status }]).to_string(), "");
            assert_eq!(
                scene.forge("gitlab").ci_status("acme/widget", "feature/x"),
                Ok(want.map(str::to_string)),
                "{status}"
            );
        }
        let none = Scene::new("pipe-none.test");
        none.glab_api(asked, 0, "[]", "");
        assert_eq!(
            none.forge("gitlab").ci_status("acme/widget", "feature/x"),
            Ok(None)
        );
        let raised = Scene::new("pipe-raised.test");
        raised.glab_api(asked, 0, r#"{"message": "403 Forbidden"}"#, "");
        assert_eq!(
            raised.forge("gitlab").ci_status("acme/widget", "feature/x"),
            Err(Raised)
        );
    }

    /// Python's `github._ROLLUP_QUERY`, byte for byte.
    const ROLLUP_QUERY: &str = "\nquery($owner:String!, $name:String!, $ref:String!) {\n  \
                                repository(owner:$owner, name:$name) {\n    \
                                ref(qualifiedName:$ref) { target { ... on Commit {\n      \
                                statusCheckRollup { state } } } }\n  }\n}\n";

    pub(super) fn rollup(scene: &Scene, code: i32, out: &str) -> PathBuf {
        let query = format!("query={ROLLUP_QUERY}");
        scene.answers(
            "gh",
            &[
                "api",
                "graphql",
                "--hostname",
                &scene.host,
                "-f",
                &query,
                "-f",
                "owner=acme",
                "-f",
                "name=widget",
                "-f",
                "ref=feature/x",
            ],
            code,
            out,
            "",
        )
    }

    #[test]
    fn a_github_branchs_ci_is_its_status_check_rollup() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let answered = |host: &str, code: i32, out: &str| {
            let scene = Scene::new(host);
            rollup(&scene, code, out);
            scene.forge("github").ci_status("acme/widget", "feature/x")
        };
        let state = |s: &str| {
            json!({"data": {"repository": {"ref": {"target": {"statusCheckRollup": {"state": s}}}}}})
                .to_string()
        };
        assert_eq!(
            answered("roll-fail.test", 0, &state("FAILURE")),
            Ok(Some("failed".into()))
        );
        assert_eq!(
            answered("roll-exp.test", 0, &state("EXPECTED")),
            Ok(Some("pending".into()))
        );
        assert_eq!(answered("roll-odd.test", 0, &state("NEUTRAL")), Ok(None));
        assert_eq!(
            answered("roll-exit.test", 1, &state("SUCCESS")),
            Ok(None),
            "a failed call is no answer"
        );
        assert_eq!(answered("roll-garbled.test", 0, "{"), Ok(None));
        // A missing or falsy link is `(x or {})` and harmless…
        for (i, falsy) in [json!(null), json!(""), json!([]), json!({}), json!(0)]
            .into_iter()
            .enumerate()
        {
            let out = json!({"data": {"repository": falsy}}).to_string();
            assert_eq!(
                answered(&format!("roll-falsy{i}.test"), 0, &out),
                Ok(None),
                "{out}"
            );
        }
        // …and a truthy one that is not an object is where Python raised.
        for (i, odd) in [json!("x"), json!(["x"]), json!(1)].into_iter().enumerate() {
            let out = json!({"data": {"repository": odd}}).to_string();
            assert_eq!(
                answered(&format!("roll-odd{i}.test"), 0, &out),
                Err(Raised),
                "{out}"
            );
        }
    }

    /// A plane declaring `host` as a GitHub, and a clone in it whose `origin` is there.
    pub(super) fn plane_with_a_clone(host: &str) -> (tempfile::TempDir, PathBuf, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let plane = std::fs::canonicalize(dir.path()).unwrap();
        std::fs::write(
            plane.join("charter.toml"),
            format!("schema = 1\n\n[[forge]]\nkind = \"github\"\nhost = \"{host}\"\n"),
        )
        .unwrap();
        let clone = plane.join("workspaces/alpha/widget");
        std::fs::create_dir_all(&clone).unwrap();
        for args in [
            vec!["init", "-q", "-b", "feature/x", "."],
            vec![
                "remote",
                "add",
                "origin",
                &format!("https://{host}/acme/widget.git"),
            ],
        ] {
            let done =
                charter_core::forklock::output(support::unsigned().args(&args).current_dir(&clone))
                    .unwrap();
            assert!(done.status.success(), "{done:?}");
        }
        (dir, plane, clone)
    }

    #[test]
    fn a_refresh_asks_the_forge_its_clones_origin_names_about_the_branch() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        use charter_core::glrefresh::{State, state_for_repo};
        let asked = "repos/acme/widget/pulls?state=open&head=acme:feature%2Fx&per_page=1";

        let scene = Scene::new("refresh.test");
        let (_d, plane, clone) = plane_with_a_clone("refresh.test");
        scene.gh_api(asked, 0, r#"[{"number": "42"}]"#, "");
        rollup(
            &scene,
            0,
            r#"{"data": {"repository": {"ref": {"target": {"statusCheckRollup": {"state": "SUCCESS"}}}}}}"#,
        );
        assert_eq!(
            state_for_repo(&plane, &clone, "feature/x"),
            State {
                change: Some(42),
                ci: Some("success".into()),
                sigil: "#".into(),
            }
        );

        // A shape Python raised on blanks all three fields together, the sigil with them.
        let raised = Scene::new("refresh-raised.test");
        let (_d2, plane, clone) = plane_with_a_clone("refresh-raised.test");
        raised.gh_api(asked, 0, r#"[{"number": 42}]"#, "");
        rollup(&raised, 0, r#"{"data": {"repository": "x"}}"#);
        assert_eq!(
            state_for_repo(&plane, &clone, "feature/x"),
            State::default()
        );
    }
}

mod refreshed {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_refresh_writes_what_the_forge_said_for_a_branch_and_nothing_for_no_branch() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let scene = Scene::new("refresh-all.test");
        let (_d, plane, clone) = child::plane_with_a_clone("refresh-all.test");
        scene.gh_api(
            "repos/acme/widget/pulls?state=open&head=acme:feature%2Fx&per_page=1",
            0,
            r#"[{"number": 7}]"#,
            "",
        );
        child::rollup(
            &scene,
            0,
            r#"{"data": {"repository": {"ref": {"target": {"statusCheckRollup": {"state": "PENDING"}}}}}}"#,
        );
        let nowhere = plane.join("workspaces/alpha/not-a-clone");
        std::fs::create_dir_all(&nowhere).unwrap();

        let cache =
            charter_core::glrefresh::refresh(&plane, &[clone.clone(), nowhere.clone()], 5.0);

        assert_eq!(
            cache[&charter_core::glrefresh::key_for(&clone)],
            json!({"branch": "feature/x", "ts": 5.0, "change": 7, "ci": "pending", "sigil": "#"})
        );
        assert_eq!(
            cache[&charter_core::glrefresh::key_for(&nowhere)],
            json!({"branch": "?", "ts": 5.0, "change": null, "ci": null, "sigil": ""})
        );
    }
}

mod quoted {
    use super::*;

    #[test]
    fn a_path_holding_a_quote_gets_the_bare_helper() {
        charter_core::unsteered!();
        if !in_child() {
            return;
        }
        let bin = std::env::var(BIN).unwrap();
        assert!(bin.contains('\''), "{bin}");
        let forge = charter_core::forge::Forge::default_of(charter_core::forge::Kind::GitLab);
        assert_eq!(
            charter_core::forge::find_cli("glab"),
            Some(Path::new(&bin).join("glab")),
            "found where the stand-in is, so the bare name below is the quote's doing"
        );
        assert_eq!(
            charter_core::forge::helper_for(&forge),
            "!glab auth git-credential"
        );
    }
}
