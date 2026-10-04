//! Which file a chat is touching right now (FM-6, #1109; V86 F6, D-86a).
//!
//! A file tool's hook already sees the path the tool was given. The hook names it to the app
//! on a line of its own ([`crate::hookwire::Touching`]) and the app tells the window, which
//! marks that file in the tree for a few seconds. **The path lives in memory only**: the line
//! is never spooled, never written to the event log (which keeps only the arguments' digest,
//! ADR 0066), and never put in `reopen.json` or any other store.
//!
//! What a chat says it touched is the chat's word, and a chat can write to the hook socket
//! with its own token. So the app believes none of it until this module has:
//!
//! - **confined it** ([`confine`]): to a path inside the chat's own folder, with no `..`, no
//!   `.git`, nothing empty, nothing unprintable and nothing too long. Anything else is dropped,
//!   so no chat can put a marker on a file outside its branch;
//! - **rated it** ([`Gate`]): at most [`AT_MOST_PER_SECOND`] markers a second per chat, so no
//!   chat can flood the window. The same path again inside [`SAME_PATH_AGAIN`] is dropped too:
//!   the window already shows it.
//!
//! Which tools count, per harness:
//!
//! - **Claude Code:** `Read`, `Grep` (its `path`, when it names one), `Write`, `Edit` and
//!   `MultiEdit`: exactly the tools the `pretooluse-read`, `pretooluse-edit` and `posttooluse`
//!   matchers arm (`hookreg`). `NotebookEdit` is in no matcher, so no hook of it ever runs, and
//!   it is not listed here either.
//! - **opencode:** `read`, `grep`, `write` and `edit`, which charter's plugin routes to the same
//!   hooks under the same names, carrying `filePath` (or `path`).
//! - **Codex:** none. Codex is armed with the Bash guard only (`plugin::CODEX`): its hooks see
//!   shell commands, and a path inside a shell command is not a file tool's path. Its edits go
//!   through `apply_patch`, which its `PreToolUse` does not report on the Bash matcher.

use std::collections::HashMap;
use std::path::{Component, Path};
use std::time::{Duration, Instant};

use serde_json::Value;

/// The tools whose input names the file they touch, by the name the hook payload carries.
///
/// The tools some armed hook's matcher names, and no others: a tool no hook runs on could never
/// reach this.
pub const FILE_TOOLS: [&str; 5] = ["Read", "Grep", "Write", "Edit", "MultiEdit"];

/// Where a file tool carries its path: Claude Code's spelling, opencode's, then `Grep`'s.
const PATH_KEYS: [&str; 3] = ["file_path", "filePath", "path"];

/// The longest path a marker is kept for, in bytes. A tree row past it is not worth drawing.
pub const LONGEST: usize = 1024;

/// The path a file tool's hook payload says the tool touches, as the harness spelled it.
///
/// `None` for any other tool, and for a file tool that names no path (a `Grep` over the whole
/// folder).
pub fn of_tool(payload: &Value) -> Option<String> {
    let tool = payload.get("tool_name")?.as_str()?;
    if !FILE_TOOLS.contains(&tool) {
        return None;
    }
    let input = payload.get("tool_input")?;
    PATH_KEYS
        .iter()
        .find_map(|key| input.get(*key)?.as_str().filter(|said| !said.is_empty()))
        .map(str::to_owned)
}

