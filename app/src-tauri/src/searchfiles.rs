//! ⌘⇧F's content search (FM-8, #1111): the Search view tab's query run over a scope — a branch,
//! a workspace, a project, or every project open in charter — with its hits streamed back to the
//! window that asked.
//!
//! Thin, as `findfiles.rs` is: what is read, how a line matches and what is never looked inside
//! are `purlis_core::files::search`'s answers. This keeps each window's running searches, one
//! per Search tab, and pages them:
//!
//! - **Streamed.** A page's files are sent as `files-searched` events to the window that asked,
//!   a batch at a time, each tagged with the tab's search `id` and the `run` it belongs to, so
//!   a late batch of an older run is dropped by the window rather than drawn under a new query.
//! - **Capped.** A page stops after [`PAGE_LINES`] matching lines, or the core's time budget, and
//!   says so; `search_files_more` continues the same walk where it stopped ("Show more").
//! - **Cancelled.** A new run for the same `id` stops the old one before it starts; closing the
//!   tab, or the window, stops it too. A window keeps at most [`KEPT`] searches, the oldest let
//!   go of first.
//!
//! **The window names projects and branches, never directories**, exactly as ⌘P does: the scope
//! is `findfiles::FileScope`, resolved against the registry, and "every open project" is the
//! registry's own list.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use purlis_core::files::{self, Ended, Named, Place, Search, Searched};

use crate::findfiles::{FileScope, branches_in, projects_of};
use crate::planes::{PlaneId, Planes};

/// The event a window is sent.
pub(crate) const HEARD: &str = "files-searched";

/// The matching lines one page shows before it stops and offers more: enough to fill the tab a
/// few times over, few enough that a common word does not send a whole repository.
pub const PAGE_LINES: usize = 200;

/// The most files one event carries, and the longest a batch is held before it is sent: the
/// first hits show at once, and a page of hundreds is not hundreds of events.
const BATCH_FILES: usize = 25;
const BATCH_TIME: Duration = Duration::from_millis(100);

/// The most searches one window keeps: one per Search tab, and past this many the oldest is
/// stopped and let go of.
pub const KEPT: usize = 8;

/// How a query is matched, as the Search tab's toggles say.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SearchOptions {
    /// A regular expression, rather than the text as typed.
    pub regex: bool,
    /// Letters match only in the case typed.
    pub match_case: bool,
    /// Only a whole word matches.
    pub whole_word: bool,
}

/// One stretch of a matching line.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct SearchedPart {
    pub text: String,
    /// Whether this stretch is what matched.
    pub hit: bool,
}

/// One matching line: its number, from 1, and its stretches.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct SearchedLine {
    pub number: u32,
    pub parts: Vec<SearchedPart>,
    /// Whether the line was longer than what is shown of it.
    pub clipped: bool,
}

/// One file with matches: its project, its branch, its path, how many of its lines match, and
/// the first of them.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct SearchedFile {
    pub plane: PlaneId,
    pub workspace: String,
    pub repo: String,
    /// No piece is the repo's own folder.
    pub piece: Option<String>,
    pub path: String,
    pub count: u32,
    pub lines: Vec<SearchedLine>,
}

/// Why a page of a search ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(rename_all = "kebab-case")]
pub enum SearchEnd {
    /// The whole scope is searched.
    Done,
    /// A page's lines were found; there may be more.
    Capped,
    /// A page's time ran out; there may be more.
    OutOfTime,
    /// A newer run, or the tab closing, stopped it.
    Stopped,
}

/// What `files-searched` carries: files a run found since the last batch, the branches it could
/// not search, and — on the page's last batch — why the page ended.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, specta::Type)]
pub struct FilesSearched {
    pub id: u32,
    pub run: u32,
    pub files: Vec<SearchedFile>,
    pub refused: Vec<String>,
    /// Each file not searched, and why — a line too long to match, or matching it took past a
    /// page's time — as `<path> in <branch>: <why>`.
    pub unsearched: Vec<String>,
    pub ended: Option<SearchEnd>,
}

