//! `charter workspace create` — a workspace directory, its baseline, and charter's layer.
//!
//! The command the whole of [`crate::wslayer`] exists for: a workspace whose directory is
//! there but whose layer is not is a chat running with none of the plane's ask/deny rules and
//! no `$CHARTER_HARNESS`, and it looks exactly like a workspace that is fine.

use std::path::Path;

use crate::repocmd::{Say, Sink};
use crate::wscmd::{self, ensure};

/// What `create` was asked for.
pub struct Request<'a> {
    pub root: &'a Path,
    pub name: &'a str,
    /// Recorded in `workspace.md`'s `## Vision`.
    pub vision: Option<&'a str>,
    /// Share the manifest and memory from birth.
    pub live: bool,
    /// Select it for this session once it exists.
    pub use_it: bool,
    /// Select it even though this session is locked to another workspace.
    pub force: bool,
    /// Repos to clone into it, by inventory name — `charter clone`'s argument.
    pub repos: &'a [String],
    pub now: chrono::DateTime<chrono::Utc>,
    pub ids: &'a crate::active::Ids,
}

/// Run it, and give back the exit code.
///
/// Exit 2 is the lock guard, as it is for `workspace use`: the workspace WAS created and only
/// the selection was refused, and a script has to be able to tell that from a name that is
/// not a workspace.
pub fn create(request: &Request, say: Sink) -> u8 {
    let author = ensure::author();
    if let Err(why) = ensure::ensure(request.root, request.name, request.now, &author) {
        say(Say::Fail(why));
        return 1;
    }
    let name = request.name;
    if let Some(text) = request.vision {
        let plane = crate::workspaces::Plane::open(request.root);
        if let Ok(workspace) = plane.workspace(name) {
            let _ = workspace.set_vision(text);
        }
    }
    if request.live {
        let _ = wscmd::set_live(request.root, name, true);
    }

    // The MODE is read off the flag, not off the workspace: `create` on a workspace that is
    // already LIVE says LOCAL when `--live` was not typed. Faithful rather than tightened —
    // the two implementations must describe one plane the same way, and this sentence is a
    // report of what this call was asked to do.
    let mode = if request.live {
        "LIVE — charter + manifest + memory committed + shared + auto-saved"
    } else {
        "LOCAL — private (nothing committed); `charter workspace live` to share"
    };
    say(Say::Done(format!(
        "Workspace '{name}' ready ({mode}) → workspaces/{name}/"
    )));
    if request.vision.is_some() {
        say(Say::Info(format!(
            "Vision recorded → workspaces/{name}/workspace.md"
        )));
    } else {
        say(Say::Info(format!(
            "⬢ No vision yet — ask the developer what this workspace is for, then record it: \
             charter workspace vision \"<the goal>\"  (it seeds workspaces/{name}/workspace.md, \
             the living charter a fork inherits)."
        )));
    }

    if request.use_it {
        let before = wscmd::select::is_locked(request.root, request.ids);
        match wscmd::select::set_active(request.root, name, request.ids, request.force) {
            wscmd::select::Scope::Locked => {
                let held = before.unwrap_or_else(|| "?".to_string());
                say(Say::Fail(wscmd::select::locked_msg(name, &held)));
                say(Say::Info(format!(
                    "Workspace '{name}' was created; start a new session to use it, or re-run \
                     with --force."
                )));
                return 2;
            }
            scope => {
                wscmd::select::announce(
                    request.root,
                    name,
                    scope,
                    before.as_deref(),
                    request.ids,
                    say,
                );
                wscmd::select::warn_env_override(name, say);
            }
        }
    }

    if !request.repos.is_empty() {
        // The early return the Python takes: with repos to clone, the clone's own report is
        // the end of this command and the "Select it with" line below is never printed.
        return crate::repocmd::clone::clone(
            &crate::repocmd::clone::Request {
                root: request.root,
                ws: name,
                repos: request.repos,
                now: request.now,
                author: &author,
            },
            say,
        );
    }
    if !request.use_it {
        say(Say::Info(format!(
            "Select it with: charter workspace use {name}  (or --workspace {name} per command)"
        )));
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plane() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("charter.toml"), "").unwrap();
        std::fs::create_dir_all(dir.path().join(".claude")).unwrap();
        std::fs::write(
            dir.path().join(".claude").join("settings.json"),
            r#"{"env":{"CHARTER_HARNESS":"claude-code"}}"#,
        )
        .unwrap();
        std::fs::create_dir_all(dir.path().join("workspaces")).unwrap();
        dir
    }

    fn now() -> chrono::DateTime<chrono::Utc> {
        "2026-05-04T11:32:17Z".parse().unwrap()
    }

    fn run(root: &Path, name: &str, vision: Option<&str>, live: bool) -> (u8, Vec<String>) {
        let mut lines = Vec::new();
        let code = create(
            &Request {
                root,
                name,
                vision,
                live,
                use_it: false,
                force: false,
                repos: &[],
                now: now(),
                ids: &crate::active::Ids::default(),
            },
            &mut |s| lines.push(s.to_string()),
        );
        (code, lines)
    }

    #[test]
    fn a_created_workspace_is_wired_before_the_command_says_it_is_ready() {
        let dir = plane();
        let (code, lines) = run(dir.path(), "gamma", None, false);
        assert_eq!(code, 0);
        assert!(
            lines[0].contains("Workspace 'gamma' ready (LOCAL"),
            "{lines:?}"
        );
        assert!(lines[1].contains("No vision yet"), "{lines:?}");
        assert!(lines[2].contains("Select it with"), "{lines:?}");
        // The whole ticket: a chat started in that directory gets the plane's layer.
        let settings = dir.path().join("workspaces/gamma/.claude/settings.json");
        assert!(
            std::fs::read_to_string(settings)
                .unwrap()
                .contains("CHARTER_HARNESS"),
            "the layer is what makes the directory usable"
        );
    }

    #[test]
    fn a_vision_replaces_the_no_vision_line_and_lands_in_the_charter() {
        let dir = plane();
        let (_code, lines) = run(dir.path(), "gamma", Some("Ship the importer"), false);
        assert!(lines[1].contains("Vision recorded"), "{lines:?}");
        assert!(!lines.iter().any(|l| l.contains("No vision yet")));
        let charter =
            std::fs::read_to_string(dir.path().join("workspaces/gamma/workspace.md")).unwrap();
        assert!(charter.contains("Ship the importer"), "{charter}");
    }

    #[test]
    fn live_says_live_and_un_ignores_the_workspaces_shared_paths() {
        let dir = plane();
        let (_code, lines) = run(dir.path(), "gamma", None, true);
        assert!(lines[0].contains("(LIVE —"), "{lines:?}");
        let ignore = std::fs::read_to_string(dir.path().join(".gitignore")).unwrap();
        assert!(
            ignore.contains("!/workspaces/gamma/workspace.json"),
            "{ignore}"
        );
    }

    #[test]
    fn a_name_that_is_not_a_workspace_name_creates_nothing_and_exits_one() {
        let dir = plane();
        let (code, lines) = run(dir.path(), "../esc", None, false);
        assert_eq!(code, 1);
        assert!(lines[0].contains("invalid workspace name"), "{lines:?}");
        assert_eq!(lines.len(), 1, "nothing else is said: {lines:?}");
        assert!(!dir.path().parent().unwrap().join("esc").exists());
    }
}