/// `said`, as a path inside `folder` written with `/`, or `None` when it is not one.
///
/// An absolute path must lie under `folder` (as given, or as the file system resolves it); a
/// relative one is read from `folder`. Either way it is refused if any part of it is `..` or
/// `.git`, if it names `folder` itself, if it holds a control character, or if it is longer
/// than [`LONGEST`]. Nothing is opened: this is a check of the words, and the window only
/// draws them.
pub fn confine(folder: &Path, said: &str) -> Option<String> {
    if said.is_empty() || said.len() > LONGEST || said.chars().any(char::is_control) {
        return None;
    }
    let said = Path::new(said);
    let inside = if said.is_absolute() {
        let resolved = folder.canonicalize().ok();
        said.strip_prefix(folder)
            .ok()
            .or_else(|| said.strip_prefix(resolved.as_deref()?).ok())?
    } else {
        said
    };
    let mut parts = Vec::new();
    for part in inside.components() {
        match part {
            Component::Normal(name) => {
                let name = name.to_str()?;
                if name == ".git" {
                    return None;
                }
                parts.push(name);
            }
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}

/// The most markers one chat may put up in a second.
pub const AT_MOST_PER_SECOND: u32 = 10;

/// How long the same path from the same chat is not told again: the window still shows it.
pub const SAME_PATH_AGAIN: Duration = Duration::from_millis(500);

/// The most chats a [`Gate`] remembers. Past it, it forgets the quietest.
const CHATS_REMEMBERED: usize = 256;

/// One chat's count in the current second, and the last path it was told.
#[derive(Debug)]
struct Rate {
    began: Instant,
    told: u32,
    last: Option<(String, Instant)>,
}

/// What keeps a chat from flooding the window with markers. In memory only.
#[derive(Debug, Default)]
pub struct Gate {
    chats: HashMap<u32, Rate>,
}

impl Gate {
    /// Whether chat `chat`'s marker on `path` at `now` is told to the window.
    pub fn lets(&mut self, chat: u32, path: &str, now: Instant) -> bool {
        if !self.chats.contains_key(&chat) && self.chats.len() >= CHATS_REMEMBERED {
            let quietest = self
                .chats
                .iter()
                .min_by_key(|(_, rate)| rate.began)
                .map(|(chat, _)| *chat);
            if let Some(quietest) = quietest {
                self.chats.remove(&quietest);
            }
        }
        let rate = self.chats.entry(chat).or_insert(Rate {
            began: now,
            told: 0,
            last: None,
        });
        if now.duration_since(rate.began) >= Duration::from_secs(1) {
            rate.began = now;
            rate.told = 0;
        }
        if let Some((last, at)) = &rate.last
            && last == path
            && now.duration_since(*at) < SAME_PATH_AGAIN
        {
            return false;
        }
        if rate.told >= AT_MOST_PER_SECOND {
            return false;
        }
        rate.told += 1;
        rate.last = Some((path.to_owned(), now));
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_claude_code_file_tool_names_the_path_it_touches() {
        for tool in ["Read", "Write", "Edit", "MultiEdit"] {
            let payload = json!({"tool_name": tool, "tool_input": {"file_path": "/w/src/a.rs"}});
            assert_eq!(of_tool(&payload).as_deref(), Some("/w/src/a.rs"), "{tool}");
        }
        let grep = json!({"tool_name": "Grep", "tool_input": {"pattern": "x", "path": "/w/src"}});
        assert_eq!(of_tool(&grep).as_deref(), Some("/w/src"));
    }

    #[test]
    fn every_file_tool_is_one_an_armed_hook_runs_on() {
        let armed: Vec<&str> = crate::hookreg::HANDLERS
            .iter()
            .filter(|h| ["pretooluse-read", "pretooluse-edit", "posttooluse"].contains(&h.name))
            .flat_map(|h| h.matcher.unwrap_or_default().split('|'))
            .collect();
        for tool in FILE_TOOLS {
            assert!(
                armed.contains(&tool),
                "{tool} is in no armed matcher: {armed:?}"
            );
        }
    }

    #[test]
    fn an_opencode_file_tool_names_the_path_it_touches() {
        // charter's opencode plugin routes `read`, `edit` and `write` under these names, with
        // opencode's own argument names.
        for tool in ["Read", "Write", "Edit"] {
            let payload = json!({"tool_name": tool, "tool_input": {"filePath": "/w/b.ts"}});
            assert_eq!(of_tool(&payload).as_deref(), Some("/w/b.ts"), "{tool}");
        }
    }

    #[test]
    fn no_other_tool_and_no_pathless_call_names_a_file() {
        for payload in [
            json!({"tool_name": "Bash", "tool_input": {"command": "cat /w/a.rs"}}),
            // Codex's shell, which is all its armed hook sees.
            json!({"tool_name": "Bash", "tool_input": {"command": "apply_patch"}}),
            json!({"tool_name": "Grep", "tool_input": {"pattern": "x"}}),
            json!({"tool_name": "Read", "tool_input": {"file_path": ""}}),
            json!({"tool_name": "WebFetch", "tool_input": {"path": "/w/a"}}),
            // No hook's matcher arms it (`hookreg`), so it is not one either.
            json!({"tool_name": "NotebookEdit", "tool_input": {"notebook_path": "/w/n.ipynb"}}),
            json!({"tool_input": {"file_path": "/w/a"}}),
            json!("not an object"),
        ] {
            assert_eq!(of_tool(&payload), None, "{payload}");
        }
    }

    #[test]
    fn a_path_inside_the_folder_is_kept_relative_to_it() {
        let folder = Path::new("/w/branch");
        assert_eq!(
            confine(folder, "/w/branch/src/a.rs").as_deref(),
            Some("src/a.rs")
        );
        assert_eq!(confine(folder, "src/./a.rs").as_deref(), Some("src/a.rs"));
        assert_eq!(confine(folder, "README.md").as_deref(), Some("README.md"));
    }

    #[test]
    fn a_path_outside_the_folder_is_dropped() {
        let folder = Path::new("/w/branch");
        for said in [
            "/w/other/a.rs",
            "/w/branch-two/a.rs",
            "/etc/passwd",
            "../branch-two/a.rs",
            "src/../../other/a.rs",
            // Inside by the time it is tidied, and still refused: `..` is never kept.
            "src/../a.rs",
            "/w/branch/src/../a.rs",
            "/w/branch",
            "/w/branch/",
            ".",
            "",
            ".git/config",
            "/w/branch/.git/HEAD",
            "sub/.git/config",
            "a\u{0}b",
            "a\nb",
        ] {
            assert_eq!(confine(folder, said), None, "{said:?}");
        }
        assert_eq!(confine(folder, &"a/".repeat(LONGEST)), None);
    }

    #[test]
    fn a_folder_reached_through_a_link_is_matched_where_it_resolves() {
        let dir = tempfile::tempdir().expect("a directory");
        let real = dir.path().join("real");
        std::fs::create_dir(&real).unwrap();
        let resolved = real.canonicalize().unwrap();
        #[cfg(unix)]
        {
            let link = dir.path().join("link");
            std::os::unix::fs::symlink(&real, &link).unwrap();
            let said = resolved.join("a.rs");
            assert_eq!(
                confine(&link, said.to_str().unwrap()).as_deref(),
                Some("a.rs")
            );
        }
    }

    #[test]
    fn a_chat_puts_up_at_most_ten_markers_a_second() {
        let mut gate = Gate::default();
        let now = Instant::now();
        let told = (0..50)
            .filter(|n| gate.lets(1, &format!("f{n}"), now))
            .count();
        assert_eq!(told, 10);
        // Another chat has its own second.
        assert!(gate.lets(2, "f0", now));
        // And the next second the first chat is heard again.
        assert!(gate.lets(1, "g", now + Duration::from_secs(1)));
    }

    #[test]
    fn the_same_path_again_at_once_is_not_told_twice() {
        let mut gate = Gate::default();
        let now = Instant::now();
        assert!(gate.lets(1, "a", now));
        assert!(!gate.lets(1, "a", now + Duration::from_millis(100)));
        assert!(gate.lets(1, "b", now + Duration::from_millis(100)));
        assert!(gate.lets(1, "a", now + Duration::from_millis(700)));
    }

    #[test]
    fn the_gate_remembers_a_bounded_number_of_chats() {
        let mut gate = Gate::default();
        let now = Instant::now();
        for chat in 0..1000 {
            gate.lets(chat, "a", now + Duration::from_millis(u64::from(chat)));
        }
        assert!(gate.chats.len() <= CHATS_REMEMBERED);
        // The newest is remembered: the quietest went first.
        assert!(gate.chats.contains_key(&999));
    }
}