/// Told each batch: the window's label and what to send it.
pub type Told = Arc<dyn Fn(&str, FilesSearched) + Send + Sync + 'static>;

/// One run of one Search tab.
struct Running {
    run: u32,
    stop: Arc<AtomicBool>,
    /// The walk, held between pages; locked while a page runs.
    search: Mutex<Search>,
    /// Each place of the scope, by its index in the core's answers.
    places: Vec<(PlaneId, Named)>,
}

/// Every window's searches, by window.
#[derive(Default)]
pub struct FileSearches {
    windows: Mutex<HashMap<String, Window>>,
}

/// One window's searches.
#[derive(Default)]
struct Window {
    /// By the Search tab's id, oldest first.
    kept: Vec<(u32, Arc<Running>)>,
    /// The newest run each tab asked for: a run whose branches took longer to list than the
    /// next keystroke's is never kept over it.
    asked: HashMap<u32, u32>,
}

impl FileSearches {
    /// The tab `id` asked for its run `run`: whatever it had running stops now, before the new
    /// run's branches are listed.
    fn begin(&self, window: &str, id: u32, run: u32) {
        let mut windows = self.windows.lock().unwrap_or_else(PoisonError::into_inner);
        let held = windows.entry(window.to_string()).or_default();
        stop_tab(&mut held.kept, id);
        held.asked.insert(id, run);
    }

    /// Keeps `running` as the window's search `id`, unless it is no longer the run the tab
    /// asked for last — a newer one was asked, or the tab closed — when it is stopped and
    /// `false` answered. The run it replaces, and past [`KEPT`]
    /// the window's oldest, are stopped.
    fn keep(&self, window: &str, id: u32, running: Arc<Running>) -> bool {
        let mut windows = self.windows.lock().unwrap_or_else(PoisonError::into_inner);
        let held = windows.entry(window.to_string()).or_default();
        if held.asked.get(&id) != Some(&running.run) {
            running.stop.store(true, Ordering::Relaxed);
            return false;
        }
        stop_tab(&mut held.kept, id);
        held.kept.push((id, running));
        while held.kept.len() > KEPT {
            let (oldest, running) = held.kept.remove(0);
            running.stop.store(true, Ordering::Relaxed);
            held.asked.remove(&oldest);
        }
        true
    }

    /// The window's search `id`, if `run` is still the one kept for it.
    fn running(&self, window: &str, id: u32, run: u32) -> Option<Arc<Running>> {
        let windows = self.windows.lock().unwrap_or_else(PoisonError::into_inner);
        windows
            .get(window)?
            .kept
            .iter()
            .find(|(one, running)| *one == id && running.run == run)
            .map(|(_, running)| Arc::clone(running))
    }

    /// The tab closed: its search stops and is let go of.
    fn end(&self, window: &str, id: u32) {
        let mut windows = self.windows.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(held) = windows.get_mut(window) {
            stop_tab(&mut held.kept, id);
            held.asked.remove(&id);
        }
    }

    /// The window went: every search it had stops and is let go of.
    pub fn forget(&self, window: &str) {
        let mut windows = self.windows.lock().unwrap_or_else(PoisonError::into_inner);
        for (_, running) in windows.remove(window).unwrap_or_default().kept {
            running.stop.store(true, Ordering::Relaxed);
        }
    }
}

/// Stops the tab `id`'s run, and lets go of it.
fn stop_tab(kept: &mut Vec<(u32, Arc<Running>)>, id: u32) {
    kept.retain(|(one, running)| {
        let same = *one == id;
        if same {
            running.stop.store(true, Ordering::Relaxed);
        }
        !same
    });
}

/// The search the app sends batches through: told by `lib.rs` how to reach a window.
pub struct SearchTeller(pub Told);

/// Starts the Search tab `id`'s run `run`: `query` over `scope`, matched as `options` say. The
/// tab's earlier run stops first. Answers how many branches the scope covers; the hits arrive as
/// `files-searched` events, and a query that cannot be searched is refused here, in a sentence.
// The projects are resolved here, against the registry; the branches are listed and the first
// page runs on a blocking thread, never the one that draws (SC-2). Not a doc comment, because
// the generated bindings carry those.
#[tauri::command]
#[specta::specta]
#[allow(clippy::too_many_arguments)]
pub async fn search_files(
    window: tauri::Window,
    planes: tauri::State<'_, Planes>,
    searches: tauri::State<'_, Arc<FileSearches>>,
    teller: tauri::State<'_, SearchTeller>,
    id: u32,
    run: u32,
    scope: FileScope,
    query: String,
    options: SearchOptions,
) -> Result<u32, String> {
    let projects = projects_of(&scope, &planes)?;
    // Stopped before anything is listed, so a keystroke never waits behind the last one's page.
    searches.begin(window.label(), id, run);
    let searches = Arc::clone(&searches);
    let told = Arc::clone(&teller.0);
    let label = window.label().to_string();
    tauri::async_runtime::spawn_blocking(move || {
        let running = Arc::new(start(&scope, &projects, run, &query, options)?);
        let branches = u32::try_from(running.places.len()).unwrap_or(u32::MAX);
        if searches.keep(&label, id, Arc::clone(&running)) {
            page(&running, id, &|batch| told(&label, batch));
        }
        Ok(branches)
    })
    .await
    .map_err(|err| format!("the search did not finish: {err}"))?
}

/// "Show more": the Search tab `id`'s run `run` continues where its last page stopped. Nothing
/// happens for a run that is no longer the tab's, or one whose page is still running.
#[tauri::command]
#[specta::specta]
pub async fn search_files_more(
    window: tauri::Window,
    searches: tauri::State<'_, Arc<FileSearches>>,
    teller: tauri::State<'_, SearchTeller>,
    id: u32,
    run: u32,
) -> Result<(), String> {
    let Some(running) = searches.running(window.label(), id, run) else {
        return Err("This search has ended; search again.".to_string());
    };
    let told = Arc::clone(&teller.0);
    let label = window.label().to_string();
    tauri::async_runtime::spawn_blocking(move || page(&running, id, &|batch| told(&label, batch)))
        .await
        .map_err(|err| format!("the search did not finish: {err}"))
}

/// The Search tab `id` closed: its search stops and is let go of.
#[tauri::command]
#[specta::specta]
pub fn search_files_end(
    window: tauri::Window,
    searches: tauri::State<'_, Arc<FileSearches>>,
    id: u32,
) {
    searches.end(window.label(), id);
}

/// A run of `query` over `scope`, once its projects are vouched for: its branches listed, and
/// the core's search built.
fn start(
    scope: &FileScope,
    projects: &[PlaneId],
    run: u32,
    query: &str,
    options: SearchOptions,
) -> Result<Running, String> {
    let places = branches_in(scope, projects, &mut |root| files::branches(root));
    let roots: Vec<std::path::PathBuf> = places
        .iter()
        .map(|(plane, _)| plane.root().into())
        .collect();
    let scope: Vec<Place<'_>> = places
        .iter()
        .zip(&roots)
        .map(|((_, one), root)| Place {
            plane: root,
            branch: one.branch(),
        })
        .collect();
    let search = files::search(
        &scope,
        query,
        files::SearchOptions {
            regex: options.regex,
            match_case: options.match_case,
            whole_word: options.whole_word,
        },
    )
    .map_err(|bad| bad.to_string())?
    // Each branch's folder found by the bounded reader's child, so walking a branch starts no
    // git to find it in this process (#1189).
    .reading_with(crate::reader());
    Ok(Running {
        run,
        stop: Arc::new(AtomicBool::new(false)),
        search: Mutex::new(search),
        places,
    })
}

/// One page of `running`, told in batches: files as they are found, a batch at most
/// [`BATCH_FILES`] files or [`BATCH_TIME`] old, and a last batch that says why the page ended.
/// A page already running for it is not run twice: this one says nothing.
fn page(running: &Running, id: u32, told: &dyn Fn(FilesSearched)) {
    let Ok(mut search) = running.search.try_lock() else {
        return;
    };
    let fresh = || FilesSearched {
        id,
        run: running.run,
        files: Vec::new(),
        refused: Vec::new(),
        unsearched: Vec::new(),
        ended: None,
    };
    let mut batch = fresh();
    let mut since = Instant::now();
    let ended = search.more(PAGE_LINES, &running.stop, &mut |step| {
        match step {
            Searched::File(hits) => batch.files.push(file_of(&running.places, hits)),
            Searched::Refused { why, .. } => batch.refused.push(why),
            Searched::NotSearched { at, path, why } => {
                let one = &running.places[at].1;
                let branch = one.piece.as_deref().unwrap_or(&one.repo);
                batch.unsearched.push(format!("{path} in {branch}: {why}"));
            }
        }
        if batch.files.len() >= BATCH_FILES || since.elapsed() >= BATCH_TIME {
            told(std::mem::replace(&mut batch, fresh()));
            since = Instant::now();
        }
    });
    batch.ended = Some(match ended {
        Ended::Done => SearchEnd::Done,
        Ended::Capped => SearchEnd::Capped,
        Ended::OutOfTime => SearchEnd::OutOfTime,
        Ended::Stopped => SearchEnd::Stopped,
    });
    told(batch);
}

/// A file the core found, named by its project and branch.
fn file_of(places: &[(PlaneId, Named)], hits: files::FileHits) -> SearchedFile {
    let (plane, one) = &places[hits.at];
    let narrow = |n: u64| u32::try_from(n).unwrap_or(u32::MAX);
    SearchedFile {
        plane: plane.clone(),
        workspace: one.ws.clone(),
        repo: one.repo.clone(),
        piece: one.piece.clone(),
        path: hits.path,
        count: narrow(hits.count),
        lines: hits
            .lines
            .into_iter()
            .map(|line| SearchedLine {
                number: narrow(line.number),
                parts: line
                    .parts
                    .into_iter()
                    .map(|part| SearchedPart {
                        text: part.text,
                        hit: part.hit,
                    })
                    .collect(),
                clipped: line.clipped,
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn git(dir: &Path, args: &[&str]) {
        let ran = purlis_core::forklock::output(
            std::process::Command::new("git")
                .arg("-C")
                .arg(dir)
                .args(["-c", "user.email=t@e.invalid", "-c", "user.name=t"])
                .args(["-c", "commit.gpgsign=false"])
                .args(args),
        )
        .unwrap();
        assert!(ran.status.success(), "git {args:?}: {ran:?}");
    }

    /// A project with a clone and one piece cut from it, whose `src/a.rs` holds `needle`
    /// `lines` times, once a line.
    fn project(lines: usize) -> (tempfile::TempDir, PlaneId) {
        let dir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        std::fs::write(root.join("charter.toml"), "schema = 1\n").unwrap();
        let clone = root.join("workspaces/alpha/thing");
        std::fs::create_dir_all(&clone).unwrap();
        git(&clone, &["init", "-q", "-b", "main", "."]);
        std::fs::write(clone.join("README.md"), "one\n").unwrap();
        git(&clone, &["add", "-A"]);
        git(&clone, &["commit", "-q", "-m", "one"]);
        let piece = purlis_core::worktree::add(&root, "alpha", "thing", "piece", None)
            .unwrap()
            .path;
        std::fs::create_dir_all(piece.join("src")).unwrap();
        std::fs::write(piece.join("src/a.rs"), "needle\n".repeat(lines)).unwrap();
        (dir, PlaneId::for_tests(&root))
    }

    fn every_open(front: Option<PlaneId>) -> FileScope {
        FileScope::OpenProjects { front, near: None }
    }

    /// Every batch one page tells.
    fn paged(running: &Running) -> Vec<FilesSearched> {
        let heard = Mutex::new(Vec::new());
        page(running, 7, &|batch| heard.lock().unwrap().push(batch));
        heard.into_inner().unwrap()
    }

    #[test]
    fn every_open_project_is_searched_and_only_those_the_registry_names() {
        let (_one, first) = project(1);
        let (_two, second) = project(1);
        let (_three, closed) = project(1);

        let running = start(
            &every_open(Some(second.clone())),
            &[first.clone(), second.clone()],
            1,
            "needle",
            SearchOptions::default(),
        )
        .unwrap();
        let batches = paged(&running);

        let found: Vec<(PlaneId, Option<String>, String)> = batches
            .iter()
            .flat_map(|batch| &batch.files)
            .map(|file| (file.plane.clone(), file.piece.clone(), file.path.clone()))
            .collect();
        assert_eq!(
            found,
            [
                (second.clone(), Some("piece".into()), "src/a.rs".into()),
                (first.clone(), Some("piece".into()), "src/a.rs".into()),
            ],
            "the project in front first, and nothing of {closed:?}"
        );
        assert_eq!(batches.last().unwrap().ended, Some(SearchEnd::Done));
        assert!(batches.iter().all(|batch| batch.id == 7 && batch.run == 1));
    }

    #[test]
    fn a_workspace_scope_covers_that_workspaces_branches_only() {
        let (_one, plane) = project(1);
        let scope = |workspace: &str| FileScope::Workspace {
            plane: plane.clone(),
            workspace: workspace.into(),
            near: None,
        };

        let alpha = start(
            &scope("alpha"),
            std::slice::from_ref(&plane),
            1,
            "x",
            SearchOptions::default(),
        )
        .unwrap();
        let other = start(
            &scope("beta"),
            std::slice::from_ref(&plane),
            1,
            "x",
            SearchOptions::default(),
        )
        .unwrap();

        assert_eq!(
            alpha.places.len(),
            2,
            "the clone's own folder and its piece"
        );
        assert!(other.places.is_empty());
    }

    #[test]
    fn a_page_stops_at_its_lines_says_so_and_more_continues_the_same_walk() {
        // Each file shows at most `FILE_LINES` of its lines, and a page counts what it shows:
        // `a.rs` and `b.rs` fill the first page, and `c.rs` is the next one's.
        let (_one, plane) = project(PAGE_LINES + 50);
        let piece = plane.root().join("workspaces/alpha/.worktrees/thing/piece");
        let shown = files::FILE_LINES;
        assert_eq!(PAGE_LINES, 2 * shown, "the page this test fills");
        std::fs::write(piece.join("src/b.rs"), "needle\n".repeat(shown)).unwrap();
        std::fs::write(piece.join("src/c.rs"), "needle\n").unwrap();
        let running = start(
            &FileScope::Branch {
                plane: plane.clone(),
                workspace: "alpha".into(),
                repo: "thing".into(),
                piece: Some("piece".into()),
            },
            std::slice::from_ref(&plane),
            3,
            "needle",
            SearchOptions::default(),
        )
        .unwrap();

        let first = paged(&running);
        let second = paged(&running);

        let paths = |batches: &[FilesSearched]| -> Vec<String> {
            batches
                .iter()
                .flat_map(|batch| &batch.files)
                .map(|file| file.path.clone())
                .collect()
        };
        assert_eq!(paths(&first), ["src/a.rs", "src/b.rs"]);
        assert_eq!(first.last().unwrap().ended, Some(SearchEnd::Capped));
        assert_eq!(
            first.iter().flat_map(|b| &b.files).next().unwrap().count,
            (PAGE_LINES + 50) as u32
        );
        assert_eq!(paths(&second), ["src/c.rs"]);
        assert_eq!(second.last().unwrap().ended, Some(SearchEnd::Done));
    }

    #[test]
    fn a_new_run_for_the_same_tab_stops_the_old_and_the_old_answers_no_more() {
        let searches = FileSearches::default();
        let (_one, plane) = project(1);
        let make = |run| {
            Arc::new(
                start(
                    &every_open(None),
                    std::slice::from_ref(&plane),
                    run,
                    "needle",
                    SearchOptions::default(),
                )
                .unwrap(),
            )
        };
        let old = make(1);
        searches.begin("main", 4, 1);
        assert!(searches.keep("main", 4, Arc::clone(&old)));
        let other_tab = make(1);
        searches.begin("main", 5, 1);
        assert!(searches.keep("main", 5, Arc::clone(&other_tab)));

        searches.begin("main", 4, 3);
        assert!(
            old.stop.load(Ordering::Relaxed),
            "asking stops the old run at once"
        );
        let slow = make(2);
        assert!(
            !searches.keep("main", 4, Arc::clone(&slow)),
            "a run older than the newest asked is not kept"
        );
        assert!(slow.stop.load(Ordering::Relaxed));
        assert!(searches.keep("main", 4, make(3)));

        assert!(old.stop.load(Ordering::Relaxed), "the old run was stopped");
        assert!(
            !other_tab.stop.load(Ordering::Relaxed),
            "another tab's run goes on"
        );
        assert!(searches.running("main", 4, 1).is_none());
        assert!(searches.running("main", 4, 3).is_some());
        assert_eq!(
            paged(&old).last().unwrap().ended,
            Some(SearchEnd::Stopped),
            "a stopped run reads nothing more"
        );
        searches.forget("main");
        assert!(other_tab.stop.load(Ordering::Relaxed));
        assert!(searches.running("main", 5, 1).is_none());
    }

    #[test]
    fn a_run_that_arrives_after_its_tab_closed_is_not_kept() {
        let searches = FileSearches::default();
        let (_one, plane) = project(1);
        let late = Arc::new(
            start(
                &every_open(None),
                std::slice::from_ref(&plane),
                1,
                "needle",
                SearchOptions::default(),
            )
            .unwrap(),
        );
        searches.begin("main", 4, 1);

        searches.end("main", 4);

        assert!(!searches.keep("main", 4, Arc::clone(&late)));
        assert!(late.stop.load(Ordering::Relaxed));
        assert!(searches.running("main", 4, 1).is_none());
    }

    #[test]
    fn a_window_keeps_only_its_newest_searches() {
        let searches = FileSearches::default();
        let (_one, plane) = project(1);
        let mut kept = Vec::new();
        for id in 0..=(KEPT as u32) {
            let running = Arc::new(
                start(
                    &every_open(None),
                    std::slice::from_ref(&plane),
                    1,
                    "needle",
                    SearchOptions::default(),
                )
                .unwrap(),
            );
            searches.begin("main", id, 1);
            assert!(searches.keep("main", id, Arc::clone(&running)));
            kept.push(running);
        }

        assert!(kept[0].stop.load(Ordering::Relaxed));
        assert!(searches.running("main", 0, 1).is_none());
        assert!(searches.running("main", KEPT as u32, 1).is_some());
    }

    #[test]
    fn a_file_not_searched_is_said_with_its_branch_and_why() {
        let (_one, plane) = project(1);
        let piece = plane.root().join("workspaces/alpha/.worktrees/thing/piece");
        let long = format!("{}needle\n", "x".repeat(files::LONGEST_LINE + 1));
        std::fs::write(piece.join("bundle.min.js"), long).unwrap();
        let running = start(
            &every_open(None),
            std::slice::from_ref(&plane),
            1,
            "needle",
            SearchOptions::default(),
        )
        .unwrap();

        let batches = paged(&running);

        let unsearched: Vec<&String> = batches.iter().flat_map(|b| &b.unsearched).collect();
        assert_eq!(unsearched.len(), 1, "{unsearched:?}");
        assert!(
            unsearched[0].starts_with("bundle.min.js in piece: ")
                && unsearched[0].contains("line longer than"),
            "{unsearched:?}"
        );
    }

    #[test]
    fn a_query_that_cannot_be_searched_is_refused_in_a_sentence() {
        let (_one, plane) = project(1);
        let bad = start(
            &every_open(None),
            std::slice::from_ref(&plane),
            1,
            "open(",
            SearchOptions {
                regex: true,
                ..SearchOptions::default()
            },
        );
        assert!(bad.is_err_and(|why| why.contains("unclosed")));
    }
}
